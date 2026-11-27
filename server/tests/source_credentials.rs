mod common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};

use common::{ORIGIN, TestApp, body_json, get, json_request, with_cookie, with_peer};
use serde_json::json;

use cion_reader::{
    config::Config,
    source_auth::{SourceCredentialsKey, service},
};

/// 32 байта 0x07 в base64: тестовый ключ AEAD.
const KEY_B64: &str = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=";

fn config_with_key(key: &str) -> Config {
    Config {
        source_credentials_key: Some(
            SourceCredentialsKey::from_base64(key).expect("valid test key"),
        ),
        ..Config::default()
    }
}

async fn app_with_key(key: &str) -> TestApp {
    TestApp::with_config(config_with_key(key)).await
}

fn delete_request(uri: &str) -> Request<Body> {
    let request = Request::builder()
        .method(Method::DELETE)
        .uri(uri)
        .header(header::ORIGIN, ORIGIN)
        .header("sec-fetch-site", "same-origin")
        .body(Body::empty())
        .expect("request");
    with_peer(request, "127.0.0.1")
}

async fn save(
    app: &TestApp,
    cookie: &str,
    code: &str,
    bearer: Option<&str>,
    secret_cookie: Option<&str>,
) -> axum::response::Response<Body> {
    app.request(with_cookie(
        json_request(
            Method::PUT,
            &format!("/api/me/source-credentials/{code}"),
            &json!({ "bearer": bearer, "cookie": secret_cookie }),
        ),
        cookie,
    ))
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
async fn credential_is_stored_encrypted_and_never_returned() {
    let app = app_with_key(KEY_B64).await;
    let alice = app.register_user("alice").await;

    let response = save(
        &app,
        &alice,
        "remanga",
        Some("secret-bearer-token"),
        Some("sid=cookie-secret; theme=dark"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK, "save credential");
    let body = body_json(response).await;
    assert_eq!(body["code"], "remanga");
    assert_eq!(body["name"], "Remanga");
    assert_eq!(body["configured"], true);
    assert!(
        body.get("bearer").is_none(),
        "response must not echo bearer"
    );
    assert!(
        body.get("cookie").is_none(),
        "response must not echo cookie"
    );
    let serialized = body.to_string();
    assert!(!serialized.contains("secret-bearer-token"));
    assert!(!serialized.contains("cookie-secret"));

    // Список отдаёт только статус: ни токена, ни cookie.
    let list = body_json(
        app.request(with_cookie(get("/api/me/source-credentials"), &alice))
            .await,
    )
    .await;
    let remanga = list["credentials"]
        .as_array()
        .expect("credentials array")
        .iter()
        .find(|item| item["code"] == "remanga")
        .expect("remanga entry");
    assert_eq!(remanga["configured"], true);
    assert!(!list.to_string().contains("secret-bearer-token"));

    // В БД только шифротекст: плейнтекста нет ни в одном столбце.
    let (ciphertext, nonce): (Vec<u8>, Vec<u8>) =
        sqlx::query_as("SELECT ciphertext, nonce FROM user_source_credentials")
            .fetch_one(&app.pool)
            .await
            .expect("stored credential");
    assert!(!contains(&ciphertext, b"secret-bearer-token"));
    assert!(!contains(&ciphertext, b"cookie-secret"));
    assert_eq!(nonce.len(), 12, "nonce is 12 random bytes");

    // Обратная расшифровка для будущих фаз отдаёт оба поля.
    let cipher = app
        .state
        .source_credentials
        .as_deref()
        .expect("credential cipher");
    let auth = service::for_user(
        &app.pool,
        cipher,
        user_id(&app, "alice").await,
        source_id(&app, "remanga").await,
    )
    .await
    .expect("decrypt")
    .expect("configured");
    assert_eq!(auth.bearer(), Some("secret-bearer-token"));
    assert_eq!(auth.cookie(), Some("sid=cookie-secret; theme=dark"));
}

#[tokio::test]
async fn credentials_are_isolated_per_user() {
    let app = app_with_key(KEY_B64).await;
    let alice = app.register_user("alice").await;
    let bob = app.register_extra_user("bob").await;

    let response = save(&app, &alice, "remanga", Some("alice-token"), None).await;
    assert_eq!(response.status(), StatusCode::OK);

    let list = body_json(
        app.request(with_cookie(get("/api/me/source-credentials"), &bob))
            .await,
    )
    .await;
    let remanga = list["credentials"]
        .as_array()
        .expect("credentials array")
        .iter()
        .find(|item| item["code"] == "remanga")
        .expect("remanga entry");
    assert_eq!(remanga["configured"], false, "bob has no credential");

    let cipher = app
        .state
        .source_credentials
        .as_deref()
        .expect("credential cipher");
    let remanga_id = source_id(&app, "remanga").await;
    assert!(
        service::for_user(&app.pool, cipher, user_id(&app, "bob").await, remanga_id)
            .await
            .expect("decrypt")
            .is_none(),
        "bob must not decrypt alice's credential"
    );

    // DELETE Боба не трогает учётку Алисы.
    let response = app
        .request(with_cookie(
            delete_request("/api/me/source-credentials/remanga"),
            &bob,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let alice_auth = service::for_user(&app.pool, cipher, user_id(&app, "alice").await, remanga_id)
        .await
        .expect("decrypt")
        .expect("still configured");
    assert_eq!(alice_auth.bearer(), Some("alice-token"));
}

#[tokio::test]
async fn replace_overwrites_payload_and_delete_clears_it() {
    let app = app_with_key(KEY_B64).await;
    let alice = app.register_user("alice").await;

    assert_eq!(
        save(&app, &alice, "remanga", Some("first"), Some("sid=one"))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        save(&app, &alice, "remanga", Some("second"), None)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(credential_rows(&app).await, 1, "upsert keeps one row");

    let cipher = app
        .state
        .source_credentials
        .as_deref()
        .expect("credential cipher");
    let auth = service::for_user(
        &app.pool,
        cipher,
        user_id(&app, "alice").await,
        source_id(&app, "remanga").await,
    )
    .await
    .expect("decrypt")
    .expect("configured");
    assert_eq!(auth.bearer(), Some("second"));
    assert_eq!(auth.cookie(), None, "payload is replaced, not merged");

    let response = app
        .request(with_cookie(
            delete_request("/api/me/source-credentials/remanga"),
            &alice,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(credential_rows(&app).await, 0);

    let list = body_json(
        app.request(with_cookie(get("/api/me/source-credentials"), &alice))
            .await,
    )
    .await;
    let remanga = list["credentials"]
        .as_array()
        .expect("credentials array")
        .iter()
        .find(|item| item["code"] == "remanga")
        .expect("remanga entry");
    assert_eq!(remanga["configured"], false);
}

#[tokio::test]
async fn routes_require_configured_key() {
    // Config::default() без SOURCE_CREDENTIALS_KEY: функция выключена.
    let app = TestApp::new().await;
    let alice = app.register_user("alice").await;

    let response = app
        .request(with_cookie(get("/api/me/source-credentials"), &alice))
        .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = body_json(response).await;
    assert_eq!(body["code"], "credentials_unavailable");

    let response = save(&app, &alice, "remanga", Some("token"), None).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body_json(response).await["code"], "credentials_unavailable");

    let response = app
        .request(with_cookie(
            delete_request("/api/me/source-credentials/remanga"),
            &alice,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn rejects_unknown_unsupported_and_empty_input() {
    let app = app_with_key(KEY_B64).await;
    let alice = app.register_user("alice").await;

    let response = save(&app, &alice, "unknown-source", Some("token"), None).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(body_json(response).await["code"], "source_not_found");

    // RuMix не объявляет Capability::Auth на этом этапе.
    let response = save(&app, &alice, "rumix", Some("token"), None).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(response).await["code"],
        "source_credentials_invalid"
    );

    let response = save(&app, &alice, "remanga", None, None).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_json(response).await;
    assert_eq!(body["code"], "source_credentials_invalid");

    let long = "x".repeat(4 * 1024 + 1);
    let response = save(&app, &alice, "remanga", Some(&long), None).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(response).await["code"],
        "source_credentials_invalid"
    );
}
