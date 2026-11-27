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

/// Ошибка поля при валидации конфига источника (422).
#[derive(Debug, Serialize)]
pub struct FieldError {
    pub path: String,
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
    /// Ошибка источника на стороне upstream: 502.
    #[error("{message}")]
    BadGateway { code: &'static str, message: String },
    /// Источник временно недоступен: 503 с необязательным Retry-After.
    #[error("{message}")]
    ServiceUnavailable {
        code: &'static str,
        message: String,
        retry_after_secs: Option<u64>,
    },
    /// Невалидный конфиг: 422 со списком полей.
    #[error("{message}")]
    Unprocessable {
        code: &'static str,
        message: String,
        errors: Vec<FieldError>,
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
    /// Машинный код для ответа и для DTO, где ошибка источника должна
    /// дойти до клиента рядом с текстом (импорт, sync, discovery).
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::InvalidCredentials { .. } => "invalid_credentials",
            Self::Forbidden { code, .. }
            | Self::BadRequest { code, .. }
            | Self::Conflict { code, .. }
            | Self::NotFound { code, .. }
            | Self::BadGateway { code, .. }
            | Self::ServiceUnavailable { code, .. }
            | Self::Unprocessable { code, .. } => code,
            Self::RateLimited { .. } => "rate_limited",
            Self::Database(_)
            | Self::Io(_)
            | Self::PasswordHash
            | Self::Random
            | Self::Task
            | Self::Internal => "internal_error",
        }
    }

    /// Короткий текст для DTO, где ошибка источника возвращается рядом с
    /// машинным кодом. Полное сообщение может быть длиннее лимита задания.
    pub fn short_message(&self) -> String {
        self.to_string().chars().take(200).collect()
    }

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

    pub fn bad_gateway(code: &'static str, message: impl Into<String>) -> Self {
        Self::BadGateway {
            code,
            message: message.into(),
        }
    }

    pub fn service_unavailable(
        code: &'static str,
        message: impl Into<String>,
        retry_after_secs: Option<u64>,
    ) -> Self {
        Self::ServiceUnavailable {
            code,
            message: message.into(),
            retry_after_secs,
        }
    }

    pub fn unprocessable(
        code: &'static str,
        message: impl Into<String>,
        errors: Vec<FieldError>,
    ) -> Self {
        Self::Unprocessable {
            code,
            message: message.into(),
            errors,
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
            Self::BadGateway { code, message } => (StatusCode::BAD_GATEWAY, code, message, None),
            Self::ServiceUnavailable {
                code,
                message,
                retry_after_secs,
            } => (
                StatusCode::SERVICE_UNAVAILABLE,
                code,
                message,
                retry_after_secs,
            ),
            Self::Unprocessable {
                code,
                message,
                errors,
            } => {
                // Валидация конфига отдаёт список полей рядом с обычным конвертом.
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(ValidationBody {
                        code,
                        message,
                        errors,
                    }),
                )
                    .into_response();
            }
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

#[derive(Debug, Serialize)]
struct ValidationBody {
    code: &'static str,
    message: String,
    errors: Vec<FieldError>,
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
