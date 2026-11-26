mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use common::{TestApp, body_json, get, json_request, with_cookie};

async fn put_library(
    app: &TestApp,
    cookie: &str,
    title_id: i64,
    body: &Value,
) -> axum::response::Response {
    app.request(with_cookie(
        json_request(Method::PUT, &format!("/api/library/{title_id}"), body),
        cookie,
    ))
    .await
}

async fn post_tab(app: &TestApp, cookie: &str, name: &str) -> axum::response::Response {
    app.request(with_cookie(
        json_request(Method::POST, "/api/library/tabs", &json!({ "name": name })),
        cookie,
    ))
    .await
}

async fn create_tab(app: &TestApp, cookie: &str, name: &str) -> i64 {
    let response = post_tab(app, cookie, name).await;
    assert_eq!(response.status(), StatusCode::CREATED, "create tab {name}");
    body_json(response).await["id"].as_i64().expect("tab id")
}

#[tokio::test]
async fn custom_statuses_crud_limit_and_membership() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    let user = app.register_extra_user("reader").await;

    let first = app.create_title(&admin, "First", "manga").await;
    let second = app.create_title(&admin, "Second", "manga").await;
    for title in [first, second] {
        let response = put_library(
            &app,
            &user,
            title,
            &json!({ "status": "planned", "score": null }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    let response = app
        .request(with_cookie(get("/api/library/tabs"), &user))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let tabs = body_json(response).await;
    assert_eq!(tabs["limit"], 3);
    assert!(tabs["tabs"].as_array().expect("tabs").is_empty());

    let reading = create_tab(&app, &user, "Reading").await;
    let duplicate = post_tab(&app, &user, "Reading").await;
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
    assert_eq!(body_json(duplicate).await["code"], "library_tab_exists");

    let favorites = create_tab(&app, &user, "Favorites").await;
    let later = create_tab(&app, &user, "Later").await;

    let over_limit = post_tab(&app, &user, "Extra").await;
    assert_eq!(over_limit.status(), StatusCode::CONFLICT);
    assert_eq!(body_json(over_limit).await["code"], "library_tab_limit");

    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/library/tabs/{later}"),
                &json!({ "name": "Reading" }),
            ),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(body_json(response).await["code"], "library_tab_exists");

    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/library/tabs/{later}"),
                &json!({ "name": "Someday" }),
            ),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["name"], "Someday");

    // Своя закладка — ещё один статус, один на тайтл
    let response = put_library(
        &app,
        &user,
        first,
        &json!({ "status": "planned", "score": null, "custom_tab_id": favorites }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["custom_tab_id"], favorites);
    assert_eq!(body["status"], "planned");

    let response = app
        .request(with_cookie(get(&format!("/api/titles/{first}")), &user))
        .await;
    let title = body_json(response).await;
    assert_eq!(title["library"]["custom_tab_id"], favorites);
    assert!(title["library"].get("tab_ids").is_none());

    let response = app
        .request(with_cookie(
            get(&format!("/api/library?tab_id={favorites}")),
            &user,
        ))
        .await;
    let list = body_json(response).await;
    assert_eq!(list["total"], 1);
    assert_eq!(list["items"][0]["title_id"], first);
    assert_eq!(list["items"][0]["custom_tab_id"], favorites);

    // Свой статус перекрывает встроенный: в planned остаётся только second
    let response = app
        .request(with_cookie(get("/api/library?status=planned"), &user))
        .await;
    let list = body_json(response).await;
    assert_eq!(list["total"], 1);
    assert_eq!(list["items"][0]["title_id"], second);
    assert_eq!(list["counts"]["all"], 2);
    assert_eq!(list["counts"]["statuses"]["planned"], 1);

    // Смена своей закладки заменяет статус, а не дополняет его
    let response = put_library(
        &app,
        &user,
        first,
        &json!({ "status": "planned", "score": null, "custom_tab_id": reading }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["custom_tab_id"], reading);

    let response = app
        .request(with_cookie(get("/api/library/tabs"), &user))
        .await;
    let tabs = body_json(response).await;
    let count_of = |tabs: &Value, id: i64| {
        tabs["tabs"]
            .as_array()
            .expect("tabs")
            .iter()
            .find(|tab| tab["id"] == id)
            .expect("tab")["count"]
            .as_i64()
            .expect("count")
    };
    assert_eq!(count_of(&tabs, reading), 1);
    assert_eq!(count_of(&tabs, favorites), 0);

    // Закладка чужого пользователя не подходит
    let other = app.register_extra_user("reader-two").await;
    let other_tab = create_tab(&app, &other, "Other").await;
    let response = put_library(
        &app,
        &user,
        first,
        &json!({ "status": "planned", "score": null, "custom_tab_id": other_tab }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(body_json(response).await["code"], "library_tab_not_found");

    // Свой статус можно выбрать и при добавлении тайтла в библиотеку
    let orphan = app.create_title(&admin, "Orphan", "manga").await;
    let response = put_library(
        &app,
        &user,
        orphan,
        &json!({ "status": "planned", "score": null, "custom_tab_id": reading }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    // Удаление закладки очищает свой статус через FK, возвращая planned
    let response = app
        .request(with_cookie(
            json_request(
                Method::DELETE,
                &format!("/api/library/tabs/{reading}"),
                &json!({}),
            ),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    for title_id in [first, orphan] {
        let response = app
            .request(with_cookie(get(&format!("/api/titles/{title_id}")), &user))
            .await;
        let title = body_json(response).await;
        assert!(title["library"]["custom_tab_id"].is_null());
        assert_eq!(title["library"]["status"], "planned");
    }

    let response = app
        .request(with_cookie(get("/api/library?status=planned"), &user))
        .await;
    assert_eq!(body_json(response).await["total"], 3);

    // Удаление из библиотеки убирает и запись статуса
    let response = app
        .request(with_cookie(
            json_request(Method::DELETE, &format!("/api/library/{first}"), &json!({})),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = app
        .request(with_cookie(get("/api/library?status=planned"), &user))
        .await;
    assert_eq!(body_json(response).await["total"], 2);
}

#[tokio::test]
async fn library_filters_counts_and_statuses() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    let user = app.register_extra_user("reader").await;

    let alpha = app.create_title(&admin, "Alpha Manga", "manga").await;
    let beta = app.create_title(&admin, "Beta Manga", "manga").await;
    let gamma = app.create_title(&admin, "Gamma Book", "other").await;

    for (title, status) in [
        (alpha, "favorite"),
        (beta, "in_progress"),
        (gamma, "not_interested"),
    ] {
        let response = put_library(
            &app,
            &user,
            title,
            &json!({ "status": status, "score": null }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK, "put {status}");
    }

    // «Любимое» — обычный статус, поэтому фильтруется через status
    let response = app
        .request(with_cookie(get("/api/library?status=favorite"), &user))
        .await;
    let list = body_json(response).await;
    assert_eq!(list["total"], 1);
    assert_eq!(list["items"][0]["title_id"], alpha);
    assert_eq!(list["items"][0]["status"], "favorite");
    assert!(list["items"][0].get("favorite").is_none());
    assert_eq!(list["counts"]["all"], 3);
    assert_eq!(list["counts"]["statuses"]["favorite"], 1);
    assert_eq!(list["counts"]["statuses"]["in_progress"], 1);
    assert_eq!(list["counts"]["statuses"]["not_interested"], 1);
    assert_eq!(list["counts"]["statuses"]["planned"], 0);

    let response = app
        .request(with_cookie(get("/api/library?q=beta"), &user))
        .await;
    let list = body_json(response).await;
    assert_eq!(list["total"], 1);
    assert_eq!(list["items"][0]["title"]["name"], "Beta Manga");

    let response = app
        .request(with_cookie(get("/api/library?sort=title"), &user))
        .await;
    let list = body_json(response).await;
    let names: Vec<&str> = list["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["title"]["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names, vec!["Alpha Manga", "Beta Manga", "Gamma Book"]);

    let response = app
        .request(with_cookie(get("/api/library?sort=unknown"), &user))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Смена статуса заменяет «Любимое», а не дополняет его
    let response = put_library(
        &app,
        &user,
        beta,
        &json!({ "status": "favorite", "score": 8 }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let entry = body_json(response).await;
    assert_eq!(entry["status"], "favorite");
    assert_eq!(entry["score"], 8);

    let response = app
        .request(with_cookie(get("/api/library?status=favorite"), &user))
        .await;
    assert_eq!(body_json(response).await["total"], 2);

    let response = app
        .request(with_cookie(get("/api/library?status=in_progress"), &user))
        .await;
    assert_eq!(body_json(response).await["total"], 0);
}

#[tokio::test]
async fn tabs_require_session_and_validate_names() {
    let app = TestApp::new().await;

    let response = app.request(get("/api/library/tabs")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    app.create_admin().await;
    let user = app.register_extra_user("reader").await;

    let response = post_tab(&app, &user, "   ").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(response).await["code"],
        "library_tab_name_invalid"
    );

    let too_long = "x".repeat(41);
    let response = post_tab(&app, &user, &too_long).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = app
        .request(with_cookie(
            json_request(Method::DELETE, "/api/library/tabs/999", &json!({})),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(body_json(response).await["code"], "library_tab_not_found");
}
