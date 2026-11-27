use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::session::CurrentUser,
    catalog::{
        alias_ranges::{self, AliasRangeDto, AliasRangeListResponse, RangeBatchInput, RangeInput},
        links::{
            self, AliasBatchInput, AliasDto, AliasInput, AliasListResponse, AttachInput,
            PrimaryInput,
        },
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

/// Язык просмотра: какие переводы показывать на странице тайтла.
#[derive(Debug, Deserialize)]
pub struct TitleQuery {
    pub lang: Option<String>,
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
    ApiQuery(query): ApiQuery<TitleQuery>,
) -> Result<Json<TitleResponse>, AppError> {
    let view_lang = service::normalize_lang(query.lang.as_deref());
    let response = service::get_title(&state, user.id, title_id, view_lang.as_deref()).await?;
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
    Ok(Json(links::attach(&state, user.id, title_id, input).await?))
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

pub async fn add_aliases(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(title_id): ApiPath<i64>,
    ApiJson(input): ApiJson<AliasBatchInput>,
) -> Result<Json<AliasListResponse>, AppError> {
    user.require_admin()?;
    Ok(Json(links::add_aliases(&state, title_id, input).await?))
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

pub async fn list_alias_ranges(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(title_id): ApiPath<i64>,
) -> Result<Json<AliasRangeListResponse>, AppError> {
    user.require_admin()?;
    Ok(Json(alias_ranges::list_ranges(&state, title_id).await?))
}

pub async fn create_alias_range(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(title_id): ApiPath<i64>,
    ApiJson(input): ApiJson<RangeInput>,
) -> Result<Json<AliasRangeDto>, AppError> {
    user.require_admin()?;
    Ok(Json(
        alias_ranges::create_range(&state, title_id, input).await?,
    ))
}

pub async fn update_alias_range(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath((title_id, range_id)): ApiPath<(i64, i64)>,
    ApiJson(input): ApiJson<RangeInput>,
) -> Result<Json<AliasRangeDto>, AppError> {
    user.require_admin()?;
    Ok(Json(
        alias_ranges::update_range(&state, title_id, range_id, input).await?,
    ))
}

pub async fn delete_alias_range(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath((title_id, range_id)): ApiPath<(i64, i64)>,
) -> Result<StatusCode, AppError> {
    user.require_admin()?;
    alias_ranges::delete_range(&state, title_id, range_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn apply_alias_ranges(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(title_id): ApiPath<i64>,
    ApiJson(input): ApiJson<RangeBatchInput>,
) -> Result<Json<AliasRangeListResponse>, AppError> {
    user.require_admin()?;
    Ok(Json(
        alias_ranges::apply_ranges(&state, title_id, input).await?,
    ))
}
