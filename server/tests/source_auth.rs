//! Phase 2: учётки источников в запросах Remanga и честные auth-ошибки.
//! Локальный двойник API проверяет заголовки, коды ошибок и гейтинг.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::json;

use common::remanga::GOOD_BEARER;
use common::{body_json, get, json_request, remanga, with_cookie};

#[tokio::test]
async fn credentials_are_sent_to_api_but_not_to_image_cdn() {
    let fixture = remanga::auth_fixture().await;
    let app = &fixture.app;

    assert_eq!(
        remanga::save_credential(app, &fixture.admin, "good-token").await,
        StatusCode::OK
    );

    // Поиск: заголовки уходят на API.
    let response = app
        .request(with_cookie(
            get("/api/discover/search?q=Public"),
            &fixture.admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    // Импорт дергает title и chapters — оба должны быть авторизованы.
    let response = remanga::import(app, &fixture.admin, "public").await;
    assert_eq!(response.status(), StatusCode::OK, "import public");
    let body = body_json(response).await;
    let title_id = body["title_id"].as_i64().expect("title id");

    for path in [
        "/api/v2/search/",
        "/api/v2/titles/public/",
        "/api/v2/titles/chapters/",
    ] {
        let captured = fixture.stub.requests_for(path);
        assert!(!captured.is_empty(), "request to {path} was made");
        for request in captured {
            assert_eq!(
                request.authorization.as_deref(),
                Some(GOOD_BEARER),
                "Authorization on {path}"
            );
            assert_eq!(
                request.cookie.as_deref(),
                Some("sid=secret-cookie"),
                "Cookie on {path}"
            );
        }
    }

    // Контент главы: сначала анонимно, но источник требует вход, поэтому
    // адаптер повторяет запрос с учёткой.
    fixture
        .stub
        .set_mode(|mode| mode.content_requires_auth = true);
    let key = remanga::chapter_key(app, &fixture.admin, title_id).await;
    let response = remanga::chapter_content(app, &fixture.admin, title_id, &key).await;
    assert_eq!(response.status(), StatusCode::OK, "content");
    let content_body = body_json(response).await;
    let page_id = content_body["content"]["pages"][0]["id"]
        .as_i64()
        .expect("page id");

    let captured = fixture.stub.requests_for("/api/v2/titles/chapters/101/");
    assert!(
        captured
            .iter()
            .any(|request| request.authorization.as_deref() == Some(GOOD_BEARER)),
        "content retry is authorized"
    );

    // Картинка лежит на другом хосте: сессионные заголовки туда не уходят.
    let response = app
        .request(with_cookie(
            get(&format!("/api/pages/{page_id}/image")),
            &fixture.admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "page image");

    let images = fixture
        .stub
        .image_requests
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    assert!(!images.is_empty(), "image was requested");
    for request in images {
        assert!(
            request.authorization.is_none(),
            "CDN must not receive Authorization"
        );
        assert!(request.cookie.is_none(), "CDN must not receive Cookie");
    }
}

#[tokio::test]
async fn auth_errors_are_distinguished_and_never_http_401() {
    let fixture = remanga::auth_fixture().await;
    let app = &fixture.app;

    // Без учётки: скрытый 404 — «возможно, нужен вход», явный 401 — «нужна
    // учётка», 403 — блокировка сайта, а не auth.
    let response = remanga::import(app, &fixture.alice, "hidden").await;
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "never HTTP 401"
    );
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        body_json(response).await["code"],
        "source_auth_maybe_required"
    );

    let response = remanga::import(app, &fixture.alice, "locked").await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        body_json(response).await["code"],
        "source_auth_not_configured"
    );

    let response = remanga::import(app, &fixture.alice, "blocked").await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(body_json(response).await["code"], "source_blocked");

    // Неверная учётка: 401 от источника становится 502 source_auth_invalid,
    // а 404 с учёткой — окончательным source_not_found.
    assert_eq!(
        remanga::save_credential(app, &fixture.alice, "wrong-token").await,
        StatusCode::OK
    );

    let response = remanga::import(app, &fixture.alice, "locked").await;
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(body_json(response).await["code"], "source_auth_invalid");

    let response = remanga::import(app, &fixture.alice, "hidden").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(body_json(response).await["code"], "source_not_found");

    // Правильная учётка открывает оба тайтла.
    assert_eq!(
        remanga::save_credential(app, &fixture.alice, "good-token").await,
        StatusCode::OK
    );
    assert_eq!(
        remanga::import(app, &fixture.alice, "hidden")
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        remanga::import(app, &fixture.alice, "locked")
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn sync_auth_errors_do_not_touch_global_state_or_ttl() {
    let fixture = remanga::auth_fixture().await;
    let app = &fixture.app;

    let response = remanga::import(app, &fixture.alice, "public").await;
    assert_eq!(response.status(), StatusCode::OK);
    let title_id = body_json(response).await["title_id"]
        .as_i64()
        .expect("title id");

    // Источник начинает требовать вход уже после импорта.
    fixture
        .stub
        .set_mode(|mode| mode.require_auth_for_public = true);

    let sync = |_cookie: &str| {
        json_request(
            Method::POST,
            &format!("/api/titles/{title_id}/sync"),
            &json!({}),
        )
    };

    let response = app
        .request(with_cookie(sync(&fixture.alice), &fixture.alice))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "sync is not 5xx");
    let body = body_json(response).await;
    assert_eq!(body["refreshed"], true);
    assert_eq!(body["entries"][0]["status"], "error");
    assert_eq!(
        body["entries"][0]["error_code"],
        "source_auth_not_configured"
    );

    let (auth_required, last_error) = remanga::entry_auth_required(app, title_id).await;
    assert!(auth_required, "401 without credential marks the entry");
    assert_eq!(last_error, None, "per-user error is not global");
    assert_eq!(
        remanga::metadata_synced_at(app, title_id).await,
        None,
        "auth error must not move the sync TTL"
    );

    // Неверная учётка — другой код, но глобальное состояние не меняется.
    assert_eq!(
        remanga::save_credential(app, &fixture.alice, "wrong-token").await,
        StatusCode::OK
    );
    let response = app
        .request(with_cookie(sync(&fixture.alice), &fixture.alice))
        .await;
    let body = body_json(response).await;
    assert_eq!(body["entries"][0]["error_code"], "source_auth_invalid");
    assert_eq!(remanga::entry_auth_required(app, title_id).await.1, None);
    assert_eq!(remanga::metadata_synced_at(app, title_id).await, None);

    // Другой пользователь не зависит от учётки Алисы.
    let bob = app.register_extra_user("bob").await;
    let response = app.request(with_cookie(sync(&bob), &bob)).await;
    let body = body_json(response).await;
    assert_eq!(
        body["entries"][0]["error_code"],
        "source_auth_not_configured"
    );
    assert_eq!(
        remanga::entry_auth_required(app, title_id).await.1,
        None,
        "Bob's auth error does not become global"
    );

    // Учётка Алисы снова рабочая: sync проходит и двигает TTL.
    assert_eq!(
        remanga::save_credential(app, &fixture.alice, "good-token").await,
        StatusCode::OK
    );
    let response = app
        .request(with_cookie(sync(&fixture.alice), &fixture.alice))
        .await;
    let body = body_json(response).await;
    assert_eq!(body["entries"][0]["status"], "ok");
    assert!(
        remanga::metadata_synced_at(app, title_id).await.is_some(),
        "successful sync moves the TTL"
    );
}
