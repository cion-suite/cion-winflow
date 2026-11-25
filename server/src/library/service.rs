use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::{
    auth::session::now_unix, catalog::service::TitleSummaryDto, error::AppError, pagination,
    state::AppState,
};

pub const STATUSES: [&str; 6] = [
    "planned",
    "in_progress",
    "completed",
    "on_hold",
    "dropped",
    "not_interested",
];

pub const MAX_TABS: i64 = 3;
const MAX_TAB_NAME_CHARS: usize = 40;
const MAX_QUERY_CHARS: usize = 200;

#[derive(Debug, Deserialize)]
pub struct LibraryInput {
    pub status: String,
    /// Отсутствие поля означает «без оценки»: UI всегда присылает текущую.
    pub score: Option<i64>,
    /// Избранное независимо от статуса: «Любимое» — отдельная закладка.
    #[serde(default)]
    pub favorite: bool,
}

#[derive(Debug, Default, Deserialize)]
pub struct LibraryParams {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub q: Option<String>,
    pub status: Option<String>,
    /// Строка, а не bool: true/false и 1/0 одинаково удобны в query.
    pub favorite: Option<String>,
    pub tab_id: Option<i64>,
    pub sort: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TabNameInput {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct TitleTabsInput {
    #[serde(default)]
    pub tab_ids: Vec<i64>,
}

#[derive(Debug, Serialize)]
pub struct LibraryEntryDto {
    pub title_id: i64,
    pub status: String,
    pub score: Option<i64>,
    pub favorite: bool,
    pub added_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize)]
pub struct LibraryItemDto {
    pub title_id: i64,
    pub status: String,
    pub score: Option<i64>,
    pub favorite: bool,
    pub added_at: i64,
    pub updated_at: i64,
    /// Сколько глав прочитано по журналу chapter_reads.
    pub read_count: i64,
    /// Число глав самого полного источника: прогресс на карточке.
    pub chapter_count: i64,
    pub tab_ids: Vec<i64>,
    pub title: TitleSummaryDto,
}

#[derive(Debug, Default, Serialize)]
pub struct StatusCounts {
    pub planned: i64,
    pub in_progress: i64,
    pub completed: i64,
    pub on_hold: i64,
    pub dropped: i64,
    pub not_interested: i64,
}

#[derive(Debug, Serialize)]
pub struct LibraryCounts {
    pub all: i64,
    pub statuses: StatusCounts,
    pub favorite: i64,
}

#[derive(Debug, Serialize)]
pub struct LibraryListResponse {
    pub items: Vec<LibraryItemDto>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub counts: LibraryCounts,
}

#[derive(Debug, Serialize)]
pub struct LibraryTabDto {
    pub id: i64,
    pub name: String,
    pub position: i64,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct LibraryTabsResponse {
    pub tabs: Vec<LibraryTabDto>,
    pub limit: i64,
}

#[derive(Debug, Serialize)]
pub struct TitleTabsResponse {
    pub tab_ids: Vec<i64>,
}

#[derive(Debug, sqlx::FromRow)]
struct LibraryRow {
    title_id: i64,
    status: String,
    score: Option<i64>,
    favorite: bool,
    added_at: i64,
    updated_at: i64,
    kind: String,
    name: String,
    year: Option<i64>,
    title_status: Option<String>,
    rating: Option<f64>,
    has_cover: bool,
    title_updated_at: i64,
    read_count: i64,
    chapter_count: i64,
}

impl LibraryRow {
    fn into_item(self, tab_ids: Vec<i64>) -> LibraryItemDto {
        LibraryItemDto {
            title_id: self.title_id,
            status: self.status,
            score: self.score,
            favorite: self.favorite,
            added_at: self.added_at,
            updated_at: self.updated_at,
            read_count: self.read_count,
            chapter_count: self.chapter_count,
            tab_ids,
            title: TitleSummaryDto {
                id: self.title_id,
                kind: self.kind,
                name: self.name,
                year: self.year,
                status: self.title_status,
                rating: self.rating,
                has_cover: self.has_cover,
                updated_at: self.title_updated_at,
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

    let status = match params.status.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(value) => Some(normalize_status(value)?),
    };
    let favorite = parse_favorite(params.favorite.as_deref())?;
    let tab_id = match params.tab_id {
        None => None,
        Some(tab_id) if tab_id > 0 => Some(tab_id),
        Some(_) => {
            return Err(AppError::bad_request(
                "validation",
                "tab_id must be a positive integer",
            ));
        }
    };
    let pattern = match params.q.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(query) => {
            if query.chars().count() > MAX_QUERY_CHARS {
                return Err(AppError::bad_request(
                    "validation",
                    format!("query must be at most {MAX_QUERY_CHARS} characters"),
                ));
            }
            Some(format!("%{}%", escape_like(&query.to_lowercase())))
        }
    };
    let order = order_by(params.sort.as_deref())?;

    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM library l JOIN titles t ON t.id = l.title_id \
         WHERE l.user_id = ? \
           AND (? IS NULL OR l.status = ?) \
           AND (? IS NULL OR l.favorite = ?) \
           AND (? IS NULL OR EXISTS ( \
                 SELECT 1 FROM library_tab_items i \
                 WHERE i.user_id = l.user_id AND i.title_id = l.title_id AND i.tab_id = ? \
               )) \
           AND (? IS NULL OR t.name_lower LIKE ? ESCAPE '\\')",
    )
    .bind(user_id)
    .bind(status.as_deref())
    .bind(status.as_deref())
    .bind(favorite)
    .bind(favorite)
    .bind(tab_id)
    .bind(tab_id)
    .bind(pattern.as_deref())
    .bind(pattern.as_deref())
    .fetch_one(pool)
    .await?;

