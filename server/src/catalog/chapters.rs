//! Объединённый список глав для страницы тайтла: только БД, без сети.
//! Варианты группируются по каноническому `chapter_key`, alias → canonical.

use std::collections::HashMap;

use serde::Serialize;
use sqlx::SqlitePool;

use crate::auth::session::now_unix;
use crate::error::AppError;

use super::selection::{self, ChapterCandidate, TitleInfo};

#[derive(Debug, Serialize)]
pub struct TitleChapterVariantDto {
    pub chapter_id: i64,
    /// Собственный ключ варианта: у групп перевода он совпадает,
    /// у источников с разной нумерацией — отличается (кандидат в alias).
    pub chapter_key: String,
    pub source_id: i64,
    pub source_code: String,
    pub source_name: String,
    pub group_name: Option<String>,
    pub language: Option<String>,
    pub availability: String,
    pub is_locked: bool,
    pub content_kind: Option<String>,
    pub page_count: Option<i64>,
    /// Выключенный источник не может отдать главу, даже если она в списке.
    pub source_enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct TitleChapterDto {
    pub chapter_key: String,
    pub number_text: Option<String>,
    pub number_milli: Option<i64>,
    pub volume: Option<i64>,
    pub chapter_kind: String,
    pub name: Option<String>,
    pub content_kind: Option<String>,
    pub page_count: Option<i64>,
    pub availability: String,
    pub is_locked: bool,
    pub published_at: Option<i64>,
    pub selected_chapter_id: i64,
    pub variants: Vec<TitleChapterVariantDto>,
}

#[derive(Debug, Serialize)]
pub struct TitleSourceDto {
    pub source_id: i64,
    pub source_entry_id: i64,
    pub code: String,
    pub name: String,
    pub enabled: bool,
    pub is_primary: bool,
    pub chapter_count: i64,
    pub available_count: i64,
    pub locked_count: i64,
    pub missing_count: i64,
    pub stats_updated_at: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TitleChapters {
    pub chapters: Vec<TitleChapterDto>,
    pub sources: Vec<TitleSourceDto>,
}

#[derive(Debug, sqlx::FromRow)]
struct EntryRow {
    source_id: i64,
    source_name: String,
    source_code: String,
    source_enabled: bool,
    source_entry_id: i64,
    is_primary: bool,
    chapter_count: i64,
    available_count: i64,
    locked_count: i64,
    missing_count: i64,
    stats_updated_at: Option<i64>,
    last_error: Option<String>,
}

pub async fn title_chapters(
    pool: &SqlitePool,
    title: &TitleInfo,
) -> Result<TitleChapters, AppError> {
    let now = now_unix();
    let ranks = selection::load_priorities(pool, title.lang.as_deref()).await?;

    let aliases: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT alias_key, canonical_key FROM chapter_aliases WHERE title_id = ?",
    )
    .bind(title.id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();

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
         WHERE e.title_id = ? AND ch.removed_at IS NULL",
    )
    .bind(title.id)
    .fetch_all(pool)
    .await?;

    let mut groups: HashMap<String, Vec<ChapterCandidate>> = HashMap::new();
    for row in rows {
        let canonical = aliases
            .get(&row.chapter_key)
            .cloned()
            .unwrap_or_else(|| row.chapter_key.clone());
        groups.entry(canonical).or_default().push(row);
    }

    let mut chapters = Vec::with_capacity(groups.len());
    for (key, variants) in groups {
        let variants = selection::order(variants, title.preferred_source_id, &ranks, now);
        // Свежий locked/missing preferred-источник не должен становиться
        // «выбранным» для отображения, если есть доступный вариант.
        let Some(selected) = selection::first_usable(&variants, now) else {
            continue;
        };

        let availability = group_availability(&variants, now);
        let effective: Vec<&ChapterCandidate> = variants
            .iter()
            .filter(|variant| variant.source_enabled)
            .collect();
        let is_locked = if effective.is_empty() {
            variants.iter().all(|variant| variant.is_locked)
        } else {
            effective.iter().all(|variant| variant.is_locked)
        };

        chapters.push(TitleChapterDto {
            chapter_key: key,
            number_text: selected.number_text.clone(),
            number_milli: selected.number_milli,
            volume: selected.volume,
            chapter_kind: selected.chapter_kind.clone(),
            name: selected.name.clone(),
            content_kind: selected.content_kind.clone(),
            page_count: selected.page_count,
            availability,
            is_locked,
            published_at: selected.published_at,
            selected_chapter_id: selected.chapter_id,
            variants: variants
                .into_iter()
                .map(|variant| TitleChapterVariantDto {
                    chapter_id: variant.chapter_id,
                    chapter_key: variant.chapter_key,
                    source_id: variant.source_id,
                    source_code: variant.source_code,
                    source_name: variant.source_name,
                    group_name: variant.group_name,
                    language: variant.language,
                    availability: variant.availability,
                    is_locked: variant.is_locked,
                    content_kind: variant.content_kind,
                    page_count: variant.page_count,
                    source_enabled: variant.source_enabled,
                })
                .collect(),
        });
    }

    chapters.sort_by_key(|chapter| {
        (
            chapter.volume.unwrap_or(0),
            chapter.number_milli.unwrap_or(i64::MAX),
            chapter
                .variants
                .first()
                .map_or(0, |variant| variant.chapter_id),
        )
    });

    let entries = sqlx::query_as::<_, EntryRow>(
        "SELECT s.id AS source_id, s.name AS source_name, s.code AS source_code, \
                (s.enabled = 1 AND s.base_url IS NOT NULL AND s.config_error IS NULL) AS source_enabled, e.id AS source_entry_id, e.is_primary, \
                e.chapter_count, e.available_count, e.locked_count, e.missing_count, \
                e.stats_updated_at, e.last_error \
         FROM source_entries e JOIN sources s ON s.id = e.source_id \
         WHERE e.title_id = ? \
         ORDER BY e.is_primary DESC, s.name, e.id",
    )
    .bind(title.id)
    .fetch_all(pool)
    .await?;

    let sources = entries
        .into_iter()
        .map(|entry| TitleSourceDto {
            source_id: entry.source_id,
            source_entry_id: entry.source_entry_id,
            code: entry.source_code,
            name: entry.source_name,
            enabled: entry.source_enabled,
            is_primary: entry.is_primary,
            chapter_count: entry.chapter_count,
            available_count: entry.available_count,
            locked_count: entry.locked_count,
            missing_count: entry.missing_count,
            stats_updated_at: entry.stats_updated_at,
            last_error: entry.last_error,
        })
        .collect();

    Ok(TitleChapters { chapters, sources })
}

fn group_availability(variants: &[ChapterCandidate], now: i64) -> String {
    // Выключенный источник не должен делать главу «доступной» в UI
    let effective: Vec<&ChapterCandidate> = variants
        .iter()
        .filter(|variant| variant.source_enabled)
        .collect();

    if effective.is_empty() {
        // Ни одного рабочего источника: открытие вернёт source_disabled,
        // поэтому «available» здесь был бы ложью.
        return "unknown".to_owned();
    }

    if effective
        .iter()
        .any(|variant| variant.availability == "available")
    {
        return "available".to_owned();
    }
    if effective
        .iter()
        .any(|variant| selection::view(variant, now) == selection::AvailabilityView::Unknown)
    {
        return "unknown".to_owned();
    }
    if variants
        .iter()
        .any(|variant| variant.availability == "locked")
    {
        return "locked".to_owned();
    }
    if variants
        .iter()
        .any(|variant| variant.availability == "missing")
    {
        return "missing".to_owned();
    }
    "unknown".to_owned()
}
