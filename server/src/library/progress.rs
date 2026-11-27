//! Импортированный прогресс и автоматическая отметка новых глав.
//!
//! Библиотека хранит отдельно `imported_read_count` — монотонный high-water
//! из закладок. Он может быть больше числа доступных сейчас глав: закрытые,
//! удалённые или ещё не появившиеся главы не видны, но считаются
//! прочитанными. Когда после sync появляется новая доступная глава (или
//! `locked`/`unknown` становится доступной), её отмечают прочитанной, но
//! только если она попадает в первые `min(high-water, доступные)` глав.
//! Неизменившиеся главы, снятые вручную, не восстанавливаются: сравнение
//! идёт с набором доступных ключей **до** upsert.

use std::collections::{HashMap, HashSet};

use sqlx::SqlitePool;

use crate::{
    auth::session::now_unix,
    catalog::{chapters, selection},
    error::AppError,
    state::AppState,
};

/// Пользователь, у которого для тайтла есть импортированный high-water.
struct HighWaterUser {
    user_id: i64,
    imported: i64,
}

/// Состояние до upsert: набор доступных канонических ключей и язык
/// источников. `None` из [`snapshot_available`] означает, что ни у кого нет
/// high-water и сравнивать не с чем.
pub struct AvailableSnapshot {
    keys: HashSet<String>,
    langs: HashMap<String, &'static str>,
    user_id: i64,
}

/// Снимок доступных глав перед upsert ищется только при наличии
/// high-water: для новых тайтлов и тайтлов без импорта работа не нужна.
pub async fn snapshot_available(
    state: &AppState,
    title_id: i64,
) -> Result<Option<AvailableSnapshot>, AppError> {
    let users = high_water_users(&state.pool, title_id).await?;
    let Some(first) = users.first() else {
        return Ok(None);
    };
    let langs = source_langs(state).await;
    let keys = available_keys(&state.pool, title_id, &langs, first.user_id).await?;
    Ok(Some(AvailableSnapshot {
        keys: keys.into_iter().collect(),
        langs,
        user_id: first.user_id,
    }))
}

/// Отмечает появившиеся доступные главы прочитанными по high-water каждого
/// пользователя. Позицию `progress` не трогает: импорт и sync не должны
/// перескакивать вперёд за читателя.
pub async fn mark_newly_available(
    state: &AppState,
    title_id: i64,
    snapshot: &AvailableSnapshot,
) -> Result<i64, AppError> {
    let users = high_water_users(&state.pool, title_id).await?;
    if users.is_empty() {
        return Ok(0);
    }

    let available =
        available_keys(&state.pool, title_id, &snapshot.langs, snapshot.user_id).await?;
    let now = now_unix();
    let mut tx = state.pool.begin().await?;
    let mut marked = 0_i64;

    for user in users {
        // High-water ограничивает, докуда прогресс вообще может доставать:
        // новый 18-й том не становится прочитанным у пользователя с 5 главами.
        let limit = usize::try_from(user.imported.max(0))
            .unwrap_or(usize::MAX)
            .min(available.len());
        for key in &available[..limit] {
            if snapshot.keys.contains(key) {
                continue;
            }
            let result = sqlx::query(
                "INSERT INTO chapter_reads (user_id, title_id, chapter_key, read_at) \
                 VALUES (?, ?, ?, ?) \
                 ON CONFLICT (user_id, title_id, chapter_key) DO NOTHING",
            )
            .bind(user.user_id)
            .bind(title_id)
            .bind(key)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            marked += i64::try_from(result.rows_affected()).unwrap_or(0);
        }
    }

    tx.commit().await?;
    Ok(marked)
}

/// Языки источников: реестр знает их, а таблица `sources` — нет.
pub async fn source_langs(state: &AppState) -> HashMap<String, &'static str> {
    let registry = state.sources.read().await;
    registry
        .entries()
        .map(|runtime| (runtime.code.clone(), runtime.descriptor.lang))
        .collect()
}

async fn high_water_users(
    pool: &SqlitePool,
    title_id: i64,
) -> Result<Vec<HighWaterUser>, AppError> {
    let rows = sqlx::query_as::<_, (i64, i64)>(
        "SELECT user_id, imported_read_count FROM library \
         WHERE title_id = ? AND imported_read_count > 0 \
         ORDER BY user_id",
    )
    .bind(title_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(user_id, imported)| HighWaterUser { user_id, imported })
        .collect())
}

/// Доступные канонические ключи в порядке чтения. Правила те же, что у
/// импорта: `locked`/`missing` не считаются, порядок — том, номер, ключ.
async fn available_keys(
    pool: &SqlitePool,
    title_id: i64,
    source_langs: &HashMap<String, &'static str>,
    user_id: i64,
) -> Result<Vec<String>, AppError> {
    let title = selection::load_title_info(pool, title_id).await?;
    let chapters = chapters::title_chapters(pool, &title, source_langs, None, user_id).await?;
    let mut available: Vec<_> = chapters
        .chapters
        .into_iter()
        .filter(|chapter| chapter.availability != "locked" && chapter.availability != "missing")
        .collect();
    available.sort_by(|left, right| {
        left.volume
            .unwrap_or(0)
            .cmp(&right.volume.unwrap_or(0))
            .then_with(|| {
                left.number_milli
                    .unwrap_or(i64::MAX)
                    .cmp(&right.number_milli.unwrap_or(i64::MAX))
            })
            .then_with(|| left.chapter_key.cmp(&right.chapter_key))
    });
    Ok(available
        .into_iter()
        .map(|chapter| chapter.chapter_key)
        .collect())
}
