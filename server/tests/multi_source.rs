#![cfg(debug_assertions)] // fixture-источник существует только в debug-сборке

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use cion_reader::{auth::session::now_unix, sources::fixture};
use common::{TestApp, body_json, get, json_request, with_cookie};

/// fixture-счётчик глобальный: тесты файла идут по очереди.
static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn lock() -> tokio::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().await
}

async fn configure(app: &TestApp, admin: &str, code: &str, base_url: &str) {
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/sources/{code}"),
                &json!({ "base_url": base_url, "enabled": true }),
            ),
            admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "configure {code}");
}

async fn configure_fixture(app: &TestApp, admin: &str) -> String {
    // Фикстуре нужен только base_url: сеть она не использует.
    configure(app, admin, "fixture", "http://127.0.0.1:9").await;
    sqlx::query_scalar::<_, i64>("SELECT id FROM sources WHERE code = 'fixture'")
        .fetch_one(&app.pool)
        .await
        .expect("fixture source id")
        .to_string()
}

async fn import_fixture(app: &TestApp, admin: &str, title_id: Option<i64>) -> Value {
    // Существующий тайтл привязывается через /sources, новый импортируется
    // ленивым добавлением: отдельного админского импорта больше нет.
    let (path, body) = match title_id {
        Some(title_id) => (
            format!("/api/titles/{title_id}/sources"),
            json!({ "code": "fixture", "external_id": "fixture-title" }),
        ),
        None => (
            "/api/discover/add".to_owned(),
            json!({ "code": "fixture", "external_id": "fixture-title" }),
        ),
    };
    let response = app
        .request(with_cookie(json_request(Method::POST, &path, &body), admin))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "import fixture");
    let mut imported = body_json(response).await;
    if imported["source_entry_id"].is_null() {
        let entry_id: i64 = sqlx::query_scalar(
            "SELECT e.id FROM source_entries e JOIN sources s ON s.id = e.source_id              WHERE e.title_id = ? AND s.code = 'fixture'",
        )
        .bind(imported["title_id"].as_i64())
        .fetch_one(&app.pool)
        .await
        .expect("fixture source entry");
        imported["source_entry_id"] = json!(entry_id);
    }
    imported
}

async fn get_json(app: &TestApp, uri: &str, cookie: &str) -> Value {
    let response = app.request(with_cookie(get(uri), cookie)).await;
    assert_eq!(response.status(), StatusCode::OK, "{uri}");
    body_json(response).await
}

fn delete_request(uri: &str, admin: &str) -> axum::http::Request<axum::body::Body> {
    axum::http::Request::builder()
        .method(Method::DELETE)
        .uri(uri)
        .header(axum::http::header::ORIGIN, common::ORIGIN)
        .header("sec-fetch-site", "same-origin")
        .header(axum::http::header::COOKIE, format!("sid={admin}"))
        .body(axum::body::Body::empty())
        .expect("request")
}

async fn source_id(app: &TestApp, code: &str) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT id FROM sources WHERE code = ?")
        .bind(code)
        .fetch_one(&app.pool)
        .await
        .expect("source id")
}

/// Второй source_entry для того же тайтла: без сети, только строки БД.
async fn seed_remanga_entry(
    app: &TestApp,
    admin: &str,
    title_id: i64,
    chapter_count: i64,
    available_count: i64,
) -> i64 {
    configure(app, admin, "remanga", "https://api.remanga.org").await;
    let remanga_id = source_id(app, "remanga").await;
    let now = now_unix();

    let entry_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO source_entries \
         (title_id, source_id, external_id, is_primary, chapter_count, available_count, \
          locked_count, missing_count, stats_updated_at, last_synced_at) \
         VALUES (?, ?, 'remanga-title', 0, ?, ?, 0, 0, ?, ?) RETURNING id",
    )
    .bind(title_id)
    .bind(remanga_id)
    .bind(chapter_count)
    .bind(available_count)
    .bind(now)
    .bind(now)
    .fetch_one(&app.pool)
    .await
    .expect("source entry");

    for key in ["c1", "c2", "c3"] {
        sqlx::query(
            "INSERT INTO chapters \
             (source_entry_id, external_id, chapter_key, number_text, number_milli, ordinal_site, \
              content_kind, is_locked, availability, availability_checked_at, last_seen_at) \
             VALUES (?, ?, ?, ?, ?, 0, 'images', 1, 'locked', ?, ?)",
        )
        .bind(entry_id)
        .bind(format!("remanga-{key}"))
        .bind(key)
        .bind(key.trim_start_matches('c'))
        .bind(key.trim_start_matches('c').parse::<i64>().unwrap_or(0) * 1000)
        .bind(now)
        .bind(now)
        .execute(&app.pool)
        .await
        .expect("chapter");
    }

    entry_id
}

