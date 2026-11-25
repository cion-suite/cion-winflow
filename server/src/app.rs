use std::{net::IpAddr, sync::Arc};

use axum::{
    Json, Router,
    extract::State,
    middleware,
    response::{IntoResponse, Response},
    routing::{MethodRouter, get, post},
};
use serde::Serialize;
use tower_governor::{GovernorError, GovernorLayer, governor::GovernorConfigBuilder};
use tower_http::{compression::CompressionLayer, trace::TraceLayer};

use crate::{
    auth::{self, limiter::ClientIpKeyExtractor},
    error::{self, AppError},
    state::AppState,
    static_files::serve_static,
};

#[derive(Debug, Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
}

/// Health проверяет и процесс, и БД: иначе мониторинг не увидит мёртвую базу.
async fn health(State(state): State<AppState>) -> Result<Json<Health>, AppError> {
    sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    }))
}

pub fn router(state: AppState) -> Router {
    // Логин и регистрация дорогие (Argon2), поэтому у них отдельные IP-лимиты.
    // Имена пользователей лимитируются внутри login (после разбора JSON).
    let login = with_rate_limit(
        post(auth::handlers::login),
        state.config.trusted_proxies.clone(),
        10,
        2,
    );
    let register = with_rate_limit(
        post(auth::handlers::register),
        state.config.trusted_proxies.clone(),
        5,
        10,
    );

    let api = Router::new()
        .route("/health", get(health))
        .route("/auth/register", register)
        .route("/auth/login", login)
        .route("/auth/logout", post(auth::handlers::logout))
        .route("/me", get(auth::handlers::me))
        .route(
            "/admin/invites",
            get(auth::handlers::list_invites).post(auth::handlers::create_invite),
        )
        .fallback(error::api_not_found)
        // Последний слой — внешний: Origin проверяется до сессии
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::middleware::require_session,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::middleware::origin_guard,
        ));

    Router::new()
        .nest("/api", api)
        .fallback(serve_static)
        .with_state(state)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
}

fn with_rate_limit(
    route: MethodRouter<AppState>,
    trusted_proxies: Vec<IpAddr>,
    burst_size: u32,
    period_secs: u64,
) -> MethodRouter<AppState> {
    let mut builder = GovernorConfigBuilder::default();
    builder.per_second(period_secs).burst_size(burst_size);

    match builder
        .key_extractor(ClientIpKeyExtractor::new(trusted_proxies))
        .finish()
    {
        Some(config) => {
            route.layer(GovernorLayer::new(Arc::new(config)).error_handler(rate_limit_response))
        }
        None => {
            // finish() вернёт None только при нулевых значениях, а они заданы
            // константами. unwrap/expect в production-коде запрещены, поэтому
            // ветка обрабатывается явно: без rate limit, но без паники.
            tracing::error!(
                burst_size,
                period_secs,
                "invalid rate limit configuration; rate limiting is disabled"
            );
            route
        }
    }
}

fn rate_limit_response(error: GovernorError) -> Response {
    match error {
        GovernorError::TooManyRequests { wait_time, .. } => AppError::RateLimited {
            message: "too many requests; try again later".to_owned(),
            retry_after_secs: wait_time,
        }
        .into_response(),
        _ => AppError::Internal.into_response(),
    }
}
