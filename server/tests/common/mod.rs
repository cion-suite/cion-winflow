#![allow(dead_code)] // каждый тест использует своё подмножество helpers

use std::net::SocketAddr;

use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{HeaderValue, Method, Request, Response, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tempfile::TempDir;
use tower::ServiceExt;

use cion_reader::{
    app::router,
    auth,
    config::{AdminBootstrap, Config},
    db,
    state::AppState,
};

pub const ORIGIN: &str = "http://localhost:1420";
pub const ADMIN_USERNAME: &str = "admin";
pub const ADMIN_PASSWORD: &str = "admin-password-123";
pub const USER_PASSWORD: &str = "user-password-123";

pub struct TestApp {
    pub pool: SqlitePool,
    /// Состояние нужно тестам, которые вызывают сервисы напрямую
    /// (например, фоновый импорт закладок).
    pub state: AppState,
    router: Router,
    // TempDir удаляет каталоги только при drop: держим их живыми
    _db_dir: TempDir,
    _static_dir: TempDir,
    _data_dir: TempDir,
}

impl TestApp {
    pub async fn new() -> Self {
        Self::with_config(Config::default()).await
    }

    pub async fn with_config(mut config: Config) -> Self {
        let static_dir = tempfile::tempdir().expect("static dir");
        std::fs::write(
            static_dir.path().join("index.html"),
            "<!doctype html><p>spa</p>",
        )
        .expect("index");
        std::fs::create_dir_all(static_dir.path().join("assets")).expect("assets dir");
        std::fs::write(static_dir.path().join("assets/app-1.js"), "console.log(1);")
            .expect("asset");
        std::fs::write(static_dir.path().join("theme.js"), "/* theme */").expect("theme");
        config.static_dir = static_dir.path().to_path_buf();

        let db_dir = tempfile::tempdir().expect("db dir");
        let database_path = db_dir.path().join("test.db");
        let database_url = format!(
            "sqlite://{}",
            database_path.to_string_lossy().replace('\\', "/")
        );
        let pool = db::connect(&database_url).await.expect("database");

        // Обложки и будущий кэш пишутся во временный каталог, не в data/
        let data_dir = tempfile::tempdir().expect("data dir");
        config.data_dir = data_dir.path().to_path_buf();
        // Тестовый источник картинок живёт на 127.0.0.1
        config.allow_loopback_images = true;

        let state = AppState::new(config, pool.clone())
            .await
            .expect("app state");
        Self {
            pool,
            router: router(state.clone()),
            state,
            _db_dir: db_dir,
            _static_dir: static_dir,
            _data_dir: data_dir,
        }
    }

    pub async fn request(&self, request: Request<Body>) -> Response<Body> {
        self.router
            .clone()
            .oneshot(request)
            .await
            .expect("router response")
    }

    pub async fn create_admin(&self) {
        let created = auth::bootstrap_admin(
            &self.pool,
            &AdminBootstrap {
                username: ADMIN_USERNAME.to_owned(),
                password: ADMIN_PASSWORD.to_owned(),
            },
        )
        .await
        .expect("bootstrap admin");
        assert!(created, "admin must be created in an empty database");
    }

    pub async fn login(&self, username: &str, password: &str) -> Response<Body> {
        self.request(json_request(
            Method::POST,
            "/api/auth/login",
            &json!({ "username": username, "password": password }),
        ))
        .await
    }

    pub async fn login_admin(&self) -> String {
        let response = self.login(ADMIN_USERNAME, ADMIN_PASSWORD).await;
        assert_eq!(response.status(), StatusCode::OK, "admin login");
        session_token(&response)
    }

    pub async fn register(&self, username: &str, password: &str, invite: &str) -> Response<Body> {
        self.request(json_request(
            Method::POST,
            "/api/auth/register",
            &json!({ "username": username, "password": password, "invite": invite }),
        ))
        .await
    }

    pub async fn create_invite(&self, admin_cookie: &str) -> String {
        let response = self
            .request(with_cookie(
                json_request(Method::POST, "/api/admin/invites", &json!({})),
                admin_cookie,
            ))
            .await;
        assert_eq!(response.status(), StatusCode::CREATED, "create invite");
        let body = body_json(response).await;
        body["code"].as_str().expect("invite code").to_owned()
    }

    pub async fn register_user(&self, username: &str) -> String {
        self.create_admin().await;
        let admin_cookie = self.login_admin().await;
        let invite = self.create_invite(&admin_cookie).await;
        let response = self.register(username, USER_PASSWORD, &invite).await;
        assert_eq!(response.status(), StatusCode::CREATED, "register user");
        session_token(&response)
    }

    /// Регистрирует ещё одного пользователя, когда администратор уже создан.
    pub async fn register_extra_user(&self, username: &str) -> String {
        let admin_cookie = self.login_admin().await;
        let invite = self.create_invite(&admin_cookie).await;
        let response = self.register(username, USER_PASSWORD, &invite).await;
        assert_eq!(response.status(), StatusCode::CREATED, "register user");
        session_token(&response)
    }

    /// Создаёт тайтл от имени администратора и возвращает его id.
    pub async fn create_title(&self, admin_cookie: &str, name: &str, kind: &str) -> i64 {
        let response = self
            .request(with_cookie(
                json_request(
                    Method::POST,
                    "/api/titles",
                    &json!({ "name": name, "kind": kind }),
                ),
                admin_cookie,
            ))
            .await;
        assert_eq!(response.status(), StatusCode::CREATED, "create title");
        let body = body_json(response).await;
        body["title"]["id"].as_i64().expect("title id")
    }
}

pub fn json_request(method: Method, uri: &str, body: &Value) -> Request<Body> {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, ORIGIN)
        .header("sec-fetch-site", "same-origin")
        .body(Body::from(body.to_string()))
        .expect("request");
    with_peer(request, "127.0.0.1")
}

