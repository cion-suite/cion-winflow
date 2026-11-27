//! Каноническая выдача главы: выбор варианта, fallback по источникам и
//! кэширование списка страниц в БД. Сеть дёргается только при открытии.

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Extension, State},
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{
    auth::session::{CurrentUser, now_unix},
    error::AppError,
    extract::{ApiPath, ApiQuery},
    source_auth::service as source_auth_service,
    sources::{
        ChapterKind, ContentKind, ExternalId, SourceChapter, SourceContent, SourceError, SourcePage,
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

/// Язык просмотра: читать вариант из выбранного перевода.
#[derive(Debug, Deserialize)]
pub struct ContentQuery {
    pub lang: Option<String>,
}

pub async fn canonical_content(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath((title_id, chapter_key)): ApiPath<(i64, String)>,
    ApiQuery(query): ApiQuery<ContentQuery>,
) -> Result<Json<ContentResponse>, AppError> {
    let view_lang = super::service::normalize_lang(query.lang.as_deref());
    Ok(Json(
        canonical(
            &state,
            user.id,
            title_id,
            &chapter_key,
            view_lang.as_deref(),
        )
        .await?,
    ))
}

pub async fn direct_content(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(chapter_id): ApiPath<i64>,
) -> Result<Json<ContentResponse>, AppError> {
    Ok(Json(direct(&state, user.id, chapter_id).await?))
}

/// Код источника → язык дескриптора для **включённых** источников тайтла.
/// Выключенный источник не может отдать главу, поэтому его язык не делаем
/// действующим: иначе старый `?lang=` останется в тупике на 503.
async fn title_source_langs(
    state: &AppState,
    title_id: i64,
) -> Result<HashMap<String, String>, AppError> {
    let codes = sqlx::query_scalar::<_, String>(
        "SELECT s.code FROM source_entries e JOIN sources s ON s.id = e.source_id \
         WHERE e.title_id = ? \
           AND s.enabled = 1 AND s.base_url IS NOT NULL AND s.config_error IS NULL",
    )
    .bind(title_id)
    .fetch_all(&state.pool)
    .await?;
    let registry = state.sources.read().await;
    Ok(codes
        .into_iter()
        .filter_map(|code| {
            let lang = registry.descriptor(&code)?.lang.to_owned();
            Some((code, lang))
        })
        .collect())
}

pub async fn canonical(
    state: &AppState,
    user_id: i64,
    title_id: i64,
    requested_key: &str,
    view_lang: Option<&str>,
) -> Result<ContentResponse, AppError> {
    let title = selection::load_title_info(&state.pool, title_id).await?;

    let canonical_key = selection::resolve_key(&state.pool, title_id, requested_key)
        .await?
        .ok_or_else(|| AppError::not_found("chapter_not_found", "chapter not found"))?;

    // Карта нужна только при явном языке: без него лишний SELECT на пути
    // открытия главы ни к чему.
    let source_langs = match view_lang {
        Some(_) => title_source_langs(state, title_id).await?,
        None => HashMap::new(),
    };
    // Устаревший URL/localStorage не должен ломать чтение: если включённого
    // источника такого языка у тайтла нет, работаем как без языка.
    let view_lang =
        view_lang.filter(|lang| source_langs.values().any(|value| value.as_str() == *lang));

    let now = now_unix();
    let ranks =
        selection::load_priorities(&state.pool, view_lang.or(title.lang.as_deref())).await?;
    let candidates = selection::candidates(&state.pool, title_id, &canonical_key)
        .await?
        .into_iter()
        // Строго в выбранном языке: если варианта нет, не подмешиваем
        // другой перевод — иначе читатель не заметит подмену.
        .filter(|candidate| {
            view_lang.is_none_or(|lang| {
                source_langs
                    .get(&candidate.source_code)
                    .is_some_and(|value| value.as_str() == lang)
            })
        })
        .collect::<Vec<_>>();

    if candidates.is_empty() {
        return Err(AppError::not_found(
            "chapter_not_found",
            "chapter not found",
        ));
    }

    let ordered = selection::order(candidates, title.preferred_source_id, &ranks, now);
    let first_source = source_ref(&ordered[0]);
    let mut fallback_reason: Option<&'static str> = None;
    let mut last_error: Option<AppError> = None;
    let mut any_disabled = false;
    let mut attempted = false;
    let mut any_locked = false;
    let mut any_missing = false;
    let mut any_auth_required = false;

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
                last_error = Some(error.into());
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

        let auth = source_auth_service::resolve(state, user_id, candidate.source_id).await?;
        let auth_present = auth.is_some();

        // Помеченную главу без собственной учётки не открываем: иначе
        // пользователь увидит чужой контент или «пустую» страницу.
        if candidate.auth_required && !auth_present {
            any_auth_required = true;
            if index == 0 {
                fallback_reason.get_or_insert("auth_required");
            }
            continue;
        }

        let verified = if candidate.auth_required {
            source_auth_service::verified_recently(&state.pool, user_id, candidate.source_id, now)
                .await?
        } else {
            true
        };

        // Уже известный список страниц: сеть не нужна. Помеченную главу
        // отдаём из кэша только при свежей проверке учётки.
        if verified && let Some(content) = cached_content(&state.pool, candidate).await? {
            return Ok(response(
                candidate,
                &canonical_key,
                content,
                fallback(index, first_source, fallback_reason),
            ));
        }

        attempted = true;
        let source_chapter = source_chapter(candidate)?;

        if candidate.auth_required {
            match adapter.content(&source_chapter, auth.as_ref()).await {
                Ok(content) => {
                    source_auth_service::mark_verified(&state.pool, user_id, candidate.source_id)
                        .await?;
                    let content = persist_success(state, candidate, content, true).await?;
                    return Ok(response(
                        candidate,
                        &canonical_key,
                        content,
                        fallback(index, first_source, fallback_reason),
                    ));
                }
                Err(error) => {
                    let (kind, mapped) =
                        classify_failure(state, user_id, candidate, error, true).await?;
                    note_failure(kind, index, &mut fallback_reason);
                    match kind {
                        ContentFailure::Locked => any_locked = true,
                        ContentFailure::Missing => any_missing = true,
                        ContentFailure::AuthRequired | ContentFailure::AuthInvalid => {
                            any_auth_required = true;
                        }
                        _ => {}
                    }
                    last_error = Some(mapped);
                }
            }
            continue;
        }

        // Обычная глава: сначала без учётки — публичный контент не должен
        // зависеть от аккаунта. При 401/404 и наличии учётки повторяем.
        match adapter.content(&source_chapter, None).await {
            Ok(content) => {
                let content = persist_success(state, candidate, content, false).await?;
                return Ok(response(
                    candidate,
                    &canonical_key,
                    content,
                    fallback(index, first_source, fallback_reason),
                ));
            }
            Err(error) if auth_present && retry_with_auth(&error) => {
                match adapter.content(&source_chapter, auth.as_ref()).await {
                    Ok(content) => {
                        source_auth_service::mark_verified(
                            &state.pool,
                            user_id,
                            candidate.source_id,
                        )
                        .await?;
                        let content = persist_success(state, candidate, content, true).await?;
                        return Ok(response(
                            candidate,
                            &canonical_key,
                            content,
                            fallback(index, first_source, fallback_reason),
                        ));
                    }
                    Err(error) => {
                        let (kind, mapped) =
                            classify_failure(state, user_id, candidate, error, true).await?;
                        note_failure(kind, index, &mut fallback_reason);
                        match kind {
                            ContentFailure::Locked => any_locked = true,
                            ContentFailure::Missing => any_missing = true,
                            ContentFailure::AuthRequired | ContentFailure::AuthInvalid => {
                                any_auth_required = true;
                            }
                            _ => {}
                        }
                        last_error = Some(mapped);
                    }
                }
            }
            Err(error) => {
                let (kind, mapped) =
                    classify_failure(state, user_id, candidate, error, false).await?;
                note_failure(kind, index, &mut fallback_reason);
                match kind {
                    ContentFailure::Locked => any_locked = true,
                    ContentFailure::Missing => any_missing = true,
                    ContentFailure::AuthRequired | ContentFailure::AuthInvalid => {
                        any_auth_required = true;
                    }
                    _ => {}
                }
                last_error = Some(mapped);
            }
        }
    }

    if !attempted {
        if any_auth_required {
            return Err(source_auth_service::auth_not_configured());
        }
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
        return Err(error);
    }
    if any_disabled {
        return Err(crate::sources::SourceError::Disabled.into());
    }
    Err(AppError::not_found(
        "chapter_not_found",
        "no available chapter variant",
    ))
}

pub async fn direct(
    state: &AppState,
    user_id: i64,
    chapter_id: i64,
) -> Result<ContentResponse, AppError> {
    let candidate = sqlx::query_as::<_, ChapterCandidate>(
        "SELECT ch.id AS chapter_id, ch.chapter_key, ch.external_id, ch.number_text, \
                ch.number_milli, ch.volume, ch.chapter_kind, ch.content_kind, ch.name, \
                ch.group_name, ch.language, ch.is_locked, ch.auth_required, ch.availability, \
                ch.availability_checked_at, ch.page_count, ch.ordinal_site, ch.external_url, \
                ch.embed_url, ch.published_at, e.id AS source_entry_id, e.source_id, e.title_id, e.is_primary, \
                s.code AS source_code, s.name AS source_name, (s.enabled = 1 AND s.base_url IS NOT NULL AND s.config_error IS NULL) AS source_enabled \
         FROM chapters ch \
         JOIN source_entries e ON e.id = ch.source_entry_id \
         JOIN sources s ON s.id = e.source_id \
         WHERE ch.id = ? AND ch.removed_at IS NULL",
    )
    .bind(chapter_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("chapter_not_found", "chapter not found"))?;

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

    let auth = source_auth_service::resolve(state, user_id, candidate.source_id).await?;
    let auth_present = auth.is_some();

    if candidate.auth_required && !auth_present {
        return Err(source_auth_service::auth_not_configured());
    }

    let verified = if candidate.auth_required {
        source_auth_service::verified_recently(&state.pool, user_id, candidate.source_id, now)
            .await?
    } else {
        true
    };

    if verified && let Some(content) = cached_content(&state.pool, &candidate).await? {
        return Ok(response(&candidate, &candidate.chapter_key, content, None));
    }

    let source_chapter = source_chapter(&candidate)?;

    let (content, used_auth) = if candidate.auth_required {
        match adapter.content(&source_chapter, auth.as_ref()).await {
            Ok(content) => (content, true),
            Err(error) => return Err(direct_failure(state, user_id, &candidate, error, true).await),
        }
    } else {
        match adapter.content(&source_chapter, None).await {
            Ok(content) => (content, false),
            Err(error) if auth_present && retry_with_auth(&error) => {
                match adapter.content(&source_chapter, auth.as_ref()).await {
                    Ok(content) => (content, true),
                    Err(error) => {
                        return Err(direct_failure(state, user_id, &candidate, error, true).await);
                    }
                }
            }
            Err(error) => {
                return Err(direct_failure(state, user_id, &candidate, error, false).await);
            }
        }
    };

    if used_auth {
        source_auth_service::mark_verified(&state.pool, user_id, candidate.source_id).await?;
    }
    let content = persist_success(state, &candidate, content, used_auth).await?;
    Ok(response(&candidate, &candidate.chapter_key, content, None))
}

/// Характер ошибки контента. От него зависит, что писать в глобальные
/// флаги доступности: auth-ошибка одного пользователя не должна помечать
/// главу закрытой для всех.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContentFailure {
    Locked,
    Missing,
    AuthRequired,
    AuthInvalid,
    Blocked,
    Other,
}

