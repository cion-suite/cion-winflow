//! Приоритеты источников по языку: меньший `rank` — выше.
//! Источник не привязан к типу тайтла, поэтому набора `(kind, lang)` больше
//! нет. Список заменяется целиком, чтобы не было дыр и повторных rank.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{auth::session::now_unix, catalog::selection, error::AppError, state::AppState};

const MAX_SOURCES: usize = 50;

#[derive(Debug, Deserialize)]
pub struct PrioritiesQuery {
    pub lang: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PriorityDto {
    pub source_id: i64,
    pub code: String,
    pub name: String,
    pub enabled: bool,
    /// `None` — источник ещё не входит в список приоритетов.
    pub rank: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct PrioritiesResponse {
    pub lang: String,
    pub sources: Vec<PriorityDto>,
}

#[derive(Debug, Deserialize)]
pub struct PrioritiesInput {
    pub lang: String,
    pub source_codes: Vec<String>,
}

pub async fn list(
    state: &AppState,
    query: PrioritiesQuery,
) -> Result<PrioritiesResponse, AppError> {
    let lang = validate_lang(query.lang.as_deref().unwrap_or("*"))?;

    let rows = sqlx::query_as::<_, (i64, String, String, bool, Option<i64>)>(
        "SELECT s.id, s.code, s.name, (s.enabled = 1 AND s.base_url IS NOT NULL AND s.config_error IS NULL), p.rank \
         FROM sources s \
         LEFT JOIN source_priorities p \
             ON p.source_id = s.id AND p.lang = ? \
         ORDER BY (p.rank IS NULL), p.rank, s.name, s.id",
    )
    .bind(&lang)
    .fetch_all(&state.pool)
    .await?;

    // Строка в `sources` без скомпилированного адаптера не участвует
    // в выборе источника и не должна вводить в заблуждение в админке.
    let registry = state.sources.read().await;
    let sources = rows
        .into_iter()
        .filter(|(_, code, _, _, _)| registry.descriptor(code).is_some())
        .map(|(source_id, code, name, enabled, rank)| PriorityDto {
            source_id,
            code,
            name,
            enabled,
            rank,
        })
        .collect();
    drop(registry);

    Ok(PrioritiesResponse { lang, sources })
}

pub async fn replace(
    state: &AppState,
    input: PrioritiesInput,
) -> Result<PrioritiesResponse, AppError> {
    let lang = validate_lang(&input.lang)?;

    if input.source_codes.len() > MAX_SOURCES {
        return Err(AppError::bad_request(
            "validation",
            format!("at most {MAX_SOURCES} sources may be ranked"),
        ));
    }

    let mut codes = Vec::with_capacity(input.source_codes.len());
    let mut seen = HashSet::new();
    for raw in &input.source_codes {
        let code = raw.trim();
        if code.is_empty() {
            return Err(AppError::bad_request(
                "validation",
                "source code must not be empty",
            ));
        }
        if !seen.insert(code.to_owned()) {
            return Err(AppError::bad_request(
                "validation",
                format!("source `{code}` is listed twice"),
            ));
        }
        codes.push(code.to_owned());
    }

    let registered = sqlx::query_as::<_, (i64, String)>("SELECT id, code FROM sources")
        .fetch_all(&state.pool)
        .await?;
    let by_code: std::collections::HashMap<String, i64> = registered
        .into_iter()
        .map(|(id, code)| (code, id))
        .collect();

    {
        let registry = state.sources.read().await;
        for code in &codes {
            if registry.descriptor(code).is_none() {
                return Err(AppError::bad_request(
                    "validation",
                    format!("source `{code}` has no compiled adapter"),
                ));
            }
        }
    }

    let mut source_ids = Vec::with_capacity(codes.len());
    for code in &codes {
        let Some(source_id) = by_code.get(code).copied() else {
            return Err(AppError::bad_request(
                "validation",
                format!("source `{code}` is not registered; configure it first"),
            ));
        };
        source_ids.push(source_id);
    }

    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM source_priorities WHERE lang = ?")
        .bind(&lang)
        .execute(&mut *tx)
        .await?;

    let created_at = now_unix();
    for (rank, source_id) in source_ids.into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO source_priorities (lang, source_id, rank, created_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(&lang)
        .bind(source_id)
        .bind(i64::try_from(rank).map_err(|_| AppError::Internal)?)
        .bind(created_at)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    // Приоритет влияет только на ничью по полноте, но preferred мог
    // измениться у любого тайтла: язык тайтла задаёт набор.
    selection::recompute_all_titles(&state.pool).await?;

    list(state, PrioritiesQuery { lang: Some(lang) }).await
}

fn validate_lang(raw: &str) -> Result<String, AppError> {
    let lang = raw.trim().to_lowercase();
    let valid = lang == "*"
        || ((2..=8).contains(&lang.chars().count())
            && lang
                .chars()
                .all(|character| character.is_ascii_alphabetic() || character == '-'));
    if !valid {
        return Err(AppError::bad_request(
            "validation",
            "lang must be `*` or 2 to 8 latin letters or hyphens",
        ));
    }
    Ok(lang)
}
