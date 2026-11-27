#![cfg(debug_assertions)] // fixture-источник существует только в debug-сборке

mod common;

use axum::http::{Method, StatusCode, header};
use serde_json::json;

use cion_reader::bookmarks::service;
use common::{TestApp, body_json, get, json_request, multipart_file_request, with_cookie};

async fn configure_fixture(app: &TestApp, admin: &str) {
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/sources/fixture",
                &json!({ "base_url": "http://127.0.0.1:9", "enabled": true }),
            ),
            admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "configure fixture");
}

async fn current_user_id(app: &TestApp, cookie: &str) -> i64 {
    let response = app.request(with_cookie(get("/api/me"), cookie)).await;
    assert_eq!(response.status(), StatusCode::OK, "me");
    body_json(response).await["user"]["id"]
        .as_i64()
        .expect("user id")
}

fn bookmarks_json(progress: i64) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "exmanga": [{
            "type": 1,
            "progress": progress,
            "progress_total": 3,
            "manga": 1,
            "name": "Fixture Title",
            "dir": "fixture-title",
        }]
    }))
    .expect("json")
}

#[tokio::test]
async fn bookmarks_import_requires_session() {
    let app = TestApp::new().await;
    let response = app
        .request(multipart_file_request(
            "/api/me/import/bookmarks",
            "file",
            "bookmarks.json",
            "application/json",
            &bookmarks_json(1),
        ))
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bookmarks_import_rejects_invalid_and_empty_files() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;

    let invalid = app
        .request(with_cookie(
            multipart_file_request(
                "/api/me/import/bookmarks",
                "file",
                "bookmarks.json",
                "application/json",
                b"not json",
            ),
            &admin,
        ))
        .await;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(invalid).await["code"], "bookmarks_invalid");

    let empty = app
        .request(with_cookie(
            multipart_file_request(
                "/api/me/import/bookmarks",
                "file",
                "bookmarks.json",
                "application/json",
                br#"{"exmanga":[],"remanga":[]}"#,
            ),
            &admin,
        ))
        .await;
    assert_eq!(empty.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(empty).await["code"], "bookmarks_empty");

    let missing = app
        .request(with_cookie(
            multipart_file_request(
                "/api/me/import/bookmarks",
                "other",
                "bookmarks.json",
                "application/json",
                b"{}",
            ),
            &admin,
        ))
        .await;
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(missing).await["code"], "bookmarks_file_required");
}

