#![cfg(debug_assertions)] // fixture-источник существует только в debug-сборке

mod common;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

use cion_reader::sources::fixture;
use common::{TestApp, body_json, get, json_request, multipart_file_request, with_cookie};

/// fixture-счётчик глобальный: тесты файла идут по очереди.
static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn lock() -> tokio::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().await
}

fn png_bytes() -> Vec<u8> {
    png_sized(4, 8)
}

fn png_sized(width: u32, height: u32) -> Vec<u8> {
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

/// Локальный HTTP-сервер картинок: считает запросы по путям.
async fn start_image_server() -> (String, Arc<Mutex<HashMap<String, usize>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("local addr");
    let hits = Arc::new(Mutex::new(HashMap::new()));
    let counters = hits.clone();

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let counters = counters.clone();
            tokio::spawn(async move {
                let mut buffer = Vec::new();
                let mut chunk = [0_u8; 1024];
                loop {
                    match socket.read(&mut chunk).await {
                        Ok(0) => break,
                        Ok(read) => {
                            buffer.extend_from_slice(&chunk[..read]);
                            if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
                                break;
                            }
                        }
                        Err(_) => return,
                    }
                }

                let request = String::from_utf8_lossy(&buffer);
                let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();
                *counters
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .entry(path)
                    .or_insert(0) += 1;

                let body = png_bytes();
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(&body).await;
                let _ = socket.shutdown().await;
            });
        }
    });

    (format!("http://{address}"), hits)
}

fn hit_count(hits: &Arc<Mutex<HashMap<String, usize>>>, path: &str) -> usize {
    *hits
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(path)
        .unwrap_or(&0)
}

async fn configure_fixture(app: &TestApp, admin: &str, base_url: &str) {
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/sources/fixture",
                &json!({ "base_url": base_url, "enabled": true }),
            ),
            admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "configure fixture");
}

async fn import_fixture(app: &TestApp, admin: &str) -> Value {
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/discover/add",
                &json!({ "code": "fixture", "external_id": "fixture-title" }),
            ),
            admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "import fixture");
    body_json(response).await
}

async fn get_json(app: &TestApp, uri: &str, cookie: &str) -> Value {
    let response = app.request(with_cookie(get(uri), cookie)).await;
    body_json(response).await
}

