//! Разбор экспорта закладок Remanga/ExManga в плоский список для импорта.
//!
//! Формат экспорта хранит статус числовым `type`, но у разных сайтов
//! собственные диапазоны: стандартный Remanga — 1..=7, ExManga — свой
//! непрерывный блок. Кастомные статусы лежат вне блока. Поэтому статус
//! определяется не по конкретному числу, а по позиции внутри найденного
//! блока из семи подряд идущих идентификаторов.

use std::collections::{BTreeMap, HashMap};

use serde::Deserialize;

use crate::{error::AppError, sources::text::normalize_name};

/// Больше этого экспорт не принимаем: импорт одного тайтла ходит в сеть,
/// и случайный файл на десятки тысяч строк занял бы сервер на часы.
pub const MAX_BOOKMARKS: usize = 5000;

/// Порядок встроенных статусов Remanga: type 1..=7.
const STANDARD: [&str; 7] = [
    "in_progress",
    "planned",
    "completed",
    "dropped",
    "on_hold",
    "not_interested",
    "favorite",
];

/// Тот же порядок внутри кастомного непрерывного блока: меньший id —
/// «Читаю», дальше по возрастанию.
const POSITIONAL: [&str; 7] = STANDARD;

const STANDARD_MAX: i64 = STANDARD.len() as i64;

#[derive(Debug, Deserialize)]
struct BookmarksFile {
    #[serde(default)]
    exmanga: Vec<RawBookmark>,
    #[serde(default)]
    remanga: Vec<RawBookmark>,
}

#[derive(Debug, Deserialize)]
struct RawBookmark {
    #[serde(rename = "type")]
    bookmark_type: Option<i64>,
    #[serde(default)]
    progress: i64,
    name: Option<String>,
    dir: Option<String>,
}

/// Статус закладки после разбора. Встроенный статус сразу переводится в
/// строку библиотеки, кастомный остаётся числом: по нему создаётся своя
/// закладка и запоминается `source_ref`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BookmarkStatus {
    Builtin(&'static str),
    Custom(i64),
}

#[derive(Debug, Clone)]
pub struct BookmarkItem {
    pub name: String,
    /// Нормализованное название: по нему склеиваются два экспорта.
    pub name_key: String,
    pub dir: String,
    /// Сколько глав прочитано. Отрицательные значения приводим к нулю.
    pub progress: i64,
    pub status: BookmarkStatus,
}

impl BookmarkItem {
    /// Статус библиотеки, если своя закладка создать не удастся.
    pub fn fallback_status(&self) -> &'static str {
        match self.status {
            BookmarkStatus::Builtin(status) => status,
            BookmarkStatus::Custom(_) if self.progress > 0 => "in_progress",
            BookmarkStatus::Custom(_) => "planned",
        }
    }
}

#[derive(Debug)]
pub struct ParseOutcome {
    pub items: Vec<BookmarkItem>,
    /// Строки, которые не удалось разобрать; они сразу попадают в ошибки
    /// задания, а не отменяют весь импорт.
    pub invalid: Vec<String>,
}

/// Разбирает файл и склеивает `exmanga`/`remanga` по названию/каталогу.
pub fn parse(bytes: &[u8]) -> Result<ParseOutcome, AppError> {
    let file: BookmarksFile = serde_json::from_slice(bytes).map_err(|error| {
        AppError::bad_request(
            "bookmarks_invalid",
            format!("cannot parse bookmarks json: {error}"),
        )
    })?;

    let (left, mut invalid) = parse_side("exmanga", file.exmanga);
    let (right, mut right_invalid) = parse_side("remanga", file.remanga);
    invalid.append(&mut right_invalid);

    if left.is_empty() && right.is_empty() && invalid.is_empty() {
        return Err(AppError::bad_request(
            "bookmarks_empty",
            "the export contains no bookmarks",
        ));
    }

    let items = merge(left, right);
    if items.is_empty() {
        let detail = invalid
            .first()
            .cloned()
            .unwrap_or_else(|| "no valid bookmarks".to_owned());
        return Err(AppError::bad_request(
            "bookmarks_invalid",
            format!("no valid bookmarks: {detail}"),
        ));
    }

    if items.len() + invalid.len() > MAX_BOOKMARKS {
        return Err(AppError::bad_request(
            "bookmarks_too_large",
            format!("at most {MAX_BOOKMARKS} bookmarks are allowed"),
        ));
    }

    Ok(ParseOutcome { items, invalid })
}

