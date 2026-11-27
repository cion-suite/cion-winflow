//! Локальный двойник Remanga API для тестов авторизации.
//!
//! Отдельный хост картинок (`127.0.0.2`) нужен, чтобы проверить запрет
//! отправлять сессионные заголовки на сторонний CDN: `127.0.0.1` — API,
//! `127.0.0.2` — картинки.

use std::sync::{Arc, Mutex};

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, Method, Request, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use cion_reader::{config::Config, source_auth::SourceCredentialsKey};

use super::{TestApp, body_json, get as http_get, json_request, with_cookie};

/// Валидный токен, который стаб считает «правильной учёткой».
pub const GOOD_BEARER: &str = "Bearer good-token";
const KEY_B64: &str = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=";

/// Пара, которую v1-логин двойника принимает как правильную.
pub const LOGIN_USER: &str = "remanga-user";
pub const LOGIN_PASSWORD: &str = "remanga-password";
/// 30-символьный непрозрачный токен, как у настоящей Remanga.
pub const LOGIN_TOKEN: &str = "0123456789abcdefghijklmnopqrst";

/// Поведение `/api/users/login/` в двойнике.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LoginMode {
    #[default]
    Ok,
    InvalidPassword,
    TwoFactor,
    BlockedHtml,
}

#[derive(Debug, Clone)]
pub struct CapturedRequest {
    pub path: String,
    pub authorization: Option<String>,
    pub cookie: Option<String>,
}

#[derive(Default)]
pub struct StubMode {
    /// Все title/chapters требуют правильную учётку.
    pub require_auth_for_public: bool,
    /// Content главы требует правильную учётку.
    pub content_requires_auth: bool,
    /// Картинки отвечают 403.
    pub image_forbidden: bool,
    /// Как отвечает v1-логин.
    pub login_mode: LoginMode,
}

#[derive(Clone)]
struct Shared {
    mode: Arc<Mutex<StubMode>>,
    requests: Arc<Mutex<Vec<CapturedRequest>>>,
    image_requests: Arc<Mutex<Vec<CapturedRequest>>>,
    image_url: Arc<Mutex<String>>,
}

pub struct RemangaStub {
    pub base_url: String,
    pub mode: Arc<Mutex<StubMode>>,
    pub requests: Arc<Mutex<Vec<CapturedRequest>>>,
    pub image_requests: Arc<Mutex<Vec<CapturedRequest>>>,
}

impl RemangaStub {
    pub fn set_mode(&self, update: impl FnOnce(&mut StubMode)) {
        update(
            &mut self
                .mode
                .lock()
                .unwrap_or_else(|poison| poison.into_inner()),
        );
    }

    pub fn requests_for(&self, path_fragment: &str) -> Vec<CapturedRequest> {
        self.requests
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .iter()
            .filter(|request| request.path.contains(path_fragment))
            .cloned()
            .collect()
    }
}

pub async fn start() -> RemangaStub {
    let mode = Arc::new(Mutex::new(StubMode::default()));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let image_requests = Arc::new(Mutex::new(Vec::new()));
    let image_url = Arc::new(Mutex::new(String::new()));

    let shared = Shared {
        mode: mode.clone(),
        requests: requests.clone(),
        image_requests: image_requests.clone(),
        image_url: image_url.clone(),
    };

    let api = Router::new()
        .route("/api/users/login/", post(login))
        .route("/api/v2/search/", get(search))
        .route("/api/v2/titles/chapters/", get(chapters))
        .route("/api/v2/titles/chapters/{id}/", get(content))
        .route("/api/v2/titles/{dir}/", get(title))
        .with_state(shared.clone());

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind api");
    let address = listener.local_addr().expect("api addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, api).await;
    });

    let image = Router::new()
        .route("/img/1.png", get(serve_image))
        .with_state(shared);
    let image_listener = TcpListener::bind("127.0.0.2:0").await.expect("bind image");
    let image_address = image_listener.local_addr().expect("image addr");
    *image_url
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = format!("http://{image_address}/img/1.png");
    tokio::spawn(async move {
        let _ = axum::serve(image_listener, image).await;
    });

    RemangaStub {
        base_url: format!("http://{address}"),
        mode,
        requests,
        image_requests,
    }
}

fn capture(requests: &Mutex<Vec<CapturedRequest>>, path: &str, headers: &HeaderMap) {
    let authorization = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let cookie = headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    requests
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .push(CapturedRequest {
            path: path.to_owned(),
            authorization,
            cookie,
        });
}

fn is_authed(headers: &HeaderMap) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        == Some(GOOD_BEARER)
}

