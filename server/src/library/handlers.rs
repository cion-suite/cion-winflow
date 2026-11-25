use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
};

use crate::{
    auth::session::CurrentUser,
    error::AppError,
    extract::{ApiJson, ApiPath, ApiQuery},
    library::service::{self, LibraryEntryDto, LibraryInput, LibraryListResponse, LibraryParams},
    state::AppState,
};

pub async fn list_library(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiQuery(params): ApiQuery<LibraryParams>,
) -> Result<Json<LibraryListResponse>, AppError> {
    let response = service::list_library(&state.pool, user.id, params).await?;
    Ok(Json(response))
}

pub async fn upsert_library(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(title_id): ApiPath<i64>,
    ApiJson(input): ApiJson<LibraryInput>,
) -> Result<Json<LibraryEntryDto>, AppError> {
    let entry = service::upsert_library(&state, user.id, title_id, input).await?;
    Ok(Json(entry))
}

pub async fn remove_library(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(title_id): ApiPath<i64>,
) -> Result<StatusCode, AppError> {
    service::remove_library(&state, user.id, title_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
