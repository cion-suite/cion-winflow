//! Объединённый список глав для страницы тайтла: только БД, без сети.
//! Варианты группируются по каноническому `chapter_key`, alias → canonical.

use std::collections::HashMap;

use serde::Serialize;
use sqlx::SqlitePool;

use crate::auth::session::now_unix;
use crate::error::AppError;

use super::selection::{self, AvailabilityView, ChapterCandidate, TitleInfo};

#[derive(Debug, Serialize)]
pub struct TitleChapterVariantDto {
    pub chapter_id: i64,
    /// Собственный ключ варианта: у групп перевода он совпадает,
    /// у источников с разной нумерацией — отличается (кандидат в alias).
    pub chapter_key: String,
    pub volume: Option<i64>,
    pub number_text: Option<String>,
    pub number_milli: Option<i64>,
    pub chapter_kind: String,
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
    /// Язык источника из дескриптора: по нему UI раскладывает связки по колонкам.
    pub lang: String,
    pub enabled: bool,
    pub is_primary: bool,
    /// Место в приоритете языка тайтла: 0 — самый важный, `null` — не задан.
    pub priority_rank: Option<i64>,
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
    /// Язык, которым реально отфильтрованы главы (после проверки источников).
    /// `null` — фильтра нет: страница показывает общий список.
    pub view_lang: Option<String>,
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

/// `source_langs` — язык из дескриптора по коду источника: реестр знает его,
/// а таблица `sources` язык не хранит. `view_lang` — выбранный пользователем
/// язык просмотра: главы без варианта этого языка не показываются.
pub async fn title_chapters(
    pool: &SqlitePool,
    title: &TitleInfo,
    source_langs: &HashMap<String, &'static str>,
    view_lang: Option<&str>,
) -> Result<TitleChapters, AppError> {
    let now = now_unix();

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

    // Устаревший URL или localStorage не должен опустошать страницу:
    // язык просмотра действует, только если такой источник включён.
    // Выключенный источник не может отдать главу, поэтому показывать его
    // перевод без переключателя было бы тупиком.
    let view_lang = view_lang.filter(|lang| {
        entries.iter().any(|entry| {
            in_view_lang(
                &entry.source_code,
                entry.source_enabled,
                Some(*lang),
                source_langs,
            )
        })
    });

    let ranks = selection::load_priorities(pool, view_lang.or(title.lang.as_deref())).await?;

    // Диапазонные пары привязаны к source_entry и не должны перетирать
    // такой же ключ другого источника. Глобальные пары — ручные/legacy.
    let entry_aliases: HashMap<(i64, String), String> = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT source_entry_id, alias_key, canonical_key FROM chapter_aliases \
         WHERE title_id = ? AND source_entry_id IS NOT NULL",
    )
    .bind(title.id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(entry_id, alias_key, canonical_key)| ((entry_id, alias_key), canonical_key))
    .collect();
    let global_aliases: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT alias_key, canonical_key FROM chapter_aliases \
         WHERE title_id = ? AND source_entry_id IS NULL",
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
        let canonical = entry_aliases
            .get(&(row.source_entry_id, row.chapter_key.clone()))
            .or_else(|| global_aliases.get(&row.chapter_key))
            .cloned()
            .unwrap_or_else(|| row.chapter_key.clone());
        groups.entry(canonical).or_default().push(row);
    }

    let mut chapters = Vec::with_capacity(groups.len());
    for (key, variants) in groups {
        let ordered = selection::order(variants, title.preferred_source_id, &ranks, now);
        // В языке просмотра глава без варианта этого языка не показывается.
        // Остальные языки остаются в `variants` — они нужны редактору связок.
        let visible: Vec<&ChapterCandidate> = ordered
            .iter()
            .filter(|candidate| {
                in_view_lang(
                    &candidate.source_code,
                    candidate.source_enabled,
                    view_lang,
                    source_langs,
                )
            })
            .collect();
        // Свежий locked/missing preferred-источник не должен становиться
        // «выбранным» для отображения, если есть доступный вариант.
        let Some(selected) = first_usable(&visible, now) else {
            continue;
        };

        let availability = group_availability(&visible, now);
        let effective: Vec<&ChapterCandidate> = visible
            .iter()
            .copied()
            .filter(|variant| variant.source_enabled)
            .collect();
        let is_locked = if effective.is_empty() {
            visible.iter().all(|variant| variant.is_locked)
        } else {
            effective.iter().all(|variant| variant.is_locked)
        };

        // Номер показывает каноническая глава, а не выбранный вариант:
        // alias могли связать главу с неверным томом (например, источник
        // забыл сменить том) с главой, где нумерация правильная. Читается
        // при этом всё равно первый доступный вариант в порядке приоритета.
        let numbering = ordered
            .iter()
            .find(|variant| variant.chapter_key == key)
            .unwrap_or(selected);
        let number_text = numbering.number_text.clone();
        let number_milli = numbering.number_milli;
        let volume = numbering.volume;
        let chapter_kind = numbering.chapter_kind.clone();
        let name = selected.name.clone();
        let content_kind = selected.content_kind.clone();
        let page_count = selected.page_count;
        let published_at = selected.published_at;
        let selected_chapter_id = selected.chapter_id;

        chapters.push(TitleChapterDto {
            chapter_key: key,
            number_text,
            number_milli,
            volume,
            chapter_kind,
            name,
            content_kind,
            page_count,
            availability,
            is_locked,
            published_at,
            selected_chapter_id,
            variants: ordered
                .into_iter()
                .map(|variant| TitleChapterVariantDto {
                    chapter_id: variant.chapter_id,
                    chapter_key: variant.chapter_key,
                    volume: variant.volume,
                    number_text: variant.number_text,
                    number_milli: variant.number_milli,
                    chapter_kind: variant.chapter_kind,
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

    let sources = entries
        .into_iter()
        .map(|entry| {
            let lang = source_langs
                .get(entry.source_code.as_str())
                .copied()
                .unwrap_or_default()
                .to_owned();
            TitleSourceDto {
                source_id: entry.source_id,
                source_entry_id: entry.source_entry_id,
                code: entry.source_code,
                name: entry.source_name,
                lang,
                enabled: entry.source_enabled,
                is_primary: entry.is_primary,
                priority_rank: ranks.get(&entry.source_id).copied(),
                chapter_count: entry.chapter_count,
                available_count: entry.available_count,
                locked_count: entry.locked_count,
                missing_count: entry.missing_count,
                stats_updated_at: entry.stats_updated_at,
                last_error: entry.last_error,
            }
        })
        .collect();

    Ok(TitleChapters {
        chapters,
        sources,
        view_lang: view_lang.map(str::to_owned),
    })
}

/// То же, что `selection::first_usable`, но по ссылкам на подмножество
/// (язык просмотра): клонировать кандидатов ради выбора не нужно.
fn first_usable<'a>(variants: &[&'a ChapterCandidate], now: i64) -> Option<&'a ChapterCandidate> {
    variants
        .iter()
        .copied()
        .find(|candidate| {
            candidate.source_enabled
                && selection::view(candidate, now) != AvailabilityView::Unavailable
        })
        .or_else(|| {
            variants
                .iter()
                .copied()
                .find(|candidate| candidate.source_enabled)
        })
        .or_else(|| variants.first().copied())
}

/// Вариант попадает в язык просмотра: источник включён и его язык совпадает.
/// Без языка просмотра фильтра нет — страница ведёт себя как до фичи.
fn in_view_lang(
    source_code: &str,
    source_enabled: bool,
    view_lang: Option<&str>,
    source_langs: &HashMap<String, &'static str>,
) -> bool {
    match view_lang {
        None => true,
        Some(lang) => {
            source_enabled
                && source_langs
                    .get(source_code)
                    .is_some_and(|value| *value == lang)
        }
    }
}

fn group_availability(variants: &[&ChapterCandidate], now: i64) -> String {
    // Выключенный источник не должен делать главу «доступной» в UI
    let effective: Vec<&ChapterCandidate> = variants
        .iter()
        .copied()
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
        .copied()
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::in_view_lang;

    fn langs() -> HashMap<String, &'static str> {
        HashMap::from([
            ("ru_source".to_owned(), "ru"),
            ("en_source".to_owned(), "en"),
        ])
    }

    #[test]
    fn without_view_language_any_variant_is_visible() {
        assert!(in_view_lang("en_source", false, None, &langs()));
        assert!(in_view_lang("unknown", false, None, &langs()));
    }

    #[test]
    fn view_language_needs_enabled_source_of_that_language() {
        let langs = langs();
        assert!(in_view_lang("en_source", true, Some("en"), &langs));
        assert!(!in_view_lang("en_source", false, Some("en"), &langs));
        assert!(!in_view_lang("ru_source", true, Some("en"), &langs));
        assert!(!in_view_lang("unknown", true, Some("en"), &langs));
    }
}
