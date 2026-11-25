//! Общие текстовые helpers адаптеров: обрезка, нормализация пробелов, dedupe.

use std::collections::HashSet;

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

pub fn dedupe(values: &mut Vec<String>) {
    let mut seen = HashSet::new();
    values.retain(|value| seen.insert(value.to_lowercase()));
}
