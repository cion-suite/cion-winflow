use axum::{
    Json,
    extract::{Extension, State},
};

use crate::{
    auth::session::CurrentUser,
    discover::service::{
        self, DiscoverAddInput, DiscoverAddResponse, DiscoverSearchParams, DiscoverSearchResponse,
    },
    error::AppError,
    extract::{ApiJson, ApiQuery},
    state::AppState,
};

pub async fn search(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiQuery(params): ApiQuery<DiscoverSearchParams>,
) -> Result<Json<DiscoverSearchResponse>, AppError> {
    Ok(Json(service::search(&state, user.id, params).await?))
}

pub async fn add(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiJson(input): ApiJson<DiscoverAddInput>,
) -> Result<Json<DiscoverAddResponse>, AppError> {
    Ok(Json(service::add(&state, user.id, input).await?))
}
