use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::{
    auth::session::CurrentUser,
    catalog::{
        links::{self, AliasDto, AliasInput, AliasListResponse, AttachInput, PrimaryInput},
        service::{self, CatalogPage, CatalogParams, TitleDto, TitleInput, TitleResponse},
    },
    error::AppError,
    extract::{ApiJson, ApiPath, ApiQuery},
    source_admin::service::ImportResponse,
    state::AppState,
};

#[derive(Debug, Serialize)]
pub struct TitlePayload {
    pub title: TitleDto,
}

pub async fn list_catalog(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<CatalogParams>,
) -> Result<Json<CatalogPage>, AppError> {
    let page = service::list_catalog(&state.pool, params).await?;
    Ok(Json(page))
}

pub async fn get_title(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(title_id): ApiPath<i64>,
) -> Result<Json<TitleResponse>, AppError> {
    let response = service::get_title(&state.pool, user.id, title_id).await?;
    Ok(Json(response))
}

pub async fn create_title(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiJson(input): ApiJson<TitleInput>,
) -> Result<Response, AppError> {
    user.require_admin()?;
    let title = service::create_title(&state, input).await?;

    Ok((StatusCode::CREATED, Json(TitlePayload { title })).into_response())
}

pub async fn update_title(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(title_id): ApiPath<i64>,
    ApiJson(input): ApiJson<TitleInput>,
) -> Result<Json<TitlePayload>, AppError> {
    user.require_admin()?;
    let title = service::update_title(&state, title_id, input).await?;

    Ok(Json(TitlePayload { title }))
}

pub async fn attach_source(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(title_id): ApiPath<i64>,
    ApiJson(input): ApiJson<AttachInput>,
) -> Result<Json<ImportResponse>, AppError> {
    user.require_admin()?;
    Ok(Json(links::attach(&state, title_id, input).await?))
}

pub async fn detach_source(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath((title_id, entry_id)): ApiPath<(i64, i64)>,
) -> Result<StatusCode, AppError> {
    user.require_admin()?;
    links::detach(&state, title_id, entry_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_primary_source(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(title_id): ApiPath<i64>,
    ApiJson(input): ApiJson<PrimaryInput>,
) -> Result<StatusCode, AppError> {
    user.require_admin()?;
    links::set_primary(&state, title_id, input).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_aliases(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(title_id): ApiPath<i64>,
) -> Result<Json<AliasListResponse>, AppError> {
    user.require_admin()?;
    Ok(Json(links::list_aliases(&state.pool, title_id).await?))
}

pub async fn add_alias(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(title_id): ApiPath<i64>,
    ApiJson(input): ApiJson<AliasInput>,
) -> Result<Json<AliasDto>, AppError> {
    user.require_admin()?;
    Ok(Json(links::add_alias(&state, title_id, input).await?))
}

pub async fn delete_alias(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath((title_id, alias_key)): ApiPath<(i64, String)>,
) -> Result<StatusCode, AppError> {
    user.require_admin()?;
    links::delete_alias(&state, title_id, &alias_key).await?;
    Ok(StatusCode::NO_CONTENT)
}
