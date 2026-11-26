//! Выбор источника: приоритеты, статистика полноты и fallback по главе.
//! Решение 0003; страница тайтла и API читают только эти метаданные.

use std::cmp::Reverse;
use std::collections::HashMap;

use sqlx::SqlitePool;

use crate::auth::session::now_unix;
use crate::error::AppError;

pub const LOCKED_TTL_SECS: i64 = 6 * 60 * 60;
pub const MISSING_TTL_SECS: i64 = 24 * 60 * 60;

#[derive(Debug, sqlx::FromRow)]
pub struct TitleInfo {
    pub id: i64,
    pub lang: Option<String>,
    pub preferred_source_id: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ChapterCandidate {
    pub chapter_id: i64,
    pub chapter_key: String,
    pub external_id: String,
    pub number_text: Option<String>,
    pub number_milli: Option<i64>,
    pub volume: Option<i64>,
    pub chapter_kind: String,
    pub content_kind: Option<String>,
    pub name: Option<String>,
    pub group_name: Option<String>,
    pub language: Option<String>,
    pub is_locked: bool,
    pub availability: String,
    pub availability_checked_at: Option<i64>,
    pub page_count: Option<i64>,
    pub ordinal_site: i64,
    pub external_url: Option<String>,
    pub embed_url: Option<String>,
    pub published_at: Option<i64>,
    pub source_entry_id: i64,
    pub source_id: i64,
    pub title_id: i64,
    pub is_primary: bool,
    pub source_code: String,
    pub source_name: String,
    pub source_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AvailabilityView {
    Available,
    Unknown,
    Unavailable,
}

#[derive(Debug, Clone, Copy)]
pub struct EntryStats {
    pub chapter_count: i64,
    pub available_count: i64,
    pub locked_count: i64,
    pub missing_count: i64,
}

pub async fn load_title_info(pool: &SqlitePool, title_id: i64) -> Result<TitleInfo, AppError> {
    sqlx::query_as::<_, TitleInfo>("SELECT id, lang, preferred_source_id FROM titles WHERE id = ?")
        .bind(title_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::not_found("title_not_found", "title not found"))
}

/// Alias важнее одноимённой главы: администратор явно связал ключи.
pub async fn resolve_key(
    pool: &SqlitePool,
    title_id: i64,
    key: &str,
) -> Result<Option<String>, AppError> {
    let alias = sqlx::query_scalar::<_, String>(
        "SELECT canonical_key FROM chapter_aliases WHERE title_id = ? AND alias_key = ?",
    )
    .bind(title_id)
    .bind(key)
    .fetch_optional(pool)
    .await?;

    if let Some(canonical) = alias {
        return Ok(Some(canonical));
    }

    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM chapters ch JOIN source_entries e ON e.id = ch.source_entry_id \
         WHERE e.title_id = ? AND ch.chapter_key = ? AND ch.removed_at IS NULL LIMIT 1",
    )
    .bind(title_id)
    .bind(key)
    .fetch_optional(pool)
    .await?;

    Ok(exists.map(|_| key.to_owned()))
}

pub async fn candidates(
    pool: &SqlitePool,
    title_id: i64,
    chapter_key: &str,
) -> Result<Vec<ChapterCandidate>, AppError> {
    // Alias-варианты обязаны участвовать в fallback: иначе связанная
    // вручную глава другого источника не попадёт в выборку и критерий
    // «locked-глава открывается из другого источника» не выполнится.
    let rows = sqlx::query_as::<_, ChapterCandidate>(
        "SELECT ch.id AS chapter_id, ch.chapter_key, ch.external_id, ch.number_text, \
                ch.number_milli, ch.volume, ch.chapter_kind, ch.content_kind, ch.name, \
                ch.group_name, ch.language, ch.is_locked, ch.availability, \
                ch.availability_checked_at, ch.page_count, ch.ordinal_site, ch.external_url, \
                ch.embed_url, ch.published_at, e.id AS source_entry_id, e.source_id, e.title_id, e.is_primary, \
                s.code AS source_code, s.name AS source_name, (s.enabled = 1 AND s.base_url IS NOT NULL AND s.config_error IS NULL) AS source_enabled \
         FROM chapters ch \
         JOIN source_entries e ON e.id = ch.source_entry_id \
         JOIN sources s ON s.id = e.source_id \
         WHERE e.title_id = ? AND ch.removed_at IS NULL \
           AND (ch.chapter_key = ? OR ch.chapter_key IN ( \
                 SELECT alias_key FROM chapter_aliases \
                 WHERE title_id = ? AND canonical_key = ? \
           ))",
    )
    .bind(title_id)
    .bind(chapter_key)
    .bind(title_id)
    .bind(chapter_key)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Свежий `locked` не перепроверяем: сеть дороже, а состояние меняется редко.
pub fn view(candidate: &ChapterCandidate, now: i64) -> AvailabilityView {
    match candidate.availability.as_str() {
        "available" => AvailabilityView::Available,
        "locked" if is_fresh(candidate.availability_checked_at, now, LOCKED_TTL_SECS) => {
            AvailabilityView::Unavailable
        }
        "missing" if is_fresh(candidate.availability_checked_at, now, MISSING_TTL_SECS) => {
            AvailabilityView::Unavailable
        }
        _ => AvailabilityView::Unknown,
    }
}

pub fn is_fresh(checked_at: Option<i64>, now: i64, ttl_secs: i64) -> bool {
    checked_at.is_some_and(|checked| now.saturating_sub(checked) < ttl_secs)
}

pub async fn load_priorities(
    pool: &SqlitePool,
    lang: Option<&str>,
) -> Result<HashMap<i64, i64>, AppError> {
    let language = lang.unwrap_or("*");

    // Набор языка тайтла полностью заменяет '*' — иначе rank'и из разных
    // наборов смешивались бы и '*' мог обогнать язык-специфичный источник.
    let rows = sqlx::query_as::<_, (i64, i64)>(
        "SELECT source_id, rank FROM source_priorities \
         WHERE lang = ? ORDER BY rank",
    )
    .bind(language)
    .fetch_all(pool)
    .await?;

    if !rows.is_empty() {
        return Ok(rows.into_iter().collect());
    }

    let fallback = sqlx::query_as::<_, (i64, i64)>(
        "SELECT source_id, rank FROM source_priorities \
         WHERE lang = '*' ORDER BY rank",
    )
    .fetch_all(pool)
    .await?;

    Ok(fallback.into_iter().collect())
}

pub fn rank_of(ranks: &HashMap<i64, i64>, source_id: i64) -> i64 {
    ranks.get(&source_id).copied().unwrap_or(i64::MAX)
}

/// Preferred source → доступность → приоритет → is_primary → ordinal_site → id.
pub fn order(
    mut list: Vec<ChapterCandidate>,
    preferred_source_id: Option<i64>,
    ranks: &HashMap<i64, i64>,
    now: i64,
) -> Vec<ChapterCandidate> {
    list.sort_by_key(|candidate| {
        (
            i32::from(!candidate.source_enabled),
            i32::from(preferred_source_id != Some(candidate.source_id)),
            view(candidate, now) as i32,
            rank_of(ranks, candidate.source_id),
            i32::from(!candidate.is_primary),
            candidate.ordinal_site,
            candidate.chapter_id,
        )
    });
    list
}

/// Вариант для отображения и проверки прогресса: рабочий источник и
/// доступность важнее preferred; при полном отсутствии — первый.
pub fn first_usable(list: &[ChapterCandidate], now: i64) -> Option<&ChapterCandidate> {
    list.iter()
        .find(|candidate| {
            candidate.source_enabled && view(candidate, now) != AvailabilityView::Unavailable
        })
        .or_else(|| list.iter().find(|candidate| candidate.source_enabled))
        .or_else(|| list.first())
}

pub async fn recompute_stats(
    pool: &SqlitePool,
    source_entry_id: i64,
) -> Result<EntryStats, AppError> {
    let (chapter_count, available_count, locked_count, missing_count) =
        sqlx::query_as::<_, (i64, i64, i64, i64)>(
            "SELECT COUNT(*), \
                    COALESCE(SUM(has_available), 0), \
                    COALESCE(SUM(CASE WHEN has_available = 0 AND has_locked = 1 THEN 1 ELSE 0 END), 0), \
                    COALESCE(SUM(CASE WHEN has_available = 0 AND has_locked = 0 THEN 1 ELSE 0 END), 0) \
             FROM ( \
                 SELECT chapter_key, \
                        MAX(CASE WHEN availability IN ('available', 'unknown') THEN 1 ELSE 0 END) AS has_available, \
                        MAX(CASE WHEN availability = 'locked' THEN 1 ELSE 0 END) AS has_locked \
                 FROM chapters WHERE source_entry_id = ? AND removed_at IS NULL \
                 GROUP BY chapter_key \
             )",
        )
        .bind(source_entry_id)
        .fetch_one(pool)
        .await?;

    let stats = EntryStats {
        chapter_count,
        available_count,
        locked_count,
        missing_count,
    };

    sqlx::query(
        "UPDATE source_entries SET chapter_count = ?, available_count = ?, locked_count = ?, \
                missing_count = ?, stats_updated_at = ? WHERE id = ?",
    )
    .bind(stats.chapter_count)
    .bind(stats.available_count)
    .bind(stats.locked_count)
    .bind(stats.missing_count)
    .bind(now_unix())
    .bind(source_entry_id)
    .execute(pool)
    .await?;

    Ok(stats)
}

/// Пересчитывает preferred у всех тайтлов, где участвует источник.
/// Нужно после включения/выключения источника и смены его доступности.
pub async fn recompute_titles_for_source(
    pool: &SqlitePool,
    source_id: i64,
) -> Result<(), AppError> {
    let title_ids = sqlx::query_scalar::<_, i64>(
        "SELECT DISTINCT title_id FROM source_entries WHERE source_id = ?",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await?;

    for title_id in title_ids {
        recompute_preferred(pool, title_id).await?;
    }
    Ok(())
}

/// Пересчитывает preferred у всех тайтлов. Приоритеты задаются на язык,
/// а язык тайтла мог измениться, поэтому после правки списка проще пройтись
/// по всем: на self-host базе это дешёвая операция.
pub async fn recompute_all_titles(pool: &SqlitePool) -> Result<(), AppError> {
    let title_ids = sqlx::query_scalar::<_, i64>("SELECT id FROM titles")
        .fetch_all(pool)
        .await?;

    for title_id in title_ids {
        recompute_preferred(pool, title_id).await?;
    }
    Ok(())
}

/// Максимальный `chapter_count`; ничья — приоритет, `is_primary`, id.
/// Источник без доступных глав не выбирается, пока есть доступные.
pub async fn recompute_preferred(
    pool: &SqlitePool,
    title_id: i64,
) -> Result<Option<i64>, AppError> {
    let title = load_title_info(pool, title_id).await?;
    let ranks = load_priorities(pool, title.lang.as_deref()).await?;

    let entries = sqlx::query_as::<_, (i64, i64, i64, bool)>(
        "SELECT e.source_id, e.chapter_count, e.available_count, e.is_primary \
         FROM source_entries e JOIN sources s ON s.id = e.source_id \
         WHERE e.title_id = ? AND s.enabled = 1 AND s.base_url IS NOT NULL \
           AND s.config_error IS NULL",
    )
    .bind(title_id)
    .fetch_all(pool)
    .await?;

    let has_available = entries.iter().any(|(_, _, available, _)| *available > 0);
    let mut eligible: Vec<_> = entries
        .into_iter()
        .filter(|(_, _, available, _)| !has_available || *available > 0)
        .collect();

    eligible.sort_by_key(|(source_id, chapter_count, _, is_primary)| {
        (
            Reverse(*chapter_count),
            rank_of(&ranks, *source_id),
            i32::from(!*is_primary),
            *source_id,
        )
    });

    let preferred = eligible.first().map(|(source_id, ..)| *source_id);

    sqlx::query(
        "UPDATE titles SET preferred_source_id = ?, preferred_source_updated_at = ? WHERE id = ?",
    )
    .bind(preferred)
    .bind(now_unix())
    .bind(title_id)
    .execute(pool)
    .await?;

    Ok(preferred)
}
