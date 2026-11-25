use crate::error::AppError;

/// Ошибки адаптера. Текст не содержит чужих URL, тел ответов и секретов:
/// детали уходят в лог, клиент видит `code` и короткое сообщение.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("source does not support kind `{0}`")]
    UnsupportedKind(String),
    #[error("source does not support this operation")]
    UnsupportedCapability,
    #[error("invalid search filter: {0}")]
    BadFilter(String),
    #[error("invalid external id")]
    InvalidExternalId,
    #[error("not found at source")]
    NotFound,
    #[error("authentication is required at source")]
    AuthRequired,
    /// 403 от сайта: чаще всего Cloudflare или блокировка по региону.
    #[error("source blocked the request")]
    Blocked,
    #[error("source rate limited the request")]
    RateLimited { retry_after_secs: Option<u64> },
    #[error("upstream request failed (status {status:?})")]
    Http { status: Option<u16> },
    #[error("cannot parse source response at `{stage}`: {detail}")]
    Parse { stage: &'static str, detail: String },
    #[error("chapter is locked at source")]
    Locked,
    #[error("source is disabled")]
    Disabled,
    #[error("network error")]
    Network,
    /// Circuit breaker открыт: источник недавно падал, запросы временно не идут.
    #[error("source circuit is open")]
    CircuitOpen,
    #[error("source configuration is invalid: {0}")]
    Config(String),
}

impl SourceError {
    /// Короткий машинный код причины для ответа `fallback`.
    pub fn reason(&self) -> &'static str {
        match self {
            Self::UnsupportedKind(_) | Self::UnsupportedCapability | Self::BadFilter(_) => {
                "bad_request"
            }
            Self::InvalidExternalId => "invalid_external_id",
            Self::NotFound => "not_found",
            Self::AuthRequired => "auth_required",
            Self::Blocked => "blocked",
            Self::RateLimited { .. } => "rate_limited",
            Self::Http { .. } | Self::Network => "upstream_failed",
            Self::Parse { .. } => "parse_failed",
            Self::Locked => "locked",
            Self::Disabled => "disabled",
            Self::CircuitOpen => "circuit_open",
            Self::Config(_) => "config",
        }
    }

    /// Временная ошибка не меняет метаданные доступности главы: глава
    /// могла быть доступна секунду назад, а упал сам запрос.
    pub fn is_temporary(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. }
                | Self::Http { .. }
                | Self::Network
                | Self::CircuitOpen
                | Self::Blocked
                | Self::Parse { .. }
        )
    }
}

impl From<SourceError> for AppError {
    fn from(error: SourceError) -> Self {
        match error {
            SourceError::UnsupportedKind(kind) => Self::bad_request(
                "source_unsupported_kind",
                format!("source does not support kind `{kind}`"),
            ),
            SourceError::UnsupportedCapability => Self::bad_request(
                "source_bad_request",
                "source does not support this operation",
            ),
            SourceError::BadFilter(message) => Self::bad_request("source_bad_request", message),
            SourceError::InvalidExternalId => {
                Self::bad_request("source_invalid_external_id", "external id is invalid")
            }
            SourceError::NotFound => Self::not_found("source_not_found", "not found at source"),
            SourceError::Locked => Self::forbidden("source_locked", "chapter is locked at source"),
            SourceError::AuthRequired => {
                Self::bad_gateway("source_auth_required", "source requires authentication")
            }
            SourceError::Blocked => {
                Self::bad_gateway("source_upstream_failed", "source blocked the request")
            }
            SourceError::RateLimited { retry_after_secs } => Self::service_unavailable(
                "source_rate_limited",
                "source rate limited the request",
                // Без заголовка источника клиенту нужен разумный интервал
                Some(retry_after_secs.unwrap_or(60)),
            ),
            SourceError::Http { .. } | SourceError::Network | SourceError::CircuitOpen => {
                Self::bad_gateway("source_upstream_failed", "source request failed")
            }
            SourceError::Parse { .. } => {
                Self::bad_gateway("source_parse_failed", "cannot parse source response")
            }
            SourceError::Disabled => {
                Self::service_unavailable("source_disabled", "source is disabled", None)
            }
            SourceError::Config(detail) => {
                tracing::warn!(detail, "source configuration error");
                Self::service_unavailable(
                    "source_disabled",
                    "source configuration is invalid",
                    None,
                )
            }
        }
    }
}
