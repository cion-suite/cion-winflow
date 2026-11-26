//! Общие текстовые helpers адаптеров: обрезка, нормализация пробелов, dedupe.

use std::collections::HashSet;

use crate::sources::dto::ChapterKind;

pub fn truncate_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_owned()
    } else {
        value.chars().take(max).collect()
    }
}

pub fn normalize_ws(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Ключ сравнения названий тайтлов: регистр и лишние пробелы не важны.
pub fn normalize_name(value: &str) -> String {
    normalize_ws(&value.to_lowercase())
}

pub fn dedupe(values: &mut Vec<String>) {
    let mut seen = HashSet::new();
    values.retain(|value| seen.insert(value.to_lowercase()));
}

/// Вид главы по подписи источника. Общий для HTML-адаптеров: ключевые
/// слова не зависят от сайта.
pub fn detect_chapter_kind(text: &str) -> ChapterKind {
    let lower = text.to_lowercase();
    if lower.contains("экстра") || lower.contains("extra") {
        ChapterKind::Extra
    } else if lower.contains("ваншот") || lower.contains("one shot") || lower.contains("one-shot")
    {
        ChapterKind::Oneshot
    } else if lower.contains("спешл") || lower.contains("special") || lower.contains("спецвыпуск")
    {
        ChapterKind::Special
    } else if lower.contains("превью") || lower.contains("preview") {
        ChapterKind::Preview
    } else {
        ChapterKind::Normal
    }
}
