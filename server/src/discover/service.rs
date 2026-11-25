use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    auth::session::now_unix,
    error::AppError,
    source_admin::service::{self as source_admin, ImportInput},
    sources::{Capability, ExternalId, MediaKind, SearchRequest, SourceError, SourceKind},
    state::AppState,
};

const MAX_QUERY_CHARS: usize = 200;
const MAX_ITEMS_PER_SOURCE: usize = 12;

#[derive(Debug, Default, Deserialize)]
pub struct DiscoverSearchParams {
    pub q: String,
    pub kind: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DiscoverItemDto {
    pub code: String,
    pub source_name: String,
    pub external_id: String,
    pub kind: &'static str,
    pub name: String,
    pub alt_names: Vec<String>,
    pub cover_ref: Option<String>,
    pub year: Option<i16>,
    pub status: Option<String>,
    /// Тайтл уже импортирован на сервер: повторный импорт не нужен.
    pub title_id: Option<i64>,
    pub in_library: bool,
}

#[derive(Debug, Serialize)]
pub struct DiscoverSourceError {
    pub code: String,
    pub source_name: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct DiscoverSearchResponse {
    pub items: Vec<DiscoverItemDto>,
    /// Ошибка одного источника не ломает выдачу остальных.
    pub errors: Vec<DiscoverSourceError>,
}

#[derive(Debug, Deserialize)]
pub struct DiscoverAddInput {
    pub code: String,
    pub external_id: String,
    pub kind: String,
}

#[derive(Debug, Serialize)]
pub struct DiscoverAddResponse {
    pub title_id: i64,
    pub title_created: bool,
    pub imported_chapters: i64,
    pub library_created: bool,
    pub status: String,
}

struct SearchJob {
    code: String,
    name: String,
    source_id: i64,
    adapter: SourceKind,
    kind: MediaKind,
}

pub async fn search(
    state: &AppState,
    user_id: i64,
    params: DiscoverSearchParams,
) -> Result<DiscoverSearchResponse, AppError> {
    let query = params.q.trim();
    if query.is_empty() {
        return Err(AppError::bad_request(
            "validation",
            "query must not be empty",
        ));
    }
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(AppError::bad_request(
            "validation",
            format!("query must be at most {MAX_QUERY_CHARS} characters"),
        ));
    }
    let requested_kind = match params.kind.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(raw) => Some(MediaKind::from_db(&raw.to_lowercase()).ok_or_else(|| {
            AppError::bad_request("validation", "kind must be one of: manga, anime, book")
        })?),
    };

    // Guard реестра не держим через сетевые .await: забираем снимок задач.
    let jobs = {
        let registry = state.sources.read().await;
        let mut jobs = Vec::new();
        for runtime in registry.entries() {
            if !runtime.enabled {
                continue;
            }
            let descriptor = runtime.descriptor;
            if !descriptor.has(Capability::Search) || !descriptor.has(Capability::Chapters) {
                continue;
            }
            let kind = match requested_kind {
                Some(kind) if descriptor.supports(kind) => kind,
                Some(_) => continue,
                None => descriptor.default_kind,
            };
            let Ok(adapter) = runtime.adapter() else {
                continue;
            };
            jobs.push(SearchJob {
                code: runtime.code.clone(),
                name: descriptor.name.to_owned(),
                source_id: runtime.id,
                adapter: adapter.clone(),
                kind,
            });
        }
        jobs
    };

    let query = query.to_owned();
    let mut handles = Vec::with_capacity(jobs.len());
    for job in jobs {
        let query = query.clone();
        let code = job.code.clone();
        let name = job.name.clone();
        let source_id = job.source_id;
        let kind = job.kind;
        let handle = tokio::spawn(async move {
            job.adapter
                .search(SearchRequest {
                    query,
                    kind,
                    cursor: None,
                    filters: Value::Null,
                })
                .await
        });
        handles.push((code, name, source_id, kind, handle));
    }

    let mut batches: Vec<(String, String, i64, Vec<crate::sources::SourceSearchItem>)> = Vec::new();
    let mut errors = Vec::new();
    for (code, name, source_id, _kind, handle) in handles {
        match handle.await {
            Ok(Ok(page)) => batches.push((name, code, source_id, page.items)),
            Ok(Err(error)) => errors.push(DiscoverSourceError {
                code,
                source_name: name,
                message: short_error(&error),
            }),
            Err(error) => errors.push(DiscoverSourceError {
                code,
                source_name: name,
                message: format!("source request failed: {error}"),
            }),
        }
    }

    // Порядок источников стабилен, порядок элементов внутри — релевантность.
    batches.sort_by(|left, right| left.0.cmp(&right.0));

    let mut entry_map: HashMap<(i64, String, String), i64> = HashMap::new();
    for (_, _, source_id, _) in &batches {
        let rows = sqlx::query_as::<_, (String, String, i64)>(
            "SELECT external_id, kind, title_id FROM source_entries WHERE source_id = ?",
        )
        .bind(source_id)
        .fetch_all(&state.pool)
        .await?;
        for (external_id, kind, title_id) in rows {
            entry_map.insert((*source_id, kind, external_id), title_id);
        }
    }

