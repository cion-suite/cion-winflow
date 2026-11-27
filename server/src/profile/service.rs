use serde::Serialize;
use sqlx::SqlitePool;

use crate::{
    error::AppError,
    library::service::{self, LibraryCounts, LibraryItemDto, LibraryParams, LibraryVisibility},
};

/// Публичная карточка пользователя: uid из URL, ник, роль и время регистрации.
/// Пароль, сессии и прочие служебные поля сюда не попадают.
#[derive(Debug, Serialize)]
pub struct ProfileUserDto {
    pub id: i64,
    pub username: String,
    pub role: String,
    pub created_at: i64,
    /// NULL — аватара нет; иначе время загрузки.
    pub avatar_updated_at: Option<i64>,
    /// Монотонная версия аватара: ETag и cache-buster.
    pub avatar_version: i64,
}

#[derive(Debug, Serialize)]
pub struct ProfileStatsDto {
    /// Сколько глав пользователь отметил прочитанными (журнал chapter_reads).
    pub read_chapters: i64,
    /// Счётчики библиотеки без `not_interested`.
    pub counts: LibraryCounts,
}

#[derive(Debug, Serialize)]
pub struct ProfileLibraryDto {
    pub items: Vec<LibraryItemDto>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, Serialize)]
pub struct ProfileResponse {
    pub user: ProfileUserDto,
    pub stats: ProfileStatsDto,
    pub library: ProfileLibraryDto,
}

type ProfileRow = (i64, String, String, i64, Option<i64>, i64);

/// Профиль любого активного пользователя. Отключённый пользователь
/// неотличим от несуществующего: 404, а не утечка его библиотеки.
pub async fn get_profile(
    pool: &SqlitePool,
    user_id: i64,
    params: LibraryParams,
) -> Result<ProfileResponse, AppError> {
    let row = sqlx::query_as::<_, ProfileRow>(
        "SELECT id, username, role, created_at, avatar_updated_at, avatar_version \
         FROM users WHERE id = ? AND disabled_at IS NULL",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("user_not_found", "user not found"))?;

    let library = service::list_library(pool, user_id, params, LibraryVisibility::Public).await?;
    let read_chapters =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM chapter_reads WHERE user_id = ?")
            .bind(user_id)
            .fetch_one(pool)
            .await?;

    Ok(ProfileResponse {
        user: ProfileUserDto {
            id: row.0,
            username: row.1,
            role: row.2,
            created_at: row.3,
            avatar_updated_at: row.4,
            avatar_version: row.5,
        },
        stats: ProfileStatsDto {
            read_chapters,
            counts: library.counts,
        },
        library: ProfileLibraryDto {
            items: library.items,
            total: library.total,
            page: library.page,
            page_size: library.page_size,
        },
    })
}
