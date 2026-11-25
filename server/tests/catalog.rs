mod common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use serde_json::{Value, json};

use common::{TestApp, body_json, get, json_request, multipart_file_request, with_cookie};

fn title_body(name: &str, kind: &str) -> Value {
    json!({
        "name": name,
        "kind": kind,
        "alt_names": ["Alt Name"],
        "authors": ["Author"],
        "genres": ["Action"],
        "description": "Description",
        "status": "ongoing",
        "year": 2020,
        "lang": "ru",
        "rating": 7.5,
    })
}

async fn post_title(app: &TestApp, cookie: &str, body: &Value) -> axum::response::Response {
    app.request(with_cookie(
        json_request(Method::POST, "/api/titles", body),
        cookie,
    ))
    .await
}

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

#[tokio::test]
async fn catalog_requires_session() {
    let app = TestApp::new().await;
    let response = app.request(get("/api/catalog")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let json = body_json(response).await;
    assert_eq!(json["code"], "unauthorized");
}

#[tokio::test]
async fn create_title_requires_admin() {
    let app = TestApp::new().await;
    let user_cookie = app.register_user("reader").await;
    let admin = app.login_admin().await;

    let response = post_title(&app, &user_cookie, &title_body("Some Manga", "manga")).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let json = body_json(response).await;
    assert_eq!(json["code"], "forbidden");

    // Каталог доступен обычному пользователю, но созданных тайтлов нет
    let response = app
        .request(with_cookie(get("/api/catalog"), &user_cookie))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["total"], 0);

    let id = app.create_title(&admin, "Admin Title", "manga").await;
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/titles/{id}"),
                &title_body("Hijacked", "manga"),
            ),
            &user_cookie,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = app
        .request(with_cookie(get(&format!("/api/titles/{id}")), &admin))
        .await;
    let json = body_json(response).await;
    assert_eq!(json["title"]["name"], "Admin Title");
}

#[tokio::test]
async fn admin_creates_and_reads_title() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;

    let response = post_title(&app, &admin, &title_body("Some Manga", "manga")).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let created = body_json(response).await;
    let id = created["title"]["id"].as_i64().expect("title id");

    assert_eq!(created["title"]["name"], "Some Manga");
    assert_eq!(created["title"]["kind"], "manga");
    assert_eq!(created["title"]["alt_names"][0], "Alt Name");
    assert_eq!(created["title"]["authors"][0], "Author");
    assert_eq!(created["title"]["genres"][0], "Action");
    assert_eq!(created["title"]["description"], "Description");
    assert_eq!(created["title"]["status"], "ongoing");
    assert_eq!(created["title"]["year"], 2020);
    assert_eq!(created["title"]["lang"], "ru");
    assert_eq!(created["title"]["rating"], 7.5);
    assert_eq!(created["title"]["has_cover"], false);
    assert!(created["title"]["origin_source_id"].is_null());

    let response = app
        .request(with_cookie(get(&format!("/api/titles/{id}")), &admin))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["title"]["name"], "Some Manga");
    assert!(json["library"].is_null());
}

#[tokio::test]
async fn title_validation_rejects_bad_input() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;

    for body in [
        json!({ "name": "   ", "kind": "manga" }),
        json!({ "name": "x", "kind": "novel" }),
        json!({ "name": "x", "kind": "manga", "rating": 11.0 }),
        json!({ "name": "x", "kind": "manga", "year": 1000 }),
        json!({ "name": "x", "kind": "manga", "status": "unknown" }),
        json!({ "name": "x", "kind": "manga", "lang": "ru-ru-ru-ru" }),
    ] {
        let response = post_title(&app, &admin, &body).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "body: {body}");
        let json = body_json(response).await;
        assert_eq!(json["code"], "validation", "body: {body}");
    }
}

