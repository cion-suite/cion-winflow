use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
};

use crate::{
    auth::session::CurrentUser,
    error::AppError,
    extract::{ApiJson, ApiPath},
    source_auth::service::{self, CredentialDto, CredentialInput, CredentialsListResponse},
    state::AppState,
};

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<CredentialsListResponse>, AppError> {
    Ok(Json(service::list(&state, user.id).await?))
}

pub async fn upsert(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(code): ApiPath<String>,
    ApiJson(input): ApiJson<CredentialInput>,
) -> Result<Json<CredentialDto>, AppError> {
    Ok(Json(service::upsert(&state, user.id, &code, input).await?))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(code): ApiPath<String>,
) -> Result<StatusCode, AppError> {
    service::delete(&state, user.id, &code).await?;
    Ok(StatusCode::NO_CONTENT)
}
