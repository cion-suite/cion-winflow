use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

/// Единый формат ошибок API: { "code": "...", "message": "..." }.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: String,
}

pub async fn api_not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorBody {
            code: "not_found",
            message: "unknown API route".to_owned(),
        }),
    )
        .into_response()
}
