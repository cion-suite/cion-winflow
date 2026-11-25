use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{
    auth::session::now_unix, catalog::service::TitleSummaryDto, error::AppError, pagination,
    state::AppState,
};

pub const STATUSES: [&str; 5] = ["planned", "in_progress", "completed", "on_hold", "dropped"];

#[derive(Debug, Deserialize)]
pub struct LibraryInput {
    pub status: String,
    /// Отсутствие поля означает «без оценки»: UI всегда присылает текущую.
    pub score: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LibraryParams {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct LibraryEntryDto {
    pub title_id: i64,
    pub status: String,
    pub score: Option<i64>,
    pub added_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize)]
pub struct LibraryItemDto {
    pub title_id: i64,
    pub status: String,
    pub score: Option<i64>,
    pub added_at: i64,
    pub updated_at: i64,
    pub title: TitleSummaryDto,
}

#[derive(Debug, Serialize)]
pub struct LibraryListResponse {
    pub items: Vec<LibraryItemDto>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct LibraryRow {
    title_id: i64,
    status: String,
    score: Option<i64>,
    added_at: i64,
    updated_at: i64,
    kind: String,
    name: String,
    year: Option<i64>,
    title_status: Option<String>,
    rating: Option<f64>,
    has_cover: bool,
    title_updated_at: i64,
}

impl From<LibraryRow> for LibraryItemDto {
    fn from(row: LibraryRow) -> Self {
        Self {
            title_id: row.title_id,
            status: row.status,
            score: row.score,
            added_at: row.added_at,
            updated_at: row.updated_at,
            title: TitleSummaryDto {
                id: row.title_id,
                kind: row.kind,
                name: row.name,
                year: row.year,
                status: row.title_status,
                rating: row.rating,
                has_cover: row.has_cover,
                updated_at: row.title_updated_at,
            },
        }
    }
}

pub async fn list_library(
    pool: &SqlitePool,
    user_id: i64,
    params: LibraryParams,
) -> Result<LibraryListResponse, AppError> {
    let (page, page_size) = pagination::resolve(params.page, params.page_size)?;

    let total = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM library WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await?;

    let rows = sqlx::query_as::<_, LibraryRow>(
        "SELECT l.title_id, l.status, l.score, l.added_at, l.updated_at, \
                t.kind, t.name, t.year, t.status AS title_status, t.rating, \
                t.cover_path IS NOT NULL AS has_cover, t.updated_at AS title_updated_at \
         FROM library l JOIN titles t ON t.id = l.title_id \
         WHERE l.user_id = ? \
         ORDER BY l.updated_at DESC, l.title_id DESC \
         LIMIT ? OFFSET ?",
    )
    .bind(user_id)
    .bind(page_size)
    .bind(pagination::offset(page, page_size))
    .fetch_all(pool)
    .await?;

    Ok(LibraryListResponse {
        items: rows.into_iter().map(LibraryItemDto::from).collect(),
        total,
        page,
        page_size,
    })
}

pub async fn upsert_library(
    state: &AppState,
    user_id: i64,
    title_id: i64,
    input: LibraryInput,
) -> Result<LibraryEntryDto, AppError> {
    let status = normalize_status(&input.status)?;
    let score = normalize_score(input.score)?;

    let title_exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM titles WHERE id = ?")
        .bind(title_id)
        .fetch_optional(&state.pool)
        .await?;
    if title_exists.is_none() {
        return Err(AppError::not_found("title_not_found", "title not found"));
    }

    let now = now_unix();
    sqlx::query(
        "INSERT INTO library (user_id, title_id, status, score, added_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT (user_id, title_id) DO UPDATE SET \
             status = excluded.status, score = excluded.score, updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(title_id)
    .bind(&status)
    .bind(score)
    .bind(now)
    .bind(now)
    .execute(&state.pool)
    .await?;

    let (added_at, updated_at) = sqlx::query_as::<_, (i64, i64)>(
        "SELECT added_at, updated_at FROM library WHERE user_id = ? AND title_id = ?",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_one(&state.pool)
    .await?;

    Ok(LibraryEntryDto {
        title_id,
        status,
        score,
        added_at,
        updated_at,
    })
}

pub async fn remove_library(state: &AppState, user_id: i64, title_id: i64) -> Result<(), AppError> {
    sqlx::query("DELETE FROM library WHERE user_id = ? AND title_id = ?")
        .bind(user_id)
        .bind(title_id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

fn normalize_status(status: &str) -> Result<String, AppError> {
    let status = status.trim().to_lowercase();
    if STATUSES.contains(&status.as_str()) {
        Ok(status)
    } else {
        Err(AppError::bad_request(
            "library_status_invalid",
            format!("status must be one of: {}", STATUSES.join(", ")),
        ))
    }
}

fn normalize_score(score: Option<i64>) -> Result<Option<i64>, AppError> {
    match score {
        None => Ok(None),
        Some(score) if (1..=10).contains(&score) => Ok(Some(score)),
        Some(_) => Err(AppError::bad_request(
            "score_invalid",
            "score must be between 1 and 10",
        )),
    }
}