    let sql = format!(
        "SELECT l.title_id, l.status, l.score, l.favorite, l.added_at, l.updated_at, \
                t.kind, t.name, t.year, t.status AS title_status, t.rating, \
                EXISTS (SELECT 1 FROM title_covers c WHERE c.title_id = t.id) AS has_cover, \
                t.updated_at AS title_updated_at, \
                (SELECT COUNT(*) FROM chapter_reads r \
                 WHERE r.user_id = l.user_id AND r.title_id = l.title_id) AS read_count, \
                (SELECT COALESCE(MAX(e.chapter_count), 0) FROM source_entries e \
                 WHERE e.title_id = l.title_id) AS chapter_count \
         FROM library l JOIN titles t ON t.id = l.title_id \
         WHERE l.user_id = ? \
           AND (? IS NULL OR l.status = ?) \
           AND (? IS NULL OR l.favorite = ?) \
           AND (? IS NULL OR EXISTS ( \
                 SELECT 1 FROM library_tab_items i \
                 WHERE i.user_id = l.user_id AND i.title_id = l.title_id AND i.tab_id = ? \
               )) \
           AND (? IS NULL OR t.name_lower LIKE ? ESCAPE '\\') \
         ORDER BY {order} \
         LIMIT ? OFFSET ?"
    );

    let rows = sqlx::query_as::<_, LibraryRow>(&sql)
        .bind(user_id)
        .bind(status.as_deref())
        .bind(status.as_deref())
        .bind(favorite)
        .bind(favorite)
        .bind(tab_id)
        .bind(tab_id)
        .bind(pattern.as_deref())
        .bind(pattern.as_deref())
        .bind(page_size)
        .bind(pagination::offset(page, page_size))
        .fetch_all(pool)
        .await?;

