//! Каноническая выдача главы: выбор варианта, fallback по источникам и
//! кэширование списка страниц в БД. Сеть дёргается только при открытии.

use axum::{Json, extract::State};
use serde::Serialize;
use sqlx::SqlitePool;

use crate::{
    auth::session::now_unix,
    error::AppError,
    extract::ApiPath,
    sources::{
        ChapterKind, ContentKind, ExternalId, MediaKind, SourceChapter, SourceContent, SourcePage,
    },
    state::AppState,
};

use super::selection::{self, AvailabilityView, ChapterCandidate};

#[derive(Debug, Clone, Serialize)]
pub struct SourceRef {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct ContentPageDto {
    pub id: i64,
    pub index: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    /// Внутренний URL: клиент не ходит на чужие сайты за картинками.
    pub image_url: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ContentDto {
    Images { pages: Vec<ContentPageDto> },
    Video { embed_url: String },
    Text { format: String },
}

#[derive(Debug, Serialize)]
pub struct FallbackDto {
    pub from_source: SourceRef,
    pub reason: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ContentResponse {
    pub chapter_id: i64,
    pub chapter_key: String,
    pub number_text: Option<String>,
    pub name: Option<String>,
    pub content_kind: String,
    pub source: SourceRef,
    pub content: ContentDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<FallbackDto>,
}

pub async fn canonical_content(
    State(state): State<AppState>,
    ApiPath((title_id, chapter_key)): ApiPath<(i64, String)>,
) -> Result<Json<ContentResponse>, AppError> {
    Ok(Json(canonical(&state, title_id, &chapter_key).await?))
}

pub async fn direct_content(
    State(state): State<AppState>,
    ApiPath(chapter_id): ApiPath<i64>,
) -> Result<Json<ContentResponse>, AppError> {
    Ok(Json(direct(&state, chapter_id).await?))
}

pub async fn canonical(
    state: &AppState,
    title_id: i64,
    requested_key: &str,
) -> Result<ContentResponse, AppError> {
    let title = selection::load_title_info(&state.pool, title_id).await?;
    let kind = MediaKind::from_db(&title.kind).ok_or(AppError::Internal)?;

    let canonical_key = selection::resolve_key(&state.pool, title_id, requested_key)
        .await?
        .ok_or_else(|| AppError::not_found("chapter_not_found", "chapter not found"))?;

    let now = now_unix();
    let ranks = selection::load_priorities(&state.pool, &title.kind, title.lang.as_deref()).await?;
    let candidates = selection::candidates(&state.pool, title_id, &canonical_key).await?;

    if candidates.is_empty() {
        return Err(AppError::not_found(
            "chapter_not_found",
            "chapter not found",
        ));
    }

    let ordered = selection::order(candidates, title.preferred_source_id, &ranks, now);
    let first_source = source_ref(&ordered[0]);
    let mut fallback_reason: Option<&'static str> = None;
    let mut last_error = None;
    let mut any_disabled = false;
    let mut attempted = false;
    let mut any_locked = false;
    let mut any_missing = false;

    for (index, candidate) in ordered.iter().enumerate() {
        let runtime = {
            let registry = state.sources.read().await;
            registry.runtime(&candidate.source_code)
        };
        let Some(runtime) = runtime else {
            any_disabled = true;
            fallback_reason.get_or_insert("disabled");
            continue;
        };

        let adapter = match runtime.adapter() {
            Ok(adapter) => adapter.clone(),
            Err(error) => {
                any_disabled = true;
                if index == 0 {
                    fallback_reason.get_or_insert(error.reason());
                }
                last_error = Some(error);
                continue;
            }
        };

        match selection::view(candidate, now) {
            AvailabilityView::Unavailable => {
                if candidate.availability == "locked" {
                    any_locked = true;
                } else {
                    any_missing = true;
                }
                if index == 0 {
                    fallback_reason.get_or_insert(if candidate.availability == "locked" {
                        "locked"
                    } else {
                        "missing"
                    });
                }
                continue;
            }
            AvailabilityView::Available | AvailabilityView::Unknown => {}
        }

        // Уже известный список страниц: сеть не нужна
        if let Some(content) = cached_content(&state.pool, candidate).await? {
            return Ok(response(
                candidate,
                &canonical_key,
                content,
                fallback(index, first_source, fallback_reason),
            ));
        }

        attempted = true;
        let source_chapter = source_chapter(candidate)?;
        match adapter.content(kind, &source_chapter).await {
            Ok(content) => {
                let content = persist_success(state, candidate, content).await?;
                return Ok(response(
                    candidate,
                    &canonical_key,
                    content,
                    fallback(index, first_source, fallback_reason),
                ));
            }
            Err(error) if matches!(&error, crate::sources::SourceError::Locked) => {
                any_locked = true;
                mark_availability(&state.pool, candidate, "locked").await?;
                if index == 0 {
                    fallback_reason.get_or_insert("locked");
                }
                last_error = Some(error);
            }
            Err(error) if matches!(&error, crate::sources::SourceError::NotFound) => {
                mark_availability(&state.pool, candidate, "missing").await?;
                if index == 0 {
                    fallback_reason.get_or_insert("missing");
                }
                last_error = Some(error);
            }
            Err(error) => {
                if index == 0 && fallback_reason.is_none() {
                    fallback_reason = Some(error.reason());
                }
                last_error = Some(error);
            }
        }
    }

    if !attempted {
        if any_locked {
            return Err(AppError::forbidden(
                "source_locked",
                "all chapter variants are locked",
            ));
        }
        if any_missing {
            return Err(AppError::not_found(
                "source_not_found",
                "all chapter variants are missing",
            ));
        }
    }
    if let Some(error) = last_error {
        return Err(error.into());
    }
    if any_disabled {
        return Err(crate::sources::SourceError::Disabled.into());
    }
    Err(AppError::not_found(
        "chapter_not_found",
        "no available chapter variant",
    ))
}

pub async fn direct(state: &AppState, chapter_id: i64) -> Result<ContentResponse, AppError> {
    let candidate = sqlx::query_as::<_, ChapterCandidate>(
        "SELECT ch.id AS chapter_id, ch.chapter_key, ch.external_id, ch.number_text, \
                ch.number_milli, ch.volume, ch.chapter_kind, ch.content_kind, ch.name, \
                ch.group_name, ch.language, ch.is_locked, ch.availability, \
                ch.availability_checked_at, ch.page_count, ch.ordinal_site, ch.external_url, \
                ch.embed_url, e.id AS source_entry_id, e.source_id, e.title_id, e.is_primary, \
                s.code AS source_code, s.name AS source_name, s.enabled AS source_enabled \
         FROM chapters ch \
         JOIN source_entries e ON e.id = ch.source_entry_id \
         JOIN sources s ON s.id = e.source_id \
         WHERE ch.id = ? AND ch.removed_at IS NULL",
    )
    .bind(chapter_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("chapter_not_found", "chapter not found"))?;

    let title = selection::load_title_info(&state.pool, candidate.title_id).await?;
    let kind = MediaKind::from_db(&title.kind).ok_or(AppError::Internal)?;
    let now = now_unix();

    if selection::view(&candidate, now) == AvailabilityView::Unavailable {
        return Err(AppError::forbidden(
            "source_locked",
            "chapter variant is locked or missing",
        ));
    }

    let runtime = {
        let registry = state.sources.read().await;
        registry.runtime(&candidate.source_code)
    }
    .ok_or(crate::sources::SourceError::Disabled)?;
    let adapter = runtime.adapter()?.clone();

    if let Some(content) = cached_content(&state.pool, &candidate).await? {
        return Ok(response(&candidate, &candidate.chapter_key, content, None));
    }

    let source_chapter = source_chapter(&candidate)?;
    match adapter.content(kind, &source_chapter).await {
        Ok(content) => {
            let content = persist_success(state, &candidate, content).await?;
            Ok(response(&candidate, &candidate.chapter_key, content, None))
        }
        Err(crate::sources::SourceError::Locked) => {
            mark_availability(&state.pool, &candidate, "locked").await?;
            Err(AppError::forbidden(
                "source_locked",
                "chapter is locked at source",
            ))
        }
        Err(crate::sources::SourceError::NotFound) => {
            mark_availability(&state.pool, &candidate, "missing").await?;
            Err(AppError::not_found(
                "source_not_found",
                "chapter is missing at source",
            ))
        }
        Err(error) => Err(error.into()),
    }
}

async fn cached_content(
    pool: &SqlitePool,
    candidate: &ChapterCandidate,
) -> Result<Option<ContentDto>, AppError> {
    match candidate.content_kind.as_deref() {
        Some("images") if candidate.page_count.unwrap_or(0) > 0 => {
            let pages = load_pages(pool, candidate.chapter_id).await?;
            if pages.is_empty() {
                Ok(None)
            } else {
                Ok(Some(ContentDto::Images { pages }))
            }
        }
        Some("video") => Ok(candidate
            .embed_url
            .clone()
            .map(|embed_url| ContentDto::Video { embed_url })),
        _ => Ok(None),
    }
}

async fn persist_success(
    state: &AppState,
    candidate: &ChapterCandidate,
    content: SourceContent,
) -> Result<ContentDto, AppError> {
    let content_kind = content.kind();
    let dto = match content {
        SourceContent::Images { pages } => {
            let pages = replace_pages(&state.pool, candidate.chapter_id, &pages).await?;
            sqlx::query(
                "UPDATE chapters SET availability = 'available', availability_checked_at = ?, \
                        content_kind = 'images', page_count = ?, embed_url = NULL, is_locked = 0 \
                 WHERE id = ?",
            )
            .bind(now_unix())
            .bind(i64::try_from(pages.len()).unwrap_or(i64::MAX))
            .bind(candidate.chapter_id)
            .execute(&state.pool)
            .await?;
            ContentDto::Images { pages }
        }
        SourceContent::Video { embed_url } => {
            sqlx::query(
                "UPDATE chapters SET availability = 'available', availability_checked_at = ?, \
                        content_kind = 'video', page_count = NULL, embed_url = ?, is_locked = 0 \
                 WHERE id = ?",
            )
            .bind(now_unix())
            .bind(&embed_url)
            .bind(candidate.chapter_id)
            .execute(&state.pool)
            .await?;
            ContentDto::Video { embed_url }
        }
        SourceContent::Text { format } => {
            sqlx::query(
                "UPDATE chapters SET availability = 'available', availability_checked_at = ?, \
                        content_kind = 'text', page_count = NULL, is_locked = 0 \
                 WHERE id = ?",
            )
            .bind(now_unix())
            .bind(candidate.chapter_id)
            .execute(&state.pool)
            .await?;
            ContentDto::Text {
                format: format.as_str().to_owned(),
            }
        }
    };

    tracing::debug!(
        chapter_id = candidate.chapter_id,
        content_kind = content_kind.as_str(),
        "chapter content resolved"
    );

    selection::recompute_stats(&state.pool, candidate.source_entry_id).await?;
    selection::recompute_preferred(&state.pool, candidate.title_id).await?;
    Ok(dto)
}

pub async fn replace_pages(
    pool: &SqlitePool,
    chapter_id: i64,
    pages: &[SourcePage],
) -> Result<Vec<ContentPageDto>, AppError> {
    let mut tx = pool.begin().await?;
    let mut result = Vec::with_capacity(pages.len());

    for page in pages {
        let index = i64::from(page.index);
        let id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO pages (chapter_id, idx, image_ref, image_url, width, height) \
             VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT (chapter_id, idx) DO UPDATE SET \
                 image_ref = excluded.image_ref, image_url = excluded.image_url, \
                 width = excluded.width, height = excluded.height \
             RETURNING id",
        )
        .bind(chapter_id)
        .bind(index)
        .bind(&page.image_ref)
        .bind(page.image_url.as_deref())
        .bind(page.width.map(i64::from))
        .bind(page.height.map(i64::from))
        .fetch_one(&mut *tx)
        .await?;

        result.push(ContentPageDto {
            id,
            index,
            width: page.width.map(i64::from),
            height: page.height.map(i64::from),
            image_url: format!("/pages/{id}/image"),
        });
    }

