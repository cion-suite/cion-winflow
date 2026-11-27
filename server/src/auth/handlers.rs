use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use axum_extra::extract::CookieJar;
use serde::{Deserialize, Serialize};

use crate::{
    auth::{
        service,
        session::{self, CurrentUser},
    },
    error::AppError,
    extract::ApiJson,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    pub invite: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUsernameRequest {
    pub username: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub user: CurrentUser,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CreateInviteRequest {
    pub expires_in_days: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct CreateInviteResponse {
    pub code: String,
    pub expires_at: i64,
}

#[derive(Debug, Serialize)]
pub struct InviteListResponse {
    pub items: Vec<service::InviteSummary>,
}

pub async fn register(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<RegisterRequest>,
) -> Result<Response, AppError> {
    let (user, token) =
        service::register(&state, request.username, request.password, request.invite).await?;
    let jar = CookieJar::new().add(session::session_cookie(token, state.config.cookie_secure));

    Ok((StatusCode::CREATED, jar, Json(AuthResponse { user })).into_response())
}

pub async fn login(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<LoginRequest>,
) -> Result<Response, AppError> {
    let (user, token) = service::login(&state, request.username, request.password).await?;
    let jar = CookieJar::new().add(session::session_cookie(token, state.config.cookie_secure));

    Ok((jar, Json(AuthResponse { user })).into_response())
}

pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> Result<Response, AppError> {
    let token = jar
        .get(session::COOKIE_NAME)
        .map(|cookie| cookie.value().to_owned());
    service::logout(&state, token).await?;

    Ok((jar.remove(session::clear_cookie()), StatusCode::NO_CONTENT).into_response())
}

pub async fn me(Extension(user): Extension<CurrentUser>) -> Json<AuthResponse> {
    Json(AuthResponse { user })
}

pub async fn update_username(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiJson(request): ApiJson<UpdateUsernameRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    let username = service::update_username(&state, user.id, request.username).await?;
    Ok(Json(AuthResponse {
        user: CurrentUser { username, ..user },
    }))
}

/// Смена пароля оставляет текущую сессию и удаляет остальные.
pub async fn change_password(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    jar: CookieJar,
    ApiJson(request): ApiJson<ChangePasswordRequest>,
) -> Result<StatusCode, AppError> {
    let token = jar
        .get(session::COOKIE_NAME)
        .map(|cookie| cookie.value().to_owned())
        .ok_or(AppError::Unauthorized)?;

    service::change_password(
        &state,
        user.id,
        request.current_password,
        request.new_password,
        &session::hash_secret(&token),
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_invites(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<InviteListResponse>, AppError> {
    user.require_admin()?;
    let items = service::list_invites(&state).await?;
    Ok(Json(InviteListResponse { items }))
}

pub async fn create_invite(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiJson(request): ApiJson<CreateInviteRequest>,
) -> Result<Response, AppError> {
    user.require_admin()?;
    let (code, expires_at) =
        service::create_invite(&state, user.id, request.expires_in_days).await?;

    Ok((
        StatusCode::CREATED,
        Json(CreateInviteResponse { code, expires_at }),
    )
        .into_response())
}
