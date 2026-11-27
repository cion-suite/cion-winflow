//! Диапазонные связки глав.
//!
//! Связка — правило «с этой главы источника и выше»: якорь связанного
//! источника сопоставляется с якорем канонической нумерации, а следующие
//! главы получают то же смещение. Точные пары хранятся в `chapter_aliases`
//! как производная (`origin = 'range'`) и пересобираются целиком; ручные
//! пары (`source_entry_id IS NULL`) остаются нетронутыми и имеют приоритет.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::{auth::session::now_unix, error::AppError, state::AppState};

const MAX_RANGES_PER_REQUEST: usize = 100;

#[derive(Debug, Deserialize)]
pub struct RangeInput {
    pub source_entry_id: i64,
    pub anchor_chapter_id: i64,
    pub canonical_chapter_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct RangeBatchInput {
    pub ranges: Vec<RangeInput>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RangeAnchorDto {
    pub chapter_id: Option<i64>,
    pub chapter_key: String,
    pub volume: Option<i64>,
    pub number_text: Option<String>,
    pub number_milli: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RangeCoverageDto {
    pub linked: i64,
    pub pending: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AliasRangeDto {
    pub id: i64,
    pub source_entry_id: i64,
    pub source_id: i64,
    pub source_code: String,
    pub source_name: String,
    pub anchor: RangeAnchorDto,
    pub canonical: RangeAnchorDto,
    pub chapter_kind: String,
    pub mapping_mode: String,
    pub status: String,
    pub coverage: RangeCoverageDto,
    pub next_range_id: Option<i64>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManualAliasDto {
    pub alias_key: String,
    pub canonical_key: String,
}

#[derive(Debug, Serialize)]
pub struct AliasRangeListResponse {
    pub ranges: Vec<AliasRangeDto>,
    /// Ручные пары, которые не свернулись в диапазон (одиночные/нерегулярные).
    pub manual_aliases: Vec<ManualAliasDto>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct RangeRow {
    id: i64,
    source_entry_id: i64,
    anchor_chapter_id: i64,
    anchor_chapter_key: String,
    anchor_volume: Option<i64>,
    anchor_number_text: Option<String>,
    anchor_number_milli: Option<i64>,
    anchor_ordinal_site: i64,
    canonical_key: String,
    canonical_chapter_id: Option<i64>,
    canonical_volume: Option<i64>,
    canonical_number_text: Option<String>,
    canonical_number_milli: Option<i64>,
    chapter_kind: String,
    mapping_mode: String,
    status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ChapterRow {
    id: i64,
    source_entry_id: i64,
    chapter_key: String,
    volume: Option<i64>,
    number_text: Option<String>,
    number_milli: Option<i64>,
    ordinal_site: i64,
    chapter_kind: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct EntryRow {
    id: i64,
    source_id: i64,
    code: String,
    name: String,
}

#[derive(Debug, Clone)]
struct RangeInsert {
    source_entry_id: i64,
    anchor_chapter_id: i64,
    anchor_chapter_key: String,
    anchor_volume: Option<i64>,
    anchor_number_text: Option<String>,
    anchor_number_milli: Option<i64>,
    anchor_ordinal_site: i64,
    canonical_key: String,
    canonical_chapter_id: i64,
    canonical_volume: Option<i64>,
    canonical_number_text: Option<String>,
    canonical_number_milli: Option<i64>,
    chapter_kind: String,
    mapping_mode: String,
}

#[derive(Debug, Default)]
pub struct MaterializeOutcome {
    /// Пересборка пропущена: есть связка, требующая проверки.
    pub skipped: bool,
    pub pairs: usize,
}

enum CanonicalMatch {
    Found(String),
    Pending,
    Ambiguous,
}

fn chapter_order(chapter: &ChapterRow) -> (i64, i64, i64, i64) {
    (
        chapter.volume.unwrap_or(0),
        chapter.number_milli.unwrap_or(i64::MAX),
        chapter.ordinal_site,
        chapter.id,
    )
}

fn range_order(range: &RangeRow) -> (i64, i64, i64, i64) {
    (
        range.anchor_volume.unwrap_or(0),
        range.anchor_number_milli.unwrap_or(i64::MAX),
        range.anchor_ordinal_site,
        range.anchor_chapter_id,
    )
}

pub async fn list_ranges(
    state: &AppState,
    title_id: i64,
) -> Result<AliasRangeListResponse, AppError> {
    crate::catalog::links::ensure_title(&state.pool, title_id).await?;
    backfill_if_needed(&state.pool, title_id).await?;
    build_list(&state.pool, title_id).await
}

pub async fn create_range(
    state: &AppState,
    title_id: i64,
    input: RangeInput,
) -> Result<AliasRangeDto, AppError> {
    crate::catalog::links::ensure_title(&state.pool, title_id).await?;
    let insert = validate_input(&state.pool, title_id, &input).await?;
    ensure_unique_anchor(&state.pool, title_id, &insert, None).await?;
    insert_range(&state.pool, title_id, &insert).await?;
    materialize_title(&state.pool, title_id).await?;

    find_range_dto(state, title_id, insert.anchor_chapter_id).await
}

pub async fn update_range(
    state: &AppState,
    title_id: i64,
    range_id: i64,
    input: RangeInput,
) -> Result<AliasRangeDto, AppError> {
    crate::catalog::links::ensure_title(&state.pool, title_id).await?;
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM chapter_alias_ranges WHERE id = ? AND title_id = ?",
    )
    .bind(range_id)
    .bind(title_id)
    .fetch_optional(&state.pool)
    .await?;
    if exists.is_none() {
        return Err(AppError::not_found(
            "alias_range_not_found",
            "chapter alias range not found",
        ));
    }

    let insert = validate_input(&state.pool, title_id, &input).await?;
    ensure_unique_anchor(&state.pool, title_id, &insert, Some(range_id)).await?;

    sqlx::query(
        "UPDATE chapter_alias_ranges SET \
             source_entry_id = ?, anchor_chapter_id = ?, anchor_chapter_key = ?, \
             anchor_volume = ?, anchor_number_text = ?, anchor_number_milli = ?, \
             anchor_ordinal_site = ?, canonical_key = ?, canonical_chapter_id = ?, \
             canonical_volume = ?, canonical_number_text = ?, canonical_number_milli = ?, \
             chapter_kind = ?, mapping_mode = ?, status = 'active' \
         WHERE id = ? AND title_id = ?",
    )
    .bind(insert.source_entry_id)
    .bind(insert.anchor_chapter_id)
    .bind(&insert.anchor_chapter_key)
    .bind(insert.anchor_volume)
    .bind(insert.anchor_number_text.as_deref())
    .bind(insert.anchor_number_milli)
    .bind(insert.anchor_ordinal_site)
    .bind(&insert.canonical_key)
    .bind(insert.canonical_chapter_id)
    .bind(insert.canonical_volume)
    .bind(insert.canonical_number_text.as_deref())
    .bind(insert.canonical_number_milli)
    .bind(&insert.chapter_kind)
    .bind(&insert.mapping_mode)
    .bind(range_id)
    .bind(title_id)
    .execute(&state.pool)
    .await?;

    materialize_title(&state.pool, title_id).await?;
    find_range_dto(state, title_id, insert.anchor_chapter_id).await
}

pub async fn delete_range(state: &AppState, title_id: i64, range_id: i64) -> Result<(), AppError> {
    let result = sqlx::query("DELETE FROM chapter_alias_ranges WHERE id = ? AND title_id = ?")
        .bind(range_id)
        .bind(title_id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::not_found(
            "alias_range_not_found",
            "chapter alias range not found",
        ));
    }

    materialize_title(&state.pool, title_id).await?;
    Ok(())
}

pub async fn apply_ranges(
    state: &AppState,
    title_id: i64,
    input: RangeBatchInput,
) -> Result<AliasRangeListResponse, AppError> {
    crate::catalog::links::ensure_title(&state.pool, title_id).await?;
    if input.ranges.is_empty() {
        return Err(AppError::bad_request(
            "validation",
            "ranges must not be empty",
        ));
    }
    if input.ranges.len() > MAX_RANGES_PER_REQUEST {
        return Err(AppError::bad_request(
            "validation",
            format!("at most {MAX_RANGES_PER_REQUEST} ranges are allowed"),
        ));
    }

    let mut inserts = Vec::with_capacity(input.ranges.len());
    let mut anchors = HashSet::new();
    for range in &input.ranges {
        let insert = validate_input(&state.pool, title_id, range).await?;
        let anchor = (
            insert.source_entry_id,
            insert.chapter_kind.clone(),
            insert.anchor_chapter_id,
        );
        if !anchors.insert(anchor) {
            return Err(AppError::bad_request(
                "alias_range_invalid",
                "duplicate anchor in request",
            ));
        }
        inserts.push(insert);
    }

    for insert in &inserts {
        ensure_unique_anchor(&state.pool, title_id, insert, None).await?;
    }

    let mut tx = state.pool.begin().await?;
    for insert in &inserts {
        insert_range_tx(&mut tx, title_id, insert).await?;
    }
    tx.commit().await?;

    materialize_title(&state.pool, title_id).await?;
    build_list(&state.pool, title_id).await
}

/// Пересобирает производные пары `origin = 'range'` для тайтла.
///
/// Если есть связка в статусе `needs_review`/`broken`, пересборка
/// пропускается: иначе пришлось бы удалить пары, которые ещё описывают
/// текущее поведение. Администратор сначала разбирает такую связку.
pub async fn materialize_title(
    pool: &SqlitePool,
    title_id: i64,
) -> Result<MaterializeOutcome, AppError> {
    let ranges = load_ranges(pool, title_id).await?;

    if ranges.is_empty() {
        sqlx::query("DELETE FROM chapter_aliases WHERE title_id = ? AND origin = 'range'")
            .bind(title_id)
            .execute(pool)
            .await?;
        return Ok(MaterializeOutcome::default());
    }

    if ranges.iter().any(|range| range.status != "active") {
        return Ok(MaterializeOutcome {
            skipped: true,
            ..MaterializeOutcome::default()
        });
    }

    let chapters = load_chapters(pool, title_id).await?;
    let chapter_by_id: HashMap<i64, &ChapterRow> = chapters
        .iter()
        .map(|chapter| (chapter.id, chapter))
        .collect();

    // Источник мог поправить нумерацию уже существующих глав. Снимок якоря
    // тогда расходится с живыми координатами: не пересчитываем молча,
    // а просим администратора подтвердить связку.
    let mut drift = HashSet::new();
    for range in &ranges {
        let anchor = chapter_by_id.get(&range.anchor_chapter_id);
        let anchor_ok = anchor.is_some_and(|chapter| {
            chapter.chapter_key == range.anchor_chapter_key
                && chapter.volume == range.anchor_volume
                && chapter.number_text == range.anchor_number_text
                && chapter.number_milli == range.anchor_number_milli
        });
        if !anchor_ok {
            drift.insert(range.id);
        }
        if let Some(canonical_id) = range.canonical_chapter_id {
            let canonical_ok = chapter_by_id.get(&canonical_id).is_some_and(|chapter| {
                chapter.chapter_key == range.canonical_key
                    && chapter.volume == range.canonical_volume
                    && chapter.number_text == range.canonical_number_text
                    && chapter.number_milli == range.canonical_number_milli
            });
            if !canonical_ok {
                drift.insert(range.id);
            }
        }
    }
    if !drift.is_empty() {
        for range_id in drift {
            sqlx::query(
                "UPDATE chapter_alias_ranges SET status = 'needs_review' \
                 WHERE id = ? AND title_id = ?",
            )
            .bind(range_id)
            .bind(title_id)
            .execute(pool)
            .await?;
        }
        return Ok(MaterializeOutcome {
            skipped: true,
            ..MaterializeOutcome::default()
        });
    }

    let mut ranges_by_entry: HashMap<i64, Vec<&RangeRow>> = HashMap::new();
    for range in &ranges {
        ranges_by_entry
            .entry(range.source_entry_id)
            .or_default()
            .push(range);
    }
    for list in ranges_by_entry.values_mut() {
        list.sort_by_key(|range| range_order(range));
    }

    let mut chapters_by_entry: HashMap<i64, Vec<&ChapterRow>> = HashMap::new();
    for chapter in &chapters {
        chapters_by_entry
            .entry(chapter.source_entry_id)
            .or_default()
            .push(chapter);
    }
    for list in chapters_by_entry.values_mut() {
        list.sort_by_key(|chapter| chapter_order(chapter));
    }

    let mut pairs: HashMap<(i64, String), String> = HashMap::new();
    let mut warnings: HashSet<&'static str> = HashSet::new();

    for (entry_id, entry_ranges) in &ranges_by_entry {
        let Some(entry_chapters) = chapters_by_entry.get(entry_id) else {
            continue;
        };

        for chapter in entry_chapters {
            let mut applicable: Option<&RangeRow> = None;
            for range in entry_ranges {
                if range.chapter_kind != chapter.chapter_kind {
                    continue;
                }
                if range_order(range) <= chapter_order(chapter) {
                    applicable = Some(range);
                }
            }
            let Some(range) = applicable else {
                continue;
            };

            let Some(chapter_milli) = chapter.number_milli else {
                continue;
            };
            let Some(chapter_text) = chapter.number_text.as_deref() else {
                continue;
            };

            let (volume_delta, number_delta) = match range.mapping_mode.as_str() {
                "volume_number" => {
                    let (
                        Some(anchor_volume),
                        Some(anchor_milli),
                        Some(canonical_volume),
                        Some(canonical_milli),
                    ) = (
                        range.anchor_volume,
                        range.anchor_number_milli,
                        range.canonical_volume,
                        range.canonical_number_milli,
                    )
                    else {
                        warnings.insert("anchor_incomplete");
                        continue;
                    };
                    let Some(volume_delta) = canonical_volume.checked_sub(anchor_volume) else {
                        continue;
                    };
                    let Some(number_delta) = canonical_milli.checked_sub(anchor_milli) else {
                        continue;
                    };
                    (Some(volume_delta), number_delta)
                }
                "number_only" => {
                    let (Some(anchor_milli), Some(canonical_milli)) =
                        (range.anchor_number_milli, range.canonical_number_milli)
                    else {
                        warnings.insert("anchor_incomplete");
                        continue;
                    };
                    let Some(number_delta) = canonical_milli.checked_sub(anchor_milli) else {
                        continue;
                    };
                    (None, number_delta)
                }
                _ => {
                    warnings.insert("mapping_mode_invalid");
                    continue;
                }
            };

            let target_volume = match range.mapping_mode.as_str() {
                "volume_number" => {
                    let Some(chapter_volume) = chapter.volume else {
                        warnings.insert("chapter_volume_missing");
                        continue;
                    };
                    let Some(volume_delta) = volume_delta else {
                        continue;
                    };
                    chapter_volume.checked_add(volume_delta)
                }
                _ => None,
            };

            let canonical = if number_delta == 0 {
                find_canonical_by_text(
                    &chapters,
                    range.source_entry_id,
                    &range.chapter_kind,
                    target_volume,
                    Some(chapter_text),
                )
            } else {
                let Some(target_milli) = chapter_milli.checked_add(number_delta) else {
                    continue;
                };
                find_canonical_by_milli(
                    &chapters,
                    range.source_entry_id,
                    &range.chapter_kind,
                    target_volume,
                    target_milli,
                )
            };

            match canonical {
                CanonicalMatch::Found(canonical_key) => {
                    if canonical_key == chapter.chapter_key {
                        continue;
                    }
                    pairs
                        .entry((*entry_id, chapter.chapter_key.clone()))
                        .or_insert(canonical_key);
                }
                CanonicalMatch::Ambiguous => {
                    warnings.insert("canonical_ambiguous");
                }
                CanonicalMatch::Pending => {}
            }
        }
    }

    // Ручная пара приоритетнее производной: не перетираем решение админа.
    let manual = load_global_aliases(pool, title_id).await?;
    pairs.retain(|(_, alias_key), canonical_key| {
        manual
            .get(alias_key)
            .is_none_or(|manual_key| manual_key == canonical_key)
    });

    // И производная пара должна резолвиться в один hop: не пишем canonical,
    // который сам является alias-ключом другой пары или ручной связи.
    let alias_keys: HashSet<String> = pairs
        .keys()
        .map(|(_, alias_key)| alias_key.clone())
        .collect();
    pairs.retain(|(_, _), canonical_key| {
        !alias_keys.contains(canonical_key) && !manual.contains_key(canonical_key)
    });

    let old_pairs = load_generated_pairs(pool, title_id).await?;
    let old_by_key: HashMap<(i64, String), String> = old_pairs
        .iter()
        .map(|(entry_id, alias_key, canonical_key)| {
            ((*entry_id, alias_key.clone()), canonical_key.clone())
        })
        .collect();

    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM chapter_aliases WHERE title_id = ? AND origin = 'range'")
        .bind(title_id)
        .execute(&mut *tx)
        .await?;

    let now = now_unix();
    for ((entry_id, alias_key), canonical_key) in &pairs {
        sqlx::query(
            "INSERT INTO chapter_aliases \
                 (title_id, source_entry_id, canonical_key, alias_key, origin, created_at) \
             VALUES (?, ?, ?, ?, 'range', ?) \
             ON CONFLICT (title_id, source_entry_id, alias_key) \
                 WHERE source_entry_id IS NOT NULL \
             DO UPDATE SET canonical_key = excluded.canonical_key, origin = 'range'",
        )
        .bind(title_id)
        .bind(entry_id)
        .bind(canonical_key)
        .bind(alias_key)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    let mapping = build_reader_mapping(&old_by_key, &pairs);
    remap_reader_state(&mut tx, title_id, &mapping).await?;
    tx.commit().await?;

    if warnings.contains("canonical_ambiguous") {
        tracing::debug!(
            title_id,
            "alias ranges: ambiguous canonical chapters skipped"
        );
    }

    Ok(MaterializeOutcome {
        skipped: false,
        pairs: pairs.len(),
    })
}

fn find_canonical_by_text(
    chapters: &[ChapterRow],
    source_entry_id: i64,
    chapter_kind: &str,
    target_volume: Option<i64>,
    target_text: Option<&str>,
) -> CanonicalMatch {
    let mut keys = HashSet::new();
    for chapter in chapters {
        if chapter.source_entry_id == source_entry_id
            || chapter.chapter_kind != chapter_kind
            || chapter.volume != target_volume
            || chapter.number_text.as_deref() != target_text
        {
            continue;
        }
        keys.insert(chapter.chapter_key.clone());
    }
    match keys.len() {
        0 => CanonicalMatch::Pending,
        1 => match keys.into_iter().next() {
            Some(key) => CanonicalMatch::Found(key),
            None => CanonicalMatch::Pending,
        },
        _ => CanonicalMatch::Ambiguous,
    }
}

fn find_canonical_by_milli(
    chapters: &[ChapterRow],
    source_entry_id: i64,
    chapter_kind: &str,
    target_volume: Option<i64>,
    target_milli: i64,
) -> CanonicalMatch {
    let mut keys = HashSet::new();
    for chapter in chapters {
        if chapter.source_entry_id == source_entry_id
            || chapter.chapter_kind != chapter_kind
            || chapter.volume != target_volume
            || chapter.number_milli != Some(target_milli)
        {
            continue;
        }
        keys.insert(chapter.chapter_key.clone());
    }
    match keys.len() {
        0 => CanonicalMatch::Pending,
        1 => match keys.into_iter().next() {
            Some(key) => CanonicalMatch::Found(key),
            None => CanonicalMatch::Pending,
        },
        _ => CanonicalMatch::Ambiguous,
    }
}

fn build_reader_mapping(
    old_by_key: &HashMap<(i64, String), String>,
    new_by_key: &HashMap<(i64, String), String>,
) -> HashMap<String, String> {
    let mut mapping = HashMap::new();
    let mut ambiguous = HashSet::new();

    for (key, old_canonical) in old_by_key {
        let Some(new_canonical) = new_by_key.get(key) else {
            continue;
        };
        if old_canonical == new_canonical {
            continue;
        }
        insert_mapping(
            &mut mapping,
            &mut ambiguous,
            old_canonical.clone(),
            new_canonical.clone(),
        );
    }

    for (key, new_canonical) in new_by_key {
        if old_by_key.contains_key(key) {
            continue;
        }
        insert_mapping(
            &mut mapping,
            &mut ambiguous,
            key.1.clone(),
            new_canonical.clone(),
        );
    }

    for key in ambiguous {
        mapping.remove(&key);
    }
    mapping
}

fn insert_mapping(
    mapping: &mut HashMap<String, String>,
    ambiguous: &mut HashSet<String>,
    from: String,
    to: String,
) {
    if from == to || ambiguous.contains(&from) {
        return;
    }
    match mapping.get(&from) {
        Some(existing) if existing != &to => {
            ambiguous.insert(from);
        }
        Some(_) => {}
        None => {
            mapping.insert(from, to);
        }
    }
}

async fn remap_reader_state(
    tx: &mut Transaction<'_, Sqlite>,
    title_id: i64,
    mapping: &HashMap<String, String>,
) -> Result<(), AppError> {
    if mapping.is_empty() {
        return Ok(());
    }

    let progress = sqlx::query_as::<_, (i64, String, Option<i64>, Option<i64>, Option<f64>, i64)>(
        "SELECT user_id, chapter_key, page, seconds, scroll, updated_at \
         FROM progress WHERE title_id = ?",
    )
    .bind(title_id)
    .fetch_all(&mut **tx)
    .await?;
    let reads = sqlx::query_as::<_, (i64, String, i64)>(
        "SELECT user_id, chapter_key, read_at FROM chapter_reads WHERE title_id = ?",
    )
    .bind(title_id)
    .fetch_all(&mut **tx)
    .await?;

    sqlx::query("DELETE FROM progress WHERE title_id = ?")
        .bind(title_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM chapter_reads WHERE title_id = ?")
        .bind(title_id)
        .execute(&mut **tx)
        .await?;

    for (user_id, chapter_key, page, seconds, scroll, updated_at) in progress {
        let target = mapping.get(&chapter_key).cloned().unwrap_or(chapter_key);
        sqlx::query(
            "INSERT INTO progress \
                 (user_id, title_id, chapter_key, page, seconds, scroll, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(title_id)
        .bind(target)
        .bind(page)
        .bind(seconds)
        .bind(scroll)
        .bind(updated_at)
        .execute(&mut **tx)
        .await?;
    }

    let mut read_rows: HashMap<(i64, String), i64> = HashMap::new();
    for (user_id, chapter_key, read_at) in reads {
        let target = mapping.get(&chapter_key).cloned().unwrap_or(chapter_key);
        let entry = read_rows.entry((user_id, target)).or_insert(read_at);
        if read_at > *entry {
            *entry = read_at;
        }
    }
    for ((user_id, chapter_key), read_at) in read_rows {
        sqlx::query(
            "INSERT INTO chapter_reads (user_id, title_id, chapter_key, read_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(title_id)
        .bind(chapter_key)
        .bind(read_at)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

async fn build_list(pool: &SqlitePool, title_id: i64) -> Result<AliasRangeListResponse, AppError> {
    let ranges = load_ranges(pool, title_id).await?;
    let chapters = load_chapters(pool, title_id).await?;
    let entries = load_entries(pool, title_id).await?;
    let generated: HashSet<(i64, String)> = load_generated_pairs(pool, title_id)
        .await?
        .into_iter()
        .map(|(entry_id, alias_key, _)| (entry_id, alias_key))
        .collect();
    let global = load_global_aliases(pool, title_id).await?;

    let entry_by_id: HashMap<i64, &EntryRow> =
        entries.iter().map(|entry| (entry.id, entry)).collect();
    let chapter_by_id: HashMap<i64, &ChapterRow> = chapters
        .iter()
        .map(|chapter| (chapter.id, chapter))
        .collect();

    let mut chapters_by_entry: HashMap<i64, Vec<&ChapterRow>> = HashMap::new();
    for chapter in &chapters {
        chapters_by_entry
            .entry(chapter.source_entry_id)
            .or_default()
            .push(chapter);
    }
    for list in chapters_by_entry.values_mut() {
        list.sort_by_key(|chapter| chapter_order(chapter));
    }

    let mut items = Vec::with_capacity(ranges.len());
    for range in &ranges {
        let Some(entry) = entry_by_id.get(&range.source_entry_id) else {
            continue;
        };
        let anchor_order = range_order(range);
        let next = ranges
            .iter()
            .filter(|candidate| {
                candidate.source_entry_id == range.source_entry_id
                    && candidate.chapter_kind == range.chapter_kind
                    && range_order(candidate) > anchor_order
            })
            .min_by_key(|candidate| range_order(candidate));
        let next_order = next.map(range_order);
        let mut linked = 0_i64;
        let mut pending = 0_i64;
        if let Some(entry_chapters) = chapters_by_entry.get(&range.source_entry_id) {
            for chapter in entry_chapters {
                if chapter.chapter_kind != range.chapter_kind {
                    continue;
                }
                let order = chapter_order(chapter);
                if order < anchor_order {
                    continue;
                }
                if let Some(next_order) = next_order
                    && order >= next_order
                {
                    continue;
                }
                let is_linked = generated
                    .contains(&(range.source_entry_id, chapter.chapter_key.clone()))
                    || global.contains_key(&chapter.chapter_key);
                if is_linked {
                    linked += 1;
                } else {
                    pending += 1;
                }
            }
        }

        let mut warnings = Vec::new();
        if range.status != "active" {
            warnings.push(range.status.clone());
        }
        if !chapter_by_id.contains_key(&range.anchor_chapter_id) {
            warnings.push("anchor_missing".to_owned());
        }
        if range
            .canonical_chapter_id
            .is_some_and(|id| !chapter_by_id.contains_key(&id))
        {
            warnings.push("canonical_missing".to_owned());
        }
        if range.mapping_mode == "number_only" {
            warnings.push("number_only".to_owned());
        }

        items.push(AliasRangeDto {
            id: range.id,
            source_entry_id: range.source_entry_id,
            source_id: entry.source_id,
            source_code: entry.code.clone(),
            source_name: entry.name.clone(),
            anchor: RangeAnchorDto {
                chapter_id: Some(range.anchor_chapter_id),
                chapter_key: range.anchor_chapter_key.clone(),
                volume: range.anchor_volume,
                number_text: range.anchor_number_text.clone(),
                number_milli: range.anchor_number_milli,
            },
            canonical: RangeAnchorDto {
                chapter_id: range.canonical_chapter_id,
                chapter_key: range.canonical_key.clone(),
                volume: range.canonical_volume,
                number_text: range.canonical_number_text.clone(),
                number_milli: range.canonical_number_milli,
            },
            chapter_kind: range.chapter_kind.clone(),
            mapping_mode: range.mapping_mode.clone(),
            status: range.status.clone(),
            coverage: RangeCoverageDto { linked, pending },
            next_range_id: next.map(|range| range.id),
            warnings,
        });
    }

    let mut manual_aliases: Vec<ManualAliasDto> = global
        .into_iter()
        .map(|(alias_key, canonical_key)| ManualAliasDto {
            alias_key,
            canonical_key,
        })
        .collect();
    manual_aliases.sort_by(|left, right| left.alias_key.cmp(&right.alias_key));

    Ok(AliasRangeListResponse {
        ranges: items,
        manual_aliases,
    })
}

async fn find_range_dto(
    state: &AppState,
    title_id: i64,
    anchor_chapter_id: i64,
) -> Result<AliasRangeDto, AppError> {
    let list = build_list(&state.pool, title_id).await?;
    list.ranges
        .into_iter()
        .find(|range| range.anchor.chapter_id == Some(anchor_chapter_id))
        .ok_or_else(|| {
            AppError::not_found("alias_range_not_found", "chapter alias range not found")
        })
}

async fn validate_input(
    pool: &SqlitePool,
    title_id: i64,
    input: &RangeInput,
) -> Result<RangeInsert, AppError> {
    let entry =
        sqlx::query_scalar::<_, i64>("SELECT 1 FROM source_entries WHERE id = ? AND title_id = ?")
            .bind(input.source_entry_id)
            .bind(title_id)
            .fetch_optional(pool)
            .await?;
    if entry.is_none() {
        return Err(AppError::bad_request(
            "alias_range_invalid",
            "source entry is not attached to this title",
        ));
    }

    let anchor = load_chapter(pool, input.anchor_chapter_id).await?;
    let canonical = load_chapter(pool, input.canonical_chapter_id).await?;

    let Some(anchor) = anchor else {
        return Err(AppError::not_found(
            "anchor_not_found",
            "anchor chapter not found",
        ));
    };
    let Some(canonical) = canonical else {
        return Err(AppError::not_found(
            "canonical_not_found",
            "canonical chapter not found",
        ));
    };

    if anchor.source_entry_id != input.source_entry_id {
        return Err(AppError::bad_request(
            "alias_range_invalid",
            "anchor chapter does not belong to the source entry",
        ));
    }
    let canonical_title = sqlx::query_scalar::<_, i64>(
        "SELECT e.title_id FROM chapters ch JOIN source_entries e ON e.id = ch.source_entry_id \
         WHERE ch.id = ?",
    )
    .bind(canonical.id)
    .fetch_optional(pool)
    .await?;
    if canonical_title != Some(title_id) {
        return Err(AppError::bad_request(
            "alias_range_invalid",
            "canonical chapter does not belong to this title",
        ));
    }
    if anchor.source_entry_id == canonical.source_entry_id {
        return Err(AppError::bad_request(
            "alias_range_invalid",
            "canonical chapter must belong to another source",
        ));
    }
    if anchor.chapter_kind != canonical.chapter_kind {
        return Err(AppError::bad_request(
            "alias_range_invalid",
            "anchor and canonical chapters must have the same kind",
        ));
    }
    if anchor.chapter_key == canonical.chapter_key {
        return Err(AppError::bad_request(
            "alias_range_invalid",
            "anchor and canonical chapters must differ",
        ));
    }

    let (Some(anchor_milli), Some(canonical_milli)) = (anchor.number_milli, canonical.number_milli)
    else {
        return Err(AppError::bad_request(
            "alias_range_invalid",
            "anchors must have numeric chapter numbers",
        ));
    };
    if anchor.number_text.is_none() || canonical.number_text.is_none() {
        return Err(AppError::bad_request(
            "alias_range_invalid",
            "anchors must have chapter numbers",
        ));
    }

    let mapping_mode = match (anchor.volume, canonical.volume) {
        (Some(_), Some(_)) => "volume_number",
        (None, None) => "number_only",
        _ => {
            return Err(AppError::bad_request(
                "alias_range_invalid",
                "volume must be present on both anchors or on neither",
            ));
        }
    };

    Ok(RangeInsert {
        source_entry_id: input.source_entry_id,
        anchor_chapter_id: anchor.id,
        anchor_chapter_key: anchor.chapter_key,
        anchor_volume: anchor.volume,
        anchor_number_text: anchor.number_text,
        anchor_number_milli: Some(anchor_milli),
        anchor_ordinal_site: anchor.ordinal_site,
        canonical_key: canonical.chapter_key,
        canonical_chapter_id: canonical.id,
        canonical_volume: canonical.volume,
        canonical_number_text: canonical.number_text,
        canonical_number_milli: Some(canonical_milli),
        chapter_kind: anchor.chapter_kind,
        mapping_mode: mapping_mode.to_owned(),
    })
}

async fn load_chapter(pool: &SqlitePool, chapter_id: i64) -> Result<Option<ChapterRow>, AppError> {
    let chapter = sqlx::query_as::<_, ChapterRow>(
        "SELECT id, source_entry_id, chapter_key, volume, number_text, number_milli, \
                ordinal_site, chapter_kind \
         FROM chapters WHERE id = ? AND removed_at IS NULL",
    )
    .bind(chapter_id)
    .fetch_optional(pool)
    .await?;
    Ok(chapter)
}

async fn ensure_unique_anchor(
    pool: &SqlitePool,
    title_id: i64,
    insert: &RangeInsert,
    exclude_id: Option<i64>,
) -> Result<(), AppError> {
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM chapter_alias_ranges \
         WHERE title_id = ? AND source_entry_id = ? AND chapter_kind = ? \
           AND anchor_chapter_id = ? AND (? IS NULL OR id <> ?)",
    )
    .bind(title_id)
    .bind(insert.source_entry_id)
    .bind(&insert.chapter_kind)
    .bind(insert.anchor_chapter_id)
    .bind(exclude_id)
    .bind(exclude_id)
    .fetch_optional(pool)
    .await?;
    if exists.is_some() {
        return Err(AppError::conflict(
            "alias_range_exists",
            "a range with this anchor already exists",
        ));
    }
    Ok(())
}

async fn insert_range(
    pool: &SqlitePool,
    title_id: i64,
    insert: &RangeInsert,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    insert_range_tx(&mut tx, title_id, insert).await?;
    tx.commit().await?;
    Ok(())
}

async fn insert_range_tx(
    tx: &mut Transaction<'_, Sqlite>,
    title_id: i64,
    insert: &RangeInsert,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO chapter_alias_ranges \
             (title_id, source_entry_id, anchor_chapter_id, anchor_chapter_key, \
              anchor_volume, anchor_number_text, anchor_number_milli, anchor_ordinal_site, \
              canonical_key, canonical_chapter_id, canonical_volume, canonical_number_text, \
              canonical_number_milli, chapter_kind, mapping_mode, status, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?)",
    )
    .bind(title_id)
    .bind(insert.source_entry_id)
    .bind(insert.anchor_chapter_id)
    .bind(&insert.anchor_chapter_key)
    .bind(insert.anchor_volume)
    .bind(insert.anchor_number_text.as_deref())
    .bind(insert.anchor_number_milli)
    .bind(insert.anchor_ordinal_site)
    .bind(&insert.canonical_key)
    .bind(insert.canonical_chapter_id)
    .bind(insert.canonical_volume)
    .bind(insert.canonical_number_text.as_deref())
    .bind(insert.canonical_number_milli)
    .bind(&insert.chapter_kind)
    .bind(&insert.mapping_mode)
    .bind(now_unix())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn load_ranges(pool: &SqlitePool, title_id: i64) -> Result<Vec<RangeRow>, AppError> {
    let ranges = sqlx::query_as::<_, RangeRow>(
        "SELECT id, source_entry_id, anchor_chapter_id, anchor_chapter_key, \
                anchor_volume, anchor_number_text, anchor_number_milli, anchor_ordinal_site, \
                canonical_key, canonical_chapter_id, canonical_volume, canonical_number_text, \
                canonical_number_milli, chapter_kind, mapping_mode, status \
         FROM chapter_alias_ranges WHERE title_id = ? ORDER BY source_entry_id, id",
    )
    .bind(title_id)
    .fetch_all(pool)
    .await?;
    Ok(ranges)
}

async fn load_chapters(pool: &SqlitePool, title_id: i64) -> Result<Vec<ChapterRow>, AppError> {
    let chapters = sqlx::query_as::<_, ChapterRow>(
        "SELECT ch.id, ch.source_entry_id, ch.chapter_key, ch.volume, ch.number_text, \
                ch.number_milli, ch.ordinal_site, ch.chapter_kind \
         FROM chapters ch JOIN source_entries e ON e.id = ch.source_entry_id \
         WHERE e.title_id = ? AND ch.removed_at IS NULL",
    )
    .bind(title_id)
    .fetch_all(pool)
    .await?;
    Ok(chapters)
}

async fn load_entries(pool: &SqlitePool, title_id: i64) -> Result<Vec<EntryRow>, AppError> {
    let entries = sqlx::query_as::<_, EntryRow>(
        "SELECT e.id, e.source_id, s.code, s.name \
         FROM source_entries e JOIN sources s ON s.id = e.source_id \
         WHERE e.title_id = ? ORDER BY e.id",
    )
    .bind(title_id)
    .fetch_all(pool)
    .await?;
    Ok(entries)
}

async fn load_generated_pairs(
    pool: &SqlitePool,
    title_id: i64,
) -> Result<Vec<(i64, String, String)>, AppError> {
    let pairs = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT source_entry_id, alias_key, canonical_key FROM chapter_aliases \
         WHERE title_id = ? AND origin = 'range' AND source_entry_id IS NOT NULL",
    )
    .bind(title_id)
    .fetch_all(pool)
    .await?;
    Ok(pairs)
}

async fn load_global_aliases(
    pool: &SqlitePool,
    title_id: i64,
) -> Result<HashMap<String, String>, AppError> {
    let aliases = crate::catalog::links::load_aliases(pool, title_id).await?;
    Ok(aliases
        .into_iter()
        .map(|alias| (alias.alias_key, alias.canonical_key))
        .collect())
}

/// Сворачивает старые точные alias в диапазоны при первом обращении к
/// списку связок. Преобразуются только непрерывные серии с одинаковым
/// смещением; одиночные и нерегулярные пары остаются ручными.
async fn backfill_if_needed(pool: &SqlitePool, title_id: i64) -> Result<(), AppError> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM chapter_alias_ranges WHERE title_id = ?",
    )
    .bind(title_id)
    .fetch_one(pool)
    .await?;
    if count > 0 {
        return Ok(());
    }

    let global = load_global_aliases(pool, title_id).await?;
    if global.is_empty() {
        return Ok(());
    }

    let chapters = load_chapters(pool, title_id).await?;
    let mut by_key: HashMap<String, Vec<&ChapterRow>> = HashMap::new();
    for chapter in &chapters {
        by_key
            .entry(chapter.chapter_key.clone())
            .or_default()
            .push(chapter);
    }
    for list in by_key.values_mut() {
        list.sort_by_key(|chapter| chapter.id);
    }

    let mut candidates: Vec<BackfillPair> = Vec::new();
    for (alias_key, canonical_key) in &global {
        let Some(alias) = by_key.get(alias_key).and_then(|list| list.first()) else {
            continue;
        };
        let Some(canonical) = by_key.get(canonical_key).and_then(|list| list.first()) else {
            continue;
        };
        if alias.source_entry_id == canonical.source_entry_id
            || alias.chapter_kind != canonical.chapter_kind
        {
            continue;
        }
        let (Some(alias_milli), Some(canonical_milli)) =
            (alias.number_milli, canonical.number_milli)
        else {
            continue;
        };
        if alias.number_text.is_none() || canonical.number_text.is_none() {
            continue;
        }
        let mapping_mode = match (alias.volume, canonical.volume) {
            (Some(_), Some(_)) => "volume_number",
            (None, None) => "number_only",
            _ => continue,
        };
        let volume_delta = match mapping_mode {
            "volume_number" => match (alias.volume, canonical.volume) {
                (Some(alias_volume), Some(canonical_volume)) => {
                    Some(canonical_volume - alias_volume)
                }
                _ => None,
            },
            _ => None,
        };
        candidates.push(BackfillPair {
            source_entry_id: alias.source_entry_id,
            chapter_kind: alias.chapter_kind.clone(),
            alias_order: chapter_order(alias),
            alias: (*alias).clone(),
            canonical: (*canonical).clone(),
            volume_delta,
            number_delta: canonical_milli - alias_milli,
            mapping_mode: mapping_mode.to_owned(),
        });
    }

    candidates.sort_by_key(|pair| {
        (
            pair.source_entry_id,
            pair.chapter_kind.clone(),
            pair.alias_order,
        )
    });

    let mut runs: Vec<Vec<&BackfillPair>> = Vec::new();
    for pair in &candidates {
        match runs.last_mut() {
            Some(run)
                if run.last().is_some_and(|previous| {
                    previous.source_entry_id == pair.source_entry_id
                        && previous.chapter_kind == pair.chapter_kind
                        && previous.volume_delta == pair.volume_delta
                        && previous.number_delta == pair.number_delta
                }) =>
            {
                run.push(pair);
            }
            _ => runs.push(vec![pair]),
        }
    }

    // Одиночная пара — не диапазон: оставляем её ручной, чтобы не связать
    // лишние главы выше единственного решения администратора.
    let runs: Vec<&Vec<&BackfillPair>> = runs.iter().filter(|run| run.len() >= 2).collect();
    if runs.is_empty() {
        return Ok(());
    }

    let mut tx = pool.begin().await?;
    for run in runs {
        let first = run[0];
        let insert = RangeInsert {
            source_entry_id: first.source_entry_id,
            anchor_chapter_id: first.alias.id,
            anchor_chapter_key: first.alias.chapter_key.clone(),
            anchor_volume: first.alias.volume,
            anchor_number_text: first.alias.number_text.clone(),
            anchor_number_milli: first.alias.number_milli,
            anchor_ordinal_site: first.alias.ordinal_site,
            canonical_key: first.canonical.chapter_key.clone(),
            canonical_chapter_id: first.canonical.id,
            canonical_volume: first.canonical.volume,
            canonical_number_text: first.canonical.number_text.clone(),
            canonical_number_milli: first.canonical.number_milli,
            chapter_kind: first.chapter_kind.clone(),
            mapping_mode: first.mapping_mode.clone(),
        };
        insert_range_tx(&mut tx, title_id, &insert).await?;

        // Пара вошла в диапазон: глобальная ручная запись больше не нужна,
        // иначе она останется в title.aliases и UI покажет её повторно.
        for pair in run {
            sqlx::query(
                "DELETE FROM chapter_aliases \
                 WHERE title_id = ? AND source_entry_id IS NULL AND alias_key = ?",
            )
            .bind(title_id)
            .bind(&pair.alias.chapter_key)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;

    materialize_title(pool, title_id).await?;
    Ok(())
}

struct BackfillPair {
    source_entry_id: i64,
    chapter_kind: String,
    alias_order: (i64, i64, i64, i64),
    alias: ChapterRow,
    canonical: ChapterRow,
    volume_delta: Option<i64>,
    number_delta: i64,
    mapping_mode: String,
}