async fn login(
    State(shared): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    capture(&shared.requests, "/api/users/login/", &headers);
    let mode = shared
        .mode
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .login_mode;

    match mode {
        LoginMode::Ok => {
            let user = body["user"].as_str().unwrap_or_default();
            let password = body["password"].as_str().unwrap_or_default();
            if user == LOGIN_USER && password == LOGIN_PASSWORD {
                Json(json!({
                    "content": {
                        "access_token": LOGIN_TOKEN,
                        "two_factor_auth": false,
                        "methods": []
                    }
                }))
                .into_response()
            } else {
                invalid_login().into_response()
            }
        }
        LoginMode::InvalidPassword => invalid_login().into_response(),
        LoginMode::TwoFactor => Json(json!({
            "content": {
                "two_factor_auth": true,
                "methods": ["email", "telegram"]
            }
        }))
        .into_response(),
        LoginMode::BlockedHtml => (
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            "<html>restricted access from your current network</html>",
        )
            .into_response(),
    }
}

/// v1 отвечает 400 с `{msg}` на неверную пару; тело наружу не отдаётся.
fn invalid_login() -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "msg": "Введен неверный логин/почта или пароль." })),
    )
}

async fn search(State(shared): State<Shared>, headers: HeaderMap) -> Response {
    capture(&shared.requests, "/api/v2/search/", &headers);
    Json(json!({
        "meta": { "total_pages": 1, "page": 1, "total_items": 1 },
        "results": [{
            "main_name": "Public Title",
            "dir": "public",
            "type": { "name": "Манга" }
        }]
    }))
    .into_response()
}

async fn title(
    State(shared): State<Shared>,
    Path(dir): Path<String>,
    headers: HeaderMap,
) -> Response {
    capture(
        &shared.requests,
        &format!("/api/v2/titles/{dir}/"),
        &headers,
    );
    let authed = is_authed(&headers);
    let require_auth = shared
        .mode
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .require_auth_for_public;

    let status = match dir.as_str() {
        "blocked" => Some(StatusCode::FORBIDDEN),
        "hidden" if !authed => Some(StatusCode::NOT_FOUND),
        "locked" if !authed => Some(StatusCode::UNAUTHORIZED),
        _ if require_auth && !authed => Some(StatusCode::UNAUTHORIZED),
        _ => None,
    };
    if let Some(status) = status {
        return status.into_response();
    }

    Json(json!({
        "dir": dir,
        "main_name": format!("{dir} title"),
        "description": "<p>stub</p>",
        "type": { "name": "Манга" },
        "branches": [{ "id": 1 }],
        "creators": [],
        "genres": [],
        "categories": [],
    }))
    .into_response()
}

async fn chapters(State(shared): State<Shared>, headers: HeaderMap) -> Response {
    capture(&shared.requests, "/api/v2/titles/chapters/", &headers);
    let require_auth = shared
        .mode
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .require_auth_for_public;
    if require_auth && !is_authed(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    Json(json!({
        "next": null,
        "results": [{
            "id": 101,
            "tome": 1,
            "chapter": "1",
            "name": "Chapter 1",
            "is_published": true
        }]
    }))
    .into_response()
}

async fn content(
    State(shared): State<Shared>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    capture(
        &shared.requests,
        &format!("/api/v2/titles/chapters/{id}/"),
        &headers,
    );
    let content_requires_auth = shared
        .mode
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .content_requires_auth;
    if content_requires_auth && !is_authed(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let image_url = shared
        .image_url
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    Json(json!({
        "is_paid": false,
        "content_type": "comics",
        "pages": [[{ "link": image_url, "width": 10, "height": 20 }]]
    }))
    .into_response()
}

async fn serve_image(State(shared): State<Shared>, headers: HeaderMap) -> Response {
    capture(&shared.image_requests, "/img/1.png", &headers);
    let forbidden = shared
        .mode
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .image_forbidden;
    if forbidden {
        return StatusCode::FORBIDDEN.into_response();
    }

    let image = image::RgbaImage::from_pixel(4, 8, image::Rgba([20, 40, 80, 255]));
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("encode png");

    (StatusCode::OK, [(header::CONTENT_TYPE, "image/png")], bytes).into_response()
}

pub fn config_with_key() -> Config {
    Config {
        source_credentials_key: Some(
            SourceCredentialsKey::from_base64(KEY_B64).expect("valid test key"),
        ),
        ..Config::default()
    }
}

pub struct AuthFixture {
    pub app: TestApp,
    pub alice: String,
    pub admin: String,
    pub stub: RemangaStub,
}

pub async fn auth_fixture() -> AuthFixture {
    let app = TestApp::with_config(config_with_key()).await;
    let alice = app.register_user("alice").await;
    let admin = app.login_admin().await;
    let stub = start().await;
    configure(&app, &admin, &stub).await;
    AuthFixture {
        app,
        alice,
        admin,
        stub,
    }
}

pub async fn configure(app: &TestApp, admin: &str, stub: &RemangaStub) {
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/sources/remanga",
                &json!({ "base_url": stub.base_url, "enabled": true }),
            ),
            admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "configure remanga");

    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/sources/remanga/config",
                &json!({
                    "site_url": stub.base_url,
                    "image_hosts": ["127.0.0.2"],
                }),
            ),
            admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "save remanga config");
}

