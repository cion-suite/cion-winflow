//! Полный список глав Mangamir: страница отдаёт свёрнутый список, а
//! недостающие главы приходят ответом Livewire на `showAllChapters()`.

use std::sync::{Arc, Mutex};

use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use cion_reader::sources::{
    ExternalId,
    http::{AdapterCtx, metadata_client},
    mangamir::{Mangamir, MangamirConfig},
};
use reqwest::Url;

const PAGE: &str = include_str!("../src/sources/mangamir/fixtures/livewire_title.html");
const UPDATE: &str = include_str!("../src/sources/mangamir/fixtures/livewire_update.json");

#[derive(Default)]
struct Captured {
    method: Option<String>,
    cookie: Option<String>,
    body: Option<Vec<u8>>,
}

async fn page() -> Response {
    // В фикстуре адрес Livewire абсолютный; тест подменяет его относительным,
    // который адаптер обязан разрешить от base_url.
    let body = PAGE.replace(
        r#""https:\/\/mangamir.com\/livewire-e4136179\/update""#,
        r#""/livewire-test/update""#,
    );

    (
        StatusCode::OK,
        [(
            header::SET_COOKIE,
            "manga_mir_session=test-session; Path=/; HttpOnly",
        )],
        body,
    )
        .into_response()
}

async fn update(
    State(captured): State<Arc<Mutex<Captured>>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let mut state = captured.lock().unwrap_or_else(|poison| poison.into_inner());
    state.method = Some("POST".to_owned());
    state.cookie = headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    state.body = Some(body.to_vec());
    drop(state);

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        UPDATE,
    )
        .into_response()
}

async fn start() -> (String, Arc<Mutex<Captured>>) {
    let captured = Arc::new(Mutex::new(Captured::default()));
    let router = Router::new()
        .route("/manga/{slug}", get(page))
        .route("/livewire-test/update", post(update))
        .with_state(captured.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    (format!("http://{address}"), captured)
}

#[tokio::test]
async fn chapters_are_requested_through_livewire() {
    let (base, captured) = start().await;

    let client = metadata_client("cion-reader-test/0.1", true).expect("http client");
    let adapter = Mangamir::new(
        AdapterCtx::new(Arc::new(client), Url::parse(&base).expect("url")),
        MangamirConfig::default(),
    )
    .expect("adapter");

    let external_id = ExternalId::new("vo-vseoruzhii").expect("external id");
    let chapters = adapter
        .chapters(&external_id, None)
        .await
        .expect("chapters");

    // Страница содержит 212, 3, 1; ответ Livewire — 212, 5, 1. Полный список
    // должен прийти именно из Livewire.
    let keys: Vec<&str> = chapters
        .iter()
        .map(|chapter| chapter.chapter_key.as_str())
        .collect();
    assert_eq!(keys, ["v1:c212", "v1:c5", "v1:c1"]);

    let state = captured.lock().unwrap_or_else(|poison| poison.into_inner());
    assert_eq!(state.method.as_deref(), Some("POST"));
    assert!(
        state
            .cookie
            .as_deref()
            .is_some_and(|cookie| cookie.contains("manga_mir_session=test-session")),
        "POST must reuse the GET session: {:?}",
        state.cookie
    );

    let body = String::from_utf8_lossy(state.body.as_deref().expect("body"));
    assert!(body.contains("\"method\":\"showAllChapters\""), "{body}");
    assert!(body.contains("\"snapshot\":"), "{body}");
    assert!(body.contains("test-csrf-token"), "{body}");
    assert!(body.contains("\"island\""), "{body}");
}
