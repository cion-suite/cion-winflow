use std::{net::IpAddr, sync::Arc};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    middleware,
    response::{IntoResponse, Response},
    routing::{MethodRouter, delete, get, post, put},
};
use serde::Serialize;
use tower_governor::{GovernorError, GovernorLayer, governor::GovernorConfigBuilder};
use tower_http::{compression::CompressionLayer, trace::TraceLayer};

use crate::{
    auth::{self, limiter::ClientIpKeyExtractor},
    catalog, discover, error,
    error::AppError,
    images, library, profile, progress, source_admin,
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
    // Ленивый импорт ходит в чужую сеть и пишет сотни строк: ограничиваем,
    // чтобы один клиент не выкачал источники целиком.
    let discover_add = with_rate_limit(
        post(discover::handlers::add),
        state.config.trusted_proxies.clone(),
        6,
        4,
    );
    // Синхронизация страницы тайтла ходит во все привязанные источники.
    // Сервис дополнительно не чаще раза в 10 минут на тайтл.
    let title_sync = with_rate_limit(
        post(source_admin::handlers::sync_title),
        state.config.trusted_proxies.clone(),
        10,
        2,
    );

    let api = Router::new()
        .route("/health", get(health))
        .route("/auth/register", register)
        .route("/auth/login", login)
        .route("/auth/logout", post(auth::handlers::logout))
        .route("/me", get(auth::handlers::me))
        .route(
            "/me/avatar",
            // Дефолтные 2 МиБ не пропустят 5-мегабайтный файл
            post(profile::avatar::upload_avatar)
                .delete(profile::avatar::delete_avatar)
                .layer(DefaultBodyLimit::max(profile::BODY_LIMIT)),
        )
        .route("/users/{id}/profile", get(profile::handlers::get_profile))
        .route("/users/{id}/avatar", get(profile::avatar::get_avatar))
        .route(
            "/admin/invites",
            get(auth::handlers::list_invites).post(auth::handlers::create_invite),
        )
        .route("/catalog", get(catalog::handlers::list_catalog))
        .route("/titles", post(catalog::handlers::create_title))
        .route(
            "/titles/{id}",
            get(catalog::handlers::get_title).put(catalog::handlers::update_title),
        )
        .route("/titles/{id}/sync", title_sync)
        .route(
            "/titles/{id}/cover",
            // Дефолтные 2 МиБ не пропустят 5-мегабайтный файл
            get(catalog::cover::get_cover)
                .post(catalog::cover::upload_cover)
                .delete(catalog::cover::delete_cover)
                .layer(DefaultBodyLimit::max(catalog::cover::BODY_LIMIT)),
        )
        .route(
            "/titles/{id}/cover/from-source",
            post(catalog::cover::fetch_source_covers),
        )
        .route(
            "/titles/{id}/sources",
            post(catalog::handlers::attach_source),
        )
        .route(
            "/titles/{id}/sources/{entry_id}",
            delete(catalog::handlers::detach_source),
        )
        .route(
            "/titles/{id}/primary-source",
            put(catalog::handlers::set_primary_source),
        )
        .route(
            "/titles/{title_id}/chapter-aliases",
            get(catalog::handlers::list_aliases).post(catalog::handlers::add_alias),
        )
        .route(
            "/titles/{title_id}/chapter-aliases/bulk",
            post(catalog::handlers::add_aliases),
        )
        .route(
            "/titles/{title_id}/chapter-aliases/{alias_key}",
            delete(catalog::handlers::delete_alias),
        )
        .route(
            "/titles/{title_id}/alias-ranges",
            get(catalog::handlers::list_alias_ranges).post(catalog::handlers::create_alias_range),
        )
        .route(
            "/titles/{title_id}/alias-ranges/apply",
            post(catalog::handlers::apply_alias_ranges),
        )
        .route(
            "/titles/{title_id}/alias-ranges/{range_id}",
            put(catalog::handlers::update_alias_range)
                .delete(catalog::handlers::delete_alias_range),
        )
        .route(
            "/admin/source-priorities",
            get(source_admin::handlers::list_priorities)
                .put(source_admin::handlers::replace_priorities),
        )
        .route(
            "/admin/refresh/{title_id}",
            post(source_admin::handlers::refresh_title),
        )
        .route("/library", get(library::handlers::list_library))
        .route(
            "/library/tabs",
            get(library::handlers::list_tabs).post(library::handlers::create_tab),
        )
        .route(
            "/library/tabs/{id}",
            put(library::handlers::rename_tab).delete(library::handlers::delete_tab),
        )
        .route(
            "/library/{title_id}",
            put(library::handlers::upsert_library).delete(library::handlers::remove_library),
        )
        .route("/discover/search", get(discover::handlers::search))
        .route("/discover/add", discover_add)
        .route(
            "/progress/{title_id}",
            get(progress::get_progress).put(progress::save_progress),
        )
        .route(
            "/titles/{title_id}/chapters/read",
            put(progress::set_chapter_reads),
        )
        .route(
            "/titles/{title_id}/chapters/{chapter_key}/content",
            get(catalog::content::canonical_content),
        )
        .route(
            "/chapters/{id}/content",
            get(catalog::content::direct_content),
        )
        .route("/pages/{id}/image", get(images::handlers::page_image))
        .route("/sources", get(source_admin::handlers::list_sources))
        .route(
            "/sources/{code}",
            put(source_admin::handlers::upsert_source),
        )
        .route(
            "/sources/{code}/config",
            put(source_admin::handlers::save_config),
        )
        .route(
            "/sources/{code}/search",
            post(source_admin::handlers::search),
        )
        .route("/sources/{code}/probe", post(source_admin::handlers::probe))
        .route("/sources/{code}/cover", get(images::handlers::source_cover))
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
