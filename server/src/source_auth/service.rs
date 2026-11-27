//! Чтение и запись per-user учёток источников.
//!
//! Сервис не отдаёт плейнтекст наружу: наружу идут только `configured` и
//! метаданные проверки. Расшифрованные значения получает [`resolve`] и
//! передаёт адаптерам; сами адаптеры о БД не знают.

use std::collections::{HashMap, HashSet};

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::{
    auth::session::now_unix,
    error::AppError,
    source_auth::{SourceAuth, crypto::CredentialCipher},
    sources::{SourceError, descriptor::Capability},
    state::AppState,
};

/// Сколько считается «недавно проверенной» учётка. Пока проверка свежая,
/// помеченную главу можно отдать из кэша без повторного запроса.
pub const VERIFIED_TTL_SECS: i64 = 24 * 60 * 60;
/// Код последней auth-ошибки в `user_source_credentials.last_error`.
pub const AUTH_ERROR_CODE: &str = "auth_invalid";

/// Заголовки не должны превращаться в способ залить мегабайты в БД.
const MAX_BEARER_LEN: usize = 4 * 1024;
const MAX_COOKIE_LEN: usize = 8 * 1024;
/// Логин источника — почта или ник: короткая строка, а не тело запроса.
const MAX_LOGIN_CHARS: usize = 200;
/// Пароль не хранится, но счётчик попыток не должен пропускать мегабайты.
const MAX_PASSWORD_LEN: usize = 1024;

#[derive(Debug, Deserialize)]
pub struct CredentialInput {
    pub bearer: Option<String>,
    pub cookie: Option<String>,
}

/// Вход по логину/паролю. Пароль обёрнут в `SecretString`: у структуры нет
/// `Debug`, а `secrecy` не даёт случайно сериализовать секрет.
#[derive(Deserialize)]
pub struct LoginInput {
    pub login: String,
    pub password: SecretString,
}

/// Ответ API: секретов здесь нет и появиться не должно.
#[derive(Debug, Serialize)]
pub struct CredentialDto {
    pub code: &'static str,
    pub name: &'static str,
    pub configured: bool,
    /// Источник умеет серверный вход по логину/паролю; UI показывает форму.
    pub login_supported: bool,
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
                login_supported: descriptor.has(Capability::Login),
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
    let (source_id, descriptor_code, descriptor_name, login_supported) = {
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
        (
            runtime.id,
            runtime.descriptor.code,
            runtime.descriptor.name,
            runtime.descriptor.has(Capability::Login),
        )
    };

    let auth = SourceAuth::new(input.bearer, input.cookie);
    validate(&auth)?;

    let cipher = state.credential_cipher()?;
    let mut tx = state.pool.begin().await?;
    let updated_at = persist_credential(&mut tx, cipher, user_id, source_id, &auth, None).await?;
    tx.commit().await?;

    tracing::info!(user_id, source_id, "source credential saved");

    Ok(CredentialDto {
        code: descriptor_code,
        name: descriptor_name,
        configured: true,
        login_supported,
        updated_at: Some(updated_at),
        last_verified_at: None,
        last_error: None,
    })
}

/// Вход по логину/паролю: получает токен у источника и сохраняет его как
/// обычную Bearer-учётку. Пароль не пишется ни в БД, ни в ответ, ни в лог.
pub async fn login_and_save(
    state: &AppState,
    user_id: i64,
    code: &str,
    input: LoginInput,
) -> Result<CredentialDto, AppError> {
    let cipher = state.credential_cipher()?;

    // Адаптер клонируется: guard реестра нельзя держать через `.await`
    // сетевого запроса, а `SourceKind` уже `Clone`.
    let (source_id, descriptor_code, descriptor_name, adapter) = {
        let registry = state.sources.read().await;
        let Some(runtime) = registry.runtime(code) else {
            return Err(AppError::not_found("source_not_found", "unknown source"));
        };
        if !runtime.descriptor.has(Capability::Auth) || !runtime.descriptor.has(Capability::Login) {
            return Err(AppError::bad_request(
                "source_credentials_invalid",
                "this source does not support login by password",
            ));
        }
        let adapter = runtime.adapter()?.clone();
        (
            runtime.id,
            runtime.descriptor.code,
            runtime.descriptor.name,
            adapter,
        )
    };

    let login = input.login.trim().to_owned();
    validate_login(&login, &input.password)?;

    let outcome = adapter.login(&login, &input.password).await?;
    let token = outcome.access_token.ok_or_else(|| {
        AppError::bad_gateway("source_parse_failed", "source did not return a token")
    })?;

    // Токен v1 непрозрачный: `SourceAuth` сам добавит схему `Bearer`.
    let auth = SourceAuth::new(Some(token), None);
    let verified_at = now_unix();

    let mut tx = state.pool.begin().await?;
    let updated_at = persist_credential(
        &mut tx,
        cipher,
        user_id,
        source_id,
        &auth,
        Some(verified_at),
    )
    .await?;
    tx.commit().await?;

    tracing::info!(user_id, source_id, "source login succeeded");

    Ok(CredentialDto {
        code: descriptor_code,
        name: descriptor_name,
        configured: true,
        login_supported: true,
        updated_at: Some(updated_at),
        last_verified_at: Some(verified_at),
        last_error: None,
    })
}