    // Если страниц стало меньше, хвост старого списка не должен остаться
    sqlx::query("DELETE FROM pages WHERE chapter_id = ? AND idx >= ?")
        .bind(chapter_id)
        .bind(i64::try_from(pages.len()).unwrap_or(i64::MAX))
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    result.sort_by_key(|page| page.index);
    Ok(result)
}

async fn load_pages(pool: &SqlitePool, chapter_id: i64) -> Result<Vec<ContentPageDto>, AppError> {
    let rows = sqlx::query_as::<_, (i64, i64, Option<i64>, Option<i64>)>(
        "SELECT id, idx, width, height FROM pages WHERE chapter_id = ? ORDER BY idx",
    )
    .bind(chapter_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, index, width, height)| ContentPageDto {
            id,
            index,
            width,
            height,
            image_url: format!("/pages/{id}/image"),
        })
        .collect())
}

async fn mark_availability(
    pool: &SqlitePool,
    candidate: &ChapterCandidate,
    availability: &str,
) -> Result<(), AppError> {
    sqlx::query("UPDATE chapters SET availability = ?, availability_checked_at = ? WHERE id = ?")
        .bind(availability)
        .bind(now_unix())
        .bind(candidate.chapter_id)
        .execute(pool)
        .await?;

    selection::recompute_stats(pool, candidate.source_entry_id).await?;
    selection::recompute_preferred(pool, candidate.title_id).await?;
    Ok(())
}

