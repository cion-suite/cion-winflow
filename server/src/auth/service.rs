use std::sync::Arc;

use serde::Serialize;
use sqlx::SqlitePool;
use tokio::sync::Semaphore;

use crate::{
    auth::{
        password,
        session::{self, CurrentUser, Role},
    },
    config::{AdminBootstrap, RegistrationMode},
    error::AppError,
    state::AppState,
};

pub const INVITE_DEFAULT_DAYS: i64 = 7;
pub const INVITE_MAX_DAYS: i64 = 90;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct InviteSummary {
    pub id: i64,
    pub created_by: Option<i64>,
    pub used_by: Option<i64>,
    pub expires_at: Option<i64>,
    pub created_at: i64,
}

pub fn validate_username(username: &str) -> Result<(), AppError> {
    let length = username.chars().count();
    if !(3..=32).contains(&length) {
        return Err(AppError::bad_request(
            "validation",
            "username must be 3 to 32 characters",
        ));
    }
    if !username
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-')
    {
        return Err(AppError::bad_request(
            "validation",
            "username may contain only letters, digits, `_` and `-`",
        ));
    }
    Ok(())
}

/// Регистрация по инвайту: пароль хешируется до транзакции, инвайт
/// захватывается атомарным UPDATE, чтобы два запроса не создали двух users.
pub async fn register(
    state: &AppState,
    username: String,
    password: String,
    invite: String,
) -> Result<(CurrentUser, String), AppError> {
    if state.config.registration_mode == RegistrationMode::Closed {
        return Err(AppError::forbidden(
            "registration_closed",
            "registration is closed",
        ));
    }

    let username = username.trim().to_owned();
    validate_username(&username)?;
    password::validate(&password)?;

    let invite_code = invite.trim().to_owned();
    if invite_code.is_empty() {
        return Err(invalid_invite());
    }
    let invite_hash = session::hash_secret(&invite_code);
    let now = session::now_unix();

    // Быстрая проверка до дорогого Argon2; ниже инвайт перепроверяется в транзакции
    let invite = find_invite(&state.pool, &invite_hash).await?;
    ensure_invite_usable(&invite, now)?;

    let password_hash = password::hash(password, state.password_semaphore.clone()).await?;

    let mut transaction = state.pool.begin().await?;

    let invite = find_invite_tx(&mut transaction, &invite_hash).await?;
    let (invite_id, _) = ensure_invite_usable(&invite, now)?;

    let inserted = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users (username, password_hash, role, created_at) \
         VALUES (?, ?, 'user', ?) RETURNING id",
    )
    .bind(&username)
    .bind(&password_hash)
    .bind(now)
    .fetch_one(&mut *transaction)
    .await;

    let user_id = match inserted {
        Ok(id) => id,
        Err(error) if is_unique_violation(&error) => {
            return Err(AppError::conflict(
                "username_taken",
                "username is already taken",
            ));
        }
        Err(error) => return Err(error.into()),
    };

    let captured = sqlx::query(
        "UPDATE invites SET used_by = ? \
         WHERE id = ? AND used_by IS NULL AND (expires_at IS NULL OR expires_at > ?)",
    )
    .bind(user_id)
    .bind(invite_id)
    .bind(now)
    .execute(&mut *transaction)
    .await?;

    if captured.rows_affected() != 1 {
        return Err(invalid_invite());
    }

    let (token, token_hash) = session::new_session_token()?;
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at, last_seen_at) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&token_hash)
    .bind(user_id)
    .bind(now)
    .bind(now + session::ABSOLUTE_SECS)
    .bind(now)
    .execute(&mut *transaction)
    .await?;

    transaction.commit().await?;

    Ok((
        CurrentUser {
            id: user_id,
            username,
            role: Role::User,
        },
        token,
    ))
}

pub async fn login(
    state: &AppState,
    username: String,
    password: String,
) -> Result<(CurrentUser, String), AppError> {
    let username = username.trim().to_owned();
    if username.is_empty() || password.is_empty() {
        return Err(AppError::invalid_credentials());
    }

    if let Err(retry_after) = state.login_limiter.check_and_record(&username) {
        return Err(AppError::RateLimited {
            message: "too many login attempts; try again later".to_owned(),
            retry_after_secs: retry_after,
        });
    }

    let row = sqlx::query_as::<_, (i64, String, String, String, Option<i64>)>(
        "SELECT id, username, password_hash, role, disabled_at FROM users WHERE username = ?",
    )
    .bind(&username)
    .fetch_optional(&state.pool)
    .await?;

    let Some((id, stored_username, stored_hash, role, disabled_at)) = row else {
        // Тратим столько же времени, сколько на существующего пользователя
        let _ = password::verify(
            password,
            password::DUMMY_HASH.to_owned(),
            state.password_semaphore.clone(),
        )
        .await?;
        return Err(AppError::invalid_credentials());
    };

    let matches = password::verify(password, stored_hash, state.password_semaphore.clone()).await?;

    if !matches || disabled_at.is_some() {
        return Err(AppError::invalid_credentials());
    }

    state.login_limiter.clear(&stored_username);

    let (token, token_hash) = session::new_session_token()?;
    session::insert_session(&state.pool, id, &token_hash, session::now_unix()).await?;

    Ok((
        CurrentUser {
            id,
            username: stored_username,
            role: Role::from_db(&role),
        },
        token,
    ))
}