/// Проверяет сохранённую учётку лёгким запросом источника и обновляет
/// `last_verified_at`/`last_error`. Пароль не нужен: проверяется токен,
/// который уже лежит в БД.
pub async fn verify(state: &AppState, user_id: i64, code: &str) -> Result<CredentialDto, AppError> {
    let cipher = state.credential_cipher()?;
    let (source_id, descriptor_code, descriptor_name, login_supported, adapter) = {
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
        let adapter = runtime.adapter()?.clone();
        (
            runtime.id,
            runtime.descriptor.code,
            runtime.descriptor.name,
            runtime.descriptor.has(Capability::Login),
            adapter,
        )
    };

    let Some(auth) = for_user(&state.pool, cipher, user_id, source_id).await? else {
        return Err(auth_not_configured());
    };
    // `updated_at` проверка не меняет; читаем до сети, чтобы параллельное
    // удаление учётки не превратило успешную проверку в 500.
    let updated_at = sqlx::query_scalar::<_, i64>(
        "SELECT updated_at FROM user_source_credentials WHERE user_id = ? AND source_id = ?",
    )
    .bind(user_id)
    .bind(source_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(auth_not_configured)?;

    match adapter.verify(Some(&auth)).await {
        Ok(()) => {
            mark_verified(&state.pool, user_id, source_id).await?;
            Ok(CredentialDto {
                code: descriptor_code,
                name: descriptor_name,
                configured: true,
                login_supported,
                updated_at: Some(updated_at),
                last_verified_at: Some(now_unix()),
                last_error: None,
            })
        }
        Err(error) => {
            // Отклонённый токен запоминаем в учётке; временные ошибки сети
            // не должны выглядеть как «учётка неверна».
            if matches!(&error, SourceError::AuthRequired) {
                mark_auth_error(&state.pool, user_id, source_id).await?;
            }
            Err(error.into_app(true))
        }
    }
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
    clear_discovery_checks(&mut tx, user_id, source_id).await?;
    tx.commit().await?;

    tracing::info!(user_id, source_id, "source credential deleted");
    Ok(())
}

/// Единая ошибка «нужна личная учётка»: один и тот же текст и код не должны
/// расходиться между выдачей контента, картинками и импортом.
pub fn auth_not_configured() -> AppError {
    AppError::forbidden(
        "source_auth_not_configured",
        "this chapter requires a source account; connect it in settings",
    )
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

/// Учётка для запросов сервиса: без ключа в env функция просто выключена и
/// источник получает анонимный запрос. Ошибку расшифровки (ключ сменился или
/// строка повреждена) не глотаем — она станет 503 у конкретного запроса.
pub async fn resolve(
    state: &AppState,
    user_id: i64,
    source_id: i64,
) -> Result<Option<SourceAuth>, AppError> {
    let Some(cipher) = state.source_credentials.as_deref() else {
        return Ok(None);
    };
    for_user(&state.pool, cipher, user_id, source_id).await
}

/// Источники, для которых у пользователя сохранена учётка. Нужна карточке
/// «Источники»: UI показывает «подключить аккаунт» без сетевого запроса.
pub async fn configured_sources(pool: &SqlitePool, user_id: i64) -> Result<HashSet<i64>, AppError> {
    let ids = sqlx::query_scalar::<_, i64>(
        "SELECT source_id FROM user_source_credentials WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(ids.into_iter().collect())
}

/// Проверка отмечается успехом любого авторизованного запроса: отдельного
/// endpoint у Remanga нет, а «сохранено, не проверено» не должно длиться вечно.
pub async fn mark_verified(
    pool: &SqlitePool,
    user_id: i64,
    source_id: i64,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE user_source_credentials SET last_verified_at = ?, last_error = NULL \
         WHERE user_id = ? AND source_id = ?",
    )
    .bind(now_unix())
    .bind(user_id)
    .bind(source_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Персональная auth-ошибка пишется только в учётку: глобальный
/// `source_entries.last_error` не должен зависеть от одного пользователя.
pub async fn mark_auth_error(
    pool: &SqlitePool,
    user_id: i64,
    source_id: i64,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE user_source_credentials SET last_error = ? WHERE user_id = ? AND source_id = ?",
    )
    .bind(AUTH_ERROR_CODE)
    .bind(user_id)
    .bind(source_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Свежесть проверки: без неё помеченную главой главу нельзя отдавать из
/// кэша — доступ мог быть отозван на источнике.
pub async fn verified_recently(
    pool: &SqlitePool,
    user_id: i64,
    source_id: i64,
    now: i64,
) -> Result<bool, AppError> {
    let verified_at: Option<Option<i64>> = sqlx::query_scalar(
        "SELECT last_verified_at FROM user_source_credentials \
         WHERE user_id = ? AND source_id = ?",
    )
    .bind(user_id)
    .bind(source_id)
    .fetch_optional(pool)
    .await?;

    Ok(verified_at
        .flatten()
        .is_some_and(|at| now.saturating_sub(at) < VERIFIED_TTL_SECS))
}

/// Пароль не хранится, поэтому здесь только защита от пустых значений и
/// гигантских тел: настоящую проверку делает источник.
fn validate_login(login: &str, password: &SecretString) -> Result<(), AppError> {
    if login.is_empty() || login.chars().count() > MAX_LOGIN_CHARS {
        return Err(AppError::bad_request(
            "source_credentials_invalid",
            format!("login must be 1..={MAX_LOGIN_CHARS} characters"),
        ));
    }
    let password = password.expose_secret();
    if password.is_empty() || password.len() > MAX_PASSWORD_LEN {
        return Err(AppError::bad_request(
            "source_credentials_invalid",
            format!("password must be 1..={MAX_PASSWORD_LEN} bytes"),
        ));
    }
    Ok(())
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

/// Общая запись учётки для ручного сохранения и входа по паролю.
/// Возвращает `updated_at`, чтобы DTO не считал время вторым вызовом.
async fn persist_credential(
    tx: &mut Transaction<'_, Sqlite>,
    cipher: &CredentialCipher,
    user_id: i64,
    source_id: i64,
    auth: &SourceAuth,
    verified_at: Option<i64>,
) -> Result<i64, AppError> {
    let sealed = cipher.seal(user_id, source_id, auth)?;
    let now = now_unix();

    sqlx::query(
        "INSERT INTO user_source_credentials \
             (user_id, source_id, ciphertext, nonce, key_version, updated_at, last_verified_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT (user_id, source_id) DO UPDATE SET \
             ciphertext = excluded.ciphertext, \
             nonce = excluded.nonce, \
             key_version = excluded.key_version, \
             updated_at = excluded.updated_at, \
             last_verified_at = excluded.last_verified_at, \
             last_error = NULL",
    )
    .bind(user_id)
    .bind(source_id)
    .bind(&sealed.ciphertext)
    .bind(&sealed.nonce)
    .bind(sealed.key_version)
    .bind(now)
    .bind(verified_at)
    .execute(&mut **tx)
    .await?;
    reset_title_sync_timestamps(tx, source_id).await?;
    // Результат поиска без аккаунта больше не актуален: после подключения
    // discovery должен проверить источник заново, не дожидаясь TTL.
    clear_discovery_checks(tx, user_id, source_id).await?;
    Ok(now)
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

/// Смена учётки делает прошлый результат discovery недействительным: без
/// аккаунта источник мог ответить `auth_required` или скрыть тайтл.
async fn clear_discovery_checks(
    tx: &mut Transaction<'_, Sqlite>,
    user_id: i64,
    source_id: i64,
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM title_source_checks WHERE user_id = ? AND source_id = ?")
        .bind(user_id)
        .bind(source_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
