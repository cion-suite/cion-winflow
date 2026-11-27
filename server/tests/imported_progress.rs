#![cfg(debug_assertions)] // fixture-источник существует только в debug-сборке

mod common;

use axum::http::{Method, StatusCode};
use serde_json::json;
use sqlx::SqlitePool;

use cion_reader::{
    bookmarks::{export, service as bookmarks},
    catalog::selection,
    source_admin::service::{self as source_admin, ImportInput},
};
use common::{TestApp, body_json, get, json_request, with_cookie};

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

async fn import_title(app: &TestApp) -> (i64, i64) {
    let response = source_admin::import(
        &app.state,
        "fixture",
        ImportInput {
            external_id: "fixture-title".to_owned(),
            title_id: None,
            auto_link: false,
        },
        None,
    )
    .await
    .expect("import fixture title");
    (response.title_id, response.source_entry_id)
}

async fn insert_chapter(
    pool: &SqlitePool,
    entry_id: i64,
    number: i64,
    availability: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO chapters (source_entry_id, external_id, chapter_key, number_text, \
                               number_milli, ordinal_site, chapter_kind, availability, last_seen_at) \
         VALUES (?, ?, ?, ?, ?, ?, 'normal', ?, 0)",
    )
    .bind(entry_id)
    .bind(format!("chapter-{number}"))
    .bind(format!("c{number}"))
    .bind(number.to_string())
    .bind(number * 1000)
    .bind(number)
    .bind(availability)
    .execute(pool)
    .await?;
    Ok(())
}

async fn run_import(app: &TestApp, user_id: i64, progress: i64) {
    let (_, handle) = bookmarks::start(&app.state, user_id, &bookmarks_json(progress), "fixture")
        .await
        .expect("start import");
    handle.await.expect("import task");
    let latest = app.state.bookmark_imports.latest(user_id).expect("job");
    assert_eq!(latest.state, bookmarks::ImportState::Completed);
}

async fn library_item(app: &TestApp, cookie: &str) -> serde_json::Value {
    let response = app.request(with_cookie(get("/api/library"), cookie)).await;
    assert_eq!(response.status(), StatusCode::OK, "library");
    let body = body_json(response).await;
    body["items"]
        .as_array()
        .and_then(|items| items.first())
        .cloned()
        .expect("library item")
}

async fn read_keys(app: &TestApp, user_id: i64, title_id: i64) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT chapter_key FROM chapter_reads WHERE user_id = ? AND title_id = ? \
         ORDER BY chapter_key",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_all(&app.pool)
    .await
    .expect("read keys")
}

#[tokio::test]
async fn imported_high_water_shows_effective_progress_and_clamps_export() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin).await;
    let user_id = current_user_id(&app, &admin).await;

    let (title_id, entry_id) = import_title(&app).await;
    // Приводим источник к виду «17 доступных глав, 17 всего»: убираем
    // закрытую c3 и добавляем c4..c18.
    sqlx::query("DELETE FROM chapters WHERE source_entry_id = ? AND chapter_key = 'c3'")
        .bind(entry_id)
        .execute(&app.pool)
        .await
        .expect("delete locked chapter");
    for number in 4..=18 {
        insert_chapter(&app.pool, entry_id, number, "unknown")
            .await
            .expect("insert chapter");
    }
    selection::recompute_stats(&app.pool, entry_id)
        .await
        .expect("recompute stats");

    run_import(&app, user_id, 930).await;

    let (imported, status): (i64, String) = sqlx::query_as(
        "SELECT imported_read_count, status FROM library WHERE user_id = ? AND title_id = ?",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_one(&app.pool)
    .await
    .expect("library row");
    assert_eq!(imported, 930);
    assert_eq!(status, "in_progress");
    // Первые 17 доступных глав отмечены, high-water остался больше.
    assert_eq!(read_keys(&app, user_id, title_id).await.len(), 17);

    let item = library_item(&app, &admin).await;
    assert_eq!(item["read_count"], 930);
    assert_eq!(item["imported_read_count"], 930);
    assert_eq!(item["available_count"], 17);
    assert_eq!(item["chapter_count"], 17);

    let exported = export::export_bookmarks(&app.pool, user_id, "fixture")
        .await
        .expect("export");
    let value: serde_json::Value = serde_json::from_slice(&exported).expect("export json");
    let bookmark = &value["remanga"][0];
    assert_eq!(
        bookmark["progress"], 17,
        "high-water is clamped to chapter_count"
    );
    assert_eq!(bookmark["progress_total"], 17);
}

