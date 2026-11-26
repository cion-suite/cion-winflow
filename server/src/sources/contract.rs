//! Общий набор проверок адаптера. Каждый источник обязан пройти его
//! на фикстурах: дескриптор, уникальность ключей и стабильность DTO.

use std::collections::HashSet;

use crate::sources::{
    ChapterKind, ContentKind, SourceDescriptor, TitleKind, config::config_example,
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
    assert!(!descriptor.site_url.is_empty());
    assert!(!descriptor.lang.is_empty());
    assert!(descriptor.version > 0);
    if !descriptor.default_base_url.is_empty() {
        let url = reqwest::Url::parse(descriptor.default_base_url)
            .expect("default_base_url must be an absolute URL");
        assert!(
            matches!(url.scheme(), "http" | "https"),
            "default_base_url must be http(s)"
        );
    }

    let mut config_keys = HashSet::new();
    for field in descriptor.config {
        assert!(!field.key.is_empty());
        assert!(
            config_keys.insert(field.key),
            "duplicate config key {}",
            field.key
        );
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

    let search = super::remanga::parse::parse_search(include_str!("remanga/fixtures/search.json"))
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

    let title = super::remanga::parse::parse_title(include_str!("remanga/fixtures/title.json"))
        .expect("title fixture");
    assert_eq!(title.kind, TitleKind::Manga);
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

#[test]
fn mangamir_passes_contract() {
    descriptor_is_sane(&crate::sources::mangamir::DESCRIPTOR);

    let site = reqwest::Url::parse("https://mangamir.com").expect("site");
    let search =
        super::mangamir::parse::parse_search(include_str!("mangamir/fixtures/search.html"), &site)
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

    let external_id =
        crate::sources::ExternalId::new(search.items[0].external_id.as_str()).expect("external id");
    let title = super::mangamir::parse::parse_title(
        include_str!("mangamir/fixtures/title.html"),
        &site,
        &external_id,
        "ru",
    )
    .expect("title fixture");
    assert_eq!(title.kind, TitleKind::Manhwa);
    assert!(!title.name.is_empty());

    let chapters = super::mangamir::parse::parse_chapters(
        include_str!("mangamir/fixtures/title.html"),
        &site,
        "ru",
    )
    .expect("chapters fixture");
    assert!(!chapters.is_empty());

    let mut keys = HashSet::new();
    for chapter in &chapters {
        assert!(!chapter.chapter_key.is_empty());
        assert!(
            keys.insert(chapter.chapter_key.clone()),
            "duplicate chapter key"
        );
        assert!(
            chapter
                .number_text
                .as_deref()
                .is_none_or(|number| !number.contains(',')),
            "number must be normalized"
        );
        assert_eq!(chapter.content_kind, Some(ContentKind::Images));
    }
    assert!(
        chapters
            .iter()
            .any(|chapter| chapter.chapter_kind != ChapterKind::Normal),
        "fixture must cover a non-normal chapter"
    );
    assert!(
        chapters.iter().any(|chapter| chapter.number_text.is_none()),
        "fixture must cover a chapter without a number"
    );

    let repeated = super::mangamir::parse::parse_chapters(
        include_str!("mangamir/fixtures/title.html"),
        &site,
        "ru",
    )
    .expect("chapters fixture");
    let first_keys: Vec<&str> = chapters
        .iter()
        .map(|chapter| chapter.chapter_key.as_str())
        .collect();
    let repeated_keys: Vec<&str> = repeated
        .iter()
        .map(|chapter| chapter.chapter_key.as_str())
        .collect();
    assert_eq!(first_keys, repeated_keys, "chapter_key must be stable");

    let content =
        super::mangamir::parse::parse_content(include_str!("mangamir/fixtures/content.html"))
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
}

#[test]
fn rumix_passes_contract() {
    descriptor_is_sane(&crate::sources::rumix::DESCRIPTOR);

    let site = reqwest::Url::parse("https://rumix.me").expect("site");
    let search =
        super::rumix::parse::parse_search(include_str!("rumix/fixtures/search.html"), &site, 0)
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

    let external_id =
        crate::sources::ExternalId::new(search.items[0].external_id.as_str()).expect("external id");
    let title = super::rumix::parse::parse_title(
        include_str!("rumix/fixtures/title.html"),
        &site,
        &external_id,
        "ru",
    )
    .expect("title fixture");
    assert_eq!(title.kind, TitleKind::Manga);
    assert!(!title.name.is_empty());

    let chapters =
        super::rumix::parse::parse_chapters(include_str!("rumix/fixtures/title.html"), &site, "ru")
            .expect("chapters fixture");
    assert!(!chapters.is_empty());

    let mut keys = HashSet::new();
    for chapter in &chapters {
        assert!(!chapter.chapter_key.is_empty());
        assert!(
            keys.insert(chapter.chapter_key.clone()),
            "duplicate chapter key"
        );
        assert!(
            chapter
                .number_text
                .as_deref()
                .is_none_or(|number| !number.contains(',')),
            "number must be normalized"
        );
        assert_eq!(chapter.content_kind, Some(ContentKind::Images));
    }
    assert!(
        chapters
            .iter()
            .any(|chapter| chapter.chapter_kind != ChapterKind::Normal),
        "fixture must cover a non-normal chapter"
    );
    assert!(
        chapters.iter().any(|chapter| chapter.number_text.is_none()),
        "fixture must cover a chapter without a number"
    );

    let repeated =
        super::rumix::parse::parse_chapters(include_str!("rumix/fixtures/title.html"), &site, "ru")
            .expect("chapters fixture");
    let first_keys: Vec<&str> = chapters
        .iter()
        .map(|chapter| chapter.chapter_key.as_str())
        .collect();
    let repeated_keys: Vec<&str> = repeated
        .iter()
        .map(|chapter| chapter.chapter_key.as_str())
        .collect();
    assert_eq!(first_keys, repeated_keys, "chapter_key must be stable");

    let content = super::rumix::parse::parse_content(include_str!("rumix/fixtures/content.html"))
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
}
