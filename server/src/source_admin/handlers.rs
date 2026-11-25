use axum::{
    Json,
    extract::{Extension, State},
};
use serde_json::Value;

use crate::{
    auth::session::CurrentUser,
    error::AppError,
    extract::{ApiJson, ApiPath},
    source_admin::service::{
        self, ConfigSaveResponse, ImportInput, ImportResponse, ProbeResponse, SearchInput,
        SearchResponse, SourceInput, SourceListResponse, SourceStateDto,
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
