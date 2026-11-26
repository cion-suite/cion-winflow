use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{
    auth::session::now_unix, catalog::service::TitleSummaryDto, error::AppError, pagination,
    state::AppState,
};

pub const STATUSES: [&str; 7] = [
    "in_progress",
    "planned",
    "favorite",
    "completed",
    "dropped",
    "on_hold",
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
    /// Своя закладка как статус: перекрывает встроенный `status`, если задана.
    #[serde(default)]
    pub custom_tab_id: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LibraryParams {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub q: Option<String>,
    pub status: Option<String>,
    pub tab_id: Option<i64>,
    pub sort: Option<String>,
    /// Направление сортировки: `asc` или `desc` (по умолчанию).
    pub order: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TabNameInput {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct LibraryEntryDto {
    pub title_id: i64,
    pub status: String,
    pub score: Option<i64>,
    pub custom_tab_id: Option<i64>,
    pub added_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize)]
pub struct LibraryItemDto {
    pub title_id: i64,
    pub status: String,
    pub score: Option<i64>,
    pub custom_tab_id: Option<i64>,
    pub added_at: i64,
    pub updated_at: i64,
    /// Сколько глав прочитано по журналу chapter_reads.
    pub read_count: i64,
    /// Число глав самого полного источника: прогресс на карточке.
    pub chapter_count: i64,
    pub title: TitleSummaryDto,
}

#[derive(Debug, Default, Serialize)]
pub struct StatusCounts {
    pub favorite: i64,
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

#[derive(Debug, sqlx::FromRow)]
struct LibraryRow {
    title_id: i64,
    status: String,
    score: Option<i64>,
    custom_tab_id: Option<i64>,
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
    fn into_item(self) -> LibraryItemDto {
        LibraryItemDto {
            title_id: self.title_id,
            status: self.status,
            score: self.score,
            custom_tab_id: self.custom_tab_id,
            added_at: self.added_at,
            updated_at: self.updated_at,
            read_count: self.read_count,
            chapter_count: self.chapter_count,
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
    let order = order_by(params.sort.as_deref(), params.order.as_deref())?;

    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM library l JOIN titles t ON t.id = l.title_id \
         WHERE l.user_id = ? \
           AND (? IS NULL OR (l.custom_tab_id IS NULL AND l.status = ?)) \
           AND (? IS NULL OR l.custom_tab_id = ?) \
           AND (? IS NULL OR t.name_lower LIKE ? ESCAPE '\\')",
    )
    .bind(user_id)
    .bind(status.as_deref())
    .bind(status.as_deref())
    .bind(tab_id)
    .bind(tab_id)
    .bind(pattern.as_deref())
    .bind(pattern.as_deref())
    .fetch_one(pool)
    .await?;

    let sql = format!(
        "SELECT l.title_id, l.status, l.score, l.custom_tab_id, l.added_at, l.updated_at, \
                t.kind, t.name, t.year, t.status AS title_status, t.rating, \
                EXISTS (SELECT 1 FROM title_covers c WHERE c.title_id = t.id) AS has_cover, \
                t.updated_at AS title_updated_at, \
                (SELECT COUNT(*) FROM chapter_reads r \
                 WHERE r.user_id = l.user_id AND r.title_id = l.title_id) AS read_count, \
                (SELECT COALESCE(MAX(e.chapter_count), 0) FROM source_entries e \
                 WHERE e.title_id = l.title_id) AS chapter_count, \
                (SELECT MAX(ch.published_at) FROM chapters ch \
                 JOIN source_entries e ON e.id = ch.source_entry_id \
                 WHERE e.title_id = l.title_id AND ch.removed_at IS NULL) \
                 AS latest_chapter_at \
         FROM library l JOIN titles t ON t.id = l.title_id \
         WHERE l.user_id = ? \
           AND (? IS NULL OR (l.custom_tab_id IS NULL AND l.status = ?)) \
           AND (? IS NULL OR l.custom_tab_id = ?) \
           AND (? IS NULL OR t.name_lower LIKE ? ESCAPE '\\') \
         ORDER BY {order} \
         LIMIT ? OFFSET ?"
    );

    let rows = sqlx::query_as::<_, LibraryRow>(&sql)
        .bind(user_id)
        .bind(status.as_deref())
        .bind(status.as_deref())
        .bind(tab_id)
        .bind(tab_id)
        .bind(pattern.as_deref())
        .bind(pattern.as_deref())
        .bind(page_size)
        .bind(pagination::offset(page, page_size))
        .fetch_all(pool)
        .await?;

    let items = rows.into_iter().map(LibraryRow::into_item).collect();

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
    let custom_tab_id = match input.custom_tab_id {
        None => None,
        Some(tab_id) if tab_id > 0 => {
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
            Some(tab_id)
        }
        Some(_) => {
            return Err(AppError::bad_request(
                "validation",
                "custom_tab_id must be a positive integer",
            ));
        }
    };

    let title_exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM titles WHERE id = ?")
        .bind(title_id)
        .fetch_optional(&state.pool)
        .await?;
    if title_exists.is_none() {
        return Err(AppError::not_found("title_not_found", "title not found"));
    }

    let now = now_unix();
    sqlx::query(
        "INSERT INTO library (user_id, title_id, status, score, custom_tab_id, added_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT (user_id, title_id) DO UPDATE SET \
             status = excluded.status, score = excluded.score, \
             custom_tab_id = excluded.custom_tab_id, \
             updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(title_id)
    .bind(&status)
    .bind(score)
    .bind(custom_tab_id)
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
        custom_tab_id,
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
        "SELECT t.id, t.name, t.position, \
                (SELECT COUNT(*) FROM library l \
                 WHERE l.user_id = t.user_id AND l.custom_tab_id = t.id) \
         FROM library_tabs t \
         WHERE t.user_id = ? \
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

struct TabRow {
    name: String,
    position: i64,
    count: i64,
}

async fn load_tab(pool: &SqlitePool, user_id: i64, tab_id: i64) -> Result<TabRow, AppError> {
    let row = sqlx::query_as::<_, (String, i64, i64)>(
        "SELECT t.name, t.position, \
                (SELECT COUNT(*) FROM library l \
                 WHERE l.user_id = t.user_id AND l.custom_tab_id = t.id) \
         FROM library_tabs t \
         WHERE t.id = ? AND t.user_id = ?",
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
    // Свой статус перекрывает встроенный: в счётчиках встроенных статусов
    // учитываются только записи без custom_tab_id.
    let rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT status, COUNT(*) FROM library \
         WHERE user_id = ? AND custom_tab_id IS NULL GROUP BY status",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    for (status, count) in rows {
        match status.as_str() {
            "favorite" => statuses.favorite = count,
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

    Ok(LibraryCounts { all, statuses })
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

/// Нейтральные поля сортировки: направление задаётся отдельно, чтобы
/// «по названию» и «по оценке» не дублировались в списке.
fn order_by(sort: Option<&str>, order: Option<&str>) -> Result<String, AppError> {
    let sort = match sort.map(str::trim) {
        None | Some("") => "updated",
        Some(value) => value,
    };

    // Без явного направления название читается от «А», остальные поля —
    // от большего к меньшему. UI присылает order всегда, API — по умолчанию.
    let direction = match order.map(str::trim) {
        None | Some("") => {
            if sort == "title" {
                "ASC"
            } else {
                "DESC"
            }
        }
        Some("desc") => "DESC",
        Some("asc") => "ASC",
        Some(_) => {
            return Err(AppError::bad_request(
                "validation",
                "order must be one of: asc, desc",
            ));
        }
    };

    let primary = match sort {
        "updated" => {
            format!(
                "(latest_chapter_at IS NULL), latest_chapter_at {direction}, l.title_id {direction}"
            )
        }
        "added" => format!("l.added_at {direction}, l.title_id {direction}"),
        "title" => format!("t.name_lower {direction}, l.title_id {direction}"),
        "chapters" => format!("chapter_count {direction}, l.title_id {direction}"),
        "rating" => format!("(t.rating IS NULL), t.rating {direction}, l.title_id {direction}"),
        _ => {
            return Err(AppError::bad_request(
                "validation",
                "sort must be one of: updated, added, title, chapters, rating",
            ));
        }
    };

    Ok(primary)
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
