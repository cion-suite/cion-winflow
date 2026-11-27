//! Чтение и запись per-user учёток источников.
//!
//! Сервис не отдаёт плейнтекст наружу: наружу идут только `configured` и
//! метаданные проверки. Расшифрованные значения получает [`for_user`] —
//! адаптеры источников на следующей фазе.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::{
    auth::session::now_unix,
    error::AppError,
    source_auth::{SourceAuth, crypto::CredentialCipher},
    sources::descriptor::Capability,
    state::AppState,
};

/// Заголовки не должны превращаться в способ залить мегабайты в БД.
const MAX_BEARER_LEN: usize = 4 * 1024;
const MAX_COOKIE_LEN: usize = 8 * 1024;

#[derive(Debug, Deserialize)]
pub struct CredentialInput {
    pub bearer: Option<String>,
    pub cookie: Option<String>,
}

/// Ответ API: секретов здесь нет и появиться не должно.
#[derive(Debug, Serialize)]
pub struct CredentialDto {
    pub code: &'static str,
    pub name: &'static str,
    pub configured: bool,
    pub updated_at: Option<i64>,
    pub last_verified_at: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CredentialsListResponse {
    pub credentials: Vec<CredentialDto>,
}

#[derive(Debug, sqlx::FromRow)]
struct StoredRow {
    source_id: i64,
    updated_at: i64,
    last_verified_at: Option<i64>,
    last_error: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct SealedRow {
    key_version: i64,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

/// Список всех источников с поддержкой per-user входа. Источники без учётки
/// тоже попадают в ответ с `configured=false`: UI должен показать форму до
/// того, как пользователь что-то сохранил.
pub async fn list(state: &AppState, user_id: i64) -> Result<CredentialsListResponse, AppError> {
    // Без ключа функция недоступна целиком: без него нельзя ни сохранить,
    // ни расшифровать — показывать «не подключено» было бы враньём.
    state.credential_cipher()?;

    let rows = sqlx::query_as::<_, StoredRow>(
        "SELECT source_id, updated_at, last_verified_at, last_error \
         FROM user_source_credentials WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;
    let by_source: HashMap<i64, StoredRow> =
        rows.into_iter().map(|row| (row.source_id, row)).collect();

    let registry = state.sources.read().await;
    // Реестр уже знает пары code → id: отдельный SELECT по `sources`
    // дублировал бы загруженное состояние и мог разойтись с ним.
    let ids: HashMap<String, i64> = registry
        .entries()
        .map(|runtime| (runtime.code.clone(), runtime.id))
        .collect();
    let credentials = registry
        .descriptors()
        .iter()
        .filter(|descriptor| descriptor.has(Capability::Auth))
        .map(|descriptor| {
            let stored = ids
                .get(descriptor.code)
                .and_then(|source_id| by_source.get(source_id));
            CredentialDto {
                code: descriptor.code,
                name: descriptor.name,
                configured: stored.is_some(),
                updated_at: stored.map(|row| row.updated_at),
                last_verified_at: stored.and_then(|row| row.last_verified_at),
                last_error: stored.and_then(|row| row.last_error.clone()),
            }
        })
        .collect();

    Ok(CredentialsListResponse { credentials })
}

/// Заменяет payload учётки. `null`/пустая строка очищает поле; сохранение
/// без единого заполненного поля отклоняется — для очистки есть DELETE.
pub async fn upsert(
    state: &AppState,
    user_id: i64,
    code: &str,
    input: CredentialInput,
) -> Result<CredentialDto, AppError> {
    let (source_id, descriptor_code, descriptor_name) = {
        let registry = state.sources.read().await;
        let Some(runtime) = registry.runtime(code) else {
            return Err(AppError::not_found("source_not_found", "unknown source"));
        };
        if !runtime.descriptor.has(Capability::Auth) {
            return Err(AppError::bad_request(
                "source_credentials_invalid",
                "this source does not support per-user credentials",
            ));
        }
        (runtime.id, runtime.descriptor.code, runtime.descriptor.name)
    };

    let auth = SourceAuth::new(input.bearer, input.cookie);
    validate(&auth)?;

    let cipher = state.credential_cipher()?;
    let sealed = cipher.seal(user_id, source_id, &auth)?;
    let now = now_unix();

    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO user_source_credentials \
             (user_id, source_id, ciphertext, nonce, key_version, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT (user_id, source_id) DO UPDATE SET \
             ciphertext = excluded.ciphertext, \
             nonce = excluded.nonce, \
             key_version = excluded.key_version, \
             updated_at = excluded.updated_at, \
             last_verified_at = NULL, \
             last_error = NULL",
    )
    .bind(user_id)
    .bind(source_id)
    .bind(&sealed.ciphertext)
    .bind(&sealed.nonce)
    .bind(sealed.key_version)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    reset_title_sync_timestamps(&mut tx, source_id).await?;
    tx.commit().await?;

    tracing::info!(user_id, source_id, "source credential saved");

    Ok(CredentialDto {
        code: descriptor_code,
        name: descriptor_name,
        configured: true,
        updated_at: Some(now),
        last_verified_at: None,
        last_error: None,
    })
}

/// Удаление идемпотентно: повторный DELETE не ошибка. Функция, как и
/// остальные маршруты учёток, доступна только при настроенном ключе.
pub async fn delete(state: &AppState, user_id: i64, code: &str) -> Result<(), AppError> {
    state.credential_cipher()?;
    let source_id = {
        let registry = state.sources.read().await;
        let Some(runtime) = registry.runtime(code) else {
            return Err(AppError::not_found("source_not_found", "unknown source"));
        };
        runtime.id
    };

    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM user_source_credentials WHERE user_id = ? AND source_id = ?")
        .bind(user_id)
        .bind(source_id)
        .execute(&mut *tx)
        .await?;
    reset_title_sync_timestamps(&mut tx, source_id).await?;
    tx.commit().await?;

    tracing::info!(user_id, source_id, "source credential deleted");
    Ok(())
}

/// Расшифрованная учётка для запросов от имени пользователя. `None` — учётка
/// не подключена; ошибка расшифровки — `credentials_unavailable` (503), а не
/// «нет учётки»: сервер обязан отличать эти состояния.
pub async fn for_user(
    pool: &SqlitePool,
    cipher: &CredentialCipher,
    user_id: i64,
    source_id: i64,
) -> Result<Option<SourceAuth>, AppError> {
    let row = sqlx::query_as::<_, SealedRow>(
        "SELECT key_version, nonce, ciphertext FROM user_source_credentials \
         WHERE user_id = ? AND source_id = ?",
    )
    .bind(user_id)
    .bind(source_id)
    .fetch_optional(pool)
    .await?;

    match row {
        None => Ok(None),
        Some(row) => cipher
            .open(
                user_id,
                source_id,
                row.key_version,
                &row.nonce,
                &row.ciphertext,
            )
            .map(Some),
    }
}

fn validate(auth: &SourceAuth) -> Result<(), AppError> {
    if auth.is_empty() {
        return Err(AppError::bad_request(
            "source_credentials_invalid",
            "at least one of bearer or cookie is required",
        ));
    }
    if auth
        .bearer()
        .is_some_and(|value| value.len() > MAX_BEARER_LEN)
    {
        return Err(AppError::bad_request(
            "source_credentials_invalid",
            "bearer value is too long",
        ));
    }
    if auth
        .cookie()
        .is_some_and(|value| value.len() > MAX_COOKIE_LEN)
    {
        return Err(AppError::bad_request(
            "source_credentials_invalid",
            "cookie value is too long",
        ));
    }
    Ok(())
}

/// После смены учётки sync страницы тайтла должен сработать сразу, а не
/// ждать 10-минутный TTL: иначе подключивший аккаунт пользователь не увидит
/// изменений. Учётки per-user, поэтому сбрасываем TTL у тайтлов, где
/// источник привязан.
async fn reset_title_sync_timestamps(
    tx: &mut Transaction<'_, Sqlite>,
    source_id: i64,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE titles SET metadata_synced_at = NULL \
         WHERE id IN (SELECT title_id FROM source_entries WHERE source_id = ?)",
    )
    .bind(source_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
