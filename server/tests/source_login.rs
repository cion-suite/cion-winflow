//! Фаза 5а: вход в Remanga по логину/паролю. Двойник v1-логина проверяет
//! формат ответа, коды ошибок и то, что пароль не покидает сервер.

mod common;

use axum::{
    body::Body,
    http::{Method, StatusCode, header},
    response::Response,
};
use serde_json::json;

use common::remanga::{self, LOGIN_PASSWORD, LOGIN_TOKEN, LOGIN_USER, LoginMode};
use common::{TestApp, body_json, get, json_request, with_cookie};

use cion_reader::source_auth::service;

fn login_request(code: &str, login: &str, password: &str) -> axum::http::Request<Body> {
    json_request(
        Method::POST,
        &format!("/api/me/source-credentials/{code}/login"),
        &json!({ "login": login, "password": password }),
    )
}

async fn login(
    app: &TestApp,
    cookie: &str,
    code: &str,
    login: &str,
    password: &str,
) -> Response<Body> {
    app.request(with_cookie(login_request(code, login, password), cookie))
        .await
}

async fn user_id(app: &TestApp, username: &str) -> i64 {
    sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
        .bind(username)
        .fetch_one(&app.pool)
        .await
        .expect("user id")
}

async fn source_id(app: &TestApp, code: &str) -> i64 {
    sqlx::query_scalar("SELECT id FROM sources WHERE code = ?")
        .bind(code)
        .fetch_one(&app.pool)
        .await
        .expect("source id")
}

