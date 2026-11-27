//! Live-проверка Mangamir. По умолчанию пропускается:
//! `cargo test --test live_mangamir -- --ignored`. Нужен интернет.

use std::sync::Arc;

use reqwest::Url;
use serde_json::Value;

use cion_reader::sources::{
    ExternalId, SearchRequest, SourceContent,
    http::{AdapterCtx, metadata_client},
    mangamir::{Mangamir, MangamirConfig},
};

#[tokio::test]
#[ignore = "live Mangamir site; needs network"]
async fn mangamir_search_title_chapters_content() {
    let client = metadata_client("cion-reader-live-test/0.1", false).expect("http client");
    let base = Url::parse("https://mangamir.com").expect("base url");
    let adapter = Mangamir::new(
        AdapterCtx::new(Arc::new(client), base),
        MangamirConfig::default(),
    )
    .expect("adapter");

    let page = adapter
        .search(
            SearchRequest {
                query: "Во всеоружии".to_owned(),
                cursor: None,
                filters: Value::Null,
            },
            None,
        )
        .await
        .expect("search");
    assert!(!page.items.is_empty(), "search must return items");

    // Строго конкретный тайтл: иначе тест мог бы молча проверять другой
    // результат поиска и не заметить, что полный список перестал приходить.
    let item = page
        .items
        .iter()
        .find(|item| item.external_id.as_str() == "vo-vseoruzhii")
        .expect("search must return vo-vseoruzhii");

    let title = adapter.title(&item.external_id, None).await.expect("title");
    assert!(!title.name.is_empty());
    assert!(title.covers.high.is_some());

    let chapters = adapter
        .chapters(&item.external_id, None)
        .await
        .expect("chapters");
    println!("{}: {} chapters", item.external_id, chapters.len());
    assert!(!chapters.is_empty(), "title must have chapters");
    // Свёрнутый список сайта показывает только первые и последние главы;
    // адаптер обязан запросить полный список через Livewire.
    assert!(
        chapters.len() > 20,
        "expected the full chapter list, got {}",
        chapters.len()
    );

    let chapter = chapters
        .iter()
        .find(|chapter| chapter.chapter_key == "v1:c0")
        .or_else(|| chapters.first())
        .expect("readable chapter");
    let content = adapter.content(chapter, None).await.expect("content");
    let SourceContent::Images { pages } = content else {
        panic!("expected images");
    };
    assert!(!pages.is_empty());

    let url = adapter
        .resolve_image(&pages[0].image_ref)
        .expect("image url");
    assert!(
        url.host_str()
            .is_some_and(|host| host.ends_with("mangamir.com")),
        "{url}"
    );
}

/// Длинный тайтл: свёрнутый список обрывается, полный приходит Livewire.
#[tokio::test]
#[ignore = "live Mangamir site; needs network"]
async fn mangamir_long_title_has_all_chapters() {
    let client = metadata_client("cion-reader-live-test/0.1", false).expect("http client");
    let base = Url::parse("https://mangamir.com").expect("base url");
    let adapter = Mangamir::new(
        AdapterCtx::new(Arc::new(client), base),
        MangamirConfig::default(),
    )
    .expect("adapter");

    let external_id = ExternalId::new("hroniki-demonicheskogo-kulta").expect("external id");
    let title = adapter.title(&external_id, None).await.expect("title");
    assert_eq!(title.name, "Хроники Демонического Культа");

    let chapters = adapter
        .chapters(&external_id, None)
        .await
        .expect("chapters");
    assert!(chapters.len() >= 198, "got {}", chapters.len());
    // Ключ совпадает с Remanga: те же "Том 1 Глава N" -> v1:cN.
    assert!(
        chapters
            .iter()
            .any(|chapter| chapter.chapter_key == "v1:c198"),
        "missing v1:c198"
    );
}