fn source_chapter(candidate: &ChapterCandidate) -> Result<SourceChapter, AppError> {
    Ok(SourceChapter {
        external_id: ExternalId::new(candidate.external_id.clone())
            .map_err(|_| AppError::Internal)?,
        chapter_key: candidate.chapter_key.clone(),
        number_text: candidate.number_text.clone(),
        number_milli: candidate.number_milli,
        volume: candidate
            .volume
            .and_then(|volume| u16::try_from(volume).ok()),
        chapter_kind: ChapterKind::from_db(&candidate.chapter_kind).ok_or(AppError::Internal)?,
        content_kind: candidate
            .content_kind
            .as_deref()
            .and_then(ContentKind::from_db),
        name: candidate.name.clone(),
        group_name: candidate.group_name.clone(),
        language: candidate.language.clone(),
        is_locked: candidate.is_locked,
        external_url: candidate.external_url.clone(),
        published_at: None,
        raw_json: None,
    })
}

fn response(
    candidate: &ChapterCandidate,
    chapter_key: &str,
    content: ContentDto,
    fallback: Option<FallbackDto>,
) -> ContentResponse {
    ContentResponse {
        chapter_id: candidate.chapter_id,
        chapter_key: chapter_key.to_owned(),
        number_text: candidate.number_text.clone(),
        name: candidate.name.clone(),
        content_kind: content_kind(&content).to_owned(),
        source: source_ref(candidate),
        content,
        fallback,
    }
}

fn content_kind(content: &ContentDto) -> &'static str {
    match content {
        ContentDto::Images { .. } => "images",
        ContentDto::Video { .. } => "video",
        ContentDto::Text { .. } => "text",
    }
}

fn source_ref(candidate: &ChapterCandidate) -> SourceRef {
    SourceRef {
        code: candidate.source_code.clone(),
        name: candidate.source_name.clone(),
    }
}

fn fallback(
    index: usize,
    first_source: SourceRef,
    reason: Option<&'static str>,
) -> Option<FallbackDto> {
    if index == 0 {
        return None;
    }
    Some(FallbackDto {
        from_source: first_source,
        reason: reason.unwrap_or("unavailable"),
    })
}