#[tokio::test]
async fn denominator_is_union_of_available_chapters_across_sources() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin).await;
    let user_id = current_user_id(&app, &admin).await;

    let (title_id, entry_id) = import_title(&app).await;
    sqlx::query("DELETE FROM chapters WHERE source_entry_id = ? AND chapter_key = 'c3'")
        .bind(entry_id)
        .execute(&app.pool)
        .await
        .expect("delete locked chapter");
    for number in 4..=18 {
        insert_chapter(&app.pool, entry_id, number, "unknown")
            .await
            .expect("insert chapter");
    }
    selection::recompute_stats(&app.pool, entry_id)
        .await
        .expect("recompute stats");
    run_import(&app, user_id, 930).await;

    // Второй источник должен быть включён: знаменатель карточки считает
    // только главы включённых источников, как и список на странице тайтла.
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                "/api/sources/rumix",
                &json!({ "base_url": "http://127.0.0.1:9", "enabled": true }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "configure rumix");

    // Второй источник того же тайтла: 4 новых главы, одна общая по ключу,
    // две связанные alias-ом (пара entry и глобальная), одна свежая закрытая
    // и одна устаревше-закрытая. Знаменатель — объединение канонических
    // ключей: общие и alias-главы не удваиваются, свежий locked скрыт, а
    // устаревший считается unknown — так же, как на странице тайтла
    // (TTL из `selection::view`).
    let fixture_source_id: i64 =
        sqlx::query_scalar("SELECT id FROM sources WHERE code = 'fixture'")
            .fetch_one(&app.pool)
            .await
            .expect("fixture source");
    let other_source_id: i64 = sqlx::query_scalar("SELECT id FROM sources WHERE code = 'rumix'")
        .fetch_one(&app.pool)
        .await
        .expect("rumix source");
    let other_entry_id: i64 = sqlx::query_scalar(
        "INSERT INTO source_entries (title_id, source_id, external_id, is_primary, \
                                     chapter_count, available_count) \
         VALUES (?, ?, 'rumix-title', 0, 8, 6) RETURNING id",
    )
    .bind(title_id)
    .bind(other_source_id)
    .fetch_one(&app.pool)
    .await
    .expect("second entry");

    for number in 19..=22 {
        insert_chapter(&app.pool, other_entry_id, number, "available")
            .await
            .expect("insert other chapter");
    }
    insert_chapter(&app.pool, other_entry_id, 1, "available")
        .await
        .expect("duplicate by key");
    insert_chapter(&app.pool, other_entry_id, 900, "available")
        .await
        .expect("alias chapter");
    insert_chapter(&app.pool, other_entry_id, 23, "locked")
        .await
        .expect("fresh locked chapter");
    sqlx::query(
        "UPDATE chapters SET availability_checked_at = unixepoch() \
         WHERE source_entry_id = ? AND chapter_key = 'c23'",
    )
    .bind(other_entry_id)
    .execute(&app.pool)
    .await
    .expect("mark fresh locked");
    insert_chapter(&app.pool, other_entry_id, 24, "locked")
        .await
        .expect("stale locked chapter");
    sqlx::query(
        "INSERT INTO chapter_aliases \
             (title_id, alias_key, canonical_key, source_entry_id, origin, created_at) \
         VALUES (?, 'c900', 'c5', ?, 'manual', 0)",
    )
    .bind(title_id)
    .bind(other_entry_id)
    .execute(&app.pool)
    .await
    .expect("alias");
    // Глобальная ручная пара (без source_entry) тоже сводится к canonical.
    insert_chapter(&app.pool, other_entry_id, 901, "available")
        .await
        .expect("global alias chapter");
    sqlx::query(
        "INSERT INTO chapter_aliases \
             (title_id, alias_key, canonical_key, source_entry_id, origin, created_at) \
         VALUES (?, 'c901', 'c6', NULL, 'manual', 0)",
    )
    .bind(title_id)
    .execute(&app.pool)
    .await
    .expect("global alias");

    // Preferred не влияет на знаменатель: карточка показывает объединение.
    sqlx::query("UPDATE titles SET preferred_source_id = ? WHERE id = ?")
        .bind(fixture_source_id)
        .bind(title_id)
        .execute(&app.pool)
        .await
        .expect("prefer fixture");
    assert_eq!(
        library_item(&app, &admin).await["available_count"],
        22,
        "17 fixture chapters + 4 unique + 1 stale locked (unknown) from the second source"
    );

    sqlx::query("UPDATE titles SET preferred_source_id = ? WHERE id = ?")
        .bind(other_source_id)
        .bind(title_id)
        .execute(&app.pool)
        .await
        .expect("prefer rumix");
    assert_eq!(
        library_item(&app, &admin).await["available_count"],
        22,
        "denominator does not depend on the preferred source"
    );

    // Выключенный источник не может отдать главу, поэтому его главы не
    // входят в знаменатель.
    sqlx::query("UPDATE sources SET enabled = 0 WHERE id = ?")
        .bind(other_source_id)
        .execute(&app.pool)
        .await
        .expect("disable rumix");
    assert_eq!(
        library_item(&app, &admin).await["available_count"],
        17,
        "disabled source chapters are not counted"
    );
}

