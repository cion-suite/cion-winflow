use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{
    auth::session::now_unix,
    catalog::{
        chapters,
        links::{self, AliasDto},
        selection,
    },
    error::AppError,
    pagination,
    sources::{CoverSize, TitleKind},
    state::AppState,
};

pub const TITLE_STATUSES: [&str; 4] = ["ongoing", "completed", "hiatus", "cancelled"];

const MAX_NAME_CHARS: usize = 300;
const MAX_LIST_ITEMS: usize = 50;
const MAX_LIST_ITEM_CHARS: usize = 200;
const MAX_DESCRIPTION_CHARS: usize = 20_000;
const MAX_QUERY_CHARS: usize = 200;

/// Полная замена тайтла: PUT присылает все поля, отсутствующие списки
/// и опциональные поля означают «пусто».
#[derive(Debug, Deserialize)]
pub struct TitleInput {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub alt_names: Vec<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub year: Option<i64>,
    pub lang: Option<String>,
    pub rating: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CatalogParams {
    pub q: Option<String>,
    pub kind: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TitleDto {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub alt_names: Vec<String>,
    pub authors: Vec<String>,
    pub genres: Vec<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub year: Option<i64>,
    pub lang: Option<String>,
    pub rating: Option<f64>,
    pub has_cover: bool,
    /// Какие размеры обложки сохранены.
    pub cover_sizes: Vec<String>,
    /// Размеры, загруженные вручную: `title_covers.source_id IS NULL`.
    /// Строки миграции 0005 тоже считаются ручными.
    pub manual_cover_sizes: Vec<String>,
    pub origin_source_id: Option<i64>,
    pub preferred_source_id: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize)]
pub struct TitleSummaryDto {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub year: Option<i64>,
    pub status: Option<String>,
    pub rating: Option<f64>,
    pub has_cover: bool,
    /// Меняется при загрузке обложки: клиент использует как cache-buster.
    pub updated_at: i64,
}

#[derive(Debug, Serialize)]
pub struct LibraryState {
    pub status: String,
    pub score: Option<i64>,
    /// Своя закладка как статус: перекрывает встроенный `status`.
    pub custom_tab_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TitleResponse {
    pub title: TitleDto,
    pub library: Option<LibraryState>,
    /// Только БД: сетевых запросов к источникам здесь нет.
    pub chapters: Vec<chapters::TitleChapterDto>,
    pub sources: Vec<chapters::TitleSourceDto>,
    /// Ручные связи ключей глав: alias -> canonical.
    pub aliases: Vec<AliasDto>,
    /// Прочитанные главы текущего пользователя по каноническому ключу.
    pub read_keys: Vec<String>,
    /// Язык, которым реально отфильтрованы главы; `null` — фильтра нет.
    pub view_lang: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CatalogPage {
    pub items: Vec<TitleSummaryDto>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct TitleRow {
    id: i64,
    kind: String,
    name: String,
    alt_names_json: String,
    authors_json: String,
    genres_json: String,
    description: Option<String>,
    status: Option<String>,
    year: Option<i64>,
    lang: Option<String>,
    rating: Option<f64>,
    cover_sizes: String,
    manual_cover_sizes: String,
    origin_source_id: Option<i64>,
    preferred_source_id: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

impl From<TitleRow> for TitleDto {
    fn from(row: TitleRow) -> Self {
        let cover_sizes = decode_cover_sizes(&row.cover_sizes);
        let manual_cover_sizes = decode_cover_sizes(&row.manual_cover_sizes);
        Self {
            id: row.id,
            kind: row.kind,
            name: row.name,
            alt_names: decode_list(&row.alt_names_json, "alt_names"),
            authors: decode_list(&row.authors_json, "authors"),
            genres: decode_list(&row.genres_json, "genres"),
            description: row.description,
            status: row.status,
            year: row.year,
            lang: row.lang,
            rating: row.rating,
            has_cover: !cover_sizes.is_empty(),
            cover_sizes,
            manual_cover_sizes,
            origin_source_id: row.origin_source_id,
            preferred_source_id: row.preferred_source_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct TitleSummaryRow {
    id: i64,
    kind: String,
    name: String,
    year: Option<i64>,
    status: Option<String>,
    rating: Option<f64>,
    has_cover: bool,
    updated_at: i64,
}

impl From<TitleSummaryRow> for TitleSummaryDto {
    fn from(row: TitleSummaryRow) -> Self {
        Self {
            id: row.id,
            kind: row.kind,
            name: row.name,
            year: row.year,
            status: row.status,
            rating: row.rating,
            has_cover: row.has_cover,
            updated_at: row.updated_at,
        }
    }
}

pub async fn list_catalog(
    pool: &SqlitePool,
    params: CatalogParams,
) -> Result<CatalogPage, AppError> {
    let (page, page_size) = pagination::resolve(params.page, params.page_size)?;

    let kind = match params.kind.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(kind) => Some(validate_kind(kind)?),
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

    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM titles \
         WHERE (? IS NULL OR kind = ?) \
           AND (? IS NULL OR name_lower LIKE ? ESCAPE '\\')",
    )
    .bind(kind.as_deref())
    .bind(kind.as_deref())
    .bind(pattern.as_deref())
    .bind(pattern.as_deref())
    .fetch_one(pool)
    .await?;

    let offset = pagination::offset(page, page_size);
    let rows = sqlx::query_as::<_, TitleSummaryRow>(
        "SELECT id, kind, name, year, status, rating, \
                EXISTS (SELECT 1 FROM title_covers c WHERE c.title_id = titles.id) AS has_cover, \
                updated_at \
         FROM titles \
         WHERE (? IS NULL OR kind = ?) \
           AND (? IS NULL OR name_lower LIKE ? ESCAPE '\\') \
         ORDER BY name_lower, id \
         LIMIT ? OFFSET ?",
    )
    .bind(kind.as_deref())
    .bind(kind.as_deref())
    .bind(pattern.as_deref())
    .bind(pattern.as_deref())
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(CatalogPage {
        items: rows.into_iter().map(TitleSummaryDto::from).collect(),
        total,
        page,
        page_size,
    })
}

/// Язык просмотра из query: пробелы по краям не значимы, пустой — не задан.
pub fn normalize_lang(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase)
}

pub async fn get_title(
    state: &AppState,
    user_id: i64,
    title_id: i64,
    view_lang: Option<&str>,
) -> Result<TitleResponse, AppError> {
    let pool = &state.pool;
    let title = find_title(pool, title_id).await?;
    let library = find_library_state(pool, user_id, title_id).await?;
    let info = selection::TitleInfo {
        id: title.id,
        lang: title.lang.clone(),
        preferred_source_id: title.preferred_source_id,
    };
    // Язык нужен DTO источников, но guard реестра не должен жить до
    // следующего `.await` к БД.
    let source_langs: HashMap<String, &'static str> = {
        let registry = state.sources.read().await;
        registry
            .descriptors()
            .iter()
            .map(|descriptor| (descriptor.code.to_owned(), descriptor.lang))
            .collect()
    };
    let chapters = chapters::title_chapters(pool, &info, &source_langs, view_lang).await?;
    let aliases = links::load_aliases(pool, title_id).await?;
    let read_keys = sqlx::query_scalar::<_, String>(
        "SELECT chapter_key FROM chapter_reads WHERE user_id = ? AND title_id = ?",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_all(pool)
    .await?;

    Ok(TitleResponse {
        title: TitleDto::from(title),
        library,
        chapters: chapters.chapters,
        sources: chapters.sources,
        aliases,
        read_keys,
        view_lang: chapters.view_lang,
    })
}

pub async fn create_title(state: &AppState, input: TitleInput) -> Result<TitleDto, AppError> {
    let title = validate_title(input)?;
    let now = now_unix();

    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO titles (kind, name, name_lower, alt_names_json, authors_json, genres_json, \
                             description, status, year, lang, rating, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(&title.kind)
    .bind(&title.name)
    .bind(&title.name_lower)
    .bind(&title.alt_names_json)
    .bind(&title.authors_json)
    .bind(&title.genres_json)
    .bind(title.description.as_deref())
    .bind(title.status.as_deref())
    .bind(title.year)
    .bind(title.lang.as_deref())
    .bind(title.rating)
    .bind(now)
    .bind(now)
    .fetch_one(&state.pool)
    .await?;

    let row = find_title(&state.pool, id).await?;
    Ok(TitleDto::from(row))
}

pub async fn update_title(
    state: &AppState,
    title_id: i64,
    input: TitleInput,
) -> Result<TitleDto, AppError> {
    let current = find_title(&state.pool, title_id).await?;
    let title = validate_title(input)?;

    sqlx::query(
        "UPDATE titles SET kind = ?, name = ?, name_lower = ?, alt_names_json = ?, \
                           authors_json = ?, genres_json = ?, description = ?, status = ?, \
                           year = ?, lang = ?, rating = ?, \
                           updated_at = max(?, updated_at + 1) \
         WHERE id = ?",
    )
    .bind(&title.kind)
    .bind(&title.name)
    .bind(&title.name_lower)
    .bind(&title.alt_names_json)
    .bind(&title.authors_json)
    .bind(&title.genres_json)
    .bind(title.description.as_deref())
    .bind(title.status.as_deref())
    .bind(title.year)
    .bind(title.lang.as_deref())
    .bind(title.rating)
    .bind(now_unix())
    .bind(title_id)
    .execute(&state.pool)
    .await?;

    // lang задаёт набор source_priorities: после смены языка preferred
    // нужно выбрать заново, иначе останется решение старого набора.
    if title.lang != current.lang {
        selection::recompute_preferred(&state.pool, title_id).await?;
    }

    let row = find_title(&state.pool, title_id).await?;
    Ok(TitleDto::from(row))
}

async fn find_title(pool: &SqlitePool, title_id: i64) -> Result<TitleRow, AppError> {
    let row = sqlx::query_as::<_, TitleRow>(
        "SELECT id, kind, name, alt_names_json, authors_json, genres_json, description, status, \
                year, lang, rating, \
                COALESCE((SELECT group_concat(c.size, ',') FROM title_covers c \
                          WHERE c.title_id = titles.id), '') AS cover_sizes, \
                COALESCE((SELECT group_concat(c.size, ',') FROM title_covers c \
                          WHERE c.title_id = titles.id AND c.source_id IS NULL), '') \
                          AS manual_cover_sizes, \
                origin_source_id, preferred_source_id, created_at, updated_at \
         FROM titles WHERE id = ?",
    )
    .bind(title_id)
    .fetch_optional(pool)
    .await?;

    row.ok_or_else(|| AppError::not_found("title_not_found", "title not found"))
}

async fn find_library_state(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
) -> Result<Option<LibraryState>, AppError> {
    let row = sqlx::query_as::<_, (String, Option<i64>, Option<i64>)>(
        "SELECT status, score, custom_tab_id FROM library WHERE user_id = ? AND title_id = ?",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_optional(pool)
    .await?;

    let Some((status, score, custom_tab_id)) = row else {
        return Ok(None);
    };

    Ok(Some(LibraryState {
        status,
        score,
        custom_tab_id,
    }))
}

struct ValidatedTitle {
    kind: String,
    name: String,
    name_lower: String,
    alt_names_json: String,
    authors_json: String,
    genres_json: String,
    description: Option<String>,
    status: Option<String>,
    year: Option<i64>,
    lang: Option<String>,
    rating: Option<f64>,
}

fn validate_title(input: TitleInput) -> Result<ValidatedTitle, AppError> {
    let name = input.name.trim().to_owned();
    if name.is_empty() {
        return Err(validation("name must not be empty"));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(validation(format!(
            "name must be at most {MAX_NAME_CHARS} characters"
        )));
    }

    let kind = validate_kind(&input.kind)?;

    let alt_names = normalize_list("alt_names", input.alt_names)?;
    let authors = normalize_list("authors", input.authors)?;
    let genres = normalize_list("genres", input.genres)?;

    let description = input
        .description
        .map(|description| description.trim().to_owned())
        .filter(|description| !description.is_empty());
    if let Some(description) = &description
        && description.chars().count() > MAX_DESCRIPTION_CHARS
    {
        return Err(validation(format!(
            "description must be at most {MAX_DESCRIPTION_CHARS} characters"
        )));
    }

    let status = match input.status.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(status) => {
            let status = status.to_lowercase();
            if !TITLE_STATUSES.contains(&status.as_str()) {
                return Err(validation(format!(
                    "status must be one of: {}",
                    TITLE_STATUSES.join(", ")
                )));
            }
            Some(status)
        }
    };

    if let Some(year) = input.year
        && !(1800..=2200).contains(&year)
    {
        return Err(validation("year must be between 1800 and 2200"));
    }

    let lang = match input.lang.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(lang) => {
            let lang = lang.to_lowercase();
            let valid = (2..=8).contains(&lang.chars().count())
                && lang
                    .chars()
                    .all(|character| character.is_ascii_alphabetic() || character == '-');
            if !valid {
                return Err(validation(
                    "lang must be 2 to 8 latin letters or hyphens, for example `ru`",
                ));
            }
            Some(lang)
        }
    };

    if let Some(rating) = input.rating
        && !(0.0..=10.0).contains(&rating)
    {
        return Err(validation("rating must be between 0 and 10"));
    }

    Ok(ValidatedTitle {
        kind,
        name_lower: name.to_lowercase(),
        name,
        alt_names_json: json_list(&alt_names),
        authors_json: json_list(&authors),
        genres_json: json_list(&genres),
        description,
        status,
        year: input.year,
        lang,
        rating: input.rating,
    })
}

fn validate_kind(kind: &str) -> Result<String, AppError> {
    let kind = kind.trim().to_lowercase();
    if TitleKind::from_db(&kind).is_some() {
        Ok(kind)
    } else {
        let allowed = TitleKind::ALL.map(TitleKind::as_str).join(", ");
        Err(validation(format!("kind must be one of: {allowed}")))
    }
}

fn normalize_list(field: &str, values: Vec<String>) -> Result<Vec<String>, AppError> {
    if values.len() > MAX_LIST_ITEMS {
        return Err(validation(format!(
            "{field} may contain at most {MAX_LIST_ITEMS} entries"
        )));
    }

    let mut normalized = Vec::with_capacity(values.len());
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if value.chars().count() > MAX_LIST_ITEM_CHARS {
            return Err(validation(format!(
                "each {field} entry must be at most {MAX_LIST_ITEM_CHARS} characters"
            )));
        }
        normalized.push(value.to_owned());
    }
    Ok(normalized)
}

fn json_list(values: &[String]) -> String {
    // Value::to_string не возвращает Result: ветки ошибки для Vec<String> нет
    serde_json::Value::from(values.to_vec()).to_string()
}

fn decode_cover_sizes(raw: &str) -> Vec<String> {
    let mut sizes: Vec<CoverSize> = raw
        .split(',')
        .filter_map(|value| CoverSize::from_db(value.trim()))
        .collect();
    sizes.sort_unstable();
    sizes.dedup();
    sizes
        .into_iter()
        .map(|size| size.as_str().to_owned())
        .collect()
}

fn decode_list(raw: &str, field: &'static str) -> Vec<String> {
    match serde_json::from_str(raw) {
        Ok(values) => values,
        Err(error) => {
            tracing::warn!(%error, field, "malformed title JSON column; using empty list");
            Vec::new()
        }
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

fn validation(message: impl Into<String>) -> AppError {
    AppError::bad_request("validation", message)
}