impl ContentFailure {
    /// Словарь `fallback.reason` для UI (`missing`/`locked`), а не
    /// `SourceError::reason`, где тот же 404 называется `not_found`.
    fn reason(self) -> &'static str {
        match self {
            Self::Locked => "locked",
            Self::Missing => "missing",
            Self::AuthRequired => "auth_required",
            Self::AuthInvalid => "auth_invalid",
            Self::Blocked => "blocked",
            Self::Other => "upstream_failed",
        }
    }
}

fn retry_with_auth(error: &SourceError) -> bool {
    matches!(
        error,
        SourceError::AuthRequired | SourceError::AuthMaybeRequired | SourceError::NotFound
    )
}

fn note_failure(kind: ContentFailure, index: usize, fallback_reason: &mut Option<&'static str>) {
    if index == 0 {
        fallback_reason.get_or_insert(kind.reason());
    }
}

async fn classify_failure(
    state: &AppState,
    user_id: i64,
    candidate: &ChapterCandidate,
    error: SourceError,
    used_auth: bool,
) -> Result<(ContentFailure, AppError), AppError> {
    let kind = match &error {
        SourceError::Locked => ContentFailure::Locked,
        SourceError::NotFound => ContentFailure::Missing,
        // С учёткой неоднозначный 404 окончателен: главы нет.
        SourceError::AuthMaybeRequired if used_auth => ContentFailure::Missing,
        SourceError::AuthRequired | SourceError::AuthMaybeRequired => {
            if used_auth {
                ContentFailure::AuthInvalid
            } else {
                ContentFailure::AuthRequired
            }
        }
        SourceError::Blocked => ContentFailure::Blocked,
        _ => ContentFailure::Other,
    };

    match kind {
        ContentFailure::Locked => {
            mark_availability(&state.pool, candidate, "locked").await?;
        }
        ContentFailure::Missing => {
            mark_availability(&state.pool, candidate, "missing").await?;
        }
        ContentFailure::AuthInvalid => {
            source_auth_service::mark_auth_error(&state.pool, user_id, candidate.source_id).await?;
        }
        _ => {}
    }

    Ok((kind, error.into_app(used_auth)))
}