pub async fn logout(state: &AppState, token: Option<String>) -> Result<(), AppError> {
    if let Some(token) = token.filter(|token| !token.is_empty()) {
        session::delete_session_by_token(&state.pool, &session::hash_secret(&token)).await?;
    }
    Ok(())
}

pub async fn list_invites(state: &AppState) -> Result<Vec<InviteSummary>, AppError> {
    let invites = sqlx::query_as::<_, InviteSummary>(
        "SELECT id, created_by, used_by, expires_at, created_at \
         FROM invites ORDER BY created_at DESC, id DESC",
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(invites)
}

/// Возвращает код инвайта и срок; код показывается администратору один раз.
pub async fn create_invite(
    state: &AppState,
    created_by: i64,
    expires_in_days: Option<i64>,
) -> Result<(String, i64), AppError> {
    let days = expires_in_days.unwrap_or(INVITE_DEFAULT_DAYS);
    if !(1..=INVITE_MAX_DAYS).contains(&days) {
        return Err(AppError::bad_request(
            "validation",
            format!("expires_in_days must be between 1 and {INVITE_MAX_DAYS}"),
        ));
    }

    let code = session::new_secret()?;
    let code_hash = session::hash_secret(&code);
    let now = session::now_unix();
    let expires_at = now + days * 24 * 60 * 60;

    sqlx::query(
        "INSERT INTO invites (code_hash, created_by, expires_at, created_at) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(&code_hash)
    .bind(created_by)
    .bind(expires_at)
    .bind(now)
    .execute(&state.pool)
    .await?;

    Ok((code, expires_at))
}

pub async fn bootstrap_admin(pool: &SqlitePool, admin: &AdminBootstrap) -> Result<bool, AppError> {
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    if users > 0 {
        return Ok(false);
    }

    let username = admin.username.trim().to_owned();
    validate_username(&username)?;
    password::validate(&admin.password)?;

    let password_hash = password::hash(admin.password.clone(), Arc::new(Semaphore::new(1))).await?;

    let inserted = sqlx::query(
        "INSERT INTO users (username, password_hash, role, created_at) \
         VALUES (?, ?, 'admin', ?)",
    )
    .bind(&username)
    .bind(&password_hash)
    .bind(session::now_unix())
    .execute(pool)
    .await;

    match inserted {
        Ok(_) => Ok(true),
        Err(error) if is_unique_violation(&error) => Err(AppError::conflict(
            "username_taken",
            "bootstrap admin username is already taken",
        )),
        Err(error) => Err(error.into()),
    }
}

type InviteRow = (i64, Option<i64>, Option<i64>);

async fn find_invite(pool: &SqlitePool, code_hash: &str) -> Result<Option<InviteRow>, AppError> {
    let invite = sqlx::query_as::<_, InviteRow>(
        "SELECT id, expires_at, used_by FROM invites WHERE code_hash = ?",
    )
    .bind(code_hash)
    .fetch_optional(pool)
    .await?;
    Ok(invite)
}

async fn find_invite_tx(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    code_hash: &str,
) -> Result<Option<InviteRow>, AppError> {
    let invite = sqlx::query_as::<_, InviteRow>(
        "SELECT id, expires_at, used_by FROM invites WHERE code_hash = ?",
    )
    .bind(code_hash)
    .fetch_optional(&mut **transaction)
    .await?;
    Ok(invite)
}

fn ensure_invite_usable(
    invite: &Option<InviteRow>,
    now: i64,
) -> Result<(i64, Option<i64>), AppError> {
    let Some((id, expires_at, used_by)) = invite else {
        return Err(invalid_invite());
    };
    if used_by.is_some() || expires_at.is_some_and(|expires_at| expires_at <= now) {
        return Err(invalid_invite());
    }
    Ok((*id, *expires_at))
}

fn invalid_invite() -> AppError {
    AppError::bad_request(
        "invalid_invite",
        "invite is invalid, expired or already used",
    )
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|error| error.is_unique_violation())
}
