use axum::{Json, Router, routing::get};
use serde::Serialize;
use tower_http::{compression::CompressionLayer, trace::TraceLayer};

use crate::config::Config;
use crate::error;
use crate::static_files::serve_static;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self { config }
    }
}

#[derive(Debug, Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

pub fn router(state: AppState) -> Router {
    // /api отделён от статики: неизвестный API-путь даёт JSON 404, а не index.html
    let api = Router::new()
        .route("/health", get(health))
        .fallback(error::api_not_found);

    Router::new()
        .nest("/api", api)
        .fallback(serve_static)
        .with_state(state)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
}