#[tokio::test]
async fn catalog_search_is_case_insensitive_and_escapes_like() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    app.create_title(&admin, "Breaking Bad", "book").await;
    app.create_title(&admin, "100% Wolf", "manga").await;
    app.create_title(&admin, "Мастер и Маргарита", "book").await;
    app.create_title(&admin, "A_B", "book").await;
    app.create_title(&admin, "AXB", "book").await;

    let response = app
        .request(with_cookie(get("/api/catalog?q=breaking"), &admin))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["total"], 1);
    assert_eq!(json["items"][0]["name"], "Breaking Bad");

    // SQLite lower() не понимает кириллицу: совпадение даёт name_lower
    let response = app
        .request(with_cookie(
            get("/api/catalog?q=%D0%BC%D0%B0%D1%81%D1%82%D0%B5%D1%80"),
            &admin,
        ))
        .await;
    let json = body_json(response).await;
    assert_eq!(json["total"], 1);
    assert_eq!(json["items"][0]["name"], "Мастер и Маргарита");

    // `%` не должен превратиться в «совпадает со всеми»
    let response = app
        .request(with_cookie(get("/api/catalog?q=100%25"), &admin))
        .await;
    let json = body_json(response).await;
    assert_eq!(json["total"], 1);
    assert_eq!(json["items"][0]["name"], "100% Wolf");

    let response = app
        .request(with_cookie(get("/api/catalog?q=%25"), &admin))
        .await;
    let json = body_json(response).await;
    assert_eq!(json["total"], 1);

    // `_` — тоже wildcard LIKE: без экранирования совпали бы A_B и AXB
    let response = app
        .request(with_cookie(get("/api/catalog?q=a_b"), &admin))
        .await;
    let json = body_json(response).await;
    assert_eq!(json["total"], 1);
    assert_eq!(json["items"][0]["name"], "A_B");
}

#[tokio::test]
async fn catalog_filters_by_kind_and_paginates() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    app.create_title(&admin, "Alpha", "manga").await;
    app.create_title(&admin, "Beta", "manga").await;
    app.create_title(&admin, "Gamma", "anime").await;

    let response = app
        .request(with_cookie(get("/api/catalog?kind=anime"), &admin))
        .await;
    let json = body_json(response).await;
    assert_eq!(json["total"], 1);
    assert_eq!(json["items"][0]["name"], "Gamma");

    let response = app
        .request(with_cookie(get("/api/catalog?page_size=2&page=1"), &admin))
        .await;
    let json = body_json(response).await;
    assert_eq!(json["total"], 3);
    assert_eq!(json["page"], 1);
    assert_eq!(json["page_size"], 2);
    assert_eq!(json["items"].as_array().expect("items").len(), 2);

    let response = app
        .request(with_cookie(get("/api/catalog?page_size=2&page=2"), &admin))
        .await;
    let json = body_json(response).await;
    assert_eq!(json["items"].as_array().expect("items").len(), 1);

    let response = app
        .request(with_cookie(get("/api/catalog?page=0"), &admin))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = body_json(response).await;
    assert_eq!(json["code"], "validation");

    let response = app
        .request(with_cookie(get("/api/catalog?page_size=0"), &admin))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = app
        .request(with_cookie(get("/api/catalog?page_size=201"), &admin))
        .await;
    let json = body_json(response).await;
    assert_eq!(json["page_size"], 200);

    let response = app.request(with_cookie(get("/api/catalog"), &admin)).await;
    let json = body_json(response).await;
    assert_eq!(json["page_size"], 50);

    let response = app
        .request(with_cookie(get("/api/catalog?page=99"), &admin))
        .await;
    let json = body_json(response).await;
    assert_eq!(json["total"], 3);
    assert_eq!(json["items"].as_array().expect("items").len(), 0);

    let response = app
        .request(with_cookie(get("/api/catalog?kind=novel"), &admin))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn invalid_extractors_return_api_error_envelope() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    let id = app.create_title(&admin, "Some Manga", "manga").await;

    let response = app
        .request(with_cookie(get("/api/catalog?page=abc"), &admin))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    let json = body_json(response).await;
    assert_eq!(json["code"], "validation");

    let response = app
        .request(with_cookie(get("/api/titles/abc"), &admin))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = body_json(response).await;
    assert_eq!(json["code"], "validation");

    let response = app
        .request(with_cookie(
            json_request(Method::PUT, &format!("/api/library/{id}"), &json!({})),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = body_json(response).await;
    assert_eq!(json["code"], "validation");
}

#[tokio::test]
async fn unknown_title_is_404() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;

    let response = app
        .request(with_cookie(get("/api/titles/999"), &admin))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let json = body_json(response).await;
    assert_eq!(json["code"], "title_not_found");
}

