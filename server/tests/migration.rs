//! Проверка опасной миграции 0008 на базе, уже перешедшей на 0007.
//!
//! SQLx для SQLite оборачивает миграцию в транзакцию, поэтому выключить
//! внешние ключи из SQL-файла нельзя. `db::run_migrations` выключает их на
//! соединении мигратора до запуска; проверяем, что DROP TABLE не унёс
//! дочерние строки и foreign_key_check чист.

use std::path::Path;

use sqlx::{
    SqlitePool,
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use tempfile::TempDir;

use cion_reader::db;

const MANIFEST: &str = env!("CARGO_MANIFEST_DIR");

const UP_TO_0007: &[&str] = &[
    "0001_auth.sql",
    "0002_catalog.sql",
    "0003_source_entry_per_source.sql",
    "0004_library_tabs.sql",
    "0005_title_covers.sql",
    "0006_favorite_status.sql",
    "0007_library_custom_status.sql",
];

const UP_TO_0011: &[&str] = &[
    "0001_auth.sql",
    "0002_catalog.sql",
    "0003_source_entry_per_source.sql",
    "0004_library_tabs.sql",
    "0005_title_covers.sql",
    "0006_favorite_status.sql",
    "0007_library_custom_status.sql",
    "0008_title_types.sql",
    "0009_title_metadata_sync.sql",
    "0010_alias_ranges.sql",
    "0011_user_profiles.sql",
];

async fn partial_migrations(names: &[&str]) -> (TempDir, Migrator) {
    let dir = tempfile::tempdir().expect("migrations dir");
    let source = Path::new(MANIFEST).join("migrations");
    for name in names {
        std::fs::copy(source.join(name), dir.path().join(name)).expect("copy migration");
    }
    let migrator = Migrator::new(dir.path()).await.expect("partial migrator");
    (dir, migrator)
}

/// 0012 добавляет монотонную версию аватара и переносит в неё уже
/// сохранённое время загрузки, не трогая пользователей без аватара.
#[tokio::test]
async fn avatar_version_backfills_from_avatar_updated_at() {
    let (_partial_dir, partial) = partial_migrations(UP_TO_0011).await;

    let db_dir = tempfile::tempdir().expect("db dir");
    let db_path = db_dir.path().join("avatar-version.db");
    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .foreign_keys(true);
    let pool: SqlitePool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("pool");

    partial.run(&pool).await.expect("migrations 0001-0011");
    sqlx::query(
        "INSERT INTO users (id, username, password_hash, role, created_at, avatar_updated_at) \
         VALUES (1, 'with-avatar', 'hash', 'user', 0, 123), \
                (2, 'without-avatar', 'hash', 'user', 0, NULL)",
    )
    .execute(&pool)
    .await
    .expect("users");
    pool.close().await;

    let url = format!("sqlite://{}", db_path.to_string_lossy().replace('\\', "/"));
    let migrated = db::connect(&url).await.expect("full migrations");
    let with_avatar: i64 = sqlx::query_scalar("SELECT avatar_version FROM users WHERE id = 1")
        .fetch_one(&migrated)
        .await
        .expect("version of user with avatar");
    let without_avatar: i64 = sqlx::query_scalar("SELECT avatar_version FROM users WHERE id = 2")
        .fetch_one(&migrated)
        .await
        .expect("version of user without avatar");

    assert_eq!(with_avatar, 123);
    assert_eq!(without_avatar, 0);
}

#[tokio::test]
async fn title_types_migration_preserves_child_rows() {
    let (_partial_dir, partial) = partial_migrations(UP_TO_0007).await;

    let db_dir = tempfile::tempdir().expect("db dir");
    let db_path = db_dir.path().join("upgrade.db");
    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .foreign_keys(true);
    let pool: SqlitePool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("pool");

    partial.run(&pool).await.expect("migrations 0001-0007");

    sqlx::query(
        "INSERT INTO users (id, username, password_hash, role, created_at) \
         VALUES (1, 'admin', 'hash', 'admin', 0)",
    )
    .execute(&pool)
    .await
    .expect("user");
    sqlx::query(
        "INSERT INTO sources (id, code, name, enabled, created_at) \
         VALUES (1, 'remanga', 'Remanga', 1, 0), (2, 'rumix', 'RuMix', 1, 0)",
    )
    .execute(&pool)
    .await
    .expect("sources");
    sqlx::query(
        "INSERT INTO titles (id, kind, name, name_lower, lang, created_at, updated_at) \
         VALUES (1, 'manga', 'A', 'a', 'ru', 0, 0), \
                (2, 'anime', 'B', 'b', 'ru', 0, 0), \
                (3, 'book', 'C', 'c', 'en', 0, 0)",
    )
    .execute(&pool)
    .await
    .expect("titles");
    sqlx::query(
        "INSERT INTO source_entries \
             (id, title_id, source_id, kind, external_id, is_primary, raw_json) \
         VALUES (1, 1, 1, 'manga', 'e1', 1, NULL), \
                (2, 2, 1, 'anime', 'e2', 0, '{\"type\":{\"name\":\"Манхва\"}}'), \
                (3, 3, 2, 'book', 'e3', 0, NULL)",
    )
    .execute(&pool)
    .await
    .expect("entries");
    sqlx::query(
        "INSERT INTO chapters (id, source_entry_id, external_id, chapter_key, published_at) \
         VALUES (1, 1, 'c1', 'c1', 100), (2, 2, 'c1', 'c1', 200), (3, 3, 'c1', 'c1', 300)",
    )
    .execute(&pool)
    .await
    .expect("chapters");
    sqlx::query("INSERT INTO pages (chapter_id, idx, image_ref) VALUES (1, 0, 'p')")
        .execute(&pool)
        .await
        .expect("page");
    sqlx::query(
        "INSERT INTO title_covers (title_id, size, path, updated_at) \
         VALUES (1, 'low', 'covers/1/low.webp', 0)",
    )
    .execute(&pool)
    .await
    .expect("cover");
    sqlx::query(
        "INSERT INTO chapter_aliases (title_id, canonical_key, alias_key, created_at) \
         VALUES (1, 'c1', 'alias-1', 0)",
    )
    .execute(&pool)
    .await
    .expect("alias");
    sqlx::query(
        "INSERT INTO chapter_texts (chapter_id, format, body, byte_len, fetched_at) \
         VALUES (1, 'plain', 'text', 4, 0)",
    )
    .execute(&pool)
    .await
    .expect("text");
    sqlx::query(
        "INSERT INTO library (user_id, title_id, status, added_at, updated_at) \
         VALUES (1, 1, 'planned', 0, 0), (1, 2, 'planned', 0, 0), (1, 3, 'planned', 0, 0)",
    )
    .execute(&pool)
    .await
    .expect("library");
    sqlx::query(
        "INSERT INTO progress (user_id, title_id, chapter_key, updated_at) \
         VALUES (1, 1, 'c1', 0)",
    )
    .execute(&pool)
    .await
    .expect("progress");
    sqlx::query(
        "INSERT INTO chapter_reads (user_id, title_id, chapter_key, read_at) \
         VALUES (1, 1, 'c1', 0)",
    )
    .execute(&pool)
    .await
    .expect("reads");
    sqlx::query(
        "INSERT INTO source_priorities (kind, lang, source_id, rank, created_at) \
         VALUES ('manga', 'ru', 1, 0, 0), ('manga', 'ru', 2, 1, 0), \
                ('anime', 'ru', 1, 0, 0), ('book', '*', 2, 0, 0)",
    )
    .execute(&pool)
    .await
    .expect("priorities");

    db::run_migrations(&pool).await.expect("migration 0008");

    let kinds = sqlx::query_as::<_, (i64, String)>("SELECT id, kind FROM titles ORDER BY id")
        .fetch_all(&pool)
        .await
        .expect("kinds");
    assert_eq!(
        kinds,
        vec![
            (1, "manga".to_owned()),
            (2, "manhwa".to_owned()),
            (3, "other".to_owned()),
        ]
    );

    let counts = sqlx::query_as::<_, (i64, i64, i64, i64, i64, i64, i64, i64, i64)>(
        "SELECT \
           (SELECT COUNT(*) FROM source_entries), \
           (SELECT COUNT(*) FROM chapters), \
           (SELECT COUNT(*) FROM pages), \
           (SELECT COUNT(*) FROM chapter_texts), \
           (SELECT COUNT(*) FROM title_covers), \
           (SELECT COUNT(*) FROM chapter_aliases), \
           (SELECT COUNT(*) FROM library), \
           (SELECT COUNT(*) FROM progress), \
           (SELECT COUNT(*) FROM chapter_reads)",
    )
    .fetch_one(&pool)
    .await
    .expect("counts");
    assert_eq!(
        counts,
        (3, 3, 1, 1, 1, 1, 3, 1, 1),
        "дочерние строки должны сохраниться"
    );

    let priorities = sqlx::query_as::<_, (String, i64, i64)>(
        "SELECT lang, source_id, rank FROM source_priorities ORDER BY lang, rank",
    )
    .fetch_all(&pool)
    .await
    .expect("priorities");
    assert_eq!(
        priorities,
        vec![
            ("*".to_owned(), 2, 0),
            ("ru".to_owned(), 1, 0),
            ("ru".to_owned(), 2, 1),
        ]
    );

    let violations =
        sqlx::query_as::<_, (String, i64, Option<String>, i64)>("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .expect("foreign key check");
    assert!(violations.is_empty(), "нарушения FK: {violations:?}");

    let fk: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&pool)
        .await
        .expect("fk pragma");
    assert_eq!(fk, 1);
}
