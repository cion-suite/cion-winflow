//! Прогресс чтения: хранится по каноническому `chapter_key`, а не по
//! `chapter_id` источника, поэтому переживает fallback и смену источника.

use axum::{
    Json,
    extract::{Extension, State},
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::session::{CurrentUser, now_unix},
    catalog::selection,
    error::AppError,
    extract::{ApiJson, ApiPath},
    sources::ContentKind,
    state::AppState,
};

const MAX_KEY_CHARS: usize = 256;

#[derive(Debug, Deserialize)]
pub struct ProgressInput {
    pub chapter_key: String,
    pub page: Option<i64>,
    pub seconds: Option<i64>,
    pub scroll: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct ProgressDto {
    pub chapter_key: String,
    pub page: Option<i64>,
    pub seconds: Option<i64>,
    pub scroll: Option<f64>,
    pub updated_at: i64,
}

#[derive(Debug, Serialize)]
pub struct ProgressResponse {
    pub progress: Option<ProgressDto>,
}

pub async fn get_progress(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(title_id): ApiPath<i64>,
) -> Result<Json<ProgressResponse>, AppError> {
    selection::load_title_info(&state.pool, title_id).await?;

    let row = sqlx::query_as::<_, (String, Option<i64>, Option<i64>, Option<f64>, i64)>(
        "SELECT chapter_key, page, seconds, scroll, updated_at \
         FROM progress WHERE user_id = ? AND title_id = ?",
    )
    .bind(user.id)
    .bind(title_id)
    .fetch_optional(&state.pool)
    .await?;

    Ok(Json(ProgressResponse {
        progress: row.map(
            |(chapter_key, page, seconds, scroll, updated_at)| ProgressDto {
                chapter_key,
                page,
                seconds,
                scroll,
                updated_at,
            },
        ),
    }))
}

pub async fn save_progress(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(title_id): ApiPath<i64>,
    ApiJson(input): ApiJson<ProgressInput>,
) -> Result<Json<ProgressDto>, AppError> {
    let key = input.chapter_key.trim();
    if key.is_empty() || key.chars().count() > MAX_KEY_CHARS {
        return Err(AppError::bad_request(
            "invalid_chapter_key",
            "chapter_key must be 1..=256 characters",
        ));
    }

    let title = selection::load_title_info(&state.pool, title_id).await?;
    let canonical = selection::resolve_key(&state.pool, title_id, key)
        .await?
        .ok_or_else(|| AppError::not_found("chapter_not_found", "chapter not found"))?;

    let ranks = selection::load_priorities(&state.pool, &title.kind, title.lang.as_deref()).await?;
    let candidates = selection::candidates(&state.pool, title_id, &canonical).await?;
    if candidates.is_empty() {
        return Err(AppError::not_found(
            "chapter_not_found",
            "chapter not found",
        ));
    }
    let now = now_unix();
    let ordered = selection::order(candidates, title.preferred_source_id, &ranks, now);
    // Сначала вариант с известным content_kind: он задаёт и поле позиции,
    // и page_count. Если тип неизвестен везде — 409 ниже.
    let chosen = ordered
        .iter()
        .find(|candidate| candidate.source_enabled && candidate.content_kind.is_some())
        .or_else(|| {
            ordered
                .iter()
                .find(|candidate| candidate.content_kind.is_some())
        })
        .or_else(|| selection::first_usable(&ordered, now))
        .ok_or_else(|| AppError::not_found("chapter_not_found", "chapter not found"))?;

    // Тип контента берём у выбранного варианта: клиент не должен угадывать
    let content_kind = chosen
        .content_kind
        .as_deref()
        .and_then(ContentKind::from_db)
        .ok_or_else(|| {
            AppError::conflict(
                "content_kind_unknown",
                "open the chapter through /content first",
            )
        })?;

    let (page, seconds, scroll) = match content_kind {
        ContentKind::Images => {
            reject_foreign(&input, true, false, false)?;
            let page = input.page.ok_or_else(|| {
                AppError::bad_request("validation", "`page` is required for image chapters")
            })?;
            if page < 0 {
                return Err(AppError::bad_request(
                    "validation",
                    "page must be greater than or equal to 0",
                ));
            }
            if let Some(page_count) = chosen.page_count
                && page >= page_count
            {
                return Err(AppError::bad_request(
                    "validation",
                    "page is out of range for this chapter",
                ));
            }
            (Some(page), None, None)
        }
        ContentKind::Video => {
            reject_foreign(&input, false, true, false)?;
            let seconds = input.seconds.ok_or_else(|| {
                AppError::bad_request("validation", "`seconds` is required for video chapters")
            })?;
            if seconds < 0 {
                return Err(AppError::bad_request(
                    "validation",
                    "seconds must be greater than or equal to 0",
                ));
            }
            (None, Some(seconds), None)
        }
        ContentKind::Text => {
            reject_foreign(&input, false, false, true)?;
            let scroll = input.scroll.ok_or_else(|| {
                AppError::bad_request("validation", "`scroll` is required for text chapters")
            })?;
            if !scroll.is_finite() || !(0.0..=1.0).contains(&scroll) {
                return Err(AppError::bad_request(
                    "validation",
                    "scroll must be between 0 and 1",
                ));
            }
            (None, None, Some(scroll))
        }
    };

    let updated_at = now_unix();
    sqlx::query(
        "INSERT INTO progress (user_id, title_id, chapter_key, page, seconds, scroll, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT (user_id, title_id) DO UPDATE SET \
             chapter_key = excluded.chapter_key, page = excluded.page, \
             seconds = excluded.seconds, scroll = excluded.scroll, \
             updated_at = excluded.updated_at",
    )
    .bind(user.id)
    .bind(title_id)
    .bind(&canonical)
    .bind(page)
    .bind(seconds)
    .bind(scroll)
    .bind(updated_at)
    .execute(&state.pool)
    .await?;

    Ok(Json(ProgressDto {
        chapter_key: canonical,
        page,
        seconds,
        scroll,
        updated_at,
    }))
}

/// Поле позиции обязано соответствовать типу контента, иначе UI незаметно
/// сохранит прогресс в никуда.
fn reject_foreign(
    input: &ProgressInput,
    page: bool,
    seconds: bool,
    scroll: bool,
) -> Result<(), AppError> {
    let mismatch = (!page && input.page.is_some())
        || (!seconds && input.seconds.is_some())
        || (!scroll && input.scroll.is_some());
    if mismatch {
        return Err(AppError::bad_request(
            "validation",
            "position field does not match the chapter content kind",
        ));
    }
    Ok(())
}
