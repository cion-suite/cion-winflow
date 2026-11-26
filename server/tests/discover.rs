#![cfg(debug_assertions)] // fixture-источник существует только в debug-сборке

mod common;

use axum::http::{Method, StatusCode};
use serde_json::json;

use common::{TestApp, body_json, get, json_request, with_cookie};

async fn configure_fixture(app: &TestApp, admin: &str, base_url: &str) {
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/sources/fixture",
                &json!({ "base_url": base_url, "enabled": true }),
            ),
            admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "configure fixture");
}

async fn add_fixture(app: &TestApp, cookie: &str) -> serde_json::Value {
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/discover/add",
                &json!({
                    "code": "fixture",
                    "external_id": "fixture-title",
                    "kind": "manga",
                }),
            ),
            cookie,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "discover add");
    body_json(response).await
}

#[tokio::test]
async fn discover_search_and_lazy_import_for_a_regular_user() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;

    // Фикстуре нужен только origin. Порт 9 закрыт: автоскачивание обложки
    // получает мгновенный отказ, а не висит на молчащем listener'е.
    configure_fixture(&app, &admin, "http://127.0.0.1:9").await;
    let user = app.register_extra_user("reader").await;

    let response = app
        .request(with_cookie(get("/api/discover/search?q=fixture"), &user))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert!(body["errors"].as_array().expect("errors").is_empty());
    let items = body["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item["code"], "fixture");
    assert_eq!(item["external_id"], "fixture-title");
    assert_eq!(item["name"], "Fixture Title");
    assert!(item["title_id"].is_null());
    assert_eq!(item["in_library"], false);

    let added = add_fixture(&app, &user).await;
    let title_id = added["title_id"].as_i64().expect("title id");
    assert_eq!(added["title_created"], true);
    assert_eq!(added["imported_chapters"], 3);
    assert_eq!(added["library_created"], true);
    assert_eq!(added["status"], "planned");

    let response = app
        .request(with_cookie(get("/api/library?q=fixture"), &user))
        .await;
    let library = body_json(response).await;
    assert_eq!(library["total"], 1);
    assert_eq!(library["items"][0]["title_id"], title_id);
    assert_eq!(library["items"][0]["status"], "planned");
    assert_eq!(library["items"][0]["chapter_count"], 3);
    assert_eq!(library["items"][0]["read_count"], 0);

    let response = app
        .request(with_cookie(get("/api/discover/search?q=fixture"), &user))
        .await;
    let body = body_json(response).await;
    assert_eq!(body["items"][0]["title_id"], title_id);
    assert_eq!(body["items"][0]["in_library"], true);

    // Повторное добавление не создаёт ни тайтл, ни запись библиотеки
    let again = add_fixture(&app, &user).await;
    assert_eq!(again["title_id"], title_id);
    assert_eq!(again["title_created"], false);
    assert_eq!(again["imported_chapters"], 0);
    assert_eq!(again["library_created"], false);

    // Администратор видит ту же выдачу
    let response = app
        .request(with_cookie(get("/api/discover/search?q=fixture"), &admin))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        body_json(response).await["items"][0]["external_id"],
        "fixture-title"
    );
}

#[tokio::test]
async fn discover_requires_session_and_rejects_bad_input() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;

    let response = app.request(get("/api/discover/search?q=fixture")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = app
        .request(json_request(
            Method::POST,
            "/api/discover/add",
            &json!({
                "code": "fixture",
                "external_id": "fixture-title",
                "kind": "manga",
            }),
        ))
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("local addr");
    configure_fixture(&app, &admin, &format!("http://{address}")).await;
    let user = app.register_extra_user("reader").await;

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/discover/add",
                &json!({
                    "code": "missing",
                    "external_id": "fixture-title",
                    "kind": "manga",
                }),
            ),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/discover/add",
                &json!({
                    "code": "fixture",
                    "external_id": "fixture-title",
                    "kind": "anime",
                }),
            ),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(response).await["code"], "source_unsupported_kind");

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/discover/add",
                &json!({
                    "code": "fixture",
                    "external_id": "../secret",
                    "kind": "manga",
                }),
            ),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(response).await["code"],
        "source_invalid_external_id"
    );

    let response = app
        .request(with_cookie(get("/api/discover/search?q="), &user))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn saved_progress_marks_the_chapter_as_read() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;

    configure_fixture(&app, &admin, "http://127.0.0.1:9").await;
    let user = app.register_extra_user("reader").await;

    let added = add_fixture(&app, &user).await;
    let title_id = added["title_id"].as_i64().expect("title id");

    // content() фикстуры отдаёт страницы без сети и записывает content_kind
    let response = app
        .request(with_cookie(
            get(&format!("/api/titles/{title_id}/chapters/c1/content")),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/progress/{title_id}"),
                &json!({ "chapter_key": "c1", "page": 0 }),
            ),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .request(with_cookie(get(&format!("/api/titles/{title_id}")), &user))
        .await;
    let title = body_json(response).await;
    let read_keys = title["read_keys"].as_array().expect("read keys");
    assert_eq!(read_keys, &vec![json!("c1")]);

    let response = app
        .request(with_cookie(get("/api/library?q=fixture"), &user))
        .await;
    let library = body_json(response).await;
    assert_eq!(library["items"][0]["read_count"], 1);
    assert_eq!(library["items"][0]["chapter_count"], 3);
}

#[tokio::test]
async fn manual_chapter_reads_can_be_bulk_marked_and_cleared() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;

    configure_fixture(&app, &admin, "http://127.0.0.1:9").await;
    let user = app.register_extra_user("reader").await;
    let added = add_fixture(&app, &user).await;
    let title_id = added["title_id"].as_i64().expect("title id");

    async fn put_reads(
        app: &TestApp,
        cookie: &str,
        title_id: i64,
        keys: &[&str],
        read: bool,
    ) -> axum::response::Response {
        app.request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/titles/{title_id}/chapters/read"),
                &json!({ "chapter_keys": keys, "read": read }),
            ),
            cookie,
        ))
        .await
    }

    async fn read_keys(app: &TestApp, cookie: &str, title_id: i64) -> serde_json::Value {
        let response = app
            .request(with_cookie(get(&format!("/api/titles/{title_id}")), cookie))
            .await;
        body_json(response).await["read_keys"].clone()
    }

    // «Эта глава и все предыдущие» приходит одним запросом
    let response = put_reads(&app, &user, title_id, &["c1", "c2"], true).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["read_keys"], json!(["c1", "c2"]));
    assert_eq!(read_keys(&app, &user, title_id).await, json!(["c1", "c2"]));

    let response = app
        .request(with_cookie(get("/api/library?q=fixture"), &user))
        .await;
    assert_eq!(body_json(response).await["items"][0]["read_count"], 2);

    // Вернуть одну главу в непрочитанное
    let response = put_reads(&app, &user, title_id, &["c1"], false).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["read_keys"], json!(["c2"]));

    // Неизвестный ключ отклоняет весь запрос: журнал не меняется
    let response = put_reads(&app, &user, title_id, &["c2", "missing"], false).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(body_json(response).await["code"], "chapter_not_found");
    assert_eq!(read_keys(&app, &user, title_id).await, json!(["c2"]));

    let response = put_reads(&app, &user, title_id, &[], true).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
