use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
};

use crate::{
    auth::session::CurrentUser,
    error::AppError,
    extract::{ApiJson, ApiPath, ApiQuery},
    library::service::{
        self, LibraryEntryDto, LibraryInput, LibraryListResponse, LibraryParams, LibraryTabDto,
        LibraryTabsResponse, TabNameInput, TitleTabsInput, TitleTabsResponse,
    },
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

pub async fn list_tabs(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<LibraryTabsResponse>, AppError> {
    Ok(Json(service::list_tabs(&state.pool, user.id).await?))
}

pub async fn create_tab(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiJson(input): ApiJson<TabNameInput>,
) -> Result<(StatusCode, Json<LibraryTabDto>), AppError> {
    let tab = service::create_tab(&state, user.id, input).await?;
    Ok((StatusCode::CREATED, Json(tab)))
}

pub async fn rename_tab(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(tab_id): ApiPath<i64>,
    ApiJson(input): ApiJson<TabNameInput>,
) -> Result<Json<LibraryTabDto>, AppError> {
    Ok(Json(
        service::rename_tab(&state, user.id, tab_id, input).await?,
    ))
}

pub async fn delete_tab(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(tab_id): ApiPath<i64>,
) -> Result<StatusCode, AppError> {
    service::delete_tab(&state, user.id, tab_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_title_tabs(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(title_id): ApiPath<i64>,
    ApiJson(input): ApiJson<TitleTabsInput>,
) -> Result<Json<TitleTabsResponse>, AppError> {
    Ok(Json(
        service::set_title_tabs(&state, user.id, title_id, input).await?,
    ))
}
