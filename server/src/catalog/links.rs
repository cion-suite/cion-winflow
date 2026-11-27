//! Админские связи тайтла: привязка/отвязка источников, primary source
//! и ручные alias ключей глав. Прогресс и группировка резолвят alias
//! в canonical, поэтому цепочки запрещены.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::{
    auth::session::now_unix,
    catalog::selection,
    error::AppError,
    source_admin::service::{self, ImportInput, ImportResponse},
    state::AppState,
};

const MAX_KEY_CHARS: usize = 256;
/// Одна пачка — auto-match по номеру: страховка от тела на тысячи пар.
const MAX_ALIASES_PER_REQUEST: usize = 500;

#[derive(Debug, Deserialize)]
pub struct AttachInput {
    pub code: String,
    pub external_id: String,
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

#[derive(Debug, Deserialize)]
pub struct AliasBatchInput {
    pub aliases: Vec<AliasInput>,
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
/// ленивое добавление, но с явным `title_id`.
pub async fn attach(
    state: &AppState,
    title_id: i64,
    input: AttachInput,
) -> Result<ImportResponse, AppError> {
    service::import(
        state,
        input.code.trim(),
        ImportInput {
            external_id: input.external_id,
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
    // Связки диапазонов привязаны к source_entry: после отвязки
    // производные пары нужно пересобрать из оставшихся источников.
    crate::catalog::alias_ranges::materialize_title(&state.pool, title_id).await?;
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
    // Производные пары диапазонов наружу не отдаются: UI показывает
    // сами связки, а не десятки материализованных пар.
    let aliases = sqlx::query_as::<_, (String, String)>(
        "SELECT alias_key, canonical_key FROM chapter_aliases \
         WHERE title_id = ? AND source_entry_id IS NULL ORDER BY alias_key",
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

    apply_aliases(
        &state.pool,
        title_id,
        vec![(alias_key.clone(), canonical_key.clone())],
    )
    .await?;

    Ok(AliasDto {
        alias_key,
        canonical_key,
    })
}

/// Пачка alias одной транзакцией: auto-match связывает десятки глав, и
/// частично применённая связка оставила бы список в неопределённом виде.
pub async fn add_aliases(
    state: &AppState,
    title_id: i64,
    input: AliasBatchInput,
) -> Result<AliasListResponse, AppError> {
    ensure_title(&state.pool, title_id).await?;

    if input.aliases.is_empty() {
        return Err(AppError::bad_request(
            "validation",
            "aliases must not be empty",
        ));
    }
    if input.aliases.len() > MAX_ALIASES_PER_REQUEST {
        return Err(AppError::bad_request(
            "validation",
            format!("at most {MAX_ALIASES_PER_REQUEST} aliases are allowed"),
        ));
    }

    let mut pairs = Vec::with_capacity(input.aliases.len());
    for alias in &input.aliases {
        let alias_key = normalize_key(&alias.alias_key, "alias_key")?;
        let canonical_key = normalize_key(&alias.canonical_key, "canonical_key")?;
        pairs.push((alias_key, canonical_key));
    }

    apply_aliases(&state.pool, title_id, pairs).await?;

    Ok(AliasListResponse {
        aliases: load_aliases(&state.pool, title_id).await?,
    })
}

/// Общая часть одиночной и пакетной связки. Сначала проверяются ключи,
/// затем всё пишется в транзакции: цепочка или несуществующий ключ
/// откатывают пачку целиком.
async fn apply_aliases(
    pool: &SqlitePool,
    title_id: i64,
    pairs: Vec<(String, String)>,
) -> Result<(), AppError> {
    let keys = chapter_keys(pool, title_id).await?;
    let mut seen_aliases = HashSet::with_capacity(pairs.len());
    for (alias_key, canonical_key) in &pairs {
        // Два разных canonical для одного alias в одной пачке дали бы
        // неоднозначный перенос прогресса: побеждала бы последняя пара.
        if !seen_aliases.insert(alias_key.clone()) {
            return Err(AppError::bad_request(
                "alias_invalid",
                "duplicate alias_key in request",
            ));
        }
        if alias_key == canonical_key {
            return Err(AppError::bad_request(
                "alias_invalid",
                "alias_key and canonical_key must differ",
            ));
        }
        if !keys.contains(alias_key) || !keys.contains(canonical_key) {
            return Err(AppError::not_found(
                "chapter_not_found",
                "alias and canonical keys must belong to the title",
            ));
        }
    }

    // Перепривязка готовая alias к другому canonical оставила бы прогресс
    // на старом ключе, который больше не является ключом этой главы.
    // Требуем явное удаление: так состояние чтения не теряется незаметно.
    let existing: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT alias_key, canonical_key FROM chapter_aliases \
         WHERE title_id = ? AND source_entry_id IS NULL",
    )
    .bind(title_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();
    for (alias_key, canonical_key) in &pairs {
        if let Some(previous) = existing.get(alias_key)
            && previous != canonical_key
        {
            return Err(AppError::conflict(
                "alias_exists",
                "delete the existing alias before changing its canonical key",
            ));
        }
    }

    let mut tx = pool.begin().await?;
    let now = now_unix();
    for (alias_key, canonical_key) in &pairs {
        sqlx::query(
            "INSERT INTO chapter_aliases \
                 (title_id, source_entry_id, canonical_key, alias_key, origin, created_at) \
             VALUES (?, NULL, ?, ?, 'manual', ?) \
             ON CONFLICT (title_id, alias_key) WHERE source_entry_id IS NULL \
             DO UPDATE SET canonical_key = excluded.canonical_key, origin = 'manual'",
        )
        .bind(title_id)
        .bind(canonical_key)
        .bind(alias_key)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    // Цепочка alias -> alias сделала бы резолв неоднозначным. Проверяем
    // всю таблицу: пачка могла создать цепочку с уже существующей связью.
    if has_alias_chain(&mut tx, title_id).await? {
        return Err(AppError::conflict(
            "alias_chain",
            "alias keys must resolve in one hop",
        ));
    }

    for (alias_key, canonical_key) in &pairs {
        migrate_reader_state(&mut tx, title_id, alias_key, canonical_key).await?;
    }

    tx.commit().await?;
    Ok(())
}

async fn has_alias_chain(
    tx: &mut Transaction<'_, Sqlite>,
    title_id: i64,
) -> Result<bool, AppError> {
    let canonical_is_alias = sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM chapter_aliases a \
         JOIN chapter_aliases b ON b.title_id = a.title_id AND b.alias_key = a.canonical_key \
         WHERE a.title_id = ? LIMIT 1",
    )
    .bind(title_id)
    .fetch_optional(&mut **tx)
    .await?
    .is_some();
    if canonical_is_alias {
        return Ok(true);
    }

    let alias_is_canonical = sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM chapter_aliases a \
         JOIN chapter_aliases b ON b.title_id = a.title_id AND b.canonical_key = a.alias_key \
         WHERE a.title_id = ? AND b.alias_key <> a.alias_key LIMIT 1",
    )
    .bind(title_id)
    .fetch_optional(&mut **tx)
    .await?
    .is_some();

    Ok(alias_is_canonical)
}

/// Прогресс и журнал прочитанного живут под каноническим ключом. После
/// связки старая отметка под alias-ключом иначе перестала бы совпадать
/// с ключом главы в списке и выглядела бы непрочитанной.
async fn migrate_reader_state(
    tx: &mut Transaction<'_, Sqlite>,
    title_id: i64,
    alias_key: &str,
    canonical_key: &str,
) -> Result<(), AppError> {
    sqlx::query("UPDATE progress SET chapter_key = ? WHERE title_id = ? AND chapter_key = ?")
        .bind(canonical_key)
        .bind(title_id)
        .bind(alias_key)
        .execute(&mut **tx)
        .await?;

    // INSERT OR IGNORE: канонический ключ мог быть отмечен раньше.
    sqlx::query(
        "INSERT OR IGNORE INTO chapter_reads (user_id, title_id, chapter_key, read_at) \
         SELECT user_id, title_id, ?, read_at FROM chapter_reads \
         WHERE title_id = ? AND chapter_key = ?",
    )
    .bind(canonical_key)
    .bind(title_id)
    .bind(alias_key)
    .execute(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM chapter_reads WHERE title_id = ? AND chapter_key = ?")
        .bind(title_id)
        .bind(alias_key)
        .execute(&mut **tx)
        .await?;

    Ok(())
}

pub async fn delete_alias(
    state: &AppState,
    title_id: i64,
    alias_key: &str,
) -> Result<(), AppError> {
    ensure_title(&state.pool, title_id).await?;
    let alias_key = normalize_key(alias_key, "alias_key")?;

    // Удаляем только ручную пару: производные пары принадлежат связкам
    // и удаляются вместе с ними.
    let result = sqlx::query(
        "DELETE FROM chapter_aliases \
         WHERE title_id = ? AND alias_key = ? AND source_entry_id IS NULL",
    )
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

pub(crate) async fn ensure_title(pool: &SqlitePool, title_id: i64) -> Result<(), AppError> {
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