fn parse_side(section: &str, raws: Vec<RawBookmark>) -> (Vec<BookmarkItem>, Vec<String>) {
    let types: Vec<i64> = raws.iter().filter_map(|raw| raw.bookmark_type).collect();
    let mapper = StatusMapper::new(&types);

    // Внутри одного экспорта `dir` — идентификатор произведения: две записи
    // с одним dir склеиваем, оставляя больший прогресс.
    let mut by_dir: HashMap<String, usize> = HashMap::new();
    let mut items: Vec<BookmarkItem> = Vec::new();
    let mut invalid = Vec::new();

    for (index, raw) in raws.into_iter().enumerate() {
        let position = index + 1;
        let Some(bookmark_type) = raw.bookmark_type else {
            invalid.push(format!("{section} #{position}: missing type"));
            continue;
        };
        let name = raw.name.as_deref().map(str::trim).filter(|v| !v.is_empty());
        let dir = raw.dir.as_deref().map(str::trim).filter(|v| !v.is_empty());
        let (Some(name), Some(dir)) = (name, dir) else {
            invalid.push(format!("{section} #{position}: missing name or dir"));
            continue;
        };

        let item = BookmarkItem {
            name: name.to_owned(),
            name_key: normalize_name(name),
            dir: dir.to_owned(),
            progress: raw.progress.max(0),
            status: mapper.classify(bookmark_type),
        };

        if let Some(&existing) = by_dir.get(dir) {
            if item.progress > items[existing].progress {
                items[existing] = item;
            }
        } else {
            by_dir.insert(dir.to_owned(), items.len());
            items.push(item);
        }
    }

    (items, invalid)
}

/// Склеивает два экспорта. Сначала ищем тот же `dir`, затем — уникальное
/// название в обоих файлах (так просил перенос: искать по name). Одинаковые
/// названия разных произведений внутри одного файла не сливаются: у них
/// разные `dir`, и по имени пара неоднозначна.
fn merge(left: Vec<BookmarkItem>, right: Vec<BookmarkItem>) -> Vec<BookmarkItem> {
    if right.is_empty() {
        return left;
    }
    if left.is_empty() {
        return right;
    }

    let mut left_names: HashMap<String, usize> = HashMap::new();
    for item in &left {
        *left_names.entry(item.name_key.clone()).or_default() += 1;
    }
    let mut right_names: HashMap<String, usize> = HashMap::new();
    for item in &right {
        *right_names.entry(item.name_key.clone()).or_default() += 1;
    }

    let mut right_by_dir: HashMap<String, usize> = HashMap::new();
    let mut right_by_name: HashMap<String, usize> = HashMap::new();
    for (index, item) in right.iter().enumerate() {
        right_by_dir.insert(item.dir.clone(), index);
        if right_names.get(item.name_key.as_str()) == Some(&1) {
            right_by_name.insert(item.name_key.clone(), index);
        }
    }

    let mut used = vec![false; right.len()];
    let mut result = Vec::with_capacity(left.len() + right.len());

    for mut item in left {
        let matched = right_by_dir.get(item.dir.as_str()).copied().or_else(|| {
            let unique_here = left_names.get(item.name_key.as_str()) == Some(&1);
            if unique_here {
                right_by_name.get(item.name_key.as_str()).copied()
            } else {
                None
            }
        });

        if let Some(index) = matched {
            used[index] = true;
            if right[index].progress > item.progress {
                item = right[index].clone();
            }
        }
        result.push(item);
    }

    for (index, item) in right.into_iter().enumerate() {
        if !used[index] {
            result.push(item);
        }
    }

    result
}

/// Определяет встроенный блок `type` и переводит статусы.
struct StatusMapper {
    /// Начало непрерывного блока из семи кастомных id, если он есть.
    block_start: Option<i64>,
}

impl StatusMapper {
    fn new(types: &[i64]) -> Self {
        let mut counts: BTreeMap<i64, usize> = BTreeMap::new();
        for &value in types {
            *counts.entry(value).or_default() += 1;
        }

        // BTreeMap даёт детерминированный порядок: при равном счёте
        // выбираем блок с меньшим началом, а не случайный из HashMap.
        let mut best: Option<(usize, i64)> = None;
        for &start in counts.keys() {
            if start <= STANDARD_MAX {
                continue;
            }
            let mut score = 0;
            let mut complete = true;
            for offset in 0..POSITIONAL.len() as i64 {
                match counts.get(&(start + offset)) {
                    Some(&count) => score += count,
                    None => {
                        complete = false;
                        break;
                    }
                }
            }
            if !complete {
                continue;
            }
            if best.is_none_or(|(best_score, _)| score > best_score) {
                best = Some((score, start));
            }
        }

        Self {
            block_start: best.map(|(_, start)| start),
        }
    }

