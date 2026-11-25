//! Общий набор проверок адаптера. Каждый источник обязан пройти его
//! на фикстурах: дескриптор, уникальность ключей и стабильность DTO.

use std::collections::HashSet;

use crate::sources::{
    ChapterKind, ContentKind, MediaKind, SourceDescriptor, config::config_example,
    descriptor::FieldType,
};

fn descriptor_is_sane(descriptor: &SourceDescriptor) {
    assert!(!descriptor.code.is_empty());
    assert!(
        descriptor
            .code
            .chars()
            .all(|character| character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'),
        "code must be url-friendly: {}",
        descriptor.code
    );
    assert!(!descriptor.name.is_empty());
    assert!(descriptor.version > 0);
    assert!(!descriptor.kinds.is_empty());
    assert!(
        descriptor.kinds.contains(&descriptor.default_kind),
        "default_kind must be one of kinds"
    );

    let mut config_keys = HashSet::new();
    for field in descriptor.config {
        assert!(!field.key.is_empty());
        assert!(
            config_keys.insert(field.key),
            "duplicate config key {}",
            field.key
        );
        for kind in field.applies_to {
            assert!(
                descriptor.kinds.contains(kind),
                "applies_to kind `{kind}` is not declared by source"
            );
        }
        if let FieldType::Select(options) | FieldType::MultiSelect(options) = field.kind {
            assert!(!options.is_empty(), "empty options for {}", field.key);
        }
    }

    let mut filter_keys = HashSet::new();
    for field in descriptor.filters {
        assert!(
            filter_keys.insert(field.key),
            "duplicate filter key {}",
            field.key
        );
    }

    // Возможность без базовой бессмысленна
    if descriptor.has(crate::sources::Capability::Images)
        || descriptor.has(crate::sources::Capability::Text)
        || descriptor.has(crate::sources::Capability::VideoEmbed)
    {
        assert!(descriptor.has(crate::sources::Capability::Chapters));
    }
    if descriptor.has(crate::sources::Capability::SearchFilters) {
        assert!(descriptor.has(crate::sources::Capability::Search));
    }

    // Схема конфига должна порождать JSON-объект
    assert!(config_example(descriptor).is_object());
}

#[test]
fn remanga_passes_contract() {
    descriptor_is_sane(&crate::sources::remanga::DESCRIPTOR);

    let search = super::remanga::parse::parse_search(
        include_str!("remanga/fixtures/search.json"),
        MediaKind::Manga,
    )
    .expect("search fixture");
    assert!(!search.items.is_empty());
    let mut external_ids = HashSet::new();
    for item in &search.items {
        assert!(!item.name.is_empty());
        assert!(
            external_ids.insert(item.external_id.clone()),
            "duplicate external id"
        );
    }

    let title = super::remanga::parse::parse_title(
        include_str!("remanga/fixtures/title.json"),
        MediaKind::Manga,
    )
    .expect("title fixture");
    assert_eq!(title.kind, MediaKind::Manga);
    assert!(!title.name.is_empty());

    let chapters = super::remanga::parse::parse_chapters(
        include_str!("remanga/fixtures/chapters.json"),
        "ru",
        None,
    )
    .expect("chapters fixture");
    assert!(!chapters.items.is_empty());

    let mut keys = HashSet::new();
    for chapter in &chapters.items {
        assert!(!chapter.chapter_key.is_empty());
        assert!(
            keys.insert(chapter.chapter_key.clone()),
            "duplicate chapter key"
        );
        assert_eq!(chapter.chapter_kind, ChapterKind::Normal);
        if let Some(number) = &chapter.number_text {
            assert!(!number.contains(','), "number must be normalized");
        }
    }

    let repeated = super::remanga::parse::parse_chapters(
        include_str!("remanga/fixtures/chapters.json"),
        "ru",
        None,
    )
    .expect("chapters fixture");
    let repeated_keys: Vec<&str> = repeated
        .items
        .iter()
        .map(|c| c.chapter_key.as_str())
        .collect();
    let first_keys: Vec<&str> = chapters
        .items
        .iter()
        .map(|c| c.chapter_key.as_str())
        .collect();
    assert_eq!(first_keys, repeated_keys, "chapter_key must be stable");

    let content =
        super::remanga::parse::parse_content(include_str!("remanga/fixtures/content.json"))
            .expect("content fixture");
    assert_eq!(content.kind(), ContentKind::Images);
    let crate::sources::SourceContent::Images { pages } = content else {
        panic!("expected images");
    };
    assert!(!pages.is_empty());
    for (position, page) in pages.iter().enumerate() {
        assert_eq!(page.index as usize, position);
        assert!(!page.image_ref.is_empty());
    }

    // Известный content_kind должен совпадать с фактическим ответом
    for chapter in &chapters.items {
        if let Some(expected) = chapter.content_kind {
            assert_eq!(expected, ContentKind::Images);
        }
    }
}
