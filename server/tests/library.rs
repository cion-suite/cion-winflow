mod common;

use axum::http::{Method, StatusCode};
use serde_json::json;

use common::{TestApp, body_json, get, json_request, with_cookie};

async fn put_library(
    app: &TestApp,
    cookie: &str,
    title_id: i64,
    body: &serde_json::Value,
) -> axum::response::Response {
    app.request(with_cookie(
        json_request(Method::PUT, &format!("/api/library/{title_id}"), body),
        cookie,
    ))
    .await
}

#[tokio::test]
async fn library_requires_session() {
    let app = TestApp::new().await;
    let response = app.request(get("/api/library")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn library_add_update_list_delete() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    let title_id = app.create_title(&admin, "Some Manga", "manga").await;
    let user = app.register_extra_user("reader").await;

    let response = put_library(
        &app,
        &user,
        title_id,
        &json!({ "status": "in_progress", "score": 8 }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let entry = body_json(response).await;
    assert_eq!(entry["title_id"], title_id);
    assert_eq!(entry["status"], "in_progress");
    assert_eq!(entry["score"], 8);

    let response = app.request(with_cookie(get("/api/library"), &user)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let list = body_json(response).await;
    let items = list["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title_id"], title_id);
    assert_eq!(items[0]["title"]["name"], "Some Manga");
    assert_eq!(items[0]["title"]["kind"], "manga");
    assert_eq!(items[0]["status"], "in_progress");

    let response = app
        .request(with_cookie(get(&format!("/api/titles/{title_id}")), &user))
        .await;
    let title = body_json(response).await;
    assert_eq!(title["library"]["status"], "in_progress");
    assert_eq!(title["library"]["score"], 8);

    // Повторный PUT обновляет запись, а не создаёт дубль
    let response = put_library(
        &app,
        &user,
        title_id,
        &json!({ "status": "completed", "score": null }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let entry = body_json(response).await;
    assert_eq!(entry["status"], "completed");
    assert!(entry["score"].is_null());

    let response = app.request(with_cookie(get("/api/library"), &user)).await;
    let list = body_json(response).await;
    assert_eq!(list["items"].as_array().expect("items").len(), 1);

    let response = app
        .request(with_cookie(
            json_request(
                Method::DELETE,
                &format!("/api/library/{title_id}"),
                &json!({}),
            ),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = app.request(with_cookie(get("/api/library"), &user)).await;
    let list = body_json(response).await;
    assert_eq!(list["items"].as_array().expect("items").len(), 0);

    let response = app
        .request(with_cookie(get(&format!("/api/titles/{title_id}")), &user))
        .await;
    let title = body_json(response).await;
    assert!(title["library"].is_null());

    // Удаление идемпотентно
    let response = app
        .request(with_cookie(
            json_request(
                Method::DELETE,
                &format!("/api/library/{title_id}"),
                &json!({}),
            ),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn library_is_per_user() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    let title_id = app.create_title(&admin, "Shared Title", "book").await;
    let first = app.register_extra_user("reader-one").await;
    let second = app.register_extra_user("reader-two").await;

    let response = put_library(
        &app,
        &first,
        title_id,
        &json!({ "status": "planned", "score": 5 }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = app.request(with_cookie(get("/api/library"), &first)).await;
    let list = body_json(response).await;
    assert_eq!(list["items"].as_array().expect("items").len(), 1);

    let response = app.request(with_cookie(get("/api/library"), &second)).await;
    let list = body_json(response).await;
    assert_eq!(list["items"].as_array().expect("items").len(), 0);

    // Второй пользователь не видит чужую запись в карточке тайтла
    let response = app
        .request(with_cookie(
            get(&format!("/api/titles/{title_id}")),
            &second,
        ))
        .await;
    let title = body_json(response).await;
    assert!(title["library"].is_null());

    let response = put_library(
        &app,
        &second,
        title_id,
        &json!({ "status": "completed", "score": 9 }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    // Первый пользователь по-прежнему видит свою запись
    let response = app
        .request(with_cookie(get(&format!("/api/titles/{title_id}")), &first))
        .await;
    let title = body_json(response).await;
    assert_eq!(title["library"]["status"], "planned");
    assert_eq!(title["library"]["score"], 5);

    // Удаление у второго не трогает запись первого
    let response = app
        .request(with_cookie(
            json_request(
                Method::DELETE,
                &format!("/api/library/{title_id}"),
                &json!({}),
            ),
            &second,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = app.request(with_cookie(get("/api/library"), &first)).await;
    let list = body_json(response).await;
    assert_eq!(list["items"].as_array().expect("items").len(), 1);

    let response = app.request(with_cookie(get("/api/library"), &second)).await;
    let list = body_json(response).await;
    assert_eq!(list["items"].as_array().expect("items").len(), 0);
}

#[tokio::test]
async fn library_paginates() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    let user = app.register_extra_user("reader").await;

    for name in ["One", "Two", "Three"] {
        let title_id = app.create_title(&admin, name, "manga").await;
        let response = put_library(
            &app,
            &user,
            title_id,
            &json!({ "status": "planned", "score": null }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    let response = app
        .request(with_cookie(get("/api/library?page_size=2&page=1"), &user))
        .await;
    let json = body_json(response).await;
    assert_eq!(json["total"], 3);
    assert_eq!(json["page"], 1);
    assert_eq!(json["page_size"], 2);
    assert_eq!(json["items"].as_array().expect("items").len(), 2);

    let response = app
        .request(with_cookie(get("/api/library?page_size=2&page=2"), &user))
        .await;
    let json = body_json(response).await;
    assert_eq!(json["items"].as_array().expect("items").len(), 1);

    let response = app
        .request(with_cookie(get("/api/library?page=0"), &user))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn library_rejects_bad_input() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    let title_id = app.create_title(&admin, "Some Manga", "manga").await;
    let user = app.register_extra_user("reader").await;

    let response = put_library(
        &app,
        &user,
        title_id,
        &json!({ "status": "reading", "score": null }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = body_json(response).await;
    assert_eq!(json["code"], "library_status_invalid");

    for score in [0, 11] {
        let response = put_library(
            &app,
            &user,
            title_id,
            &json!({ "status": "planned", "score": score }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let json = body_json(response).await;
        assert_eq!(json["code"], "score_invalid");
    }

    let response = put_library(
        &app,
        &user,
        999,
        &json!({ "status": "planned", "score": null }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let json = body_json(response).await;
    assert_eq!(json["code"], "title_not_found");
}