#[tokio::test]
async fn priorities_replace_and_preferred_source_recompute() {
    let _guard = lock().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    let fixture_id = configure_fixture(&app, &admin)
        .await
        .parse::<i64>()
        .expect("fixture id");

    let imported = import_fixture(&app, &admin, None).await;
    let title_id = imported["title_id"].as_i64().expect("title id");
    let remanga_id = seed_remanga_entry(&app, &admin, title_id, 3, 3).await;
    let remanga_source_id = source_id(&app, "remanga").await;

    // Оба источника равны по числу глав; побеждает приоритет.
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/admin/source-priorities",
                &json!({
                    "lang": "ru",
                    "source_codes": ["remanga", "fixture"],
                }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let priorities = body_json(response).await;
    assert_eq!(priorities["sources"][0]["code"], "remanga");
    assert_eq!(priorities["sources"][0]["rank"], 0);

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    assert_eq!(title["title"]["preferred_source_id"], remanga_source_id);

    // Меняем порядок — preferred пересчитывается.
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/admin/source-priorities",
                &json!({
                    "lang": "ru",
                    "source_codes": ["fixture", "remanga"],
                }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    assert_eq!(title["title"]["preferred_source_id"], fixture_id);

    // Полнота больше не перебивает приоритет: fixture остаётся первым,
    // хотя у Remanga стало больше глав.
    sqlx::query("UPDATE source_entries SET chapter_count = 4 WHERE id = ?")
        .bind(remanga_id)
        .execute(&app.pool)
        .await
        .expect("update stats");

    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/admin/source-priorities",
                &json!({
                    "lang": "ru",
                    "source_codes": ["fixture", "remanga"],
                }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    assert_eq!(title["title"]["preferred_source_id"], fixture_id);
}

#[tokio::test]
async fn exact_language_priorities_override_star_fallback() {
    let _guard = lock().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    let fixture_id = configure_fixture(&app, &admin)
        .await
        .parse::<i64>()
        .expect("fixture id");

    let imported = import_fixture(&app, &admin, None).await;
    let title_id = imported["title_id"].as_i64().expect("title id");
    seed_remanga_entry(&app, &admin, title_id, 3, 3).await;
    let remanga_source_id = source_id(&app, "remanga").await;

    // Языковой набор побеждает '*' даже при равном числе глав.
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/admin/source-priorities",
                &json!({ "lang": "ru", "source_codes": ["fixture"] }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/admin/source-priorities",
                &json!({ "lang": "*", "source_codes": ["remanga"] }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    assert_eq!(title["title"]["preferred_source_id"], fixture_id);

    // Пустой языковой список означает «использовать '*'».
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/admin/source-priorities",
                &json!({ "lang": "ru", "source_codes": [] }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    assert_eq!(title["title"]["preferred_source_id"], remanga_source_id);
}

#[tokio::test]
async fn priorities_validation_is_typed() {
    let _guard = lock().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin).await;

    // PUT без base_url не должен стирать уже настроенный origin.
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/sources/fixture",
                &json!({ "enabled": true }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let base_url: Option<String> =
        sqlx::query_scalar("SELECT base_url FROM sources WHERE code = 'fixture'")
            .fetch_one(&app.pool)
            .await
            .expect("base url");
    assert!(base_url.is_some(), "base_url must be preserved");

    for body in [
        json!({ "lang": "", "source_codes": [] }),
        json!({ "lang": "ru", "source_codes": ["nope"] }),
        json!({ "lang": "ru", "source_codes": ["fixture", "fixture"] }),
    ] {
        let response = app
            .request(with_cookie(
                json_request(Method::PUT, "/api/admin/source-priorities", &body),
                &admin,
            ))
            .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
    }

    let response = app
        .request(with_cookie(
            get("/api/admin/source-priorities?lang=ru"),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn locked_preferred_source_uses_available_variant_and_aliases() {
    let _guard = lock().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin).await;

    let imported = import_fixture(&app, &admin, None).await;
    let title_id = imported["title_id"].as_i64().expect("title id");
    let _remanga_entry = seed_remanga_entry(&app, &admin, title_id, 3, 2).await;
    let remanga_source_id = source_id(&app, "remanga").await;

    // Preferred — Remanga; его c1 свежий locked, поэтому для показа сразу
    // выбирается доступный fixture: известная недоступность важнее приоритета.
    sqlx::query("UPDATE titles SET preferred_source_id = ? WHERE id = ?")
        .bind(remanga_source_id)
        .bind(title_id)
        .execute(&app.pool)
        .await
        .expect("preferred");

    let content = get_json(
        &app,
        &format!("/api/titles/{title_id}/chapters/c1/content"),
        &admin,
    )
    .await;
    assert_eq!(content["source"]["code"], "fixture");
    assert!(
        content["fallback"].is_null(),
        "locked variant is skipped before the network request"
    );

    // Для отображения выбран доступный вариант, а не locked preferred.
    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    let chapter = title["chapters"]
        .as_array()
        .expect("chapters")
        .iter()
        .find(|chapter| chapter["chapter_key"] == "c1")
        .expect("c1 group");
    assert_eq!(chapter["is_locked"], false);
    let selected_id = chapter["selected_chapter_id"]
        .as_i64()
        .expect("selected id");
    let selected = chapter["variants"]
        .as_array()
        .expect("variants")
        .iter()
        .find(|variant| variant["chapter_id"].as_i64() == Some(selected_id))
        .expect("selected variant");
    assert_eq!(selected["source_code"], "fixture");

    // Ключи разошлись: alias обязан добавить вариант в fallback.
    sqlx::query(
        "UPDATE chapters SET chapter_key = 'r1' \
         WHERE source_entry_id = (SELECT e.id FROM source_entries e \
                                  JOIN sources s ON s.id = e.source_id \
                                  WHERE e.title_id = ? AND s.code = 'remanga' LIMIT 1) \
           AND chapter_key = 'c1'",
    )
    .bind(title_id)
    .execute(&app.pool)
    .await
    .expect("rename remanga key");
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/titles/{title_id}/chapter-aliases"),
                &json!({ "alias_key": "r1", "canonical_key": "c1" }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    // Первый успешный content мог пересчитать preferred; для проверки
    // alias-варианта снова ставим Remanga предпочтительным.
    sqlx::query("UPDATE titles SET preferred_source_id = ? WHERE id = ?")
        .bind(remanga_source_id)
        .bind(title_id)
        .execute(&app.pool)
        .await
        .expect("preferred again");

    let content = get_json(
        &app,
        &format!("/api/titles/{title_id}/chapters/c1/content"),
        &admin,
    )
    .await;
    assert_eq!(content["source"]["code"], "fixture");
    assert!(
        content["fallback"].is_null(),
        "alias variant is ordered by availability too"
    );

    let response = app
        .request(delete_request(
            &format!("/api/titles/{title_id}/chapter-aliases/r1"),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Все варианты locked: 403, без похода в сеть.
    fixture::reset_calls();
    let response = app
        .request(with_cookie(
            get(&format!("/api/titles/{title_id}/chapters/c3/content")),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(body_json(response).await["code"], "source_locked");
    assert_eq!(fixture::call_count(), 0);

    // alias резолвится в canonical для прогресса.
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/titles/{title_id}/chapter-aliases"),
                &json!({ "alias_key": "c3", "canonical_key": "c1" }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/progress/{title_id}"),
                &json!({ "chapter_key": "c3", "page": 0 }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["chapter_key"], "c1");

    // Цепочка alias запрещена: c1 уже canonical для c3.
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/titles/{title_id}/chapter-aliases"),
                &json!({ "alias_key": "c1", "canonical_key": "c2" }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(body_json(response).await["code"], "alias_chain");

    // Удаление alias
    let response = app
        .request(delete_request(
            &format!("/api/titles/{title_id}/chapter-aliases/c3"),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    assert!(
        title["aliases"]
            .as_array()
            .is_some_and(|items| items.is_empty()),
        "alias must be deleted"
    );
}

#[tokio::test]
async fn translation_groups_share_one_chapter_and_unique_key_stats() {
    let _guard = lock().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin).await;
    let imported = import_fixture(&app, &admin, None).await;
    let title_id = imported["title_id"].as_i64().expect("title id");
    let entry_id = imported["source_entry_id"].as_i64().expect("entry id");
    let now = now_unix();

    sqlx::query(
        "INSERT INTO chapters \
         (source_entry_id, external_id, chapter_key, number_text, number_milli, ordinal_site, \
          content_kind, group_name, is_locked, availability, availability_checked_at, last_seen_at) \
         VALUES (?, 'chapter-1-b', 'c1', '1', 1000, 1, 'images', 'Group B', 1, 'locked', ?, ?)",
    )
    .bind(entry_id)
    .bind(now)
    .bind(now)
    .execute(&app.pool)
    .await
    .expect("second group");

    cion_reader::catalog::selection::recompute_stats(&app.pool, entry_id)
        .await
        .expect("stats");

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    let chapter = title["chapters"]
        .as_array()
        .expect("chapters")
        .iter()
        .find(|chapter| chapter["chapter_key"] == "c1")
        .expect("c1");
    assert_eq!(chapter["variants"].as_array().expect("variants").len(), 2);
    assert!(
        chapter["variants"]
            .as_array()
            .expect("variants")
            .iter()
            .any(|variant| variant["group_name"] == "Group B"),
        "translation group must be visible"
    );
    assert_eq!(
        chapter["is_locked"], false,
        "one variant is still available"
    );

    // Полнота считается по уникальным ключам, а не по строкам групп.
    assert_eq!(title["sources"][0]["chapter_count"], 3);
    assert_eq!(title["sources"][0]["available_count"], 2);
}

#[tokio::test]
async fn attach_detach_primary_refresh_and_disabled_source() {
    let _guard = lock().await;
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin).await;

    let title_id = app.create_title(&admin, "Manual title", "manga").await;

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/titles/{title_id}/sources"),
                &json!({ "code": "fixture", "external_id": "fixture-title" }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let attached = body_json(response).await;
    let entry_id = attached["source_entry_id"].as_i64().expect("entry id");
    assert_eq!(attached["imported_chapters"], 3);

    // Второй external_id того же источника к тому же тайтлу неоднозначен.
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/titles/{title_id}/sources"),
                &json!({
                    "code": "fixture",
                    "external_id": "fixture-title-2",
                }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(body_json(response).await["code"], "source_already_attached");

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    assert_eq!(title["title"]["lang"], "ru", "attach fills missing lang");

    // Primary source
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/titles/{title_id}/primary-source"),
                &json!({ "source_entry_id": entry_id }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    assert_eq!(title["sources"][0]["is_primary"], true);

    // Refresh
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
    assert_eq!(response.status(), StatusCode::OK);
    let refreshed = body_json(response).await;
    assert_eq!(refreshed["entries"][0]["status"], "ok");
    assert_eq!(refreshed["entries"][0]["imported_chapters"], 3);

    // Alias переживает выключение источника, но не отвязку.
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/titles/{title_id}/chapter-aliases"),
                &json!({ "alias_key": "c3", "canonical_key": "c1" }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    // Выключение источника: preferred сбрасывается, контент — 503.
    let base_url =
        sqlx::query_scalar::<_, String>("SELECT base_url FROM sources WHERE code = 'fixture'")
            .fetch_one(&app.pool)
            .await
            .expect("base url");
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/sources/fixture",
                &json!({ "base_url": base_url, "enabled": false }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    assert_eq!(title["title"]["preferred_source_id"], Value::Null);

    let response = app
        .request(with_cookie(
            get(&format!("/api/titles/{title_id}/chapters/c1/content")),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body_json(response).await["code"], "source_disabled");

    // Detach
    let response = app
        .request(delete_request(
            &format!("/api/titles/{title_id}/sources/{entry_id}"),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    assert_eq!(title["sources"].as_array().expect("sources").len(), 0);
    assert_eq!(title["chapters"].as_array().expect("chapters").len(), 0);
    assert!(
        title["aliases"]
            .as_array()
            .is_some_and(|items| items.is_empty()),
        "dangling aliases must be removed with the detached source"
    );

    // Не-админ не проходит
    let user = app.register_extra_user("reader").await;
    let response = app
        .request(with_cookie(
            get("/api/admin/source-priorities?lang=ru"),
            &user,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
