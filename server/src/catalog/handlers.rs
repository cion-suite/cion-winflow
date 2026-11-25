use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::{
    auth::session::CurrentUser,
    catalog::service::{self, CatalogPage, CatalogParams, TitleDto, TitleInput, TitleResponse},
    error::AppError,
    extract::{ApiJson, ApiPath, ApiQuery},
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
