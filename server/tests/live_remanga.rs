//! Live-проверка Remanga v2. По умолчанию пропускается: `cargo test -- --ignored`.
//! Нужен интернет и живой API; в CI не запускается.

use std::sync::Arc;

use reqwest::Url;
use serde_json::Value;

use cion_reader::sources::{
    SearchRequest, SourceContent,
    http::{AdapterCtx, metadata_client},
    remanga::{Remanga, RemangaConfig},
};

#[tokio::test]
#[ignore = "live Remanga API; needs network"]
async fn remanga_search_title_chapters_content() {
    let client = metadata_client("cion-reader-live-test/0.1", false).expect("http client");
    let base = Url::parse("https://api.remanga.org").expect("base url");
    let adapter = Remanga::new(
        AdapterCtx::new(Arc::new(client), base),
        RemangaConfig::default(),
    )
    .expect("adapter");

    let page = adapter
        .search(SearchRequest {
            query: "naruto".to_owned(),
            cursor: None,
            filters: Value::Null,
        })
        .await
        .expect("search");
    assert!(!page.items.is_empty(), "search must return items");

    // Выдача отдаёт ссылку на обложку; она должна разрешаться allowlist'ом.
    if let Some(item) = page.items.iter().find(|item| item.cover_ref.is_some())
        && let Some(reference) = item.cover_ref.as_deref()
    {
        adapter.resolve_cover(reference).expect("search cover url");
    }

    let first = &page.items[0];
    let title = adapter.title(&first.external_id).await.expect("title");
    assert!(!title.name.is_empty());

    // Все варианты обложки из ответа `titles/{dir}` разрешаются в URL.
    assert!(!title.covers.is_empty(), "title must expose covers");
    for (size, reference) in title.covers.entries() {
        let url = adapter
            .resolve_cover(reference)
            .unwrap_or_else(|error| panic!("{size} cover must resolve: {error}"));
        assert!(url.host_str().is_some(), "{size} cover has no host");
    }

    let chapters = adapter
        .chapters(&first.external_id)
        .await
        .expect("chapters");
    assert!(!chapters.is_empty(), "title must have chapters");

    let chapter = chapters
        .iter()
        .find(|chapter| !chapter.is_locked)
        .expect("at least one unlocked chapter");

    let content = adapter.content(chapter).await.expect("content");
    match content {
        SourceContent::Images { pages } => assert!(!pages.is_empty()),
        other => panic!("expected images, got {other:?}"),
    }
}
