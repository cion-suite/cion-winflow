//! Phase 2: контент и картинки помеченной главы гейтятся per-user.

mod common;

use axum::http::StatusCode;

use common::{body_json, get, remanga, with_cookie};

async fn import_public(fixture: &remanga::AuthFixture) -> (i64, String) {
    let app = &fixture.app;
    let response = remanga::import(app, &fixture.alice, "public").await;
    assert_eq!(response.status(), StatusCode::OK);
    let title_id = body_json(response).await["title_id"]
        .as_i64()
        .expect("title id");
    let key = remanga::chapter_key(app, &fixture.alice, title_id).await;
    (title_id, key)
}

#[tokio::test]
async fn content_and_images_are_gated_per_user() {
    let fixture = remanga::auth_fixture().await;
    let app = &fixture.app;
    let (title_id, key) = import_public(&fixture).await;

    fixture
        .stub
        .set_mode(|mode| mode.content_requires_auth = true);

    // Без учётки: 401 источника → «подключите аккаунт», глава не помечается.
    let response = remanga::chapter_content(app, &fixture.alice, title_id, &key).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        body_json(response).await["code"],
        "source_auth_not_configured"
    );
    assert!(
        !remanga::chapter_flags(app, title_id).await.0,
        "anonymous 401 must not mark the chapter"
    );

    // С учёткой: анонимная попытка, затем авторизованная; глава помечена.
    assert_eq!(
        remanga::save_credential(app, &fixture.alice, "good-token").await,
        StatusCode::OK
    );
    let response = remanga::chapter_content(app, &fixture.alice, title_id, &key).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let page_id = body["content"]["pages"][0]["id"].as_i64().expect("page id");
    assert!(
        remanga::chapter_flags(app, title_id).await.0,
        "chapter is auth-gated"
    );

    // Другой пользователь без учётки не получает ни контент, ни кэш Алисы.
    let bob = app.register_extra_user("bob").await;
    let response = remanga::chapter_content(app, &bob, title_id, &key).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        body_json(response).await["code"],
        "source_auth_not_configured"
    );

    // Со своей учёткой Боб открывает главу; после успешной проверки
    // повторное открытие идёт из кэша без нового запроса.
    assert_eq!(
        remanga::save_credential(app, &bob, "good-token").await,
        StatusCode::OK
    );
    let response = remanga::chapter_content(app, &bob, title_id, &key).await;
    assert_eq!(response.status(), StatusCode::OK, "Bob's first open");
    let content_requests_before = fixture
        .stub
        .requests_for("/api/v2/titles/chapters/101/")
        .len();
    let response = remanga::chapter_content(app, &bob, title_id, &key).await;
    assert_eq!(response.status(), StatusCode::OK);
    let content_requests_after = fixture
        .stub
        .requests_for("/api/v2/titles/chapters/101/")
        .len();
    assert_eq!(
        content_requests_before, content_requests_after,
        "verified credential serves cached pages without a request"
    );

    // Картинки: без учётки — отказ; при 403 картинки глава не становится locked.
    let charlie = app.register_extra_user("charlie").await;
    let response = app
        .request(with_cookie(
            get(&format!("/api/pages/{page_id}/image")),
            &charlie,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    fixture.stub.set_mode(|mode| mode.image_forbidden = true);
    let response = app
        .request(with_cookie(
            get(&format!("/api/pages/{page_id}/image")),
            &bob,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(body_json(response).await["code"], "source_blocked");
    let (_, availability, is_locked) = remanga::chapter_flags(app, title_id).await;
    assert_ne!(availability, "locked", "403 image is not a global lock");
    assert!(!is_locked);

    // Неверная учётка Боба — персональная ошибка, не 401 наружу.
    assert_eq!(
        remanga::save_credential(app, &bob, "wrong-token").await,
        StatusCode::OK
    );
    let response = remanga::chapter_content(app, &bob, title_id, &key).await;
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(body_json(response).await["code"], "source_auth_invalid");
    assert_eq!(
        remanga::credential_last_error(app, "bob").await.as_deref(),
        Some("auth_invalid")
    );

    // Удаление учётки возвращает пользователя к «не подключено».
    let response = app
        .request(with_cookie(
            remanga::delete_request("/api/me/source-credentials/remanga"),
            &bob,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = remanga::chapter_content(app, &bob, title_id, &key).await;
    assert_eq!(
        body_json(response).await["code"],
        "source_auth_not_configured"
    );
}

#[tokio::test]
async fn direct_content_requires_the_users_own_account() {
    let fixture = remanga::auth_fixture().await;
    let app = &fixture.app;
    let (title_id, key) = import_public(&fixture).await;

    fixture
        .stub
        .set_mode(|mode| mode.content_requires_auth = true);
    assert_eq!(
        remanga::save_credential(app, &fixture.alice, "good-token").await,
        StatusCode::OK
    );
    assert_eq!(
        remanga::chapter_content(app, &fixture.alice, title_id, &key)
            .await
            .status(),
        StatusCode::OK
    );

    let title = remanga::title_json(app, &fixture.alice, title_id).await;
    let chapter_id = title["chapters"][0]["selected_chapter_id"]
        .as_i64()
        .expect("chapter id");

    let bob = app.register_extra_user("bob").await;
    let response = remanga::direct_content(app, &bob, chapter_id).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        body_json(response).await["code"],
        "source_auth_not_configured"
    );

    assert_eq!(
        remanga::save_credential(app, &bob, "good-token").await,
        StatusCode::OK
    );
    let response = remanga::direct_content(app, &bob, chapter_id).await;
    assert_eq!(response.status(), StatusCode::OK);
}
