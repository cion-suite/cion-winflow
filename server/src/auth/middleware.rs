use axum::{
    extract::{OriginalUri, Request, State},
    http::{Method, header::ORIGIN},
    middleware::Next,
    response::{IntoResponse, Response},
};
use axum_extra::extract::CookieJar;

use crate::{
    auth::session::{self, CurrentUser, Role},
    error::AppError,
    state::AppState,
};

const SEC_FETCH_SITE: &str = "sec-fetch-site";

/// Публичные пути перечислены явно: всё остальное под /api требует сессию.
/// Внутри `nest("/api", ...)` URI уже без префикса, поэтому берём OriginalUri.
fn is_public(request: &Request) -> bool {
    let path = request
        .extensions()
        .get::<OriginalUri>()
        .map_or_else(|| request.uri().path(), |uri| uri.0.path());

    path == "/api/health" || path.starts_with("/api/auth/")
}

pub async fn require_session(
    State(state): State<AppState>,
    jar: CookieJar,
    mut request: Request,
    next: Next,
) -> Result<Response, AppError> {
    if is_public(&request) {
        return Ok(next.run(request).await);
    }

    let Some(token) = jar
        .get(session::COOKIE_NAME)
        .map(|cookie| cookie.value().to_owned())
    else {
        return Err(AppError::Unauthorized);
    };

    let token_hash = session::hash_secret(&token);
    let Some(row) = session::find_session(&state.pool, &token_hash).await? else {
        return Err(AppError::Unauthorized);
    };

    let now = session::now_unix();
    let expired = row.expires_at <= now || now - row.last_seen_at > session::IDLE_SECS;

    if row.disabled_at.is_some() || expired {
        if let Err(error) = session::delete_session(&state.pool, row.session_id).await {
            tracing::warn!(%error, "cannot delete expired session");
        }
        return Err(AppError::Unauthorized);
    }

    session::touch_session(&state.pool, row.session_id, now).await;

    request.extensions_mut().insert(CurrentUser {
        id: row.user_id,
        username: row.username,
        role: Role::from_db(&row.role),
        avatar_updated_at: row.avatar_updated_at,
        avatar_version: row.avatar_version,
    });

    Ok(next.run(request).await)
}

/// CSRF: мутации принимаются только с разрешённого Origin. SameSite=Lax
/// и Sec-Fetch-Site — вторая линия, а не замена проверке.
pub async fn origin_guard(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let mutating = matches!(
        request.method(),
        &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE
    );

    if mutating {
        let origin_allowed = request
            .headers()
            .get(ORIGIN)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|origin| {
                state
                    .config
                    .allowed_origins
                    .iter()
                    .any(|allowed| allowed == origin)
            });

        let fetch_site_allowed = request
            .headers()
            .get(SEC_FETCH_SITE)
            .and_then(|value| value.to_str().ok())
            .is_none_or(|value| value != "cross-site");

        if !origin_allowed || !fetch_site_allowed {
            return AppError::forbidden("origin_not_allowed", "request origin is not allowed")
                .into_response();
        }
    }

    next.run(request).await
}