#[tokio::test]
async fn bookmarks_import_fails_fast_when_source_is_disabled() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;

    let response = app
        .request(with_cookie(
            multipart_file_request(
                "/api/me/import/bookmarks",
                "file",
                "bookmarks.json",
                "application/json",
                &bookmarks_json(1),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body_json(response).await["code"], "source_disabled");
}

#[tokio::test]
async fn bookmarks_import_marks_chapters_and_sets_progress() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin).await;
    let user_id = current_user_id(&app, &admin).await;

    // 10 глав в файле, но у fixture только c1/c2 доступны, c3 закрыта.
    let (job, handle) = service::start(&app.state, user_id, &bookmarks_json(10), "fixture")
        .await
        .expect("start import");
    handle.await.expect("import task");

    assert_eq!(job.state, service::ImportState::Running);

    let latest = app.state.bookmark_imports.latest(user_id).expect("job");
    assert_eq!(latest.state, service::ImportState::Completed);
    assert_eq!(latest.total, 1);
    assert_eq!(latest.processed, 1);
    assert_eq!(latest.succeeded, 1);
    assert_eq!(latest.failed, 0);
    assert_eq!(latest.titles_imported, 1);
    assert_eq!(latest.library_created, 1);
    assert_eq!(latest.read_marked, 2);

    let (title_id, status): (i64, String) =
        sqlx::query_as("SELECT title_id, status FROM library WHERE user_id = ?")
            .bind(user_id)
            .fetch_one(&app.pool)
            .await
            .expect("library row");
    assert_eq!(status, "in_progress");

    let read_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM chapter_reads WHERE user_id = ? AND title_id = ?")
            .bind(user_id)
            .bind(title_id)
            .fetch_one(&app.pool)
            .await
            .expect("read count");
    assert_eq!(read_count, 2);

    let progress_key: String =
        sqlx::query_scalar("SELECT chapter_key FROM progress WHERE user_id = ? AND title_id = ?")
            .bind(user_id)
            .bind(title_id)
            .fetch_one(&app.pool)
            .await
            .expect("progress");
    assert_eq!(progress_key, "c2");

    // Экспорт отдаёт ту же нумерацию, что принимает импорт.
    let exported = cion_reader::bookmarks::export::export_bookmarks(&app.pool, user_id, "fixture")
        .await
        .expect("export");
    let value: serde_json::Value = serde_json::from_slice(&exported).expect("export json");
    let items = value["remanga"].as_array().expect("export items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["type"], 1);
    // В файле 10 глав, high-water клампится к chapter_count (3): закрытая c3
    // тоже входит в общий счётчик глав источника. Фактически отмечено 2
    // главы (c1/c2), но экспорт отдаёт эффективный прогресс, а не число
    // строк chapter_reads.
    assert_eq!(items[0]["progress"], 3);
    assert_eq!(items[0]["progress_total"], 3);
    assert_eq!(items[0]["name"], "Fixture Title");
    assert_eq!(items[0]["dir"], "fixture-title");

    // Повторный импорт не создаёт дублей и не снимает отметки.
    let (_, handle) = service::start(&app.state, user_id, &bookmarks_json(10), "fixture")
        .await
        .expect("re-import");
    handle.await.expect("re-import task");
    let titles: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM titles")
        .fetch_one(&app.pool)
        .await
        .expect("titles");
    assert_eq!(titles, 1);
    let read_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM chapter_reads WHERE user_id = ? AND title_id = ?")
            .bind(user_id)
            .bind(title_id)
            .fetch_one(&app.pool)
            .await
            .expect("read count");
    assert_eq!(read_count, 2);
}

#[tokio::test]
async fn bookmarks_import_keeps_newer_library_status_and_updates_older() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin).await;
    let user_id = current_user_id(&app, &admin).await;

    let (_, handle) = service::start(&app.state, user_id, &bookmarks_json(2), "fixture")
        .await
        .expect("first import");
    handle.await.expect("first import task");

    // Пользователь сам поставил «Брошено» после импорта.
    sqlx::query("UPDATE library SET status = 'dropped' WHERE user_id = ?")
        .bind(user_id)
        .execute(&app.pool)
        .await
        .expect("manual status");

    // В файле прогресс меньше текущего — статус не перезаписываем.
    let (_, handle) = service::start(&app.state, user_id, &bookmarks_json(1), "fixture")
        .await
        .expect("lower import");
    handle.await.expect("lower import task");
    let status: String = sqlx::query_scalar("SELECT status FROM library WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(&app.pool)
        .await
        .expect("status");
    assert_eq!(status, "dropped");

    // В файле прогресс больше текущего — статус берём из файла.
    let (_, handle) = service::start(&app.state, user_id, &bookmarks_json(3), "fixture")
        .await
        .expect("higher import");
    handle.await.expect("higher import task");
    let status: String = sqlx::query_scalar("SELECT status FROM library WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(&app.pool)
        .await
        .expect("status");
    assert_eq!(status, "in_progress");

    let latest = app.state.bookmark_imports.latest(user_id).expect("job");
    assert_eq!(latest.library_updated, 1);
}

#[tokio::test]
async fn bookmarks_import_creates_custom_tab_for_external_status() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin).await;
    let user_id = current_user_id(&app, &admin).await;

    let bytes = serde_json::to_vec(&json!({
        "exmanga": [{
            "type": 34956,
            "progress": 0,
            "name": "Fixture Title",
            "dir": "fixture-title",
        }]
    }))
    .expect("json");

    let (_, handle) = service::start(&app.state, user_id, &bytes, "fixture")
        .await
        .expect("start import");
    handle.await.expect("import task");

    let (name, source_ref): (String, Option<String>) =
        sqlx::query_as("SELECT name, source_ref FROM library_tabs WHERE user_id = ?")
            .bind(user_id)
            .fetch_one(&app.pool)
            .await
            .expect("tab");
    assert_eq!(name, "new1");
    assert_eq!(source_ref.as_deref(), Some("fixture:bookmark:34956"));

    let (status, custom_tab_id): (String, Option<i64>) =
        sqlx::query_as("SELECT status, custom_tab_id FROM library WHERE user_id = ?")
            .bind(user_id)
            .fetch_one(&app.pool)
            .await
            .expect("library");
    assert_eq!(status, "planned");
    assert!(custom_tab_id.is_some());

    let exported = cion_reader::bookmarks::export::export_bookmarks(&app.pool, user_id, "fixture")
        .await
        .expect("export");
    let value: serde_json::Value = serde_json::from_slice(&exported).expect("export json");
    assert_eq!(value["remanga"][0]["type"], 34_956);

    // Повторный импорт переиспользует закладку, а не создаёт новую.
    let (_, handle) = service::start(&app.state, user_id, &bytes, "fixture")
        .await
        .expect("re-import");
    handle.await.expect("re-import task");
    let tabs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM library_tabs WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(&app.pool)
        .await
        .expect("tabs");
    assert_eq!(tabs, 1);
}

