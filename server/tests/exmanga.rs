//! ExManga: вход по email/паролю, импорт глав и выдача контента.
//!
//! Локальный двойник API проверяет, что адаптер ходит только на свой хост,
//! отправляет Bearer в `/api/chapter` и не пытается тянуть картинки с
//! Remanga CDN.

mod common;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get as route_get, post as route_post},
};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use cion_reader::{config::Config, source_auth::SourceCredentialsKey};
use common::{TestApp, body_json, get, json_request, with_cookie};

/// 32 байта 0x07 в base64: тестовый ключ AEAD.
const KEY_B64: &str = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=";
const EMAIL: &str = "reader@example.com";
const PASSWORD: &str = "secret-password";
const TOKEN: &str = "test-token";
const TITLE_DIR: &str = "fixture-dir";

static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn lock() -> tokio::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().await
}

type Requests = Arc<Mutex<Vec<(String, Option<String>)>>>;

#[derive(Clone)]
struct Stub {
    base_url: String,
    requests: Requests,
}

impl Stub {
    fn capture(&self, path: &str, headers: &HeaderMap) {
        let authorization = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        self.requests
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push((path.to_owned(), authorization));
    }

    fn saw_bearer(&self, path: &str) -> bool {
        self.requests
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .iter()
            .any(|(request_path, authorization)| {
                request_path.contains(path)
                    && authorization.as_deref() == Some(&format!("Bearer {TOKEN}"))
            })
    }
}

async fn start() -> Stub {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind stub");
    let address = listener.local_addr().expect("stub address");
    let stub = Stub {
        base_url: format!("http://{address}"),
        requests: Arc::new(Mutex::new(Vec::new())),
    };

    let router = Router::new()
        .route("/api/auth/login", route_post(login))
        .route("/api/auth", route_get(auth))
        .route("/api/manga", route_get(manga))
        .route("/api/manga/history", route_get(history))
        .route("/api/chapter", route_get(content))
        .route("/media/cover.png", route_get(image))
        .route("/storage/{name}/1.webp", route_get(image))
        .with_state(stub.clone());

    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    stub
}

async fn login(State(stub): State<Stub>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    stub.capture("/api/auth/login", &headers);
    if body["email"] == EMAIL && body["password"] == PASSWORD {
        Json(json!({ "success": true, "data": TOKEN })).into_response()
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "data": "Неверный email или пароль" })),
        )
            .into_response()
    }
}

async fn auth(State(stub): State<Stub>, headers: HeaderMap) -> Response {
    stub.capture("/api/auth", &headers);
    if headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        == Some(format!("Bearer {TOKEN}").as_str())
    {
        Json(json!({ "success": true, "data": { "level": 1 } })).into_response()
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "success": false, "data": "Ошибка авторизации" })),
        )
            .into_response()
    }
}

async fn manga(
    State(stub): State<Stub>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    stub.capture("/api/manga", &headers);
    let data = if query.contains_key("dir") || query.contains_key("name") {
        json!([title_item(&stub.base_url)])
    } else {
        json!([])
    };
    Json(json!({ "success": true, "statusCode": 200, "data": data })).into_response()
}

async fn history(State(stub): State<Stub>, headers: HeaderMap) -> Response {
    stub.capture("/api/manga/history", &headers);
    Json(json!({
        "success": true,
        "statusCode": 200,
        "data": [
            { "id": 2827338, "tome": 3, "chapter": "936", "index": 936,
              "views": 173, "date": "2026-11-27T23:39:00.000Z" },
            { "id": 2827336, "tome": 3, "chapter": "18.5", "index": 935,
              "views": 159, "date": "2026-11-27T22:23:29.000Z" },
            { "id": 700000, "tome": 1, "chapter": "", "index": 1,
              "views": 10, "date": "2026-11-27T21:00:00.000Z" }
        ]
    }))
    .into_response()
}