async fn credential_rows(app: &TestApp) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM user_source_credentials")
        .fetch_one(&app.pool)
        .await
        .expect("credential count")
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[tokio::test]
async fn login_stores_encrypted_token_and_marks_verified() {
    let fixture = remanga::auth_fixture().await;
    let app = &fixture.app;

    let response = login(app, &fixture.alice, "remanga", LOGIN_USER, LOGIN_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::OK, "login");
    let body = body_json(response).await;
    assert_eq!(body["code"], "remanga");
    assert_eq!(body["name"], "Remanga");
    assert_eq!(body["configured"], true);
    assert_eq!(body["login_supported"], true);
    assert!(
        body["last_verified_at"].as_i64().is_some(),
        "login marks the credential verified"
    );

    // Ни пароль, ни токен не возвращаются клиенту.
    let serialized = body.to_string();
    assert!(
        !serialized.contains(LOGIN_PASSWORD),
        "password must not leak"
    );
    assert!(!serialized.contains(LOGIN_TOKEN), "token must not leak");

    // В БД только шифротекст, а last_verified_at заполнен.
    let (ciphertext, nonce, verified_at): (Vec<u8>, Vec<u8>, Option<i64>) =
        sqlx::query_as("SELECT ciphertext, nonce, last_verified_at FROM user_source_credentials")
            .fetch_one(&app.pool)
            .await
            .expect("stored credential");
    assert!(!contains(&ciphertext, LOGIN_PASSWORD.as_bytes()));
    assert!(!contains(&ciphertext, LOGIN_TOKEN.as_bytes()));
    assert_eq!(nonce.len(), 12, "nonce is 12 random bytes");
    assert!(verified_at.is_some());

    // Расшифровка даёт ровно тот Bearer, который выдал источник.
    let cipher = app
        .state
        .source_credentials
        .as_deref()
        .expect("credential cipher");
    let auth = service::for_user(
        &app.pool,
        cipher,
        user_id(app, "alice").await,
        source_id(app, "remanga").await,
    )
    .await
    .expect("decrypt")
    .expect("configured");
    assert_eq!(auth.bearer(), Some(LOGIN_TOKEN));
    assert_eq!(auth.cookie(), None);

    // Список показывает подключённую учётку и поддержку входа.
    let list = body_json(
        app.request(with_cookie(
            get("/api/me/source-credentials"),
            &fixture.alice,
        ))
        .await,
    )
    .await;
    let remanga_entry = list["credentials"]
        .as_array()
        .expect("credentials array")
        .iter()
        .find(|item| item["code"] == "remanga")
        .expect("remanga entry");
    assert_eq!(remanga_entry["configured"], true);
    assert_eq!(remanga_entry["login_supported"], true);
    assert!(!list.to_string().contains(LOGIN_TOKEN));

    // Полученный токен действительно уходит в запросы от имени пользователя.
    let response = app
        .request(with_cookie(
            get("/api/discover/search?q=Public"),
            &fixture.alice,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "discover search");
    let expected = format!("Bearer {LOGIN_TOKEN}");
    let captured = fixture.stub.requests_for("/api/v2/search/");
    assert!(
        captured
            .iter()
            .any(|request| request.authorization.as_deref() == Some(expected.as_str())),
        "login token is used for source requests"
    );
}

#[tokio::test]
async fn wrong_password_is_rejected_without_saving() {
    let fixture = remanga::auth_fixture().await;

    let response = login(
        &fixture.app,
        &fixture.alice,
        "remanga",
        LOGIN_USER,
        "not-the-password",
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_json(response).await;
    assert_eq!(body["code"], "source_login_failed");
    assert!(
        !body.to_string().contains("not-the-password"),
        "password must not echo"
    );
    assert_eq!(credential_rows(&fixture.app).await, 0);
}

#[tokio::test]
async fn two_factor_returns_a_dedicated_error() {
    let fixture = remanga::auth_fixture().await;
    fixture
        .stub
        .set_mode(|mode| mode.login_mode = LoginMode::TwoFactor);

    let response = login(
        &fixture.app,
        &fixture.alice,
        "remanga",
        LOGIN_USER,
        LOGIN_PASSWORD,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = body_json(response).await;
    assert_eq!(body["code"], "source_login_2fa_required");
    let message = body["message"].as_str().expect("message");
    assert!(message.contains("email"), "methods are reported: {message}");
    assert!(
        message.contains("telegram"),
        "methods are reported: {message}"
    );
    assert_eq!(credential_rows(&fixture.app).await, 0);
}

#[tokio::test]
async fn blocked_html_is_not_mistaken_for_bad_credentials() {
    let fixture = remanga::auth_fixture().await;
    fixture
        .stub
        .set_mode(|mode| mode.login_mode = LoginMode::BlockedHtml);

    let response = login(
        &fixture.app,
        &fixture.alice,
        "remanga",
        LOGIN_USER,
        LOGIN_PASSWORD,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(body_json(response).await["code"], "source_blocked");
    assert_eq!(credential_rows(&fixture.app).await, 0);
}

#[tokio::test]
async fn invalid_password_body_is_rejected_the_same_way() {
    let fixture = remanga::auth_fixture().await;
    fixture
        .stub
        .set_mode(|mode| mode.login_mode = LoginMode::InvalidPassword);

    let response = login(
        &fixture.app,
        &fixture.alice,
        "remanga",
        LOGIN_USER,
        LOGIN_PASSWORD,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(response).await["code"], "source_login_failed");
    assert_eq!(credential_rows(&fixture.app).await, 0);
}

#[tokio::test]
async fn login_requires_the_encryption_key() {
    // Config::default() без SOURCE_CREDENTIALS_KEY: функция выключена.
    let app = TestApp::new().await;
    let alice = app.register_user("alice").await;

    let response = login(&app, &alice, "remanga", LOGIN_USER, LOGIN_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body_json(response).await["code"], "credentials_unavailable");
    assert_eq!(credential_rows(&app).await, 0);
}

#[tokio::test]
async fn other_users_do_not_see_the_login() {
    let fixture = remanga::auth_fixture().await;
    let app = &fixture.app;

    let response = login(app, &fixture.alice, "remanga", LOGIN_USER, LOGIN_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bob = app.register_extra_user("bob").await;

    let list = body_json(
        app.request(with_cookie(get("/api/me/source-credentials"), &bob))
            .await,
    )
    .await;
    let remanga_entry = list["credentials"]
        .as_array()
        .expect("credentials array")
        .iter()
        .find(|item| item["code"] == "remanga")
        .expect("remanga entry");
    assert_eq!(remanga_entry["configured"], false, "bob has no login");
}

#[tokio::test]
async fn unknown_source_and_unsupported_login_are_rejected() {
    let fixture = remanga::auth_fixture().await;

    let response = login(
        &fixture.app,
        &fixture.alice,
        "unknown-source",
        LOGIN_USER,
        LOGIN_PASSWORD,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(body_json(response).await["code"], "source_not_found");

    // RuMix объявляет Auth? Нет. Вход требует обоих флагов — Auth и Login.
    let response = login(
        &fixture.app,
        &fixture.alice,
        "rumix",
        LOGIN_USER,
        LOGIN_PASSWORD,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(response).await["code"],
        "source_credentials_invalid"
    );
}

#[tokio::test]
async fn per_user_attempt_limit_stops_password_guessing() {
    let fixture = remanga::auth_fixture().await;

    for _ in 0..5 {
        let response = login(
            &fixture.app,
            &fixture.alice,
            "remanga",
            LOGIN_USER,
            "wrong-password",
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    let response = login(
        &fixture.app,
        &fixture.alice,
        "remanga",
        LOGIN_USER,
        "wrong-password",
    )
    .await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(
        response.headers().get(header::RETRY_AFTER).is_some(),
        "retry-after is set"
    );
    assert_eq!(
        fixture.stub.requests_for("/api/users/login/").len(),
        5,
        "the blocked attempt never reaches the source"
    );
}
