//! Админские связи тайтла: привязка/отвязка источников, primary source
//! и ручные alias ключей глав. Прогресс и группировка резолвят alias
//! в canonical, поэтому цепочки запрещены.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{
    auth::session::now_unix,
    catalog::selection,
    error::AppError,
    source_admin::service::{self, ImportInput, ImportResponse},
    state::AppState,
};

const MAX_KEY_CHARS: usize = 256;

#[derive(Debug, Deserialize)]
pub struct AttachInput {
    pub code: String,
    pub external_id: String,
    /// По умолчанию — тип тайтла; несовпадение проверяет импорт.
    pub kind: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PrimaryInput {
    pub source_entry_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct AliasInput {
    pub alias_key: String,
    pub canonical_key: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AliasDto {
    pub alias_key: String,
    pub canonical_key: String,
}

#[derive(Debug, Serialize)]
pub struct AliasListResponse {
    pub aliases: Vec<AliasDto>,
}

/// Привязка источника к существующему тайтлу — тот же импорт, что и
/// `POST /api/sources/{code}/import`, но с явным `title_id`.
pub async fn attach(
    state: &AppState,
    title_id: i64,
    input: AttachInput,
) -> Result<ImportResponse, AppError> {
    let title_kind = sqlx::query_scalar::<_, String>("SELECT kind FROM titles WHERE id = ?")
        .bind(title_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("title_not_found", "title not found"))?;

    let kind = input
        .kind
        .as_deref()
        .map(str::trim)
        .filter(|kind| !kind.is_empty())
        .map(str::to_owned)
        .unwrap_or(title_kind);

    service::import(
        state,
        input.code.trim(),
        ImportInput {
            external_id: input.external_id,
            kind,
            title_id: Some(title_id),
        },
    )
    .await
}

pub async fn detach(state: &AppState, title_id: i64, entry_id: i64) -> Result<(), AppError> {
    let mut tx = state.pool.begin().await?;
    let exists =
        sqlx::query_scalar::<_, i64>("SELECT 1 FROM source_entries WHERE id = ? AND title_id = ?")
            .bind(entry_id)
            .bind(title_id)
            .fetch_optional(&mut *tx)
            .await?;

    if exists.is_none() {
        return Err(AppError::not_found(
            "source_entry_not_found",
            "source entry is not attached to this title",
        ));
    }

    // Главы и страницы удаляются каскадом (foreign_keys=ON).
    sqlx::query("DELETE FROM source_entries WHERE id = ? AND title_id = ?")
        .bind(entry_id)
        .bind(title_id)
        .execute(&mut *tx)
        .await?;

    // Удаляем alias, у которых исчез сам alias-ключ: резолвить уже нечего.
    // Если исчез только canonical-ключ, связь сохраняется — canonical остаётся
    // стабильным ключом прогресса, а варианты находятся по alias.
    sqlx::query(
        "DELETE FROM chapter_aliases \
         WHERE title_id = ? AND NOT EXISTS ( \
             SELECT 1 FROM chapters ch JOIN source_entries e ON e.id = ch.source_entry_id \
             WHERE e.title_id = chapter_aliases.title_id \
               AND ch.chapter_key = chapter_aliases.alias_key AND ch.removed_at IS NULL \
         )",
    )
    .bind(title_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    selection::recompute_preferred(&state.pool, title_id).await?;
    Ok(())
}

pub async fn set_primary(
    state: &AppState,
    title_id: i64,
    input: PrimaryInput,
) -> Result<(), AppError> {
    let mut tx = state.pool.begin().await?;
    let exists =
        sqlx::query_scalar::<_, i64>("SELECT 1 FROM source_entries WHERE id = ? AND title_id = ?")
            .bind(input.source_entry_id)
            .bind(title_id)
            .fetch_optional(&mut *tx)
            .await?;

    if exists.is_none() {
        return Err(AppError::not_found(
            "source_entry_not_found",
            "source entry is not attached to this title",
        ));
    }

    // Частичный уникальный индекс допускает только одну primary-запись,
    // поэтому сначала снимаем флаг со всех, затем ставим выбранной.
    sqlx::query("UPDATE source_entries SET is_primary = 0 WHERE title_id = ?")
        .bind(title_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE source_entries SET is_primary = 1 WHERE id = ? AND title_id = ?")
        .bind(input.source_entry_id)
        .bind(title_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    selection::recompute_preferred(&state.pool, title_id).await?;
    Ok(())
}

pub async fn list_aliases(pool: &SqlitePool, title_id: i64) -> Result<AliasListResponse, AppError> {
    ensure_title(pool, title_id).await?;
    Ok(AliasListResponse {
        aliases: load_aliases(pool, title_id).await?,
    })
}

/// Список alias без проверки существования тайтла: вызывающий уже его знает.
pub async fn load_aliases(pool: &SqlitePool, title_id: i64) -> Result<Vec<AliasDto>, AppError> {
    let aliases = sqlx::query_as::<_, (String, String)>(
        "SELECT alias_key, canonical_key FROM chapter_aliases \
         WHERE title_id = ? ORDER BY alias_key",
    )
    .bind(title_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(alias_key, canonical_key)| AliasDto {
        alias_key,
        canonical_key,
    })
    .collect();

    Ok(aliases)
}

pub async fn add_alias(
    state: &AppState,
    title_id: i64,
    input: AliasInput,
) -> Result<AliasDto, AppError> {
    ensure_title(&state.pool, title_id).await?;

    let alias_key = normalize_key(&input.alias_key, "alias_key")?;
    let canonical_key = normalize_key(&input.canonical_key, "canonical_key")?;

    if alias_key == canonical_key {
        return Err(AppError::bad_request(
            "alias_invalid",
            "alias_key and canonical_key must differ",
        ));
    }

    let keys = chapter_keys(&state.pool, title_id).await?;
    if !keys.contains(&alias_key) || !keys.contains(&canonical_key) {
        return Err(AppError::not_found(
            "chapter_not_found",
            "alias and canonical keys must belong to the title",
        ));
    }

    // Цепочка alias -> alias сделала бы резолв неоднозначным.
    let canonical_is_alias = sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM chapter_aliases WHERE title_id = ? AND alias_key = ?",
    )
    .bind(title_id)
    .bind(&canonical_key)
    .fetch_optional(&state.pool)
    .await?
    .is_some();
    if canonical_is_alias {
        return Err(AppError::conflict(
            "alias_chain",
            "canonical_key is already an alias",
        ));
    }

    let alias_is_canonical = sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM chapter_aliases WHERE title_id = ? AND canonical_key = ? AND alias_key <> ?",
    )
    .bind(title_id)
    .bind(&alias_key)
    .bind(&alias_key)
    .fetch_optional(&state.pool)
    .await?
    .is_some();
    if alias_is_canonical {
        return Err(AppError::conflict(
            "alias_chain",
            "alias_key is already a canonical key for another alias",
        ));
    }

    sqlx::query(
        "INSERT INTO chapter_aliases (title_id, canonical_key, alias_key, created_at) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT (title_id, alias_key) DO UPDATE SET canonical_key = excluded.canonical_key",
    )
    .bind(title_id)
    .bind(&canonical_key)
    .bind(&alias_key)
    .bind(now_unix())
    .execute(&state.pool)
    .await?;

    Ok(AliasDto {
        alias_key,
        canonical_key,
    })
}

pub async fn delete_alias(
    state: &AppState,
    title_id: i64,
    alias_key: &str,
) -> Result<(), AppError> {
    ensure_title(&state.pool, title_id).await?;
    let alias_key = normalize_key(alias_key, "alias_key")?;

    let result = sqlx::query("DELETE FROM chapter_aliases WHERE title_id = ? AND alias_key = ?")
        .bind(title_id)
        .bind(&alias_key)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::not_found(
            "alias_not_found",
            "chapter alias not found",
        ));
    }
    Ok(())
}

async fn ensure_title(pool: &SqlitePool, title_id: i64) -> Result<(), AppError> {
    let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM titles WHERE id = ?")
        .bind(title_id)
        .fetch_optional(pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::not_found("title_not_found", "title not found"));
    }
    Ok(())
}

async fn chapter_keys(pool: &SqlitePool, title_id: i64) -> Result<HashSet<String>, AppError> {
    let keys = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT ch.chapter_key FROM chapters ch \
         JOIN source_entries e ON e.id = ch.source_entry_id \
         WHERE e.title_id = ? AND ch.removed_at IS NULL",
    )
    .bind(title_id)
    .fetch_all(pool)
    .await?;
    Ok(keys.into_iter().collect())
}

fn normalize_key(raw: &str, field: &'static str) -> Result<String, AppError> {
    let key = raw.trim();
    if key.is_empty()
        || key.chars().count() > MAX_KEY_CHARS
        || key.chars().any(char::is_control)
        || key.contains('/')
        || key.contains("..")
        || key.contains("://")
    {
        return Err(AppError::bad_request(
            "alias_invalid",
            format!("{field} is not a valid chapter key"),
        ));
    }
    Ok(key.to_owned())
}
