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

    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}