    let mut tab_map: HashMap<i64, Vec<i64>> = HashMap::new();
    let tab_rows = sqlx::query_as::<_, (i64, i64)>(
        "SELECT title_id, tab_id FROM library_tab_items WHERE user_id = ? ORDER BY tab_id",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    for (title_id, tab) in tab_rows {
        tab_map.entry(title_id).or_default().push(tab);
    }

    let items = rows
        .into_iter()
        .map(|row| {
            let tab_ids = tab_map.remove(&row.title_id).unwrap_or_default();
            row.into_item(tab_ids)
        })
        .collect();

    let counts = load_counts(pool, user_id).await?;

    Ok(LibraryListResponse {
        items,
        total,
        page,
        page_size,
        counts,
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
        "INSERT INTO library (user_id, title_id, status, score, favorite, added_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT (user_id, title_id) DO UPDATE SET \
             status = excluded.status, score = excluded.score, \
             favorite = excluded.favorite, updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(title_id)
    .bind(&status)
    .bind(score)
    .bind(input.favorite)
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
        favorite: input.favorite,
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

pub async fn list_tabs(pool: &SqlitePool, user_id: i64) -> Result<LibraryTabsResponse, AppError> {
    let rows = sqlx::query_as::<_, (i64, String, i64, i64)>(
        "SELECT t.id, t.name, t.position, COUNT(i.title_id) \
         FROM library_tabs t \
         LEFT JOIN library_tab_items i ON i.tab_id = t.id \
         WHERE t.user_id = ? \
         GROUP BY t.id \
         ORDER BY t.position, t.id",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(LibraryTabsResponse {
        tabs: rows
            .into_iter()
            .map(|(id, name, position, count)| LibraryTabDto {
                id,
                name,
                position,
                count,
            })
            .collect(),
        limit: MAX_TABS,
    })
}

pub async fn create_tab(
    state: &AppState,
    user_id: i64,
    input: TabNameInput,
) -> Result<LibraryTabDto, AppError> {
    let name = normalize_tab_name(&input.name)?;

    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM library_tabs WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(&state.pool)
        .await?;
    if count >= MAX_TABS {
        return Err(AppError::conflict(
            "library_tab_limit",
            format!("at most {MAX_TABS} custom tabs are allowed"),
        ));
    }

    ensure_tab_name_free(&state.pool, user_id, &name, None).await?;

    let position = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(position) + 1, 0) FROM library_tabs WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_one(&state.pool)
    .await?;

    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO library_tabs (user_id, name, position, created_at) \
         VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(user_id)
    .bind(&name)
    .bind(position)
    .bind(now_unix())
    .fetch_one(&state.pool)
    .await?;

    Ok(LibraryTabDto {
        id,
        name,
        position,
        count: 0,
    })
}

pub async fn rename_tab(
    state: &AppState,
    user_id: i64,
    tab_id: i64,
    input: TabNameInput,
) -> Result<LibraryTabDto, AppError> {
    let name = normalize_tab_name(&input.name)?;
    let current = load_tab(&state.pool, user_id, tab_id).await?;

    if current.name != name {
        ensure_tab_name_free(&state.pool, user_id, &name, Some(tab_id)).await?;
        sqlx::query("UPDATE library_tabs SET name = ? WHERE id = ? AND user_id = ?")
            .bind(&name)
            .bind(tab_id)
            .bind(user_id)
            .execute(&state.pool)
            .await?;
    }

    Ok(LibraryTabDto {
        id: tab_id,
        name,
        position: current.position,
        count: current.count,
    })
}

pub async fn delete_tab(state: &AppState, user_id: i64, tab_id: i64) -> Result<(), AppError> {
    let result = sqlx::query("DELETE FROM library_tabs WHERE id = ? AND user_id = ?")
        .bind(tab_id)
        .bind(user_id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::not_found(
            "library_tab_not_found",
            "custom tab not found",
        ));
    }
    Ok(())
}

pub async fn set_title_tabs(
    state: &AppState,
    user_id: i64,
    title_id: i64,
    input: TitleTabsInput,
) -> Result<TitleTabsResponse, AppError> {
    let in_library =
        sqlx::query_scalar::<_, i64>("SELECT 1 FROM library WHERE user_id = ? AND title_id = ?")
            .bind(user_id)
            .bind(title_id)
            .fetch_optional(&state.pool)
            .await?
            .is_some();
    if !in_library {
        return Err(AppError::conflict(
            "library_not_in_library",
            "add the title to the library first",
        ));
    }

    let mut tab_ids = Vec::with_capacity(input.tab_ids.len());
    for tab_id in input.tab_ids {
        if !tab_ids.contains(&tab_id) {
            let exists = sqlx::query_scalar::<_, i64>(
                "SELECT 1 FROM library_tabs WHERE id = ? AND user_id = ?",
            )
            .bind(tab_id)
            .bind(user_id)
            .fetch_optional(&state.pool)
            .await?
            .is_some();
            if !exists {
                return Err(AppError::not_found(
                    "library_tab_not_found",
                    "custom tab not found",
                ));
            }
            tab_ids.push(tab_id);
        }
    }

    let now = now_unix();
    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM library_tab_items WHERE user_id = ? AND title_id = ?")
        .bind(user_id)
        .bind(title_id)
        .execute(&mut *tx)
        .await?;
    for tab_id in &tab_ids {
        insert_tab_item(&mut tx, user_id, *tab_id, title_id, now).await?;
    }
    tx.commit().await?;

    Ok(TitleTabsResponse { tab_ids })
}

async fn insert_tab_item(
    tx: &mut Transaction<'_, Sqlite>,
    user_id: i64,
    tab_id: i64,
    title_id: i64,
    now: i64,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO library_tab_items (user_id, tab_id, title_id, added_at) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(tab_id)
    .bind(title_id)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

struct TabRow {
    name: String,
    position: i64,
    count: i64,
}

async fn load_tab(pool: &SqlitePool, user_id: i64, tab_id: i64) -> Result<TabRow, AppError> {
    let row = sqlx::query_as::<_, (String, i64, i64)>(
        "SELECT t.name, t.position, COUNT(i.title_id) \
         FROM library_tabs t \
         LEFT JOIN library_tab_items i ON i.tab_id = t.id \
         WHERE t.id = ? AND t.user_id = ? \
         GROUP BY t.id",
    )
    .bind(tab_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("library_tab_not_found", "custom tab not found"))?;

    Ok(TabRow {
        name: row.0,
        position: row.1,
        count: row.2,
    })
}

async fn ensure_tab_name_free(
    pool: &SqlitePool,
    user_id: i64,
    name: &str,
    except_id: Option<i64>,
) -> Result<(), AppError> {
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM library_tabs WHERE user_id = ? AND name = ? AND (? IS NULL OR id <> ?)",
    )
    .bind(user_id)
    .bind(name)
    .bind(except_id)
    .bind(except_id)
    .fetch_optional(pool)
    .await?
    .is_some();

    if exists {
        return Err(AppError::conflict(
            "library_tab_exists",
            "a custom tab with this name already exists",
        ));
    }
    Ok(())
}

async fn load_counts(pool: &SqlitePool, user_id: i64) -> Result<LibraryCounts, AppError> {
    let mut statuses = StatusCounts::default();
    let rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT status, COUNT(*) FROM library WHERE user_id = ? GROUP BY status",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    for (status, count) in rows {
        match status.as_str() {
            "planned" => statuses.planned = count,
            "in_progress" => statuses.in_progress = count,
            "completed" => statuses.completed = count,
            "on_hold" => statuses.on_hold = count,
            "dropped" => statuses.dropped = count,
            "not_interested" => statuses.not_interested = count,
            other => tracing::warn!(status = other, "unknown library status in database"),
        }
    }

    let all = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM library WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    let favorite = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM library WHERE user_id = ? AND favorite = 1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    Ok(LibraryCounts {
        all,
        statuses,
        favorite,
    })
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

fn normalize_tab_name(raw: &str) -> Result<String, AppError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(AppError::bad_request(
            "library_tab_name_invalid",
            "tab name must not be empty",
        ));
    }
    if name.chars().count() > MAX_TAB_NAME_CHARS {
        return Err(AppError::bad_request(
            "library_tab_name_invalid",
            format!("tab name must be at most {MAX_TAB_NAME_CHARS} characters"),
        ));
    }
    if name.chars().any(char::is_control) {
        return Err(AppError::bad_request(
            "library_tab_name_invalid",
            "tab name must not contain control characters",
        ));
    }
    Ok(name.to_owned())
}

fn parse_favorite(raw: Option<&str>) -> Result<Option<i64>, AppError> {
    match raw.map(str::trim) {
        None | Some("") => Ok(None),
        Some("1" | "true") => Ok(Some(1)),
        Some("0" | "false") => Ok(Some(0)),
        Some(_) => Err(AppError::bad_request(
            "validation",
            "favorite must be true or false",
        )),
    }
}

fn order_by(sort: Option<&str>) -> Result<&'static str, AppError> {
    match sort.map(str::trim) {
        None | Some("") | Some("updated") => Ok("l.updated_at DESC, l.title_id DESC"),
        Some("added") => Ok("l.added_at DESC, l.title_id DESC"),
        Some("title") => Ok("t.name_lower ASC, l.title_id ASC"),
        Some("score") => Ok("l.score IS NULL, l.score DESC, l.updated_at DESC"),
        Some(_) => Err(AppError::bad_request(
            "validation",
            "sort must be one of: updated, added, title, score",
        )),
    }
}

fn escape_like(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}
