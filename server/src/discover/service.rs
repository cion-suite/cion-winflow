use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    auth::session::now_unix,
    catalog::selection,
    error::AppError,
    source_admin::service::{self as source_admin, ImportInput},
    source_auth::service as source_auth_service,
    source_discovery,
    sources::{
        Capability, ExternalId, SearchRequest, SourceError, SourceKind, TitleKind,
        text::normalize_name,
    },
    state::AppState,
};

const MAX_QUERY_CHARS: usize = 200;
const MAX_ITEMS_PER_SOURCE: usize = 12;

#[derive(Debug, Default, Deserialize)]
pub struct DiscoverSearchParams {
    pub q: String,
}

#[derive(Debug, Serialize)]
pub struct DiscoverItemDto {
    pub code: String,
    pub source_name: String,
    pub external_id: String,
    /// Тип из выдачи источника, если он его отдаёт.
    pub title_kind: Option<&'static str>,
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
    /// Код источника, у которого произошла ошибка.
    pub code: String,
    pub source_name: String,
    pub message: String,
    /// Машинный код ошибки API: UI переводит его, а не сырой текст.
    pub error_code: String,
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
    lang: &'static str,
    adapter: SourceKind,
}

/// Результаты одного источника вместе с его приоритетом: они нужны, чтобы
/// оставить один результат на тайтл — от самого приоритетного источника.
struct SearchBatch {
    source_name: String,
    code: String,
    source_id: i64,
    rank: i64,
    items: Vec<crate::sources::SourceSearchItem>,
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
            let Ok(adapter) = runtime.adapter() else {
                continue;
            };
            jobs.push(SearchJob {
                code: runtime.code.clone(),
                name: descriptor.name.to_owned(),
                source_id: runtime.id,
                lang: descriptor.lang,
                adapter: adapter.clone(),
            });
        }
        jobs
    };

    // Приоритет нужен и для порядка выдачи, и для дедупликации: одинаковый
    // тайтл показываем один раз — карточкой самого приоритетного источника.
    let mut priorities: HashMap<&'static str, HashMap<i64, i64>> = HashMap::new();
    for job in &jobs {
        if !priorities.contains_key(job.lang) {
            let ranks = selection::load_priorities(&state.pool, Some(job.lang))
                .await
                .unwrap_or_default();
            priorities.insert(job.lang, ranks);
        }
    }

    // Учётки резолвим до spawn: задача не должна трогать БД, а guard
    // реестра нельзя держать через сетевой `.await`.
    let mut prepared = Vec::with_capacity(jobs.len());
    let mut errors = Vec::new();
    for job in jobs {
        match source_auth_service::resolve(state, user_id, job.source_id).await {
            Ok(auth) => prepared.push((job, auth)),
            Err(error) => errors.push(DiscoverSourceError {
                code: job.code.clone(),
                source_name: job.name,
                message: error.short_message(),
                error_code: error.code().to_owned(),
            }),
        }
    }

    let query = query.to_owned();
    let mut handles = Vec::with_capacity(prepared.len());
    for (job, auth) in prepared {
        let rank = selection::rank_of(&priorities[job.lang], job.source_id);
        let query = query.clone();
        let code = job.code;
        let name = job.name;
        let source_id = job.source_id;
        let auth_present = auth.is_some();
        let handle = tokio::spawn(async move {
            job.adapter
                .search(
                    SearchRequest {
                        query,
                        cursor: None,
                        filters: Value::Null,
                    },
                    auth.as_ref(),
                )
                .await
        });
        handles.push((name, code, source_id, rank, auth_present, handle));
    }

    let mut batches = Vec::new();
    for (source_name, code, source_id, rank, auth_present, handle) in handles {
        match handle.await {
            Ok(Ok(page)) => batches.push(SearchBatch {
                source_name,
                code,
                source_id,
                rank,
                items: page.items,
            }),
            Ok(Err(error)) => {
                let mapped = error.into_app(auth_present);
                errors.push(DiscoverSourceError {
                    code,
                    source_name,
                    message: mapped.short_message(),
                    error_code: mapped.code().to_owned(),
                });
            }
            Err(error) => errors.push(DiscoverSourceError {
                code,
                source_name,
                message: format!("source request failed: {error}"),
                error_code: "internal_error".to_owned(),
            }),
        }
    }

    sort_batches_by_priority(&mut batches);
    retain_unique_titles(&mut batches);

    let mut entry_map: HashMap<(i64, String), i64> = HashMap::new();
    for batch in &batches {
        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT external_id, title_id FROM source_entries WHERE source_id = ?",
        )
        .bind(batch.source_id)
        .fetch_all(&state.pool)
        .await?;
        for (external_id, title_id) in rows {
            entry_map.insert((batch.source_id, external_id), title_id);
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
    for batch in batches {
        for item in batch.items.into_iter().take(MAX_ITEMS_PER_SOURCE) {
            let external_id = item.external_id.as_str().to_owned();
            let title_id = entry_map
                .get(&(batch.source_id, external_id.clone()))
                .copied();
            items.push(DiscoverItemDto {
                code: batch.code.clone(),
                source_name: batch.source_name.clone(),
                external_id,
                title_kind: item.kind.map(TitleKind::as_str),
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

/// Порядок источников — приоритет, затем код; порядок элементов внутри
/// источника остаётся релевантностью выдачи.
fn sort_batches_by_priority(batches: &mut [SearchBatch]) {
    batches.sort_by(|left, right| {
        left.rank
            .cmp(&right.rank)
            .then_with(|| left.code.cmp(&right.code))
    });
}

/// Оставляет один результат на нормализованное название: первый в порядке
/// приоритета источника. Пустые названия не объединяются.
fn retain_unique_titles(batches: &mut [SearchBatch]) {
    let mut seen: HashSet<String> = HashSet::new();
    for batch in batches.iter_mut() {
        batch.items.retain(|item| {
            let key = normalize_name(&item.name);
            key.is_empty() || seen.insert(key)
        });
    }
}

/// После добавления привязывает копии с остальных включённых источников.
/// Поиск идёт под общей блокировкой на тайтл: если открытие страницы уже
/// ищет, повторный запуск пропускается, иначе два запроса импортировали бы
/// один и тот же источник дважды. Ошибки отдельных источников не отменяют
/// добавление.
async fn link_missing_sources(state: &AppState, title_id: i64, user_id: i64) -> Vec<String> {
    let lock = state.title_lock(title_id);
    let outcome = match lock.try_lock() {
        Ok(_guard) => source_discovery::discover_for_title(state, title_id, user_id)
            .await
            .ok(),
        Err(_) => None,
    };
    state.release_title_lock(title_id);

    match outcome {
        Some(outcome) => {
            if !outcome.linked.is_empty() {
                tracing::info!(
                    title_id,
                    sources = ?outcome.linked,
                    "auto-linked missing sources"
                );
            }
            outcome.linked
        }
        None => Vec::new(),
    }
}

pub async fn add(
    state: &AppState,
    user_id: i64,
    input: DiscoverAddInput,
) -> Result<DiscoverAddResponse, AppError> {
    let runtime = { state.sources.read().await.runtime(&input.code) }
        .ok_or_else(|| AppError::not_found("source_not_found", "unknown source"))?;

    let descriptor = runtime.descriptor;
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

    // Учётка зрителя на весь импорт: иначе добавление 18+ тайтла сломается
    // даже у пользователя, который уже подключил аккаунт.
    let auth = source_auth_service::resolve(state, user_id, runtime.id).await?;

    let existing = sqlx::query_as::<_, (i64, i64)>(
        "SELECT id, title_id FROM source_entries \
         WHERE source_id = ? AND external_id = ?",
    )
    .bind(runtime.id)
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
                        match adapter.title(&external_id, auth.as_ref()).await {
                            Ok(source_title) => {
                                crate::catalog::cover::import_source_covers(
                                    state,
                                    title_id,
                                    runtime.id,
                                    &adapter,
                                    &source_title.covers,
                                    auth.as_ref(),
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
                    title_id: None,
                    auto_link: true,
                },
                auth.as_ref(),
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
        "INSERT INTO library (user_id, title_id, status, score, added_at, updated_at) \
         VALUES (?, ?, 'planned', NULL, ?, ?) \
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

    // Добавление одного источника автоматически привязывает тот же тайтл
    // с остальных включённых источников: пользователь не повторяет это
    // вручную для каждого произведения.
    link_missing_sources(state, title_id, user_id).await;

    Ok(DiscoverAddResponse {
        title_id,
        title_created,
        imported_chapters,
        library_created: inserted.rows_affected() == 1,
        status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::SourceSearchItem;

    fn item(external_id: &str, name: &str, alt_names: &[&str]) -> SourceSearchItem {
        SourceSearchItem {
            external_id: ExternalId::new(external_id).expect("external id"),
            kind: None,
            name: name.to_owned(),
            alt_names: alt_names.iter().map(|name| (*name).to_owned()).collect(),
            cover_ref: None,
            year: None,
            status: None,
            external_url: None,
        }
    }

    fn batch(code: &str, rank: i64, items: Vec<SourceSearchItem>) -> SearchBatch {
        SearchBatch {
            source_name: code.to_owned(),
            code: code.to_owned(),
            source_id: i64::from(items.len() as u32),
            rank,
            items,
        }
    }

    #[test]
    fn search_dedupes_same_title_by_source_priority() {
        let mut batches = vec![
            batch(
                "mangamir",
                1,
                vec![
                    item("m1", "Во Всеоружии", &[]),
                    item("m2", "Третий тайтл", &[]),
                ],
            ),
            batch("remanga", 0, vec![item("r1", "во всеоружии", &[])]),
        ];

        sort_batches_by_priority(&mut batches);
        retain_unique_titles(&mut batches);

        assert_eq!(batches[0].code, "remanga");
        assert_eq!(batches[0].items.len(), 1);
        assert_eq!(batches[1].items.len(), 1);
        assert_eq!(batches[1].items[0].external_id.as_str(), "m2");
    }
}