#[test]
#[ignore = "needs BOOKMARKS_JSON pointing to a real Remanga/ExManga export"]
fn parses_real_export_from_env() {
    let path = std::env::var("BOOKMARKS_JSON").expect("BOOKMARKS_JSON");
    let bytes = std::fs::read(&path).expect("read export");
    let outcome = cion_reader::bookmarks::parse::parse(&bytes).expect("parse export");
    let mut statuses: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for item in &outcome.items {
        let label = match item.status {
            cion_reader::bookmarks::parse::BookmarkStatus::Builtin(status) => status.to_owned(),
            cion_reader::bookmarks::parse::BookmarkStatus::Custom(id) => format!("custom:{id}"),
        };
        *statuses.entry(label).or_default() += 1;
    }
    eprintln!(
        "bookmarks: {} valid, {} invalid; statuses: {statuses:?}",
        outcome.items.len(),
        outcome.invalid.len()
    );
    assert!(!outcome.items.is_empty());
}

#[tokio::test]
async fn bookmarks_import_does_not_move_progress_backwards() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin).await;
    let user_id = current_user_id(&app, &admin).await;

    let (_, handle) = service::start(&app.state, user_id, &bookmarks_json(2), "fixture")
        .await
        .expect("first import");
    handle.await.expect("first import task");

    // Журнал потерян, но позиция осталась на c2 (так выглядит гонка с
    // читалкой, которая сохранила более позднюю главу).
    sqlx::query("DELETE FROM chapter_reads WHERE user_id = ?")
        .bind(user_id)
        .execute(&app.pool)
        .await
        .expect("clear reads");

    let (_, handle) = service::start(&app.state, user_id, &bookmarks_json(1), "fixture")
        .await
        .expect("lower import");
    handle.await.expect("lower import task");

    let progress_key: String =
        sqlx::query_scalar("SELECT chapter_key FROM progress WHERE user_id = ?")
            .bind(user_id)
            .fetch_one(&app.pool)
            .await
            .expect("progress");
    assert_eq!(progress_key, "c2");
}

#[tokio::test]
async fn bookmarks_export_requires_session_and_is_a_file() {
    let app = TestApp::new().await;
    let anonymous = app.request(get("/api/me/export/bookmarks")).await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    app.create_admin().await;
    let admin = app.login_admin().await;
    let response = app
        .request(with_cookie(get("/api/me/export/bookmarks"), &admin))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_DISPOSITION],
        "attachment; filename=\"bookmarks.json\""
    );
    let value = body_json(response).await;
    assert_eq!(value["remanga"], json!([]));
}