    let library_ids: HashSet<i64> =
        sqlx::query_scalar::<_, i64>("SELECT title_id FROM library WHERE user_id = ?")
            .bind(user_id)
            .fetch_all(&state.pool)
            .await?
            .into_iter()
            .collect();

    let mut items = Vec::new();
    for (source_name, code, source_id, source_items) in batches {
        for item in source_items.into_iter().take(MAX_ITEMS_PER_SOURCE) {
            let kind = item.kind.as_str().to_owned();
            let title_id = entry_map
                .get(&(source_id, kind, item.external_id.as_str().to_owned()))
                .copied();
            items.push(DiscoverItemDto {
                code: code.clone(),
                source_name: source_name.clone(),
                external_id: item.external_id.as_str().to_owned(),
                kind: item.kind.as_str(),
                name: item.name,
                alt_names: item.alt_names,
                cover_ref: item.cover_ref,
                year: item.year,
                status: item.status.map(|status| status.as_str().to_owned()),
                title_id,
                in_library: title_id.is_some_and(|id| library_ids.contains(&id)),
            });
        }
    }

    Ok(DiscoverSearchResponse { items, errors })
}

pub async fn add(
    state: &AppState,
    user_id: i64,
    input: DiscoverAddInput,
) -> Result<DiscoverAddResponse, AppError> {
    let runtime = { state.sources.read().await.runtime(&input.code) }
        .ok_or_else(|| AppError::not_found("source_not_found", "unknown source"))?;

    let descriptor = runtime.descriptor;
    let kind = MediaKind::from_db(&input.kind.trim().to_lowercase()).ok_or_else(|| {
        AppError::bad_request("validation", "kind must be one of: manga, anime, book")
    })?;
    if !descriptor.supports(kind) {
        return Err(SourceError::UnsupportedKind(kind.as_str().to_owned()).into());
    }
    if !descriptor.has(Capability::Chapters) {
        return Err(AppError::bad_request(
            "source_bad_request",
            "source does not support chapters",
        ));
    }
    // add() не ходит в сеть для уже импортированного тайтла, но выключенный
    // источник не должен оставаться путём к контенту.
    if !runtime.enabled {
        return Err(SourceError::Disabled.into());
    }

    let external_id = ExternalId::new(input.external_id)?;

    let existing = sqlx::query_as::<_, (i64, i64)>(
        "SELECT id, title_id FROM source_entries \
         WHERE source_id = ? AND kind = ? AND external_id = ?",
    )
    .bind(runtime.id)
    .bind(kind.as_str())
    .bind(external_id.as_str())
    .fetch_optional(&state.pool)
    .await?;

    let (title_id, title_created, imported_chapters) = match existing {
        Some((_, title_id)) => {
            // Импорт до появления автоскачивания обложек не сохранил их:
            // доберём при добавлении, иначе карточка останется с заглушкой.
            let has_cover = sqlx::query_scalar::<_, i64>(
                "SELECT 1 FROM title_covers WHERE title_id = ? LIMIT 1",
            )
            .bind(title_id)
            .fetch_optional(&state.pool)
            .await?
            .is_some();
            if !has_cover {
                match runtime.adapter() {
                    Ok(adapter) => {
                        let adapter = adapter.clone();
                        match adapter.title(kind, &external_id).await {
                            Ok(source_title) => {
                                crate::catalog::cover::import_source_covers(
                                    state,
                                    title_id,
                                    runtime.id,
                                    &adapter,
                                    &source_title.covers,
                                )
                                .await;
                            }
                            Err(error) => {
                                tracing::warn!(title_id, %error, "cannot backfill covers");
                            }
                        }
                    }
                    Err(error) => {
                        tracing::warn!(title_id, %error, "source adapter unavailable");
                    }
                }
            }
            (title_id, false, 0)
        }
        None => {
            let response = source_admin::import(
                state,
                &runtime.code,
                ImportInput {
                    external_id: external_id.as_str().to_owned(),
                    kind: kind.as_str().to_owned(),
                    title_id: None,
                },
            )
            .await?;
            (
                response.title_id,
                response.title_created,
                response.imported_chapters,
            )
        }
    };

    let now = now_unix();
    let inserted = sqlx::query(
        "INSERT INTO library (user_id, title_id, status, score, favorite, added_at, updated_at) \
         VALUES (?, ?, 'planned', NULL, 0, ?, ?) \
         ON CONFLICT (user_id, title_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(title_id)
    .bind(now)
    .bind(now)
    .execute(&state.pool)
    .await?;

    let status = sqlx::query_scalar::<_, String>(
        "SELECT status FROM library WHERE user_id = ? AND title_id = ?",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_one(&state.pool)
    .await?;

    Ok(DiscoverAddResponse {
        title_id,
        title_created,
        imported_chapters,
        library_created: inserted.rows_affected() == 1,
        status,
    })
}

fn short_error(error: &SourceError) -> String {
    error.to_string().chars().take(200).collect()
}