#[tokio::test]
async fn source_endpoints_require_admin() {
    let _guard = lock().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    let user = app.register_extra_user("reader").await;

    let response = app.request(get("/api/sources")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = app.request(with_cookie(get("/api/sources"), &user)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let sources = get_json(&app, "/api/sources", &admin).await;
    let list = sources["sources"].as_array().expect("sources array");
    assert!(list.iter().any(|source| source["code"] == "remanga"));
    assert!(list.iter().any(|source| source["code"] == "fixture"));
    let remanga = list
        .iter()
        .find(|source| source["code"] == "remanga")
        .expect("remanga descriptor");
    assert_eq!(remanga["configured"], false);
    assert!(
        remanga["capabilities"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
}

#[tokio::test]
async fn source_config_validation_and_base_url_rules() {
    let _guard = lock().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;

    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/sources/remanga",
                &json!({ "base_url": "https://api.remanga.org", "enabled": true }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    // URL с путём не является origin-ом
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/sources/remanga",
                &json!({ "base_url": "https://api.remanga.org/api/", "enabled": true }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Неизвестный код
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/sources/nope",
                &json!({ "base_url": "https://example.com", "enabled": true }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Некорректный список хостов — 422 со списком полей
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/sources/remanga/config",
                &json!({ "image_hosts": ["*"] }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(response).await;
    assert_eq!(body["code"], "source_config_invalid");
    assert!(
        body["errors"]
            .as_array()
            .is_some_and(|errors| !errors.is_empty())
    );

    // Неизвестный ключ — предупреждение, а не ошибка
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/sources/remanga/config",
                &json!({ "image_hosts": ["*.reimg2.org"], "surprise": 1 }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert!(
        body["warnings"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
}

#[tokio::test]
async fn search_import_and_title_reads_only_database() {
    let _guard = lock().await;
    let (base_url, _server) = start_image_server().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin, &base_url).await;

    fixture::reset_calls();
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/sources/fixture/search",
                &json!({ "query": "fixture" }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let search = body_json(response).await;
    assert_eq!(search["items"][0]["external_id"], "fixture-title");
    assert_eq!(search["items"][0]["cover_ref"], "/cover.jpg");

    let imported = import_fixture(&app, &admin).await;
    assert_eq!(imported["title_created"], true);
    assert_eq!(imported["imported_chapters"], 3);

    let title_id = imported["title_id"].as_i64().expect("title id");
    let before = fixture::call_count();
    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    assert_eq!(
        fixture::call_count(),
        before,
        "title page must not hit the source"
    );
    assert_eq!(title["title"]["cover_sizes"], json!(["mid"]));
    assert_eq!(title["title"]["has_cover"], true);

    // Обложка уже на диске: импорт её скачал, запроса к источнику нет.
    let response = app
        .request(with_cookie(
            get(&format!("/api/titles/{title_id}/cover?size=mid")),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/webp");

    assert_eq!(title["title"]["name"], "Fixture Title");
    assert_eq!(title["chapters"].as_array().expect("chapters").len(), 3);
    assert_eq!(title["sources"].as_array().expect("sources").len(), 1);
    assert_eq!(title["sources"][0]["chapter_count"], 3);
    assert_eq!(title["sources"][0]["available_count"], 2);

    let catalog = get_json(&app, "/api/catalog?q=fixture", &admin).await;
    assert_eq!(catalog["total"], 1);

    // Повторный импорт идемпотентен
    let again = import_fixture(&app, &admin).await;
    assert_eq!(again["title_created"], false);
    assert_eq!(again["title_id"], title_id);
    let titles: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM titles")
        .fetch_one(&app.pool)
        .await
        .expect("titles count");
    assert_eq!(titles, 1);
    let chapters: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chapters")
        .fetch_one(&app.pool)
        .await
        .expect("chapters count");
    assert_eq!(chapters, 3);
}

#[tokio::test]
async fn view_language_filters_known_language_and_ignores_absent_one() {
    let _guard = lock().await;
    let (base_url, _server) = start_image_server().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin, &base_url).await;
    let imported = import_fixture(&app, &admin).await;
    let title_id = imported["title_id"].as_i64().expect("title id");

    let plain = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    let ru = get_json(&app, &format!("/api/titles/{title_id}?lang=ru"), &admin).await;
    let en = get_json(&app, &format!("/api/titles/{title_id}?lang=en"), &admin).await;

    let plain_count = plain["chapters"].as_array().expect("chapters").len();
    assert!(plain_count > 0);
    assert!(plain["view_lang"].is_null());
    assert_eq!(ru["view_lang"], "ru");
    assert_eq!(
        ru["chapters"].as_array().expect("ru chapters").len(),
        plain_count
    );
    // Языка en у тайтла нет: параметр игнорируется, а не опустошает список.
    assert!(en["view_lang"].is_null());
    assert_eq!(
        en["chapters"].as_array().expect("en chapters").len(),
        plain_count
    );
    assert_eq!(en["sources"][0]["lang"], "ru");

    let ru_content = get_json(
        &app,
        &format!("/api/titles/{title_id}/chapters/c1/content?lang=ru"),
        &admin,
    )
    .await;
    assert_eq!(ru_content["chapter_key"], "c1");
    let en_content = get_json(
        &app,
        &format!("/api/titles/{title_id}/chapters/c1/content?lang=en"),
        &admin,
    )
    .await;
    assert_eq!(en_content["chapter_key"], "c1");
}

#[tokio::test]
async fn content_fallback_progress_and_typed_errors() {
    let _guard = lock().await;
    let (base_url, _server) = start_image_server().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin, &base_url).await;
    let imported = import_fixture(&app, &admin).await;
    let title_id = imported["title_id"].as_i64().expect("title id");

    let content = get_json(
        &app,
        &format!("/api/titles/{title_id}/chapters/c1/content"),
        &admin,
    )
    .await;
    assert_eq!(content["content"]["kind"], "images");
    assert_eq!(
        content["content"]["pages"].as_array().expect("pages").len(),
        2
    );
    assert!(
        content["content"]["pages"][0]["image_url"]
            .as_str()
            .is_some_and(|url| url.starts_with("/pages/"))
    );
    assert_eq!(content["source"]["code"], "fixture");

    // Повторное открытие главы не ходит в сеть
    let before = fixture::call_count();
    let cached = get_json(
        &app,
        &format!("/api/titles/{title_id}/chapters/c1/content"),
        &admin,
    )
    .await;
    assert_eq!(fixture::call_count(), before);
    assert_eq!(cached["chapter_id"], content["chapter_id"]);

    // Залоченная глава не запрашивается
    let response = app
        .request(with_cookie(
            get(&format!("/api/titles/{title_id}/chapters/c3/content")),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(body_json(response).await["code"], "source_locked");

    // Неизвестный ключ
    let response = app
        .request(with_cookie(
            get(&format!("/api/titles/{title_id}/chapters/nope/content")),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(body_json(response).await["code"], "chapter_not_found");

    // Прогресс — по каноническому ключу
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/progress/{title_id}"),
                &json!({ "chapter_key": "c1", "page": 1 }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let saved = body_json(response).await;
    assert_eq!(saved["chapter_key"], "c1");
    assert_eq!(saved["page"], 1);

    let progress = get_json(&app, &format!("/api/progress/{title_id}"), &admin).await;
    assert_eq!(progress["progress"]["chapter_key"], "c1");
    assert_eq!(progress["progress"]["page"], 1);

    // Поле позиции не того типа
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/progress/{title_id}"),
                &json!({ "chapter_key": "c1", "seconds": 5 }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/progress/{title_id}"),
                &json!({ "chapter_key": "unknown", "page": 1 }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn image_proxy_caches_and_cover_allowlist() {
    let _guard = lock().await;
    let (base_url, hits) = start_image_server().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin, &base_url).await;
    let imported = import_fixture(&app, &admin).await;
    let title_id = imported["title_id"].as_i64().expect("title id");

    let content = get_json(
        &app,
        &format!("/api/titles/{title_id}/chapters/c1/content"),
        &admin,
    )
    .await;
    let page_id = content["content"]["pages"][0]["id"]
        .as_i64()
        .expect("page id");

    let request = Request::builder()
        .uri(format!("/api/pages/{page_id}/image"))
        .header(header::COOKIE, format!("sid={admin}"))
        .body(Body::empty())
        .expect("request");
    let response = app.request(request).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    let etag = response
        .headers()
        .get(header::ETAG)
        .expect("etag")
        .to_str()
        .expect("etag string")
        .to_owned();
    assert_eq!(hit_count(&hits, "/img/0.png"), 1);

    // Второй запрос — из дискового кэша
    let request = Request::builder()
        .uri(format!("/api/pages/{page_id}/image"))
        .header(header::COOKIE, format!("sid={admin}"))
        .body(Body::empty())
        .expect("request");
    let response = app.request(request).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(hit_count(&hits, "/img/0.png"), 1);

    // ETag даёт 304 без чтения источника
    let request = Request::builder()
        .uri(format!("/api/pages/{page_id}/image"))
        .header(header::COOKIE, format!("sid={admin}"))
        .header(header::IF_NONE_MATCH, etag)
        .body(Body::empty())
        .expect("request");
    let response = app.request(request).await;
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(hit_count(&hits, "/img/0.png"), 1);

    // Обложка из выдачи поиска — через тот же allowlist. Импорт уже
    // скачал её для тайтла, поэтому первый запрос прокси — второй хит.
    let cover_hits = hit_count(&hits, "/cover.jpg");
    let response = app
        .request(with_cookie(
            get("/api/sources/fixture/cover?ref=%2Fcover.jpg"),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(hit_count(&hits, "/cover.jpg"), cover_hits + 1);

    // Чужой хост отклоняется
    let response = app
        .request(with_cookie(
            get("/api/sources/fixture/cover?ref=https%3A%2F%2Fevil.example%2Fx.jpg"),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(
        body_json(response).await["code"],
        "source_image_host_not_allowed"
    );

    // Обложка нужна и обычному пользователю для поиска по источникам;
    // сессия обязательна, роль администратора — нет.
    let user = app.register_extra_user("reader").await;
    let response = app
        .request(with_cookie(
            get("/api/sources/fixture/cover?ref=%2Fcover.jpg"),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(hit_count(&hits, "/cover.jpg"), cover_hits + 1);

    let response = app
        .request(get("/api/sources/fixture/cover?ref=%2Fcover.jpg"))
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn discover_add_rejects_bad_id_and_unknown_source() {
    let _guard = lock().await;
    let (base_url, _server) = start_image_server().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin, &base_url).await;

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/discover/add",
                &json!({ "code": "fixture", "external_id": "a/b" }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(response).await["code"],
        "source_invalid_external_id"
    );

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/sources/nope/search",
                &json!({ "query": "x" }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Источник есть в реестре, но не настроен (base_url NULL) — использовать нельзя
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/discover/add",
                &json!({ "code": "remanga", "external_id": "x" }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body_json(response).await["code"], "source_disabled");

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/discover/add",
                &json!({ "code": "nope", "external_id": "fixture-title" }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn discover_add_links_same_name_to_existing_title() {
    let _guard = lock().await;
    let (base_url, _server) = start_image_server().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin, &base_url).await;

    let manual_id = app.create_title(&admin, "Fixture Title", "manga").await;

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/discover/add",
                &json!({ "code": "fixture", "external_id": "fixture-title" }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let added = body_json(response).await;
    assert_eq!(added["title_id"], manual_id);
    assert_eq!(added["title_created"], false);

    let title = get_json(&app, &format!("/api/titles/{manual_id}"), &admin).await;
    let sources = title["sources"].as_array().expect("sources");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0]["code"], "fixture");
    assert_eq!(sources[0]["chapter_count"], 3);
    assert_eq!(sources[0]["lang"], "ru");
}

#[tokio::test]
async fn title_sync_refreshes_and_keeps_manual_edits() {
    let _guard = lock().await;
    let (base_url, _server) = start_image_server().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin, &base_url).await;
    let imported = import_fixture(&app, &admin).await;
    let title_id = imported["title_id"].as_i64().expect("title id");

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/titles/{title_id}/sync"),
                &json!({}),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let sync = body_json(response).await;
    assert_eq!(sync["refreshed"], true);
    // Импорт уже записал те же метаданные, поэтому поля не изменились.
    assert_eq!(sync["metadata_updated"], false);
    assert_eq!(sync["entries"][0]["status"], "ok");

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    assert_eq!(title["title"]["name"], "Fixture Title");
    assert_eq!(title["title"]["description"], "Fixture description");
    assert_eq!(title["title"]["kind"], "manga");

    // Повторный вызов в пределах TTL не ходит в сеть.
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/titles/{title_id}/sync"),
                &json!({}),
            ),
            &admin,
        ))
        .await;
    assert_eq!(body_json(response).await["refreshed"], false);

    // Ручная правка переживает следующую синхронизацию.
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/titles/{title_id}"),
                &json!({
                    "name": "Fixture Title",
                    "kind": "manga",
                    "description": "Manual description",
                }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    sqlx::query("UPDATE titles SET metadata_synced_at = NULL WHERE id = ?")
        .bind(title_id)
        .execute(&app.pool)
        .await
        .expect("reset sync timestamp");

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/titles/{title_id}/sync"),
                &json!({}),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["refreshed"], true);

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    assert_eq!(title["title"]["description"], "Manual description");
}

#[tokio::test]
async fn title_sync_without_sources_is_a_noop() {
    let _guard = lock().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    let title_id = app.create_title(&admin, "Manual only", "manga").await;

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/titles/{title_id}/sync"),
                &json!({}),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let sync = body_json(response).await;
    assert_eq!(sync["refreshed"], true);
    assert_eq!(sync["metadata_updated"], false);
    assert_eq!(sync["entries"], json!([]));
}

async fn fetch_cover(app: &TestApp, title_id: i64, size: &str, cookie: &str) -> Vec<u8> {
    let response = app
        .request(with_cookie(
            get(&format!("/api/titles/{title_id}/cover?size={size}")),
            cookie,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "cover {size}");
    axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("cover body")
        .to_vec()
}

#[tokio::test]
async fn manual_cover_is_not_overwritten_by_source_or_refresh() {
    let _guard = lock().await;
    let (base_url, _hits) = start_image_server().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin, &base_url).await;
    let imported = import_fixture(&app, &admin).await;
    let title_id = imported["title_id"].as_i64().expect("title id");

    // Автоимпорт уже сохранил mid из источника.
    let automatic = fetch_cover(&app, title_id, "mid", &admin).await;

    // Ручная загрузка заменяет размер.
    let response = app
        .request(with_cookie(
            multipart_file_request(
                &format!("/api/titles/{title_id}/cover?size=mid"),
                "file",
                "manual.png",
                "image/png",
                &png_sized(40, 60),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let manual = fetch_cover(&app, title_id, "mid", &admin).await;
    assert_ne!(
        automatic, manual,
        "manual cover must replace the source one"
    );

    // Refresh добирает отсутствующие размеры, но занятый ручной не трогает.
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/admin/refresh/{title_id}"),
                &json!({}),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "refresh");

    let after_refresh = fetch_cover(&app, title_id, "mid", &admin).await;
    assert_eq!(
        after_refresh, manual,
        "auto cover import must not overwrite a manual cover"
    );
}

#[tokio::test]
async fn cover_from_source_refreshes_source_cover() {
    let _guard = lock().await;
    let (base_url, hits) = start_image_server().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin, &base_url).await;
    let imported = import_fixture(&app, &admin).await;
    let title_id = imported["title_id"].as_i64().expect("title id");

    let before = hit_count(&hits, "/cover.jpg");

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/titles/{title_id}/cover/from-source"),
                &json!({ "replace_manual": false }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "fetch from source");
    let body = body_json(response).await;
    assert_eq!(body["sources"], json!(["fixture"]));
    assert_eq!(body["imported"], json!(["mid"]));
    assert_eq!(body["skipped_manual"], json!([]));
    assert_eq!(body["errors"], json!([]));
    assert_eq!(
        hit_count(&hits, "/cover.jpg"),
        before + 1,
        "explicit refresh must download the source cover again"
    );

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    assert_eq!(title["title"]["manual_cover_sizes"], json!([]));
}

#[tokio::test]
async fn cover_from_source_keeps_manual_until_confirmed() {
    let _guard = lock().await;
    let (base_url, hits) = start_image_server().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin, &base_url).await;
    let imported = import_fixture(&app, &admin).await;
    let title_id = imported["title_id"].as_i64().expect("title id");

    let response = app
        .request(with_cookie(
            multipart_file_request(
                &format!("/api/titles/{title_id}/cover?size=mid"),
                "file",
                "manual.png",
                "image/png",
                &png_sized(40, 60),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let manual = fetch_cover(&app, title_id, "mid", &admin).await;

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    assert_eq!(title["title"]["manual_cover_sizes"], json!(["mid"]));

    // Без подтверждения ручной размер сохраняется и даже не скачивается.
    let downloads_before = hit_count(&hits, "/cover.jpg");
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/titles/{title_id}/cover/from-source"),
                &json!({}),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["imported"], json!([]));
    assert_eq!(body["skipped_manual"], json!(["mid"]));
    let kept = fetch_cover(&app, title_id, "mid", &admin).await;
    assert_eq!(
        kept, manual,
        "manual cover must survive without confirmation"
    );
    assert_eq!(
        hit_count(&hits, "/cover.jpg"),
        downloads_before,
        "manual size must not be downloaded without confirmation"
    );

    // Подтверждённая замена ставит обложку источника.
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/titles/{title_id}/cover/from-source"),
                &json!({ "replace_manual": true }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["imported"], json!(["mid"]));
    assert_eq!(body["skipped_manual"], json!([]));
    let replaced = fetch_cover(&app, title_id, "mid", &admin).await;
    assert_ne!(
        replaced, manual,
        "confirmed refresh replaces the manual cover"
    );

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    assert_eq!(title["title"]["manual_cover_sizes"], json!([]));
}

#[tokio::test]
async fn cover_from_source_without_entries_returns_empty() {
    let _guard = lock().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    let title_id = app.create_title(&admin, "No sources", "manga").await;

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/titles/{title_id}/cover/from-source"),
                &json!({}),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["sources"], json!([]));
    assert_eq!(body["imported"], json!([]));
    assert_eq!(body["skipped_manual"], json!([]));
    assert_eq!(body["errors"], json!([]));
}

#[tokio::test]
async fn cover_from_source_requires_admin_and_known_title() {
    let _guard = lock().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    let user = app.register_extra_user("reader").await;
    let title_id = app.create_title(&admin, "Manual", "manga").await;
    let uri = format!("/api/titles/{title_id}/cover/from-source");

    let response = app
        .request(json_request(Method::POST, &uri, &json!({})))
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = app
        .request(with_cookie(
            json_request(Method::POST, &uri, &json!({})),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                "/api/titles/999999/cover/from-source",
                &json!({}),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
