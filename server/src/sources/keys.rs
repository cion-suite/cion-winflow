//! Канонический `chapter_key` и нормализация номера главы.
//! Одна семантика для всех адаптеров: план §7.3, решение 0002.

use sha2::{Digest, Sha256};

use crate::sources::dto::{ChapterKey, ChapterKind, ExternalId};

/// `chapter_key` по §7.3: том и вид главы — префиксы, без номера — стабильный хеш.
///
/// Ключ строит адаптер: канонический слой только хранит его и фиксирует при
/// первом импорте, поэтому повторный импорт не должен менять результат.
pub fn build_chapter_key(
    volume: Option<u16>,
    kind: ChapterKind,
    number_text: Option<&str>,
    external_id: &ExternalId,
) -> ChapterKey {
    let volume_segment = volume.map_or_else(String::new, |volume| format!("v{volume}:"));
    let kind_segment = if kind == ChapterKind::Normal {
        String::new()
    } else {
        format!("{}:", kind.as_str())
    };

    match number_text.filter(|number| !number.is_empty()) {
        Some(number) => format!("{volume_segment}{kind_segment}c{number}"),
        None => {
            let hash = hex::encode(Sha256::digest(external_id.as_str().as_bytes()));
            format!("{volume_segment}{kind_segment}x:h{}", &hash[..16])
        }
    }
}

/// «, » → «.», остаются только цифры, точка и дефис: безопасно для URL и БД.
pub fn normalize_number(raw: &str) -> String {
    raw.trim()
        .replace(',', ".")
        .chars()
        .filter(|character| character.is_ascii_digit() || *character == '.' || *character == '-')
        .collect()
}

/// «12.5» → 12500, «1-2» → 1000 (начало диапазона), без номера — None.
pub fn number_milli(text: &str) -> Option<i64> {
    let head = text.split('-').next()?.trim();
    if head.is_empty() {
        return None;
    }
    let mut parts = head.splitn(2, '.');
    let integer = parts.next()?.parse::<i64>().ok()?;
    let fraction = parts.next().unwrap_or("");
    let fraction_milli = if fraction.is_empty() {
        0
    } else {
        let padded = format!("{fraction}000");
        padded.get(..3)?.parse::<i64>().ok()?
    };
    Some(integer * 1000 + fraction_milli)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn external(value: &str) -> ExternalId {
        ExternalId::new(value).expect("external id")
    }

    #[test]
    fn builds_volume_kind_and_number_segments() {
        let id = external("123");

        assert_eq!(
            build_chapter_key(None, ChapterKind::Normal, Some("12.5"), &id),
            "c12.5"
        );
        assert_eq!(
            build_chapter_key(Some(3), ChapterKind::Normal, Some("0"), &id),
            "v3:c0"
        );
        assert_eq!(
            build_chapter_key(Some(1), ChapterKind::Extra, Some("7"), &id),
            "v1:extra:c7"
        );

        let special = build_chapter_key(Some(2), ChapterKind::Special, None, &id);
        assert!(special.starts_with("v2:special:x:h"), "{special}");
        assert_eq!(special.len(), "v2:special:x:h".len() + 16);
    }

    #[test]
    fn no_number_key_is_stable_and_depends_on_external_id() {
        let first = build_chapter_key(None, ChapterKind::Oneshot, None, &external("chapter-a"));
        let again = build_chapter_key(None, ChapterKind::Oneshot, None, &external("chapter-a"));
        let other = build_chapter_key(None, ChapterKind::Oneshot, None, &external("chapter-b"));

        assert_eq!(first, again);
        assert_ne!(first, other);
    }

    #[test]
    fn normalizes_numbers_and_ranges() {
        assert_eq!(normalize_number(" 1,5 "), "1.5");
        assert_eq!(normalize_number("1 - 2"), "1-2");
        assert_eq!(normalize_number("Спешл"), "");

        assert_eq!(number_milli("12.5"), Some(12_500));
        assert_eq!(number_milli("1-2"), Some(1_000));
        assert_eq!(number_milli("0"), Some(0));
        assert_eq!(number_milli("x"), None);
    }
}
