//! Live-проверка ExManga. По умолчанию пропускается:
//! `EXMANGA_EMAIL=... EXMANGA_PASSWORD=... cargo test --test live_exmanga -- --ignored`.
//!
//! Проверяет реальный `/api/auth/login`, список глав по `dir` и страницы
//! главы. Главное — ни одна страница не должна оказаться на хосте Remanga:
//! адаптер обязан ходить только на ExManga.

use std::sync::Arc;

use reqwest::Url;
use secrecy::SecretString;

use serde_json::Value;

use cion_reader::{
    source_auth::SourceAuth,
    sources::{
        ExternalId, SearchRequest, SourceContent,
        exmanga::{Exmanga, ExmangaConfig},
        http::{AdapterCtx, metadata_client},
    },
};

const DEFAULT_DIR: &str = "how-the-demon-emperor-became-a-butler";

fn live_adapter() -> Exmanga {
    let client = metadata_client("cion-reader-live-test/0.1", false).expect("http client");
    let base = Url::parse("https://exmanga.org").expect("base url");
    Exmanga::new(
        AdapterCtx::new(Arc::new(client), base),
        ExmangaConfig::default(),
    )
    .expect("adapter")
}

/// Публичные ручки не требуют учётки: проверяются без секретов.
#[tokio::test]
#[ignore = "live ExManga public API; needs network"]
async fn exmanga_live_public_search_title_and_chapters() {
    let adapter = live_adapter();

    let page = adapter
        .search(
            SearchRequest {
                query: "демон".to_owned(),
                cursor: None,
                filters: Value::Null,
            },
            None,
        )
        .await
        .expect("search");
    assert!(!page.items.is_empty(), "search must return items");

    let external = ExternalId::new(DEFAULT_DIR).expect("title dir");
    let title = adapter.title(&external, None).await.expect("title");
    assert_eq!(title.external_id.as_str(), DEFAULT_DIR);
    assert!(!title.name.is_empty());

    let chapters = adapter.chapters(&external, None).await.expect("chapters");
    assert!(chapters.len() > 100, "history must be the full list");
    assert!(
        chapters
            .iter()
            .any(|chapter| chapter.chapter_key == "v3:c936"),
        "known paid chapter must share Remanga's key"
    );
}

#[tokio::test]
#[ignore = "live ExManga; needs EXMANGA_EMAIL and EXMANGA_PASSWORD"]
async fn exmanga_live_login_and_content() {
    let email = std::env::var("EXMANGA_EMAIL").expect("EXMANGA_EMAIL must be set");
    let password = std::env::var("EXMANGA_PASSWORD").expect("EXMANGA_PASSWORD must be set");
    let dir = std::env::var("EXMANGA_DIR").unwrap_or_else(|_| DEFAULT_DIR.to_owned());

    let adapter = live_adapter();

    let login = adapter
        .login(&email, &SecretString::from(password))
        .await
        .expect("login");
    let auth = SourceAuth::new(login.access_token, None);
    adapter.verify(Some(&auth)).await.expect("verify");

    let external = ExternalId::new(dir).expect("title dir");
    let title = adapter.title(&external, Some(&auth)).await.expect("title");
    assert!(!title.name.is_empty());

    let chapters = adapter
        .chapters(&external, Some(&auth))
        .await
        .expect("chapters");
    assert!(!chapters.is_empty(), "title must have chapters");

    // Самая свежая глава: у ExManga полный список, но контент есть только
    // у загруженных в пул — свежая глава должна быть доступна.
    let chapter = chapters
        .iter()
        .max_by_key(|chapter| chapter.number_milli.unwrap_or_default())
        .expect("chapter");
    let content = adapter
        .content(chapter, Some(&auth))
        .await
        .expect("content");
    let SourceContent::Images { pages } = content else {
        panic!("expected images");
    };
    assert!(!pages.is_empty());

    for page in pages {
        let url = adapter
            .resolve_image(&page.image_ref)
            .expect("page image must pass allowlist");
        let host = url.host_str().unwrap_or_default();
        assert!(
            host == "exmanga.org" || host.ends_with(".exmanga.org"),
            "page must come from ExManga, got {host}"
        );
        assert!(
            !host.contains("remanga") && !host.contains("reimg"),
            "adapter must never use Remanga hosts, got {host}"
        );
    }
}
