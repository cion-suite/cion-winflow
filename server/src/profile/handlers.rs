use axum::{Json, extract::State};

use crate::{
    error::AppError,
    extract::{ApiPath, ApiQuery},
    library::service::LibraryParams,
    profile::service::{self, ProfileResponse},
    state::AppState,
};

/// Публичный профиль по uid: ник, аватар, статистика и чужой библиотека.
/// Доступен любой сессии — приложение закрытое, но внутри него профили общие.
pub async fn get_profile(
    State(state): State<AppState>,
    ApiPath(user_id): ApiPath<i64>,
    ApiQuery(params): ApiQuery<LibraryParams>,
) -> Result<Json<ProfileResponse>, AppError> {
    Ok(Json(
        service::get_profile(&state.pool, user_id, params).await?,
    ))
}