    fn classify(&self, value: i64) -> BookmarkStatus {
        if (1..=STANDARD_MAX).contains(&value) {
            return BookmarkStatus::Builtin(STANDARD[(value - 1) as usize]);
        }
        if let Some(start) = self.block_start {
            let offset = value - start;
            if (0..POSITIONAL.len() as i64).contains(&offset) {
                return BookmarkStatus::Builtin(POSITIONAL[offset as usize]);
            }
        }
        BookmarkStatus::Custom(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_file(value: serde_json::Value) -> ParseOutcome {
        parse(&serde_json::to_vec(&value).expect("json")).expect("parse")
    }

    fn item(name: &str, dir: &str, bookmark_type: i64, progress: i64) -> serde_json::Value {
        serde_json::json!({
            "type": bookmark_type,
            "progress": progress,
            "name": name,
            "dir": dir,
        })
    }

    #[test]
    fn standard_remanga_types_map_in_order() {
        let types = [1, 2, 3, 4, 5, 6, 7];
        let expected = [
            "in_progress",
            "planned",
            "completed",
            "dropped",
            "on_hold",
            "not_interested",
            "favorite",
        ];
        let entries: Vec<_> = types
            .iter()
            .zip(expected)
            .map(|(bookmark_type, _)| item("t", "d", *bookmark_type, 0))
            .collect();
        // У каждого произведения свой dir, иначе они склеятся.
        let mut entries = entries;
        for (index, entry) in entries.iter_mut().enumerate() {
            entry["dir"] = serde_json::json!(format!("d{index}"));
        }
        let outcome = parse_file(serde_json::json!({ "exmanga": entries }));
        assert_eq!(outcome.items.len(), 7);
        for (entry, status) in outcome.items.iter().zip(expected) {
            assert_eq!(entry.status, BookmarkStatus::Builtin(status));
        }
    }

    #[test]
    fn custom_block_is_mapped_by_position_and_custom_status_stays_custom() {
        let types = [
            50120392, 50120393, 50120394, 50120395, 50120396, 50120397, 50120398, 34956, 2, 3,
        ];
        let entries: Vec<_> = types
            .iter()
            .enumerate()
            .map(|(index, bookmark_type)| item("t", &format!("d{index}"), *bookmark_type, 0))
            .collect();
        let outcome = parse_file(serde_json::json!({ "exmanga": entries }));

        let statuses: Vec<&BookmarkStatus> =
            outcome.items.iter().map(|item| &item.status).collect();
        assert_eq!(
            statuses,
            vec![
                &BookmarkStatus::Builtin("in_progress"),
                &BookmarkStatus::Builtin("planned"),
                &BookmarkStatus::Builtin("completed"),
                &BookmarkStatus::Builtin("dropped"),
                &BookmarkStatus::Builtin("on_hold"),
                &BookmarkStatus::Builtin("not_interested"),
                &BookmarkStatus::Builtin("favorite"),
                &BookmarkStatus::Custom(34956),
                &BookmarkStatus::Builtin("planned"),
                &BookmarkStatus::Builtin("completed"),
            ]
        );
    }

    #[test]
    fn same_side_same_name_different_dirs_are_kept() {
        let outcome = parse_file(serde_json::json!({
            "exmanga": [
                item("Игрок", "player_", 50120392, 60),
                item("Игрок", "the_gamer_", 50120393, 0),
            ]
        }));
        assert_eq!(outcome.items.len(), 2);
    }

    #[test]
    fn two_exports_merge_by_name_and_keep_larger_progress() {
        let outcome = parse_file(serde_json::json!({
            "exmanga": [
                item("Same", "same", 50120392, 5),
                item("Other", "other", 50120393, 1),
            ],
            "remanga": [
                item("Same", "same", 1, 9),
                item("Second", "second", 2, 0),
            ]
        }));
        assert_eq!(outcome.items.len(), 3);
        let same = outcome
            .items
            .iter()
            .find(|item| item.name == "Same")
            .expect("same");
        assert_eq!(same.progress, 9);
        assert_eq!(same.status, BookmarkStatus::Builtin("in_progress"));
        assert!(outcome.items.iter().any(|item| item.name == "Other"));
        assert!(outcome.items.iter().any(|item| item.name == "Second"));
    }

    #[test]
    fn two_exports_merge_by_unique_name_when_dirs_differ() {
        let outcome = parse_file(serde_json::json!({
            "exmanga": [
                item("Same", "mirror-dir", 50120392, 5),
            ],
            "remanga": [
                item("Same", "remanga-dir", 1, 9),
            ]
        }));
        assert_eq!(outcome.items.len(), 1);
        assert_eq!(outcome.items[0].progress, 9);
        assert_eq!(outcome.items[0].dir, "remanga-dir");
    }

    #[test]
    fn ambiguous_name_is_not_merged_across_exports() {
        let outcome = parse_file(serde_json::json!({
            "exmanga": [
                item("Игрок", "player_", 50120392, 60),
                item("Игрок", "the_gamer_", 50120393, 0),
            ],
            "remanga": [
                item("Игрок", "player_", 1, 61),
            ]
        }));
        assert_eq!(outcome.items.len(), 2);
        let player = outcome
            .items
            .iter()
            .find(|item| item.dir == "player_")
            .expect("player");
        assert_eq!(player.progress, 61);
    }

    #[test]
    fn invalid_records_are_reported_without_failing_the_file() {
        let outcome = parse_file(serde_json::json!({
            "exmanga": [
                item("Good", "good", 50120392, 0),
                { "progress": 1, "name": "No dir", "type": 50120393 },
                { "progress": 1, "name": "No type", "dir": "x" },
            ]
        }));
        assert_eq!(outcome.items.len(), 1);
        assert_eq!(outcome.invalid.len(), 2);
    }

    #[test]
    fn empty_export_is_rejected() {
        let error = parse(br#"{"exmanga":[],"remanga":[]}"#).expect_err("empty");
        assert_eq!(error.to_string(), "the export contains no bookmarks");
    }
}
