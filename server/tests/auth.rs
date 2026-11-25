mod common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use serde_json::json;

use cion_reader::config::{Config, RegistrationMode};

use common::{
    ADMIN_PASSWORD, ADMIN_USERNAME, ORIGIN, TestApp, USER_PASSWORD, body_json, get, json_request,
    password_hash, session_token, with_cookie, with_origin, with_peer, without_origin,
};

#[tokio::test]
async fn me_without_session_is_401_json() {
    let app = TestApp::new().await;
    let response = app.request(get("/api/me")).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let json = body_json(response).await;
    assert_eq!(json["code"], "unauthorized");
}

#[tokio::test]
async fn mutation_without_origin_is_forbidden() {
    let app = TestApp::new().await;
    let request = without_origin(json_request(
        Method::POST,
        "/api/auth/login",
        &json!({ "username": "someone", "password": "some-password" }),
    ));
    let response = app.request(request).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let json = body_json(response).await;
    assert_eq!(json["code"], "origin_not_allowed");
}

#[tokio::test]
async fn mutation_with_foreign_origin_is_forbidden() {
    let app = TestApp::new().await;
    let request = with_origin(
        json_request(
            Method::POST,
            "/api/auth/login",
            &json!({ "username": "someone", "password": "some-password" }),
        ),
        "http://evil.example",
    );
    let response = app.request(request).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let json = body_json(response).await;
    assert_eq!(json["code"], "origin_not_allowed");
}

#[tokio::test]
async fn register_login_me_logout_flow() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin_cookie = app.login_admin().await;
    let invite = app.create_invite(&admin_cookie).await;

    let response = app.register("reader", USER_PASSWORD, &invite).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let register_cookie = session_token(&response);
    let json = body_json(response).await;
    assert_eq!(json["user"]["username"], "reader");
    assert_eq!(json["user"]["role"], "user");

    let me = app
        .request(with_cookie(get("/api/me"), &register_cookie))
        .await;
    assert_eq!(me.status(), StatusCode::OK);
    let me_json = body_json(me).await;
    assert_eq!(me_json["user"]["username"], "reader");

    let logout = app
        .request(with_cookie(
            json_request(Method::POST, "/api/auth/logout", &json!({})),
            &register_cookie,
        ))
        .await;
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);

    let me_after_logout = app
        .request(with_cookie(get("/api/me"), &register_cookie))
        .await;
    assert_eq!(me_after_logout.status(), StatusCode::UNAUTHORIZED);

    let response = app.login("reader", USER_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::OK);
    let login_cookie = session_token(&response);
    let me_after_login = app
        .request(with_cookie(get("/api/me"), &login_cookie))
        .await;
    assert_eq!(me_after_login.status(), StatusCode::OK);
}

#[tokio::test]
async fn password_is_stored_as_argon2_hash() {
    let app = TestApp::new().await;
    let _cookie = app.register_user("reader").await;

    let hash = password_hash(&app.pool, "reader").await;
    assert!(hash.starts_with("$argon2id$"), "unexpected hash: {hash}");
    assert!(!hash.contains(USER_PASSWORD));
}

#[tokio::test]
async fn invite_cannot_be_used_twice() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin_cookie = app.login_admin().await;
    let invite = app.create_invite(&admin_cookie).await;

    let first = app.register("reader", USER_PASSWORD, &invite).await;
    assert_eq!(first.status(), StatusCode::CREATED);

    let second = app.register("other", USER_PASSWORD, &invite).await;
    assert_eq!(second.status(), StatusCode::BAD_REQUEST);
    let json = body_json(second).await;
    assert_eq!(json["code"], "invalid_invite");
}

#[tokio::test]
async fn unknown_invite_is_bad_request() {
    let app = TestApp::new().await;
    let response = app
        .register("reader", USER_PASSWORD, "not-a-real-invite")
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = body_json(response).await;
    assert_eq!(json["code"], "invalid_invite");
}

#[tokio::test]
async fn closed_registration_is_forbidden() {
    let app = TestApp::with_config(Config {
        registration_mode: RegistrationMode::Closed,
        ..Config::default()
    })
    .await;
    let response = app.register("reader", USER_PASSWORD, "any-invite").await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let json = body_json(response).await;
    assert_eq!(json["code"], "registration_closed");
}