#[tokio::test]
async fn update_title_changes_fields_but_kind_is_locked_with_sources() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    let id = app.create_title(&admin, "Old Name", "manga").await;

    let mut body = title_body("New Name", "manga");
    body["status"] = json!(null);
    body["year"] = json!(null);
    let response = app
        .request(with_cookie(
            json_request(Method::PUT, &format!("/api/titles/{id}"), &body),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["title"]["name"], "New Name");
    assert!(json["title"]["status"].is_null());
    assert!(json["title"]["year"].is_null());

    // name_lower обновляется вместе с name: поиск идёт по новому имени
    let response = app
        .request(with_cookie(get("/api/catalog?q=new%20name"), &admin))
        .await;
    let json = body_json(response).await;
    assert_eq!(json["total"], 1);
    assert_eq!(json["items"][0]["name"], "New Name");

    let response = app
        .request(with_cookie(get("/api/catalog?q=old%20name"), &admin))
        .await;
    let json = body_json(response).await;
    assert_eq!(json["total"], 0);

    // Пока источников нет, kind менять можно
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/titles/{id}"),
                &title_body("Interim", "anime"),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["title"]["kind"], "anime");

    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/titles/{id}"),
                &title_body("New Name", "manga"),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let source_id: i64 = sqlx::query_scalar(
        "INSERT INTO sources (code, name, enabled, created_at) VALUES ('test', 'Test', 1, 0) \
         RETURNING id",
    )
    .fetch_one(&app.pool)
    .await
    .expect("source");
    sqlx::query(
        "INSERT INTO source_entries (title_id, source_id, kind, external_id) \
         VALUES (?, ?, 'manga', 'external-1')",
    )
    .bind(id)
    .bind(source_id)
    .execute(&app.pool)
    .await
    .expect("source entry");

    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/titles/{id}"),
                &title_body("New Name", "anime"),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let json = body_json(response).await;
    assert_eq!(json["code"], "kind_locked");

    let response = app
        .request(with_cookie(get(&format!("/api/titles/{id}")), &admin))
        .await;
    let json = body_json(response).await;
    assert_eq!(json["title"]["kind"], "manga");
    assert_eq!(json["title"]["name"], "New Name");

    // Смена остальных полей при привязанном источнике разрешена
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/titles/{id}"),
                &title_body("Renamed Again", "manga"),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .request(with_cookie(get("/api/catalog?q=renamed%20again"), &admin))
        .await;
    let json = body_json(response).await;
    assert_eq!(json["total"], 1);

    let response = app
        .request(with_cookie(get("/api/catalog?q=new%20name"), &admin))
        .await;
    let json = body_json(response).await;
    assert_eq!(json["total"], 0);
}

