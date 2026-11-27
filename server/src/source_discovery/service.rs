//! Ленивый поиск тайтла на ещё не привязанных источниках.
//!
//! Логика выбора кандидатов и сопоставления названий переиспользуется
//! [`crate::discover::service::add`]: добавление тайтла с одного источника
//! сразу привязывает копии с остальных. При открытии страницы тот же поиск
//! выполняется под общей блокировкой на тайтл, с per-user кэшем
//! (`title_source_checks`), бюджетом источников и общим таймаутом.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tokio::time::timeout;

use crate::{
    auth::session::now_unix,
    catalog::selection,
    error::AppError,
    source_admin::service::{self as source_admin, ImportInput},
    source_auth::{SourceAuth, service as source_auth_service},
    sources::{
        Capability, ExternalId, SearchPage, SearchRequest, SourceError, SourceKind,
        text::normalize_name,
    },
    state::AppState,
};

const MAX_QUERY_CHARS: usize = 200;
/// Сколько альтернативных названий пробовать в поиске одного источника.
const MAX_ALT_NAME_QUERIES: usize = 5;
/// Сколько источников проверяем за одно открытие: поиск ходит в чужую сеть.
const MAX_SOURCES_PER_RUN: usize = 3;
/// Негативный результат живёт сутки, auth/error — час.
const NOT_FOUND_TTL_SECS: i64 = 24 * 60 * 60;
const ERROR_TTL_SECS: i64 = 60 * 60;
/// Страница уже отрисована, но запрос не должен висеть бесконечно.
const DISCOVERY_TIMEOUT_SECS: u64 = 20;

/// Итог для ответа `sync`: сколько источников проверили, сколько пропустили
/// по свежему кэшу или бюджету, и ошибки с машинным кодом для UI.
#[derive(Debug, Default, Serialize)]
pub struct DiscoveryReport {
    pub checked: usize,
    pub skipped: usize,
    pub errors: Vec<DiscoveryError>,
}

#[derive(Debug, Serialize)]
pub struct DiscoveryError {
    /// Код источника, на котором произошла ошибка.
    pub code: String,
    /// Машинный код ошибки API: UI переводит его, а не сырой текст.
    pub error_code: String,
    pub message: String,
}

/// Полный итог поиска. `linked` — источники, привязанные в этом вызове.
#[derive(Debug, Default)]
pub struct DiscoveryOutcome {
    pub linked: Vec<String>,
    pub report: DiscoveryReport,
    /// Таймаут: даже без ссылок sync обязан обновить главы.
    pub timed_out: bool,
    /// Auth-ошибка действующего пользователя: sync не двигает TTL.
    pub auth_error: bool,
}

struct TitleNames {
    name: String,
    alt_names: Vec<String>,
    lang: Option<String>,
}

struct Candidate {
    code: String,
    source_id: i64,
    adapter: SourceKind,
}

#[derive(Debug, sqlx::FromRow)]
struct CheckRow {
    source_id: i64,
    checked_at: i64,
    status: String,
}

/// Ищет тайтл на включённых источниках того же языка, которых нет в привязке,
/// и импортирует точные совпадения. Блокировку на тайтл держит вызывающий:
/// общий lock нужен `discover::add` и открытию страницы.
pub async fn discover_for_title(
    state: &AppState,
    title_id: i64,
    user_id: i64,
) -> Result<DiscoveryOutcome, AppError> {
    let Some(names) = title_names(state, title_id).await? else {
        return Ok(DiscoveryOutcome::default());
    };
    // Без языка тайтла правило «того же языка» не определено: не гадаем и не
    // ищем по всем источникам подряд.
    let Some(lang) = names.lang.as_deref() else {
        return Ok(DiscoveryOutcome::default());
    };

    let (candidates, overflow) = load_candidates(state, title_id, lang).await?;
    if candidates.is_empty() {
        return Ok(DiscoveryOutcome::default());
    }

    let discovery = discover_inner(state, title_id, user_id, &names, candidates, overflow);
    match timeout(Duration::from_secs(DISCOVERY_TIMEOUT_SECS), discovery).await {
        Ok(result) => result,
        Err(_) => {
            tracing::warn!(title_id, "source discovery timed out");
            Ok(DiscoveryOutcome {
                timed_out: true,
                ..DiscoveryOutcome::default()
            })
        }
    }
}

