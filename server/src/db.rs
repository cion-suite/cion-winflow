use std::time::Duration;

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("migrations failed: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("cannot create database directory: {0}")]
    Io(#[from] std::io::Error),
    #[error("foreign key check after migrations found {0} violation(s)")]
    ForeignKeyCheck(usize),
}

/// Пул SQLite с настройками из плана и миграциями при старте.
/// `create_if_missing` нужен потому, что sqlx сам файл БД не создаёт.
pub async fn connect(database_url: &str) -> Result<SqlitePool, DbError> {
    let options: SqliteConnectOptions = database_url.parse()?;

    let filename = options.get_filename().to_path_buf();
    if let Some(parent) = filename.parent()
        && !parent.as_os_str().is_empty()
    {
        tokio::fs::create_dir_all(parent).await?;
    }

    let options = options
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true);

    // Один процесс и WAL: 4-8 соединений достаточно, лишние только ждут блокировок
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;

    run_migrations(&pool).await?;
    Ok(pool)
}

/// Применяет миграции с выключенными внешними ключами.
///
/// 0008 пересобирает `titles` и `source_entries`. Дочерние таблицы ссылаются
/// на них с ON DELETE CASCADE, а SQLx для SQLite всегда оборачивает миграцию
/// в транзакцию, где `PRAGMA foreign_keys` — no-op. Выключаем ключи на
/// соединении мигратора до запуска и включаем после, иначе `DROP TABLE`
/// родителя каскадно удалит пользовательские данные. После включения
/// проверяем целостность: миграция не должна оставить висячие ссылки.
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), DbError> {
    let migrator = sqlx::migrate!("./migrations");
    let mut connection = pool.acquire().await?;
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *connection)
        .await?;
    let migrated = migrator.run(&mut *connection).await;
    let fk_restored = sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *connection)
        .await;

    migrated?;
    fk_restored?;

    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&mut *connection)
        .await?;
    let violations = violations.len();
    drop(connection);

    if violations > 0 {
        return Err(DbError::ForeignKeyCheck(violations));
    }
    Ok(())
}