pub async fn save_credential(app: &TestApp, cookie: &str, bearer: &str) -> StatusCode {
    app.request(with_cookie(
        json_request(
            Method::PUT,
            "/api/me/source-credentials/remanga",
            &json!({ "bearer": bearer, "cookie": "sid=secret-cookie" }),
        ),
        cookie,
    ))
    .await
    .status()
}

pub async fn import(app: &TestApp, cookie: &str, external_id: &str) -> Response<Body> {
    // `add` ограничен по IP: серия проверок легко упирается в 429.
    loop {
        let response = app
            .request(with_cookie(
                json_request(
                    Method::POST,
                    "/api/discover/add",
                    &json!({ "code": "remanga", "external_id": external_id }),
                ),
                cookie,
            ))
            .await;
        if response.status() != StatusCode::TOO_MANY_REQUESTS {
            return response;
        }
        let wait = response
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(4);
        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
    }
}

pub async fn title_json(app: &TestApp, cookie: &str, title_id: i64) -> Value {
    body_json(
        app.request(with_cookie(
            http_get(&format!("/api/titles/{title_id}")),
            cookie,
        ))
        .await,
    )
    .await
}

pub async fn chapter_key(app: &TestApp, cookie: &str, title_id: i64) -> String {
    let value = title_json(app, cookie, title_id).await;
    value["chapters"][0]["chapter_key"]
        .as_str()
        .expect("chapter key")
        .to_owned()
}

pub async fn chapter_content(
    app: &TestApp,
    cookie: &str,
    title_id: i64,
    chapter_key: &str,
) -> Response<Body> {
    app.request(with_cookie(
        http_get(&format!(
            "/api/titles/{title_id}/chapters/{chapter_key}/content"
        )),
        cookie,
    ))
    .await
}

pub async fn direct_content(app: &TestApp, cookie: &str, chapter_id: i64) -> Response<Body> {
    app.request(with_cookie(
        http_get(&format!("/api/chapters/{chapter_id}/content")),
        cookie,
    ))
    .await
}

pub async fn entry_auth_required(app: &TestApp, title_id: i64) -> (bool, Option<String>) {
    sqlx::query_as(
        "SELECT auth_required, last_error FROM source_entries WHERE title_id = ? LIMIT 1",
    )
    .bind(title_id)
    .fetch_one(&app.pool)
    .await
    .expect("source entry")
}

pub async fn chapter_flags(app: &TestApp, title_id: i64) -> (bool, String, bool) {
    sqlx::query_as(
        "SELECT ch.auth_required, ch.availability, ch.is_locked FROM chapters ch \
         JOIN source_entries e ON e.id = ch.source_entry_id WHERE e.title_id = ? LIMIT 1",
    )
    .bind(title_id)
    .fetch_one(&app.pool)
    .await
    .expect("chapter flags")
}

pub async fn metadata_synced_at(app: &TestApp, title_id: i64) -> Option<i64> {
    sqlx::query_scalar("SELECT metadata_synced_at FROM titles WHERE id = ?")
        .bind(title_id)
        .fetch_one(&app.pool)
        .await
        .expect("title timestamp")
}

pub async fn credential_last_error(app: &TestApp, username: &str) -> Option<String> {
    sqlx::query_scalar(
        "SELECT c.last_error FROM user_source_credentials c \
         JOIN users u ON u.id = c.user_id WHERE u.username = ?",
    )
    .bind(username)
    .fetch_one(&app.pool)
    .await
    .expect("credential last error")
}

pub fn delete_request(uri: &str) -> Request<Body> {
    let request = Request::builder()
        .method(Method::DELETE)
        .uri(uri)
        .header(header::ORIGIN, super::ORIGIN)
        .header("sec-fetch-site", "same-origin")
        .body(Body::empty())
        .expect("request");
    super::with_peer(request, "127.0.0.1")
}
