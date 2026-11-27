//! Live-проверка RuMix. По умолчанию пропускается: `cargo test -- --ignored`.
//! Нужен интернет и живой сайт; в CI не запускается.

mod common;

use std::sync::Arc;

use reqwest::Url;
use serde_json::Value;

use axum::http::{Method, StatusCode, header};
use serde_json::json;

use cion_reader::sources::{
    SearchRequest, SourceContent,
    http::{AdapterCtx, metadata_client},
    rumix::{Rumix, RumixConfig},
};
use common::{TestApp, body_json, get, json_request, with_cookie};

#[tokio::test]
#[ignore = "live RuMix site; needs network"]
async fn rumix_search_title_chapters_content() {
    let client = metadata_client("cion-reader-live-test/0.1", false).expect("http client");
    let base = Url::parse("https://rumix.me").expect("base url");
    let adapter = Rumix::new(
        AdapterCtx::new(Arc::new(client), base),
        RumixConfig::default(),
    )
    .expect("adapter");

    let page = adapter
        .search(
            SearchRequest {
                query: "arifureta".to_owned(),
                cursor: None,
                filters: Value::Null,
            },
            None,
        )
        .await
        .expect("search");
    assert!(!page.items.is_empty(), "search must return items");

    // Первая выдача может быть спин-оффом с платными главами; берём
    // основной тайтл, который есть в фикстурах.
    let item = page
        .items
        .iter()
        .find(|item| item.external_id.as_str() == "arifureta__silneishii_remeslennik_v_mire")
        .unwrap_or(&page.items[0]);

    let title = adapter.title(&item.external_id, None).await.expect("title");
    assert!(!title.name.is_empty());

    let chapters = adapter
        .chapters(&item.external_id, None)
        .await
        .expect("chapters");
    assert!(!chapters.is_empty(), "title must have chapters");

    // Первая глава (c0) бесплатная и не запланирована; новейшие могут быть
    // платными и уводить на /internal/purchase.
    let chapter = chapters
        .iter()
        .find(|chapter| chapter.chapter_key == "v1:c0")
        .or_else(|| chapters.last())
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
            .is_some_and(|host| host.ends_with("rumix.me")),
        "{url}"
    );
}

/// Полный путь через роутер: источник → импорт → БД → content → прокси картинки.
#[tokio::test]
#[ignore = "live RuMix site and image CDN; needs network"]
async fn rumix_end_to_end_through_router() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;

    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/sources/rumix",
                &json!({ "base_url": "https://rumix.me", "enabled": true }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "configure rumix");

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/sources/rumix/search",
                &json!({ "query": "arifureta" }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let search = body_json(response).await;
    let item = search["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|item| item["external_id"] == "arifureta__silneishii_remeslennik_v_mire")
        .unwrap_or_else(|| &search["items"][0]);
    let external_id = item["external_id"].as_str().expect("external id");

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/discover/add",
                &json!({ "code": "rumix", "external_id": external_id }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "import");
    let imported = body_json(response).await;
    let title_id = imported["title_id"].as_i64().expect("title id");

    let response = app
        .request(with_cookie(
            get(&format!("/api/titles/{title_id}/chapters/v1:c0/content")),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "content");
    let content = body_json(response).await;
    assert_eq!(content["source"]["code"], "rumix");
    let page_id = content["content"]["pages"][0]["id"]
        .as_i64()
        .expect("page id");

    let response = app
        .request(with_cookie(
            get(&format!("/api/pages/{page_id}/image")),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "page image");
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/jpeg");
}