async fn discover_inner(
    state: &AppState,
    title_id: i64,
    user_id: i64,
    names: &TitleNames,
    candidates: Vec<Candidate>,
    overflow: usize,
) -> Result<DiscoveryOutcome, AppError> {
    let now = now_unix();
    let checks = load_checks(state, title_id, user_id).await?;
    let mut outcome = DiscoveryOutcome {
        report: DiscoveryReport {
            skipped: overflow,
            ..DiscoveryReport::default()
        },
        ..DiscoveryOutcome::default()
    };

    for candidate in candidates {
        if check_is_fresh(checks.get(&candidate.source_id), now) {
            outcome.report.skipped += 1;
            continue;
        }

        outcome.report.checked += 1;
        let auth = match source_auth_service::resolve(state, user_id, candidate.source_id).await {
            Ok(auth) => auth,
            Err(error) => {
                // Ошибка расшифровки — персональная проблема пользователя,
                // а не результат поиска: кэшируем как временную ошибку.
                record_check(
                    state,
                    title_id,
                    candidate.source_id,
                    user_id,
                    "error",
                    Some(&error.short_message()),
                )
                .await?;
                outcome.auth_error = true;
                push_error(&mut outcome.report.errors, &candidate.code, &error);
                continue;
            }
        };
        let auth_present = auth.is_some();

        match find_candidate(&candidate.adapter, names, auth.as_ref()).await {
            Ok(Some(external_id)) => {
                let imported = source_admin::import(
                    state,
                    &candidate.code,
                    ImportInput {
                        external_id: external_id.as_str().to_owned(),
                        title_id: Some(title_id),
                        auto_link: true,
                    },
                    auth.as_ref(),
                )
                .await;

                match imported {
                    Ok(response) => {
                        record_check(state, title_id, candidate.source_id, user_id, "found", None)
                            .await?;
                        tracing::info!(
                            title_id,
                            source = %candidate.code,
                            chapters = response.imported_chapters,
                            "discovery linked source by title name"
                        );
                        outcome.linked.push(candidate.code);
                    }
                    Err(error) => {
                        record_check(
                            state,
                            title_id,
                            candidate.source_id,
                            user_id,
                            "error",
                            Some(&error.short_message()),
                        )
                        .await?;
                        if requires_account(error.code()) {
                            outcome.auth_error = true;
                        }
                        push_error(&mut outcome.report.errors, &candidate.code, &error);
                    }
                }
            }
            Ok(None) => {
                record_check(
                    state,
                    title_id,
                    candidate.source_id,
                    user_id,
                    "not_found",
                    None,
                )
                .await?;
            }
            Err(error) => {
                // Без учётки 404 у Remanga неоднозначен (18+ тайтл скрыт или
                // отсутствует): это не окончательный `not_found` на сутки.
                let auth_required = matches!(
                    error,
                    SourceError::AuthRequired | SourceError::AuthMaybeRequired
                );
                let mapped = error.into_app(auth_present);
                let status = if auth_required {
                    outcome.auth_error = true;
                    "auth_required"
                } else {
                    "error"
                };
                record_check(
                    state,
                    title_id,
                    candidate.source_id,
                    user_id,
                    status,
                    Some(&mapped.short_message()),
                )
                .await?;
                push_error(&mut outcome.report.errors, &candidate.code, &mapped);
            }
        }
    }

    Ok(outcome)
}