#[tokio::test]
async fn cover_upload_and_serving() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    let id = app.create_title(&admin, "Covered", "manga").await;

    let response = app
        .request(with_cookie(get(&format!("/api/titles/{id}/cover")), &admin))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let json = body_json(response).await;
    assert_eq!(json["code"], "cover_not_found");

    let png = png_bytes(40, 60);
    let response = app
        .request(with_cookie(
            multipart_file_request(
                &format!("/api/titles/{id}/cover"),
                "file",
                "cover.png",
                "image/png",
                &png,
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["has_cover"], true);

    let response = app
        .request(with_cookie(get(&format!("/api/titles/{id}")), &admin))
        .await;
    let title_json = body_json(response).await;
    assert_eq!(title_json["title"]["has_cover"], true);
    let first_updated_at = title_json["title"]["updated_at"]
        .as_i64()
        .expect("title updated_at");

    let response = app
        .request(with_cookie(get("/api/catalog?q=covered"), &admin))
        .await;
    let catalog = body_json(response).await;
    assert_eq!(catalog["items"][0]["has_cover"], true);

    for size in ["thumb", "full"] {
        let request = Request::builder()
            .uri(format!("/api/titles/{id}/cover?size={size}"))
            .header(header::COOKIE, format!("sid={admin}"))
            .body(Body::empty())
            .expect("request");
        let response = app.request(request).await;
        assert_eq!(response.status(), StatusCode::OK, "size {size}");
        assert_eq!(response.headers()[header::CONTENT_TYPE], "image/webp");
        assert!(
            response.headers()[header::CACHE_CONTROL]
                .to_str()
                .expect("cache header")
                .contains("private")
        );
        let etag = response
            .headers()
            .get(header::ETAG)
            .expect("etag")
            .to_str()
            .expect("etag str")
            .to_owned();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("cover body");
        assert!(bytes.starts_with(b"RIFF"), "not a webp: {size}");

        let request = Request::builder()
            .uri(format!("/api/titles/{id}/cover?size={size}"))
            .header(header::COOKIE, format!("sid={admin}"))
            .header(header::IF_NONE_MATCH, etag)
            .body(Body::empty())
            .expect("request");
        let response = app.request(request).await;
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED, "size {size}");
    }

    // Перезагрузка заменяет оба файла: запись атомарная, старый файл не мешает.
    // ETag и `v={updated_at}` обязаны измениться даже в ту же секунду,
    // иначе браузер отдаст старую обложку из кэша.
    let request = Request::builder()
        .uri(format!("/api/titles/{id}/cover?size=full"))
        .header(header::COOKIE, format!("sid={admin}"))
        .body(Body::empty())
        .expect("request");
    let response = app.request(request).await;
    assert_eq!(response.status(), StatusCode::OK);
    let first_etag = response
        .headers()
        .get(header::ETAG)
        .expect("etag")
        .to_str()
        .expect("etag str")
        .to_owned();
    let first_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("cover body");
    assert!(first_bytes.starts_with(b"RIFF"));

    let response = app
        .request(with_cookie(
            multipart_file_request(
                &format!("/api/titles/{id}/cover"),
                "file",
                "cover2.png",
                "image/png",
                &png_bytes(80, 120),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let request = Request::builder()
        .uri(format!("/api/titles/{id}/cover?size=full"))
        .header(header::COOKIE, format!("sid={admin}"))
        .body(Body::empty())
        .expect("request");
    let response = app.request(request).await;
    assert_eq!(response.status(), StatusCode::OK);
    let second_etag = response
        .headers()
        .get(header::ETAG)
        .expect("etag")
        .to_str()
        .expect("etag str")
        .to_owned();
    let second_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("cover body");
    assert!(second_bytes.starts_with(b"RIFF"));
    assert_ne!(first_etag, second_etag);
    assert_ne!(first_bytes, second_bytes);

    let response = app
        .request(with_cookie(get(&format!("/api/titles/{id}")), &admin))
        .await;
    let title_json = body_json(response).await;
    let second_updated_at = title_json["title"]["updated_at"]
        .as_i64()
        .expect("title updated_at");
    assert!(second_updated_at > first_updated_at);
}

#[tokio::test]
async fn cover_upload_rejects_non_images_and_foreign_users() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    let id = app.create_title(&admin, "Covered", "manga").await;

    let response = app
        .request(with_cookie(
            multipart_file_request(
                &format!("/api/titles/{id}/cover"),
                "file",
                "evil.html",
                "text/html",
                b"<script>alert(1)</script>",
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = body_json(response).await;
    assert_eq!(json["code"], "cover_invalid");

    let user_cookie = app.register_extra_user("reader").await;
    let response = app
        .request(with_cookie(
            multipart_file_request(
                &format!("/api/titles/{id}/cover"),
                "file",
                "cover.png",
                "image/png",
                &png_bytes(10, 10),
            ),
            &user_cookie,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Отказ не должен создать обложку
    let response = app
        .request(with_cookie(get(&format!("/api/titles/{id}")), &admin))
        .await;
    let json = body_json(response).await;
    assert_eq!(json["title"]["has_cover"], false);

    let response = app
        .request(with_cookie(
            multipart_file_request(
                "/api/titles/999/cover",
                "file",
                "cover.png",
                "image/png",
                &png_bytes(10, 10),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let json = body_json(response).await;
    assert_eq!(json["code"], "title_not_found");
}

#[tokio::test]
async fn cover_upload_validates_file_and_size() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    let id = app.create_title(&admin, "Covered", "manga").await;

    // Нет поля `file`
    let response = app
        .request(with_cookie(
            multipart_file_request(
                &format!("/api/titles/{id}/cover"),
                "other",
                "cover.png",
                "image/png",
                &png_bytes(10, 10),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = body_json(response).await;
    assert_eq!(json["code"], "cover_file_required");

    // Файл больше 5 МиБ отклоняется до декодирования
    let oversized = vec![0_u8; cion_reader::catalog::cover::MAX_FILE_BYTES + 1];
    let response = app
        .request(with_cookie(
            multipart_file_request(
                &format!("/api/titles/{id}/cover"),
                "file",
                "huge.png",
                "image/png",
                &oversized,
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = body_json(response).await;
    assert_eq!(json["code"], "cover_too_large");

    // Слишком широкое изображение не проходит лимиты декодирования
    let wide = png_bytes(25_000, 1);
    let response = app
        .request(with_cookie(
            multipart_file_request(
                &format!("/api/titles/{id}/cover"),
                "file",
                "wide.png",
                "image/png",
                &wide,
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = body_json(response).await;
    assert_eq!(json["code"], "cover_invalid");

    // Тело больше лимита маршрута (6 МиБ) не должно ничего сохранить
    let too_big = vec![0_u8; 7 * 1024 * 1024];
    let response = app
        .request(with_cookie(
            multipart_file_request(
                &format!("/api/titles/{id}/cover"),
                "file",
                "too-big.png",
                "image/png",
                &too_big,
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = app
        .request(with_cookie(get(&format!("/api/titles/{id}/cover")), &admin))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let json = body_json(response).await;
    assert_eq!(json["code"], "cover_not_found");

    // Неизвестный размер в GET
    let response = app
        .request(with_cookie(
            get(&format!("/api/titles/{id}/cover?size=bogus")),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = body_json(response).await;
    assert_eq!(json["code"], "validation");
}
