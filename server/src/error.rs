use axum::{
    Json,
    http::{HeaderValue, StatusCode, header::RETRY_AFTER},
    response::{IntoResponse, Response},
};
use serde::Serialize;

/// Единый формат ошибок API: { "code": "...", "message": "..." }.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: String,
}

/// Доменные ошибки API. Внутренние причины логируются, но не попадают в ответ.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("authentication required")]
    Unauthorized,
    #[error("{message}")]
    InvalidCredentials { message: String },
    #[error("{message}")]
    Forbidden { code: &'static str, message: String },
    #[error("{message}")]
    BadRequest { code: &'static str, message: String },
    #[error("{message}")]
    Conflict { code: &'static str, message: String },
    #[error("{message}")]
    NotFound { code: &'static str, message: String },
    #[error("{message}")]
    RateLimited {
        message: String,
        retry_after_secs: u64,
    },
    #[error("database error")]
    Database(#[from] sqlx::Error),
    #[error("io error")]
    Io(#[from] std::io::Error),
    #[error("password hashing error")]
    PasswordHash,
    #[error("random source error")]
    Random,
    #[error("background task error")]
    Task,
    #[error("internal error")]
    Internal,
}

impl AppError {
    pub fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self::BadRequest {
            code,
            message: message.into(),
        }
    }

    pub fn forbidden(code: &'static str, message: impl Into<String>) -> Self {
        Self::Forbidden {
            code,
            message: message.into(),
        }
    }

    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::Conflict {
            code,
            message: message.into(),
        }
    }

    pub fn not_found(code: &'static str, message: impl Into<String>) -> Self {
        Self::NotFound {
            code,
            message: message.into(),
        }
    }

    pub fn invalid_credentials() -> Self {
        Self::InvalidCredentials {
            message: "invalid username or password".to_owned(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message, retry_after) = match self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "authentication required".to_owned(),
                None,
            ),
            Self::InvalidCredentials { message } => (
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                message,
                None,
            ),
            Self::Forbidden { code, message } => (StatusCode::FORBIDDEN, code, message, None),
            Self::BadRequest { code, message } => (StatusCode::BAD_REQUEST, code, message, None),
            Self::Conflict { code, message } => (StatusCode::CONFLICT, code, message, None),
            Self::NotFound { code, message } => (StatusCode::NOT_FOUND, code, message, None),
            Self::RateLimited {
                message,
                retry_after_secs,
            } => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                message,
                Some(retry_after_secs),
            ),
            Self::Database(error) => {
                tracing::error!(%error, "database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "internal server error".to_owned(),
                    None,
                )
            }
            Self::Io(error) => {
                tracing::error!(%error, "io error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "internal server error".to_owned(),
                    None,
                )
            }
            Self::PasswordHash => {
                tracing::error!("password hashing failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "internal server error".to_owned(),
                    None,
                )
            }
            Self::Random => {
                tracing::error!("random source failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "internal server error".to_owned(),
                    None,
                )
            }
            Self::Task => {
                tracing::error!("background task failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "internal server error".to_owned(),
                    None,
                )
            }
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "internal server error".to_owned(),
                None,
            ),
        };

        let mut response = (status, Json(ErrorBody { code, message })).into_response();

        if let Some(seconds) = retry_after
            && let Ok(value) = HeaderValue::from_str(&seconds.to_string())
        {
            response.headers_mut().insert(RETRY_AFTER, value);
        }

        response
    }
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