async fn direct_failure(
    state: &AppState,
    user_id: i64,
    candidate: &ChapterCandidate,
    error: SourceError,
    used_auth: bool,
) -> AppError {
    match classify_failure(state, user_id, candidate, error, used_auth).await {
        Ok((_, mapped)) => mapped,
        Err(error) => error,
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
    auth_required: bool,
) -> Result<ContentDto, AppError> {
    let content_kind = content.kind();
    let dto = match content {
        SourceContent::Images { pages } => {
            let pages = replace_pages(&state.pool, candidate.chapter_id, &pages).await?;
            sqlx::query(
                "UPDATE chapters SET availability = 'available', availability_checked_at = ?, \
                        content_kind = 'images', page_count = ?, embed_url = NULL, is_locked = 0, \
                        auth_required = ? \
                 WHERE id = ?",
            )
            .bind(now_unix())
            .bind(i64::try_from(pages.len()).unwrap_or(i64::MAX))
            .bind(i64::from(auth_required))
            .bind(candidate.chapter_id)
            .execute(&state.pool)
            .await?;
            ContentDto::Images { pages }
        }
        SourceContent::Video { embed_url } => {
            sqlx::query(
                "UPDATE chapters SET availability = 'available', availability_checked_at = ?, \
                        content_kind = 'video', page_count = NULL, embed_url = ?, is_locked = 0, \
                        auth_required = ? \
                 WHERE id = ?",
            )
            .bind(now_unix())
            .bind(&embed_url)
            .bind(i64::from(auth_required))
            .bind(candidate.chapter_id)
            .execute(&state.pool)
            .await?;
            ContentDto::Video { embed_url }
        }
        SourceContent::Text { format } => {
            sqlx::query(
                "UPDATE chapters SET availability = 'available', availability_checked_at = ?, \
                        content_kind = 'text', page_count = NULL, is_locked = 0, \
                        auth_required = ? \
                 WHERE id = ?",
            )
            .bind(now_unix())
            .bind(i64::from(auth_required))
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
    sqlx::query(
        "UPDATE chapters SET availability = ?, availability_checked_at = ?, is_locked = ? \
         WHERE id = ?",
    )
    .bind(availability)
    .bind(now_unix())
    .bind(i64::from(availability == "locked"))
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