async fn content(State(stub): State<Stub>, headers: HeaderMap) -> Response {
    stub.capture("/api/chapter", &headers);
    if headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        != Some(format!("Bearer {TOKEN}").as_str())
    {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "success": false, "data": "Ошибка авторизации" })),
        )
            .into_response();
    }

    Json(json!({
        "success": true,
        "statusCode": 200,
        "data": {
            "id": 2827338,
            "is_paid": true,
            "is_bought": true,
            "links": [
                format!("{}/storage/chapter/1.webp?sign=a&expires=1", stub.base_url),
                format!("{}/storage/chapter/1.webp?sign=b&expires=1", stub.base_url)
            ]
        }
    }))
    .into_response()
}

async fn image() -> Response {
    let image = image::RgbaImage::from_pixel(4, 8, image::Rgba([20, 40, 80, 255]));
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("png");
    ([(header::CONTENT_TYPE, "image/png")], bytes).into_response()
}

fn title_item(base_url: &str) -> Value {
    json!({
        "id": 4153,
        "dir": TITLE_DIR,
        "name": "Фикстура",
        "meta": {
            "en": "Fixture Alt",
            "img": format!("{base_url}/media/cover.png"),
            "type": "Маньхуа",
            "rating": 9.4,
            "genres": "Экшен, Фэнтези",
            "categories": "Веб"
        },
        "ignore": false,
        "license": true
    })
}

async fn get_json(app: &TestApp, uri: &str, cookie: &str) -> Value {
    let response = app.request(with_cookie(get(uri), cookie)).await;
    assert_eq!(response.status(), StatusCode::OK, "{uri}");
    body_json(response).await
}

#[tokio::test]
async fn exmanga_login_import_and_content() {
    let _guard = lock().await;
    let stub = start().await;
    let app = TestApp::with_config(Config {
        source_credentials_key: Some(SourceCredentialsKey::from_base64(KEY_B64).expect("test key")),
        ..Config::default()
    })
    .await;

    app.create_admin().await;
    let admin = app.login_admin().await;
    let user = app.register_extra_user("reader").await;
    let other = app.register_extra_user("other").await;

    // Включаем источник на локальный двойник.
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/sources/exmanga",
                &json!({ "base_url": stub.base_url.clone(), "enabled": true }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "enable exmanga");

    // Вход по email/паролю сохраняет JWT как Bearer-учётку.
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/me/source-credentials/exmanga/login",
                &json!({ "login": EMAIL, "password": PASSWORD }),
            ),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "source login");
    let login = body_json(response).await;
    assert_eq!(login["configured"], true);
    assert_eq!(login["last_error"], Value::Null);

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/me/source-credentials/exmanga/verify",
                &json!({}),
            ),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "source verify");

    // Импорт тайтла из ExManga.
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/discover/add",
                &json!({ "code": "exmanga", "external_id": TITLE_DIR }),
            ),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "discover add");
    let added = body_json(response).await;
    let title_id = added["title_id"].as_i64().expect("title id");
    assert_eq!(added["imported_chapters"], 3);

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &user).await;
    assert!(
        title["sources"]
            .as_array()
            .expect("sources")
            .iter()
            .any(|source| source["code"] == "exmanga")
    );
    assert_eq!(
        title["chapters"].as_array().expect("chapters").len(),
        3,
        "all exmanga chapters imported"
    );

    // Контент: без учётки у другого пользователя — понятная ошибка.
    let content_uri = format!("/api/titles/{title_id}/chapters/v3:c936/content");
    let response = app.request(with_cookie(get(&content_uri), &other)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = body_json(response).await;
    assert_eq!(body["code"], "source_auth_not_configured");

    // С учёткой — страницы через прокси; адаптер отправил Bearer.
    let content = get_json(&app, &content_uri, &user).await;
    assert_eq!(content["source"]["code"], "exmanga");
    assert_eq!(content["content"]["kind"], "images");
    let pages = content["content"]["pages"].as_array().expect("pages");
    assert_eq!(pages.len(), 2);
    assert!(stub.saw_bearer("/api/chapter"), "content must use Bearer");

    let page_id = pages[0]["id"].as_i64().expect("page id");
    let response = app
        .request(with_cookie(
            get(&format!("/api/pages/{page_id}/image")),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "image proxy");
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/png")
    );

    // Публичные запросы ExManga не должны получать Bearer.
    let requests = stub
        .requests
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    for (path, authorization) in requests {
        if path.contains("/api/manga") {
            assert_eq!(authorization, None, "{path} must stay public");
        }
    }
}
