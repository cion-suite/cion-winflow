mod common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use image::GenericImageView;
use serde_json::{Value, json};

use common::{TestApp, body_json, get, json_request, multipart_file_request, with_cookie};

fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([20, 40, 80, 255]));
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("encode png");
    bytes
}

async fn me(app: &TestApp, cookie: &str) -> Value {
    let response = app.request(with_cookie(get("/api/me"), cookie)).await;
    assert_eq!(response.status(), StatusCode::OK, "me");
    body_json(response).await
}

async fn put_library(app: &TestApp, cookie: &str, title_id: i64, status: &str, score: Option<i64>) {
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/library/{title_id}"),
                &json!({ "status": status, "score": score }),
            ),
            cookie,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "library upsert");
}

#[tokio::test]
async fn profile_requires_session() {
    let app = TestApp::new().await;
    let response = app.request(get("/api/users/1/profile")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn profile_shows_user_stats_and_public_library() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    let reading = app.create_title(&admin, "Reading Now", "manga").await;
    let completed = app.create_title(&admin, "Finished", "manhwa").await;
    let hidden = app.create_title(&admin, "Not Interested", "manhua").await;

    let reader = app.register_extra_user("reader").await;
    let reader_id = me(&app, &reader).await["user"]["id"]
        .as_i64()
        .expect("reader id");

    put_library(&app, &reader, reading, "in_progress", Some(7)).await;
    put_library(&app, &reader, completed, "completed", None).await;
    put_library(&app, &reader, hidden, "not_interested", None).await;

    // Журнал прочитанного — два канонических ключа в двух тайтлах
    for (title_id, key) in [(reading, "c1"), (completed, "c2.5")] {
        sqlx::query(
            "INSERT INTO chapter_reads (user_id, title_id, chapter_key, read_at) \
             VALUES (?, ?, ?, 0)",
        )
        .bind(reader_id)
        .bind(title_id)
        .bind(key)
        .execute(&app.pool)
        .await
        .expect("chapter read");
    }

    let response = app
        .request(with_cookie(
            get(&format!("/api/users/{reader_id}/profile")),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let profile = body_json(response).await;

    assert_eq!(profile["user"]["id"], reader_id);
    assert_eq!(profile["user"]["username"], "reader");
    assert_eq!(profile["user"]["role"], "user");
    assert!(profile["user"]["created_at"].is_i64());
    assert!(profile["user"]["avatar_updated_at"].is_null());

    assert_eq!(profile["stats"]["read_chapters"], 2);
    assert_eq!(profile["stats"]["counts"]["all"], 2);
    assert_eq!(profile["stats"]["counts"]["statuses"]["in_progress"], 1);
    assert_eq!(profile["stats"]["counts"]["statuses"]["completed"], 1);
    // «Не интересно» не публикуется ни в счётчиках, ни в списке
    assert_eq!(profile["stats"]["counts"]["statuses"]["not_interested"], 0);

    assert_eq!(profile["library"]["total"], 2);
    let items = profile["library"]["items"].as_array().expect("items");
    assert_eq!(items.len(), 2);
    let ids: Vec<i64> = items
        .iter()
        .filter_map(|item| item["title_id"].as_i64())
        .collect();
    assert!(ids.contains(&reading));
    assert!(ids.contains(&completed));
    assert!(!ids.contains(&hidden));

    // Чужой профиль не требует роли и не отдаёт пароль/сессии
    let public_user = profile["user"].as_object().expect("user object");
    assert!(!public_user.contains_key("password_hash"));
    assert!(!public_user.contains_key("sessions"));
}

#[tokio::test]
async fn profile_shows_custom_tab_entries_under_their_status() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    let title_id = app.create_title(&admin, "Tabbed", "manga").await;
    let reader = app.register_extra_user("tab-reader").await;
    let reader_id = me(&app, &reader).await["user"]["id"]
        .as_i64()
        .expect("reader id");

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/library/tabs",
                &json!({ "name": "Favourites" }),
            ),
            &reader,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let tab_id = body_json(response).await["id"]
        .as_i64()
        .expect("custom tab id");

    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/library/{title_id}"),
                &json!({
                    "status": "in_progress",
                    "score": null,
                    "custom_tab_id": tab_id,
                }),
            ),
            &reader,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    // Публичный профиль не знает о личных закладках: запись видна под
    // своим встроенным статусом, а чужой tab_id ничего не фильтрует.
    let response = app
        .request(with_cookie(
            get(&format!(
                "/api/users/{reader_id}/profile?tab_id={tab_id}&status=in_progress"
            )),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let profile = body_json(response).await;
    assert_eq!(profile["stats"]["counts"]["all"], 1);
    assert_eq!(profile["stats"]["counts"]["statuses"]["in_progress"], 1);
    assert_eq!(profile["library"]["total"], 1);
    let items = profile["library"]["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    // Чужая закладка не раскрывается: в публичной выдаче только статус
    assert!(items[0]["custom_tab_id"].is_null());
    assert_eq!(items[0]["status"], "in_progress");
}

#[tokio::test]
async fn profile_of_unknown_or_disabled_user_is_404() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;

    let response = app
        .request(with_cookie(get("/api/users/999/profile"), &admin))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let error = body_json(response).await;
    assert_eq!(error["code"], "user_not_found");

    let user = app.register_extra_user("disabled").await;
    let user_id = me(&app, &user).await["user"]["id"]
        .as_i64()
        .expect("user id");
    let response = app
        .request(with_cookie(
            multipart_file_request(
                "/api/me/avatar",
                "file",
                "avatar.png",
                "image/png",
                &png_bytes(10, 10),
            ),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "avatar before disabling");

    sqlx::query("UPDATE users SET disabled_at = 1 WHERE id = ?")
        .bind(user_id)
        .execute(&app.pool)
        .await
        .expect("disable user");

    let response = app
        .request(with_cookie(
            get(&format!("/api/users/{user_id}/profile")),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Аватар отключённого пользователя тоже неотличим от несуществующего
    let response = app
        .request(with_cookie(
            get(&format!("/api/users/{user_id}/avatar")),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn avatar_upload_serve_and_delete() {
    let app = TestApp::new().await;
    let user = app.register_user("avatar-user").await;
    let user_id = me(&app, &user).await["user"]["id"]
        .as_i64()
        .expect("user id");

    // До загрузки аватара нет ни в профиле, ни в файловом ответе
    let response = app
        .request(with_cookie(
            get(&format!("/api/users/{user_id}/avatar")),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = app
        .request(with_cookie(
            multipart_file_request(
                "/api/me/avatar",
                "file",
                "avatar.png",
                "image/png",
                &png_bytes(40, 60),
            ),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "avatar upload");
    let uploaded = body_json(response).await;
    let updated_at = uploaded["avatar_updated_at"]
        .as_i64()
        .expect("avatar updated_at");
    let version = uploaded["avatar_version"].as_i64().expect("avatar version");
    assert!(updated_at > 0);
    assert_eq!(version, 1);

    // Текущий пользователь сразу получает время и версию в /api/me
    let me_body = me(&app, &user).await;
    assert_eq!(me_body["user"]["avatar_updated_at"], updated_at);
    assert_eq!(me_body["user"]["avatar_version"], version);

    // Повторная загрузка (даже в ту же секунду) обязана менять версию:
    // иначе ?v= и ETag оставят у браузера старую картинку.
    let response = app
        .request(with_cookie(
            multipart_file_request(
                "/api/me/avatar",
                "file",
                "avatar.png",
                "image/png",
                &png_bytes(60, 40),
            ),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "avatar re-upload");
    let replaced = body_json(response).await;
    let replaced_at = replaced["avatar_updated_at"]
        .as_i64()
        .expect("avatar updated_at");
    let replaced_version = replaced["avatar_version"].as_i64().expect("avatar version");
    assert!(replaced_at > updated_at);
    assert_eq!(replaced_version, version + 1);

    let response = app
        .request(with_cookie(
            get(&format!("/api/users/{user_id}/avatar")),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "avatar get");
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/webp");
    assert_eq!(
        response.headers()[header::CACHE_CONTROL],
        "private, max-age=86400"
    );
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    let etag = response.headers()[header::ETAG]
        .to_str()
        .expect("etag")
        .to_owned();
    assert_eq!(etag, format!("\"{user_id}-{replaced_version}\""));
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("avatar body");
    let dimensions = image::load_from_memory(&bytes)
        .expect("avatar image")
        .dimensions();
    assert_eq!(dimensions, (512, 512));

    // Аватар чужого пользователя доступен любой сессии
    let admin = app.login_admin().await;
    let response = app
        .request(with_cookie(
            get(&format!("/api/users/{user_id}/avatar")),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "foreign avatar get");

    let conditional = Request::builder()
        .uri(format!("/api/users/{user_id}/avatar"))
        .header(header::IF_NONE_MATCH, &etag)
        .body(Body::empty())
        .expect("request");
    let response = app.request(with_cookie(conditional, &user)).await;
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);

    let profile = body_json(
        app.request(with_cookie(
            get(&format!("/api/users/{user_id}/profile")),
            &user,
        ))
        .await,
    )
    .await;
    assert_eq!(profile["user"]["avatar_updated_at"], replaced_at);
    assert_eq!(profile["user"]["avatar_version"], replaced_version);

    let response = app
        .request(with_cookie(
            json_request(Method::DELETE, "/api/me/avatar", &json!({})),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let me_body = me(&app, &user).await;
    assert!(
        me_body["user"]["avatar_updated_at"].is_null(),
        "avatar presence is cleared"
    );
    // Версия — монотонный счётчик: удаление её не сбрасывает, иначе
    // загрузка в ту же секунду повторила бы старый ETag.
    assert_eq!(me_body["user"]["avatar_version"], replaced_version);
    let response = app
        .request(with_cookie(
            get(&format!("/api/users/{user_id}/avatar")),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = app
        .request(with_cookie(
            multipart_file_request(
                "/api/me/avatar",
                "file",
                "avatar.png",
                "image/png",
                &png_bytes(20, 20),
            ),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "upload after delete");
    let restored = body_json(response).await;
    assert_eq!(restored["avatar_version"], replaced_version + 1);
    assert!(restored["avatar_updated_at"].as_i64().is_some());
}

#[tokio::test]
async fn avatar_upload_rejects_foreign_and_invalid_data() {
    let app = TestApp::new().await;
    let user = app.register_user("avatar-owner").await;

    // Без сессии загружать нельзя
    let response = app
        .request(multipart_file_request(
            "/api/me/avatar",
            "file",
            "avatar.png",
            "image/png",
            &png_bytes(10, 10),
        ))
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // Не картинка — 400, файл не записывается
    let response = app
        .request(with_cookie(
            multipart_file_request(
                "/api/me/avatar",
                "file",
                "avatar.png",
                "image/png",
                b"not an image",
            ),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(response).await["code"], "avatar_invalid");

    // Лимит файла проверяется до декодирования: 5 МиБ + 1 байт
    let oversized = vec![0_u8; 5 * 1024 * 1024 + 1];
    let response = app
        .request(with_cookie(
            multipart_file_request(
                "/api/me/avatar",
                "file",
                "avatar.png",
                "image/png",
                &oversized,
            ),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(response).await["code"], "avatar_too_large");

    // Без поля file — 400
    let response = app
        .request(with_cookie(
            multipart_file_request(
                "/api/me/avatar",
                "other",
                "avatar.png",
                "image/png",
                &png_bytes(10, 10),
            ),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(response).await["code"], "avatar_file_required");
}
