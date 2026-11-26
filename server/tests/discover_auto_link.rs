//! Добавление тайтла с одного источника автоматически привязывает его копии
//! с других включённых источников по основному или альтернативным названиям.

use std::collections::{HashMap, HashSet};

use axum::{
    Router,
    body::Bytes,
    extract::{Query, State},
    http::{Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};

use cion_reader::sources::mangamir::parse::{
    livewire_chapters, livewire_fragments, page_is_collapsed,
};

mod common;
use common::{TestApp, body_json, get as http_get, json_request, with_cookie};

const SEARCH_MAIN: &str = r#"<!doctype html>
<html><head></head><body>
<script type="application/ld+json">
{"@type":"ItemList","numberOfItems":1,"itemListElement":[{"@type":"ListItem","name":"Fixture Title","url":"https://mangamir.com/manga/fixture-title"}]}
</script>
</body></html>"#;

/// Основного названия на источнике нет, но альтернативное совпадает.
const SEARCH_ALT: &str = r#"<!doctype html>
<html><head></head><body>
<script type="application/ld+json">
{"@type":"ItemList","numberOfItems":1,"itemListElement":[{"@type":"ListItem","name":"Fixture Alt","url":"https://mangamir.com/manga/fixture-title"}]}
</script>
</body></html>"#;

const SEARCH_EMPTY: &str = r#"<!doctype html>
<html><head></head><body>
<script type="application/ld+json">
{"@type":"ItemList","numberOfItems":0,"itemListElement":[]}
</script>
</body></html>"#;

const PAGE: &str = include_str!("../src/sources/mangamir/fixtures/livewire_title.html");
const UPDATE: &str = include_str!("../src/sources/mangamir/fixtures/livewire_update.json");

/// `alt_only` прячет основной результат: авто-привязка обязана перейти
/// ко второму проходу — по альтернативным названиям.
async fn search_page(
    State(alt_only): State<bool>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let query = params.get("q").map(String::as_str).unwrap_or_default();
    let body = match (alt_only, query) {
        (true, "Fixture Title") => SEARCH_EMPTY,
        (true, _) => SEARCH_ALT,
        (false, _) => SEARCH_MAIN,
    };

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
}

async fn chapter_page() -> Response {
    // В фикстуре адрес Livewire абсолютный; тест подменяет его относительным.
    let body = PAGE.replace(
        r#""https:\/\/mangamir.com\/livewire-e4136179\/update""#,
        r#""/livewire-test/update""#,
    );
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
        .into_response()
}

async fn update(_body: Bytes) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        UPDATE,
    )
        .into_response()
}

async fn start_mock(alt_only: bool) -> String {
    let router = Router::new()
        .route("/manga", get(search_page))
        .route("/manga/{slug}", get(chapter_page))
        .route("/livewire-test/update", post(update))
        .with_state(alt_only);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    format!("http://{address}")
}

async fn configure_source(app: &TestApp, admin: &str, code: &str, base_url: &str) {
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/sources/{code}"),
                &json!({ "base_url": base_url, "enabled": true }),
            ),
            admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "configure {code}");
}

async fn get_json(app: &TestApp, uri: &str, cookie: &str) -> Value {
    let response = app.request(with_cookie(http_get(uri), cookie)).await;
    body_json(response).await
}

async fn assert_auto_link(alt_only: bool) {
    let mock = start_mock(alt_only).await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;

    configure_source(&app, &admin, "fixture", &mock).await;
    configure_source(&app, &admin, "mangamir", &mock).await;

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/discover/add",
                &json!({ "code": "fixture", "external_id": "fixture-title" }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let added = body_json(response).await;
    let title_id = added["title_id"].as_i64().expect("title id");

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    let sources = title["sources"].as_array().expect("sources");
    let codes: HashSet<&str> = sources
        .iter()
        .filter_map(|source| source["code"].as_str())
        .collect();
    assert!(codes.contains("fixture"), "{codes:?}");
    assert!(
        codes.contains("mangamir"),
        "the second source must be attached automatically: {codes:?}"
    );

    let mangamir = sources
        .iter()
        .find(|source| source["code"] == "mangamir")
        .expect("mangamir source");
    assert_eq!(mangamir["chapter_count"], 3);
}

#[tokio::test]
async fn add_auto_links_by_main_name() {
    assert_auto_link(false).await;
}

#[tokio::test]
async fn add_auto_links_by_alternative_name_when_main_name_is_missing() {
    assert_auto_link(true).await;
}

/// Реальный сценарий: тайтл с Remanga, копия с Mangamir привязывается сама.
#[tokio::test]
#[ignore = "live Remanga + Mangamir; needs network"]
async fn live_add_auto_links_other_sources() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;

    configure_source(&app, &admin, "remanga", "https://api.remanga.org").await;
    configure_source(&app, &admin, "mangamir", "https://mangamir.com").await;

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/discover/add",
                &json!({
                    "code": "remanga",
                    "external_id": "<29.04.2026>magical-reincarnation",
                }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let added = body_json(response).await;
    let title_id = added["title_id"].as_i64().expect("title id");

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    let sources = title["sources"].as_array().expect("sources");
    let codes: HashSet<&str> = sources
        .iter()
        .filter_map(|source| source["code"].as_str())
        .collect();
    assert!(codes.contains("remanga"), "{codes:?}");
    assert!(
        codes.contains("mangamir"),
        "Mangamir copy must be linked automatically: {codes:?}"
    );

    let mangamir = sources
        .iter()
        .find(|source| source["code"] == "mangamir")
        .expect("mangamir source");
    assert!(
        mangamir["chapter_count"].as_i64().unwrap_or_default() >= 198,
        "{mangamir}"
    );
}

#[test]
fn collapsed_pages_are_detected() {
    // Фикстуры и разбор ответа Livewire проверяются здесь же: тест выше
    // опирается на них целиком.
    assert!(page_is_collapsed(PAGE));

    let site = reqwest::Url::parse("https://mangamir.com").expect("site");
    let livewire = livewire_chapters(PAGE, &site)
        .expect("parse")
        .expect("livewire component");
    assert!(!livewire.showing_all);

    let fragments = livewire_fragments(UPDATE);
    assert_eq!(fragments.len(), 1);
}
