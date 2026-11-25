use std::sync::Arc;

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use tokio::sync::Semaphore;

use crate::error::AppError;

pub const MIN_PASSWORD_CHARS: usize = 10;
pub const MAX_PASSWORD_CHARS: usize = 128;

/// Хеш пароля-пустышки: проверка для несуществующего пользователя стоит
/// столько же, сколько для существующего, и не выдаёт наличие логина.
pub const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$C1JkvvnDHxs2nIrF/cGHsw$BIZ1F5MtoSonoYSDTJek7zUzT3kuL3heM4oXSyDIeRA";

pub fn validate(password: &str) -> Result<(), AppError> {
    let length = password.chars().count();
    if length < MIN_PASSWORD_CHARS {
        return Err(AppError::bad_request(
            "validation",
            format!("password must be at least {MIN_PASSWORD_CHARS} characters"),
        ));
    }
    if length > MAX_PASSWORD_CHARS {
        return Err(AppError::bad_request(
            "validation",
            format!("password must be at most {MAX_PASSWORD_CHARS} characters"),
        ));
    }
    Ok(())
}

/// Argon2 CPU-bound: уводим в blocking-пул и ограничиваем число параллельных
/// хешей семафором, чтобы вход не выедал tokio-воркеры.
pub async fn hash(password: String, semaphore: Arc<Semaphore>) -> Result<String, AppError> {
    let permit = semaphore
        .acquire_owned()
        .await
        .map_err(|_| AppError::Task)?;

    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|hash| hash.to_string())
            .map_err(|_| AppError::PasswordHash)
    })
    .await
    .map_err(|_| AppError::Task)?
}

pub async fn verify(
    password: String,
    stored_hash: String,
    semaphore: Arc<Semaphore>,
) -> Result<bool, AppError> {
    let permit = semaphore
        .acquire_owned()
        .await
        .map_err(|_| AppError::Task)?;

    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let parsed = match PasswordHash::new(&stored_hash) {
            Ok(parsed) => parsed,
            Err(_) => {
                // Битый хеш в БД — ошибка данных, а не верный пароль
                tracing::error!("stored password hash is invalid");
                return false;
            }
        };

        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
    .await
    .map_err(|_| AppError::Task)
}