pub fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(Body::empty())
        .expect("request")
}

/// multipart/form-data с одним файлом: загрузка обложки.
pub fn multipart_file_request(
    uri: &str,
    field: &str,
    filename: &str,
    content_type: &str,
    bytes: &[u8],
) -> Request<Body> {
    let boundary = "cion-reader-test-boundary";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{field}\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let request = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .header(header::ORIGIN, ORIGIN)
        .header("sec-fetch-site", "same-origin")
        .body(Body::from(body))
        .expect("request");
    with_peer(request, "127.0.0.1")
}

pub fn with_origin(mut request: Request<Body>, origin: &str) -> Request<Body> {
    request.headers_mut().insert(
        header::ORIGIN,
        HeaderValue::from_str(origin).expect("origin header"),
    );
    request
}

pub fn without_origin(mut request: Request<Body>) -> Request<Body> {
    request.headers_mut().remove(header::ORIGIN);
    request
}

pub fn with_cookie(mut request: Request<Body>, token: &str) -> Request<Body> {
    request.headers_mut().insert(
        header::COOKIE,
        HeaderValue::from_str(&format!("sid={token}")).expect("cookie header"),
    );
    request
}

pub fn with_peer(mut request: Request<Body>, ip: &str) -> Request<Body> {
    let address = SocketAddr::new(ip.parse().expect("peer ip"), 45_000);
    request.extensions_mut().insert(ConnectInfo(address));
    request
}

pub async fn body_json(response: Response<Body>) -> Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("json body")
}

pub fn session_token(response: &Response<Body>) -> String {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .find_map(|value| {
            let value = value.to_str().ok()?;
            let rest = value.strip_prefix("sid=")?;
            rest.split(';').next().map(str::to_owned)
        })
        .expect("sid cookie")
}

pub async fn password_hash(pool: &SqlitePool, username: &str) -> String {
    sqlx::query_scalar("SELECT password_hash FROM users WHERE username = ?")
        .bind(username)
        .fetch_one(pool)
        .await
        .expect("password hash")
}
