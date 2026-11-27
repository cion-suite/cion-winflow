use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
};

use crate::{
    auth::session::CurrentUser,
    error::AppError,
    extract::{ApiJson, ApiPath},
    source_auth::service::{
        self, CredentialDto, CredentialInput, CredentialsListResponse, LoginInput,
    },
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

/// Вход по логину/паролю. Счётчик на пользователя проверяется до сети:
/// пять неудачных попыток за десять минут, затем 429 с `Retry-After`.
pub async fn login(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(code): ApiPath<String>,
    ApiJson(input): ApiJson<LoginInput>,
) -> Result<Json<CredentialDto>, AppError> {
    let key = user.id.to_string();
    if let Err(retry_after_secs) = state.source_login_limiter.check_and_record(&key) {
        return Err(AppError::RateLimited {
            message: "too many login attempts; try again later".to_owned(),
            retry_after_secs,
        });
    }

    let result = service::login_and_save(&state, user.id, &code, input).await;
    if result.is_ok() {
        // Удачный вход сбрасывает окно: пользователь доказал, что пароль его.
        state.source_login_limiter.clear(&key);
    }

    Ok(Json(result?))
}