/// Свежий результат поиска не повторяем. `found` не встречается в выборке
/// кандидатов (источник уже привязан), поэтому TTL для него не нужен.
fn check_is_fresh(check: Option<&CheckRow>, now: i64) -> bool {
    let Some(check) = check else {
        return false;
    };
    let ttl = match check.status.as_str() {
        "not_found" => NOT_FOUND_TTL_SECS,
        "auth_required" | "error" => ERROR_TTL_SECS,
        _ => return false,
    };
    selection::is_fresh(Some(check.checked_at), now, ttl)
}

async fn load_checks(
    state: &AppState,
    title_id: i64,
    user_id: i64,
) -> Result<HashMap<i64, CheckRow>, AppError> {
    let rows = sqlx::query_as::<_, CheckRow>(
        "SELECT source_id, checked_at, status FROM title_source_checks \
         WHERE title_id = ? AND user_id = ?",
    )
    .bind(title_id)
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows.into_iter().map(|row| (row.source_id, row)).collect())
}

async fn record_check(
    state: &AppState,
    title_id: i64,
    source_id: i64,
    user_id: i64,
    status: &str,
    error: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO title_source_checks (title_id, source_id, user_id, checked_at, status, error) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT (title_id, source_id, user_id) DO UPDATE SET \
             checked_at = excluded.checked_at, \
             status = excluded.status, \
             error = excluded.error",
    )
    .bind(title_id)
    .bind(source_id)
    .bind(user_id)
    .bind(now_unix())
    .bind(status)
    .bind(error)
    .execute(&state.pool)
    .await?;
    Ok(())
}

fn push_error(errors: &mut Vec<DiscoveryError>, code: &str, error: &AppError) {
    errors.push(DiscoveryError {
        code: code.to_owned(),
        error_code: error.code().to_owned(),
        message: error.short_message(),
    });
}

fn requires_account(code: &str) -> bool {
    matches!(
        code,
        "source_auth_not_configured" | "source_auth_invalid" | "source_auth_maybe_required"
    )
}

async fn title_names(state: &AppState, title_id: i64) -> Result<Option<TitleNames>, AppError> {
    let row = sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT name, alt_names_json, lang FROM titles WHERE id = ?",
    )
    .bind(title_id)
    .fetch_optional(&state.pool)
    .await?;

    Ok(row.map(|(name, alt_names_json, lang)| TitleNames {
        name,
        alt_names: serde_json::from_str(&alt_names_json).unwrap_or_default(),
        lang,
    }))
}

/// Источники, которые стоит проверить: включены, умеют искать и отдавать
/// главы, того же языка, ещё не привязаны. Порядок — приоритет источника.
async fn load_candidates(
    state: &AppState,
    title_id: i64,
    lang: &str,
) -> Result<(Vec<Candidate>, usize), AppError> {
    let attached: HashSet<i64> =
        sqlx::query_scalar::<_, i64>("SELECT source_id FROM source_entries WHERE title_id = ?")
            .bind(title_id)
            .fetch_all(&state.pool)
            .await?
            .into_iter()
            .collect();

    let mut candidates = {
        let registry = state.sources.read().await;
        let mut candidates = Vec::new();
        for runtime in registry.entries() {
            if !runtime.enabled || attached.contains(&runtime.id) {
                continue;
            }
            let descriptor = runtime.descriptor;
            if !descriptor.has(Capability::Search) || !descriptor.has(Capability::Chapters) {
                continue;
            }
            // Тайтл принадлежит одному языку: английская копия не должна
            // подмешиваться к русской карточке.
            if descriptor.lang != lang {
                continue;
            }
            let Ok(adapter) = runtime.adapter() else {
                continue;
            };
            candidates.push(Candidate {
                code: runtime.code.clone(),
                source_id: runtime.id,
                adapter: adapter.clone(),
            });
        }
        candidates
    };

    if candidates.is_empty() {
        return Ok((Vec::new(), 0));
    }

    let ranks = selection::load_priorities(&state.pool, Some(lang))
        .await
        .unwrap_or_default();
    candidates.sort_by(|left, right| {
        selection::rank_of(&ranks, left.source_id)
            .cmp(&selection::rank_of(&ranks, right.source_id))
            .then_with(|| left.code.cmp(&right.code))
    });

    let total = candidates.len();
    candidates.truncate(MAX_SOURCES_PER_RUN);
    let overflow = total.saturating_sub(candidates.len());
    Ok((candidates, overflow))
}

