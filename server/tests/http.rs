mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use common::{TestApp, body_json, get, with_cookie};

#[tokio::test]
async fn health_returns_ok_json_without_session() {
    let app = TestApp::new().await;
    let response = app.request(get("/api/health")).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    let json = body_json(response).await;
    assert_eq!(json["status"], "ok");
    assert!(json["version"].is_string());
}

#[tokio::test]
async fn unknown_api_route_without_session_is_json_401() {
    let app = TestApp::new().await;
    let request = Request::builder()
        .uri("/api/missing")
        .header(header::ACCEPT, "text/html")
        .header("sec-fetch-mode", "navigate")
        .body(Body::empty())
        .expect("request");
    let response = app.request(request).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    let json = body_json(response).await;
    assert_eq!(json["code"], "unauthorized");
}

#[tokio::test]
async fn unknown_api_route_with_session_is_json_404() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let cookie = app.login_admin().await;

    let request = with_cookie(
        Request::builder()
            .uri("/api/missing")
            .header(header::ACCEPT, "text/html")
            .header("sec-fetch-mode", "navigate")
            .body(Body::empty())
            .expect("request"),
        &cookie,
    );
    let response = app.request(request).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    let json = body_json(response).await;
    assert_eq!(json["code"], "not_found");
}

#[tokio::test]
async fn navigation_gets_spa_even_for_dotted_paths() {
    let app = TestApp::new().await;
    let request = Request::builder()
        .uri("/reader/1/c12.5")
        .header(header::ACCEPT, "text/html")
        .body(Body::empty())
        .expect("request");
    let response = app.request(request).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/html; charset=utf-8"
    );
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");
    assert!(
        String::from_utf8(
            axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body")
                .to_vec()
        )
        .expect("utf8")
        .contains("spa")
    );
}

#[tokio::test]
async fn login_page_is_spa_navigation() {
    let app = TestApp::new().await;
    let request = Request::builder()
        .uri("/login")
        .header(header::ACCEPT, "text/html")
        .body(Body::empty())
        .expect("request");
    let response = app.request(request).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/html; charset=utf-8"
    );
}

#[tokio::test]
async fn missing_asset_is_404_even_for_navigation() {
    let app = TestApp::new().await;
    let request = Request::builder()
        .uri("/assets/missing.js")
        .header(header::ACCEPT, "text/html")
        .header("sec-fetch-mode", "navigate")
        .body(Body::empty())
        .expect("request");
    let response = app.request(request).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn script_request_for_missing_asset_is_not_spa() {
    let app = TestApp::new().await;
    let request = Request::builder()
        .uri("/assets/missing.js")
        .header(header::ACCEPT, "*/*")
        .header("sec-fetch-mode", "no-cors")
        .body(Body::empty())
        .expect("request");
    let response = app.request(request).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn assets_are_immutable_and_index_is_not_cached() {
    let app = TestApp::new().await;
    let asset = app.request(get("/assets/app-1.js")).await;
    assert_eq!(asset.status(), StatusCode::OK);
    assert_eq!(
        asset.headers()[header::CACHE_CONTROL],
        "public, max-age=31536000, immutable"
    );

    let request = Request::builder()
        .uri("/")
        .header(header::ACCEPT, "text/html")
        .body(Body::empty())
        .expect("request");
    let index = app.request(request).await;
    assert_eq!(index.status(), StatusCode::OK);
    assert_eq!(index.headers()[header::CACHE_CONTROL], "no-cache");
}

#[tokio::test]
async fn root_static_files_are_served_without_cache() {
    let app = TestApp::new().await;
    let response = app.request(get("/theme.js")).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");
    assert!(
        String::from_utf8(
            axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body")
                .to_vec()
        )
        .expect("utf8")
        .contains("theme")
    );
}