#[tokio::test]
async fn import_marks_reappeared_chapter_but_not_for_other_users() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin).await;
    let user_id = current_user_id(&app, &admin).await;

    import_title(&app).await;
    run_import(&app, user_id, 2).await;

    let title_id: i64 = sqlx::query_scalar("SELECT title_id FROM library WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(&app.pool)
        .await
        .expect("title id");
    let entry_id: i64 = sqlx::query_scalar(
        "SELECT e.id FROM source_entries e JOIN sources s ON s.id = e.source_id \
         WHERE e.title_id = ? AND s.code = 'fixture'",
    )
    .bind(title_id)
    .fetch_one(&app.pool)
    .await
    .expect("entry id");

    let other = app.register_extra_user("reader2").await;
    let other_id = current_user_id(&app, &other).await;

    // Глава «пропала» из источника и из журнала прочитанного.
    sqlx::query(
        "DELETE FROM chapter_reads WHERE user_id = ? AND title_id = ? AND chapter_key = 'c2'",
    )
    .bind(user_id)
    .bind(title_id)
    .execute(&app.pool)
    .await
    .expect("unread c2");
    sqlx::query("DELETE FROM chapters WHERE source_entry_id = ? AND chapter_key = 'c2'")
        .bind(entry_id)
        .execute(&app.pool)
        .await
        .expect("drop c2");

    // Повторный импорт возвращает c2; high-water 2 делает её прочитанной.
    source_admin::import(
        &app.state,
        "fixture",
        ImportInput {
            external_id: "fixture-title".to_owned(),
            title_id: None,
            auto_link: false,
        },
        None,
    )
    .await
    .expect("re-import");

    assert_eq!(
        read_keys(&app, user_id, title_id).await,
        vec!["c1".to_owned(), "c2".to_owned()]
    );
    assert!(
        read_keys(&app, other_id, title_id).await.is_empty(),
        "auto-mark is per-user"
    );
}

#[tokio::test]
async fn sync_marks_new_chapter_and_keeps_manually_unread_chapters() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure_fixture(&app, &admin).await;
    let user_id = current_user_id(&app, &admin).await;

    import_title(&app).await;
    run_import(&app, user_id, 2).await;

    let title_id: i64 = sqlx::query_scalar("SELECT title_id FROM library WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(&app.pool)
        .await
        .expect("title id");
    let entry_id: i64 = sqlx::query_scalar(
        "SELECT e.id FROM source_entries e JOIN sources s ON s.id = e.source_id \
         WHERE e.title_id = ? AND s.code = 'fixture'",
    )
    .bind(title_id)
    .fetch_one(&app.pool)
    .await
    .expect("entry id");

    // Пользователь вручную снял отметку с c1 (глава осталась доступной).
    sqlx::query(
        "DELETE FROM chapter_reads WHERE user_id = ? AND title_id = ? AND chapter_key = 'c1'",
    )
    .bind(user_id)
    .bind(title_id)
    .execute(&app.pool)
    .await
    .expect("unread c1");

    // c2 «появилась заново»: sync должен отметить её по high-water.
    sqlx::query(
        "DELETE FROM chapter_reads WHERE user_id = ? AND title_id = ? AND chapter_key = 'c2'",
    )
    .bind(user_id)
    .bind(title_id)
    .execute(&app.pool)
    .await
    .expect("unread c2");
    sqlx::query("DELETE FROM chapters WHERE source_entry_id = ? AND chapter_key = 'c2'")
        .bind(entry_id)
        .execute(&app.pool)
        .await
        .expect("drop c2");

    sqlx::query("UPDATE titles SET metadata_synced_at = NULL WHERE id = ?")
        .bind(title_id)
        .execute(&app.pool)
        .await
        .expect("reset sync ttl");

    let response = source_admin::sync_title(&app.state, title_id, user_id)
        .await
        .expect("sync");
    assert!(response.refreshed);

    let keys = read_keys(&app, user_id, title_id).await;
    assert_eq!(
        keys,
        vec!["c2".to_owned()],
        "new chapter is marked, manual unread stays"
    );
}
