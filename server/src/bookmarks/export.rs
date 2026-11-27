//! Экспорт библиотеки в JSON-формате закладок Remanga.
//!
//! Формат намеренно совпадает с тем, что принимает импорт:
//! `{ "remanga": [{ type, progress, progress_total, name, dir }] }`.
//! Своя закладка экспортируется исходным `type` из `source_ref`; если
//! закладка создана вручную (source_ref нет), берётся встроенный статус.

use std::collections::HashMap;

use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::AppError;

#[derive(Debug, Serialize)]
struct ExportFile {
    remanga: Vec<ExportBookmark>,
}

#[derive(Debug, Serialize)]
struct ExportBookmark {
    #[serde(rename = "type")]
    bookmark_type: i64,
    /// Сколько глав прочитано, но не больше `progress_total`: high-water
    /// 930 при 17 доступных главах в формате Remanga выглядел бы битым.
    progress: i64,
    /// Сколько глав у самого полного источника; импорт это поле не читает.
    progress_total: i64,
    name: String,
    dir: String,
}

/// Собирает экспорт для тайтлов, привязанных к `source_code`.
/// Тайтлы без привязки к источнику в файл не попадают: у них нет `dir`.
pub async fn export_bookmarks(
    pool: &SqlitePool,
    user_id: i64,
    source_code: &str,
) -> Result<Vec<u8>, AppError> {
    let tabs: HashMap<i64, Option<i64>> = sqlx::query_as::<_, (i64, Option<String>)>(
        "SELECT id, source_ref FROM library_tabs WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(id, source_ref)| {
        (
            id,
            source_ref
                .as_deref()
                .and_then(|value| bookmark_type(source_code, value)),
        )
    })
    .collect();

    let rows = sqlx::query_as::<_, (String, Option<i64>, String, i64, i64, Option<String>)>(
        "SELECT l.status, l.custom_tab_id, t.name, \
                (SELECT MAX( \
                     (SELECT COUNT(*) FROM chapter_reads r \
                      WHERE r.user_id = l.user_id AND r.title_id = l.title_id), \
                     COALESCE(l.imported_read_count, 0))) AS read_count, \
                (SELECT COALESCE(MAX(e.chapter_count), 0) FROM source_entries e \
                 WHERE e.title_id = l.title_id) AS chapter_count, \
                (SELECT e.external_id FROM source_entries e \
                 JOIN sources s ON s.id = e.source_id \
                 WHERE e.title_id = l.title_id AND s.code = ? \
                 ORDER BY e.is_primary DESC, e.id LIMIT 1) AS dir \
         FROM library l JOIN titles t ON t.id = l.title_id \
         WHERE l.user_id = ? \
         ORDER BY l.updated_at DESC, l.title_id",
    )
    .bind(source_code)
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let items = rows
        .into_iter()
        .filter_map(
            |(status, custom_tab_id, name, read_count, chapter_count, dir)| {
                let dir = dir?;
                let bookmark_type = custom_tab_id
                    .and_then(|tab_id| tabs.get(&tab_id).copied().flatten())
                    .or_else(|| standard_type(&status))?;
                Some(ExportBookmark {
                    bookmark_type,
                    progress: read_count.min(chapter_count),
                    progress_total: chapter_count,
                    name,
                    dir,
                })
            },
        )
        .collect();

    let file = ExportFile { remanga: items };
    serde_json::to_vec_pretty(&file).map_err(|error| {
        tracing::error!(%error, "cannot serialize bookmarks export");
        AppError::Internal
    })
}

fn bookmark_type(source_code: &str, source_ref: &str) -> Option<i64> {
    let prefix = format!("{source_code}:bookmark:");
    source_ref.strip_prefix(&prefix)?.parse().ok()
}

fn standard_type(status: &str) -> Option<i64> {
    match status {
        "in_progress" => Some(1),
        "planned" => Some(2),
        "completed" => Some(3),
        "dropped" => Some(4),
        "on_hold" => Some(5),
        "not_interested" => Some(6),
        "favorite" => Some(7),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_statuses_map_to_remanga_types() {
        assert_eq!(standard_type("in_progress"), Some(1));
        assert_eq!(standard_type("planned"), Some(2));
        assert_eq!(standard_type("completed"), Some(3));
        assert_eq!(standard_type("dropped"), Some(4));
        assert_eq!(standard_type("on_hold"), Some(5));
        assert_eq!(standard_type("not_interested"), Some(6));
        assert_eq!(standard_type("favorite"), Some(7));
        assert_eq!(standard_type("unknown"), None);
    }

    #[test]
    fn source_ref_round_trips_custom_type() {
        assert_eq!(
            bookmark_type("remanga", "remanga:bookmark:34956"),
            Some(34_956)
        );
        assert_eq!(bookmark_type("remanga", "rumix:bookmark:34956"), None);
        assert_eq!(bookmark_type("remanga", "remanga:bookmark:oops"), None);
    }
}
