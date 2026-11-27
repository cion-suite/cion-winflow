//! При открытии страницы тайтла сервер ищет его на ещё не привязанных
//! источниках того же языка и привязывает точные совпадения. Негативный
//! результат кэшируется per-user, поэтому повторное открытие сеть не трогает.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};

mod common;
use common::{TestApp, body_json, get as http_get, json_request, with_cookie};

const SEARCH_MAIN: &str = r#"<!doctype html>
<html><head></head><body>
<script type="application/ld+json">
{"@type":"ItemList","numberOfItems":1,"itemListElement":[{"@type":"ListItem","name":"Fixture Title","url":"https://mangamir.com/manga/fixture-title"}]}
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

#[derive(Clone, Copy)]
enum Mode {
    /// Поиск находит тайтл по основному названию.
    Match,
    /// Поиск успешен, но ничего не находит.
    Empty,
    /// Поиск отвечает 404 — источник не подтверждает отсутствие тайтла.
    Missing,
}

#[derive(Clone)]
struct MockState {
    mode: Mode,
    searches: Arc<AtomicUsize>,
}

async fn search_page(State(state): State<MockState>) -> Response {
    state.searches.fetch_add(1, Ordering::SeqCst);
    let body = match state.mode {
        Mode::Match => SEARCH_MAIN,
        Mode::Empty => SEARCH_EMPTY,
        Mode::Missing => return StatusCode::NOT_FOUND.into_response(),
    };
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
        .into_response()
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

async fn start_mock(mode: Mode) -> (String, Arc<AtomicUsize>) {
    let searches = Arc::new(AtomicUsize::new(0));
    let state = MockState {
        mode,
        searches: searches.clone(),
    };
    let router = Router::new()
        .route("/manga", get(search_page))
        .route("/manga/{slug}", get(chapter_page))
        .route("/livewire-test/update", post(update))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    (format!("http://{address}"), searches)
}

async fn configure_source(app: &TestApp, admin: &str, code: &str, base_url: &str, enabled: bool) {
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/sources/{code}"),
                &json!({ "base_url": base_url, "enabled": enabled }),
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

async fn add_fixture_title(app: &TestApp, admin: &str) -> i64 {
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/discover/add",
                &json!({ "code": "fixture", "external_id": "fixture-title" }),
            ),
            admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "add fixture title");
    body_json(response).await["title_id"]
        .as_i64()
        .expect("title id")
}

async fn sync(app: &TestApp, admin: &str, title_id: i64) -> Value {
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/titles/{title_id}/sync"),
                &json!({}),
            ),
            admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "sync title");
    body_json(response).await
}

fn source_codes(title: &Value) -> Vec<&str> {
    title["sources"]
        .as_array()
        .expect("sources")
        .iter()
        .filter_map(|source| source["code"].as_str())
        .collect()
}

#[tokio::test]
async fn sync_links_missing_source_on_open() {
    let (mock, searches) = start_mock(Mode::Match).await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;

    configure_source(&app, &admin, "fixture", &mock, true).await;
    // Выключенный источник `add` не проверяет: тайтл остаётся с одним
    // источником, и привязку делает именно открытие страницы.
    configure_source(&app, &admin, "mangamir", &mock, false).await;

    let title_id = add_fixture_title(&app, &admin).await;
    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    assert_eq!(source_codes(&title), vec!["fixture"]);

    configure_source(&app, &admin, "mangamir", &mock, true).await;
    let before = searches.load(Ordering::SeqCst);

    let response = sync(&app, &admin, title_id).await;
    assert_eq!(response["refreshed"], true);
    assert_eq!(response["sources_linked"], json!(["mangamir"]));
    assert!(response["discovery"]["checked"].as_u64().unwrap_or(0) >= 1);
    assert!(
        searches.load(Ordering::SeqCst) > before,
        "mangamir was searched"
    );

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    let codes = source_codes(&title);
    assert!(codes.contains(&"fixture"), "{codes:?}");
    assert!(codes.contains(&"mangamir"), "{codes:?}");
    let mangamir = title["sources"]
        .as_array()
        .expect("sources")
        .iter()
        .find(|source| source["code"] == "mangamir")
        .expect("mangamir source");
    assert_eq!(mangamir["chapter_count"], 3);

    // Все источники привязаны: discovery больше не ищет, а главы свежие.
    let before = searches.load(Ordering::SeqCst);
    let response = sync(&app, &admin, title_id).await;
    assert_eq!(response["refreshed"], false);
    assert_eq!(response["sources_linked"], json!([]));
    assert_eq!(response["discovery"]["checked"], 0);
    assert_eq!(searches.load(Ordering::SeqCst), before);
}

#[tokio::test]
async fn negative_discovery_is_cached_between_opens() {
    let (mock, searches) = start_mock(Mode::Empty).await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;

    configure_source(&app, &admin, "fixture", &mock, true).await;
    configure_source(&app, &admin, "mangamir", &mock, true).await;

    // `add` уже проверил остальные источники и записал негативный результат.
    let title_id = add_fixture_title(&app, &admin).await;
    let after_add = searches.load(Ordering::SeqCst);
    assert!(after_add > 0);

    let response = sync(&app, &admin, title_id).await;
    assert_eq!(response["sources_linked"], json!([]));
    assert_eq!(response["discovery"]["checked"], 0);
    assert!(response["discovery"]["skipped"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(
        searches.load(Ordering::SeqCst),
        after_add,
        "cached not_found must not be searched again"
    );

    let status: String = sqlx::query_scalar(
        "SELECT status FROM title_source_checks WHERE title_id = ? \
         AND source_id = (SELECT id FROM sources WHERE code = 'mangamir')",
    )
    .bind(title_id)
    .fetch_one(&app.pool)
    .await
    .expect("check row");
    assert_eq!(status, "not_found");
}

#[tokio::test]
async fn search_error_is_cached_for_an_hour_not_a_day() {
    let (mock, searches) = start_mock(Mode::Missing).await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;

    configure_source(&app, &admin, "fixture", &mock, true).await;
    configure_source(&app, &admin, "mangamir", &mock, true).await;

    let title_id = add_fixture_title(&app, &admin).await;
    let after_add = searches.load(Ordering::SeqCst);

    // 404 источника — не окончательное «тайтла нет» на сутки.
    let status: String = sqlx::query_scalar(
        "SELECT status FROM title_source_checks WHERE title_id = ? \
         AND source_id = (SELECT id FROM sources WHERE code = 'mangamir')",
    )
    .bind(title_id)
    .fetch_one(&app.pool)
    .await
    .expect("check row");
    assert_eq!(status, "error");

    // В пределах часа результат считается свежим и сеть не трогается.
    sync(&app, &admin, title_id).await;
    assert_eq!(searches.load(Ordering::SeqCst), after_add);

    // Час спустя discovery повторяет поиск.
    sqlx::query("UPDATE title_source_checks SET checked_at = checked_at - 3601 WHERE title_id = ?")
        .bind(title_id)
        .execute(&app.pool)
        .await
        .expect("age check row");

    sync(&app, &admin, title_id).await;
    assert!(
        searches.load(Ordering::SeqCst) > after_add,
        "expired discovery result must be re-checked"
    );
}