#[tokio::test]
async fn session_cookie_has_security_flags() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let response = app.login(ADMIN_USERNAME, ADMIN_PASSWORD).await;

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookie = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect::<Vec<_>>()
        .join("; ");

    assert!(set_cookie.contains("sid="), "no sid cookie: {set_cookie}");
    assert!(set_cookie.contains("HttpOnly"), "no HttpOnly: {set_cookie}");
    assert!(
        set_cookie.contains("SameSite=Lax"),
        "no SameSite: {set_cookie}"
    );
    assert!(set_cookie.contains("Path=/"), "no Path: {set_cookie}");
    assert!(set_cookie.contains("Max-Age="), "no Max-Age: {set_cookie}");
    assert!(!set_cookie.contains("Secure"), "unexpected Secure in dev");
}

#[tokio::test]
async fn disabled_user_session_is_rejected() {
    let app = TestApp::new().await;
    let cookie = app.register_user("reader").await;

    sqlx::query("UPDATE users SET disabled_at = ? WHERE username = ?")
        .bind(cion_reader::auth::session::now_unix())
        .bind("reader")
        .execute(&app.pool)
        .await
        .expect("disable user");

    let response = app.request(with_cookie(get("/api/me"), &cookie)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_invites_require_admin_role() {
    let app = TestApp::new().await;
    let user_cookie = app.register_user("reader").await;

    let forbidden = app
        .request(with_cookie(get("/api/admin/invites"), &user_cookie))
        .await;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    let json = body_json(forbidden).await;
    assert_eq!(json["code"], "forbidden");

    let admin_cookie = app.login_admin().await;
    let allowed = app
        .request(with_cookie(get("/api/admin/invites"), &admin_cookie))
        .await;
    assert_eq!(allowed.status(), StatusCode::OK);
    let json = body_json(allowed).await;
    assert!(json["items"].is_array());
}

#[tokio::test]
async fn login_is_rate_limited_by_ip() {
    let app = TestApp::new().await;
    let mut limited = false;

    // Квота пополняется со временем, а Argon2 занимает CPU: ждём 429
    // в пределах разумного числа быстрых попыток с разных username.
    for index in 0..30 {
        let request = json_request(
            Method::POST,
            "/api/auth/login",
            &json!({
                "username": format!("nobody-{index}"),
                "password": "wrong-password",
            }),
        );
        let response = app.request(request).await;

        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            let json = body_json(response).await;
            assert_eq!(json["code"], "rate_limited");
            limited = true;
            break;
        }

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "request {index}"
        );
    }

    assert!(limited, "login must be rate limited by IP");
}

#[tokio::test]
async fn login_is_rate_limited_by_username() {
    let app = TestApp::new().await;

    for index in 0..10 {
        let request = with_peer(
            json_request(
                Method::POST,
                "/api/auth/login",
                &json!({ "username": "target-user", "password": "wrong-password" }),
            ),
            &format!("10.0.0.{}", index + 1),
        );
        let response = app.request(request).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "request {index}"
        );
    }

    let request = with_peer(
        json_request(
            Method::POST,
            "/api/auth/login",
            &json!({ "username": "target-user", "password": "wrong-password" }),
        ),
        "10.0.0.11",
    );
    let response = app.request(request).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let json = body_json(response).await;
    assert_eq!(json["code"], "rate_limited");
}

#[tokio::test]
async fn bootstrap_admin_runs_only_on_empty_database() {
    let app = TestApp::new().await;
    app.create_admin().await;

    let created = cion_reader::auth::bootstrap_admin(
        &app.pool,
        &cion_reader::config::AdminBootstrap {
            username: "another-admin".to_owned(),
            password: "another-password-123".to_owned(),
        },
    )
    .await
    .expect("bootstrap");
    assert!(!created);
}

#[tokio::test]
async fn malformed_json_is_api_error_envelope() {
    let app = TestApp::new().await;

    let request = with_peer(
        Request::builder()
            .method(Method::POST)
            .uri("/api/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ORIGIN, ORIGIN)
            .header("sec-fetch-site", "same-origin")
            .body(Body::from("not json"))
            .expect("request"),
        "127.0.0.1",
    );
    let response = app.request(request).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    let json = body_json(response).await;
    assert_eq!(json["code"], "validation");

    let response = app
        .request(json_request(Method::POST, "/api/auth/login", &json!({})))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = body_json(response).await;
    assert_eq!(json["code"], "validation");
}

#[tokio::test]
async fn login_with_wrong_password_is_401() {
    let app = TestApp::new().await;
    app.create_admin().await;

    let response = app.login(ADMIN_USERNAME, "wrong-password").await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let json = body_json(response).await;
    assert_eq!(json["code"], "invalid_credentials");
    assert_eq!(json["message"], "invalid username or password");
}
