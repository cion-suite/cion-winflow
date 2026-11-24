use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use cion_reader::app::{AppState, router};
use cion_reader::config::Config;

fn state(dir: &std::path::Path) -> AppState {
    AppState::new(Config {
        bind_addr: "127.0.0.1:0".parse().expect("test address"),
        static_dir: dir.to_path_buf(),
    })
}

fn spa_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("index.html"), "<!doctype html><p>spa</p>").expect("index");
    std::fs::create_dir_all(dir.path().join("assets")).expect("assets dir");
    std::fs::write(dir.path().join("assets/app-1.js"), "console.log(1);").expect("asset");
    std::fs::write(dir.path().join("theme.js"), "/* theme */").expect("theme");
    dir
}

async fn send(dir: &std::path::Path, request: Request<Body>) -> axum::response::Response {
    router(state(dir)).oneshot(request).await.expect("response")
}

async fn body(response: axum::response::Response) -> Vec<u8> {
    response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes()
        .to_vec()
}

fn get(uri: &str) -> axum::http::request::Builder {
    Request::get(uri)
}

#[tokio::test]
async fn health_returns_ok_json() {
    let dir = spa_dir();
    let response = send(
        dir.path(),
        get("/api/health").body(Body::empty()).expect("request"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    let json: serde_json::Value = serde_json::from_slice(&body(response).await).expect("json");
    assert_eq!(json["status"], "ok");
    assert!(json["version"].is_string());
}

#[tokio::test]
async fn unknown_api_route_is_json_404_even_with_navigation_headers() {
    let dir = spa_dir();
    let response = send(
        dir.path(),
        get("/api/missing")
            .header(header::ACCEPT, "text/html")
            .header("sec-fetch-mode", "navigate")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    let json: serde_json::Value = serde_json::from_slice(&body(response).await).expect("json");
    assert_eq!(json["code"], "not_found");
    assert_eq!(json["message"], "unknown API route");
}

#[tokio::test]
async fn navigation_gets_spa_even_for_dotted_paths() {
    let dir = spa_dir();
    let response = send(
        dir.path(),
        get("/reader/1/c12.5")
            .header(header::ACCEPT, "text/html")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/html; charset=utf-8"
    );
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");
    assert!(
        String::from_utf8(body(response).await)
            .expect("utf8")
            .contains("spa")
    );
}

#[tokio::test]
async fn missing_asset_is_404_even_for_navigation() {
    let dir = spa_dir();
    let response = send(
        dir.path(),
        get("/assets/missing.js")
            .header(header::ACCEPT, "text/html")
            .header("sec-fetch-mode", "navigate")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn script_request_for_missing_asset_is_not_spa() {
    let dir = spa_dir();
    let response = send(
        dir.path(),
        get("/assets/missing.js")
            .header(header::ACCEPT, "*/*")
            .header("sec-fetch-mode", "no-cors")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn assets_are_immutable_and_index_is_not_cached() {
    let dir = spa_dir();
    let asset = send(
        dir.path(),
        get("/assets/app-1.js")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(asset.status(), StatusCode::OK);
    assert_eq!(
        asset.headers()[header::CACHE_CONTROL],
        "public, max-age=31536000, immutable"
    );

    let index = send(
        dir.path(),
        get("/")
            .header(header::ACCEPT, "text/html")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(index.status(), StatusCode::OK);
    assert_eq!(index.headers()[header::CACHE_CONTROL], "no-cache");
    assert!(
        String::from_utf8(body(index).await)
            .expect("utf8")
            .contains("spa")
    );
}

#[tokio::test]
async fn root_static_files_are_served_without_cache() {
    let dir = spa_dir();
    let response = send(
        dir.path(),
        get("/theme.js").body(Body::empty()).expect("request"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");
    assert!(
        String::from_utf8(body(response).await)
            .expect("utf8")
            .contains("theme")
    );
}