/// Сначала строгое совпадение по основному названию, затем до пяти запросов
/// по альтернативным. Ошибка поиска возвращается наверх: по ней discovery
/// отличает «нет тайтла» от «нужна учётка».
async fn find_candidate(
    adapter: &SourceKind,
    names: &TitleNames,
    auth: Option<&SourceAuth>,
) -> Result<Option<ExternalId>, SourceError> {
    let main_key = normalize_name(&names.name);
    if main_key.is_empty() {
        return Ok(None);
    }
    let mut wanted: HashSet<String> = names
        .alt_names
        .iter()
        .map(|name| normalize_name(name))
        .filter(|name| !name.is_empty())
        .collect();
    wanted.insert(main_key.clone());

    if names.name.chars().count() <= MAX_QUERY_CHARS {
        let page = search(adapter, names.name.trim(), auth).await?;
        if let Some(item) = page
            .items
            .into_iter()
            .find(|item| normalize_name(&item.name) == main_key)
        {
            return Ok(Some(item.external_id));
        }
    }

    let mut queries_left = MAX_ALT_NAME_QUERIES;
    let mut seen = HashSet::new();
    for alt_name in &names.alt_names {
        if queries_left == 0 {
            break;
        }
        let query = alt_name.trim();
        let key = normalize_name(query);
        if key.is_empty() || query.chars().count() > MAX_QUERY_CHARS || !seen.insert(key) {
            continue;
        }
        queries_left -= 1;
        let page = search(adapter, query, auth).await?;
        if let Some(item) = page
            .items
            .into_iter()
            .find(|item| item_matches_names(item, &wanted))
        {
            return Ok(Some(item.external_id));
        }
    }

    Ok(None)
}

async fn search(
    adapter: &SourceKind,
    query: &str,
    auth: Option<&SourceAuth>,
) -> Result<SearchPage, SourceError> {
    adapter
        .search(
            SearchRequest {
                query: query.to_owned(),
                cursor: None,
                filters: Value::Null,
            },
            auth,
        )
        .await
}

/// Основное или альтернативное имя кандидата совпадает с набором названий
/// искомого тайтла.
fn item_matches_names(item: &crate::sources::SourceSearchItem, wanted: &HashSet<String>) -> bool {
    wanted.contains(&normalize_name(&item.name))
        || item
            .alt_names
            .iter()
            .any(|alt_name| wanted.contains(&normalize_name(alt_name)))
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

    #[test]
    fn alt_name_matches_any_title_name() {
        let wanted: HashSet<String> = [
            "хроники демонического культа",
            "chronicles of the demon faction",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();

        assert!(item_matches_names(
            &item("a", "Chronicles of the Demon Faction", &[]),
            &wanted
        ));
        assert!(item_matches_names(
            &item("b", "Совсем другое", &["Хроники Демонического Культа"]),
            &wanted
        ));
        assert!(!item_matches_names(
            &item("c", "Совсем другое", &["Не то"]),
            &wanted
        ));
    }

    #[test]
    fn negative_results_expire_sooner_than_not_found() {
        let now = 100_000;
        let not_found = CheckRow {
            source_id: 1,
            checked_at: now - ERROR_TTL_SECS - 1,
            status: "not_found".to_owned(),
        };
        let error = CheckRow {
            source_id: 2,
            checked_at: now - ERROR_TTL_SECS - 1,
            status: "error".to_owned(),
        };

        // Час спустя `not_found` ещё свежий, а `error` — уже нет.
        assert!(check_is_fresh(Some(&not_found), now));
        assert!(!check_is_fresh(Some(&error), now));
    }
}
