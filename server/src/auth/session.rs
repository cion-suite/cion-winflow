use axum_extra::extract::cookie::{Cookie, SameSite};
use cookie::time::Duration as CookieDuration;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::error::AppError;

pub const COOKIE_NAME: &str = "sid";
pub const IDLE_SECS: i64 = 30 * 24 * 60 * 60;
pub const ABSOLUTE_SECS: i64 = 90 * 24 * 60 * 60;
pub const LAST_SEEN_UPDATE_SECS: i64 = 5 * 60;

/// Пользователь, прошедший middleware сессии. Он же DTO ответа /api/me.
#[derive(Clone, Debug, Serialize)]
pub struct CurrentUser {
    pub id: i64,
    pub username: String,
    pub role: Role,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    User,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::User => "user",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "admin" => Self::Admin,
            _ => Self::User,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct SessionRow {
    pub session_id: i64,
    pub last_seen_at: i64,
    pub expires_at: i64,
    pub user_id: i64,
    pub username: String,
    pub role: String,
    pub disabled_at: Option<i64>,
}

pub fn now_unix() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

/// 32 случайных байта из ОС: токен сессии и код инвайта.
pub fn new_secret() -> Result<String, AppError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| AppError::Random)?;
    Ok(hex::encode(bytes))
}

pub fn hash_secret(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}

pub fn new_session_token() -> Result<(String, String), AppError> {
    let token = new_secret()?;
    let token_hash = hash_secret(&token);
    Ok((token, token_hash))
}

pub async fn find_session(
    pool: &SqlitePool,
    token_hash: &str,
) -> Result<Option<SessionRow>, AppError> {
    let row = sqlx::query_as::<_, SessionRow>(
        "SELECT s.id AS session_id, s.last_seen_at, s.expires_at, \
                u.id AS user_id, u.username, u.role, u.disabled_at \
         FROM sessions s JOIN users u ON u.id = s.user_id \
         WHERE s.token_hash = ?",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

pub async fn insert_session(
    pool: &SqlitePool,
    user_id: i64,
    token_hash: &str,
    now: i64,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at, last_seen_at) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(token_hash)
    .bind(user_id)
    .bind(now)
    .bind(now + ABSOLUTE_SECS)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn delete_session(pool: &SqlitePool, session_id: i64) -> Result<(), AppError> {
    sqlx::query("DELETE FROM sessions WHERE id = ?")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_session_by_token(pool: &SqlitePool, token_hash: &str) -> Result<(), AppError> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
        .bind(token_hash)
        .execute(pool)
        .await?;
    Ok(())
}

/// last_seen_at обновляется не чаще раза в 5 минут, иначе каждый запрос
/// пишет в единственную SQLite-базу.
pub async fn touch_session(pool: &SqlitePool, session_id: i64, now: i64) {
    let result =
        sqlx::query("UPDATE sessions SET last_seen_at = ? WHERE id = ? AND last_seen_at < ?")
            .bind(now)
            .bind(session_id)
            .bind(now - LAST_SEEN_UPDATE_SECS)
            .execute(pool)
            .await;

    if let Err(error) = result {
        tracing::warn!(%error, "cannot update session last_seen_at");
    }
}

pub fn session_cookie(token: String, secure: bool) -> Cookie<'static> {
    Cookie::build((COOKIE_NAME, token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        // Браузерная cookie живёт по idle-сроку; абсолютный срок проверяет сервер
        .max_age(CookieDuration::days(30))
        .build()
}

pub fn clear_cookie() -> Cookie<'static> {
    Cookie::build((COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(CookieDuration::seconds(0))
        .build()
}
