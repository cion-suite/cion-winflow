use axum::{
    Json,
    extract::{Extension, State},
};
use serde_json::Value;

use crate::{
    auth::session::CurrentUser,
    error::AppError,
    extract::{ApiJson, ApiPath, ApiQuery},
    source_admin::{
        priorities::{self, PrioritiesInput, PrioritiesQuery, PrioritiesResponse},
        service::{
            self, ConfigSaveResponse, ImportInput, ImportResponse, ProbeResponse, RefreshResponse,
            SearchInput, SearchResponse, SourceInput, SourceListResponse, SourceStateDto,
        },
    },
    state::AppState,
};

pub async fn list_sources(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<SourceListResponse>, AppError> {
    user.require_admin()?;
    Ok(Json(service::list_sources(&state).await))
}

pub async fn upsert_source(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(code): ApiPath<String>,
    ApiJson(input): ApiJson<SourceInput>,
) -> Result<Json<SourceStateDto>, AppError> {
    user.require_admin()?;
    Ok(Json(service::upsert_source(&state, &code, input).await?))
}

pub async fn save_config(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(code): ApiPath<String>,
    ApiJson(patch): ApiJson<Value>,
) -> Result<Json<ConfigSaveResponse>, AppError> {
    user.require_admin()?;
    Ok(Json(service::save_config(&state, &code, patch).await?))
}

pub async fn search(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(code): ApiPath<String>,
    ApiJson(input): ApiJson<SearchInput>,
) -> Result<Json<SearchResponse>, AppError> {
    user.require_admin()?;
    Ok(Json(service::search(&state, &code, input).await?))
}

pub async fn import(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(code): ApiPath<String>,
    ApiJson(input): ApiJson<ImportInput>,
) -> Result<Json<ImportResponse>, AppError> {
    user.require_admin()?;
    Ok(Json(service::import(&state, &code, input).await?))
}

pub async fn probe(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(code): ApiPath<String>,
) -> Result<Json<ProbeResponse>, AppError> {
    user.require_admin()?;
    Ok(Json(service::probe(&state, &code).await?))
}

pub async fn list_priorities(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiQuery(query): ApiQuery<PrioritiesQuery>,
) -> Result<Json<PrioritiesResponse>, AppError> {
    user.require_admin()?;
    Ok(Json(priorities::list(&state, query).await?))
}

pub async fn replace_priorities(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiJson(input): ApiJson<PrioritiesInput>,
) -> Result<Json<PrioritiesResponse>, AppError> {
    user.require_admin()?;
    Ok(Json(priorities::replace(&state, input).await?))
}

pub async fn refresh_title(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(title_id): ApiPath<i64>,
) -> Result<Json<RefreshResponse>, AppError> {
    user.require_admin()?;
    Ok(Json(service::refresh_title(&state, title_id).await?))
}
