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
    /// 404 без учётки у источника, который скрывает часть тайтлов без входа
    /// (Remanga и 18+ тайтлы). С учёткой это тот же 404, но окончательный.
    #[error("authentication may be required at source")]
    AuthMaybeRequired,
    /// Источник отклонил логин/пароль. Текст ответа источника наружу не
    /// уходит: он может содержать чужие детали и не переводится.
    #[error("source rejected the login")]
    LoginFailed,
    /// У аккаунта включён второй фактор. Методы показываем в сообщении,
    /// структурное поле появится вместе с вводом кода в фазе 5б.
    #[error("two-factor authentication is required at source")]
    TwoFactorRequired { methods: Vec<String> },
    /// 403 от сайта: чаще всего Cloudflare или блокировка по региону.
    #[error("source blocked the request")]
    Blocked,
    #[error("source rate limited the request")]
    RateLimited { retry_after_secs: Option<u64> },
    #[error("upstream request failed (status {status:?})")]
    Http { status: Option<u16> },
    /// Редирект без следования: адаптер сам решает, это locked, missing
    /// или обычная ошибка. Location остаётся в варианте, но не в тексте.
    #[error("source redirected the request")]
    Redirect { location: String },
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
            Self::AuthRequired | Self::AuthMaybeRequired => "auth_required",
            Self::LoginFailed => "login_failed",
            Self::TwoFactorRequired { .. } => "two_factor_required",
            Self::Blocked => "blocked",
            Self::RateLimited { .. } => "rate_limited",
            Self::Http { .. } | Self::Redirect { .. } | Self::Network => "upstream_failed",
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
                | Self::Redirect { .. }
                | Self::Network
                | Self::CircuitOpen
                | Self::Blocked
                | Self::Parse { .. }
        )
    }
}

impl From<SourceError> for AppError {
    fn from(error: SourceError) -> Self {
        // Без контекста учётки считаем, что её не было: это безопаснее, чем
        // показать «учётка отклонена» пользователю, который её не подключал.
        error.into_app(false)
    }
}

impl SourceError {
    /// Маппинг с учётом того, была ли учётка у действующего пользователя.
    /// Один и тот же 401/404 означает разные вещи: «подключите аккаунт» и
    /// «сохранённый аккаунт отклонён». HTTP 401 наружу не отдаём никогда:
    /// фронтенд принял бы его за истёкшую сессию и разлогинил пользователя.
    pub fn into_app(self, auth_present: bool) -> AppError {
        match self {
            Self::AuthRequired if auth_present => {
                AppError::bad_gateway("source_auth_invalid", "source rejected the saved account")
            }
            Self::AuthRequired => AppError::forbidden(
                "source_auth_not_configured",
                "source requires an account; connect it in settings",
            ),
            // С учёткой неоднозначный 404 становится окончательным.
            Self::AuthMaybeRequired if auth_present => {
                AppError::not_found("source_not_found", "not found at source")
            }
            Self::AuthMaybeRequired => AppError::forbidden(
                "source_auth_maybe_required",
                "the title may require an account at this source",
            ),
            Self::LoginFailed => {
                AppError::bad_request("source_login_failed", "source rejected the login")
            }
            Self::TwoFactorRequired { methods } => {
                let message = if methods.is_empty() {
                    "two-factor authentication is required at source".to_owned()
                } else {
                    format!(
                        "two-factor authentication is required at source; methods: {}",
                        methods.join(", ")
                    )
                };
                AppError::conflict("source_login_2fa_required", message)
            }
            Self::UnsupportedKind(kind) => AppError::bad_request(
                "source_unsupported_kind",
                format!("source does not support kind `{kind}`"),
            ),
            Self::UnsupportedCapability => AppError::bad_request(
                "source_bad_request",
                "source does not support this operation",
            ),
            Self::BadFilter(message) => AppError::bad_request("source_bad_request", message),
            Self::InvalidExternalId => {
                AppError::bad_request("source_invalid_external_id", "external id is invalid")
            }
            Self::NotFound => AppError::not_found("source_not_found", "not found at source"),
            Self::Locked => AppError::forbidden("source_locked", "chapter is locked at source"),
            Self::Blocked => AppError::bad_gateway("source_blocked", "source blocked the request"),
            Self::RateLimited { retry_after_secs } => AppError::service_unavailable(
                "source_rate_limited",
                "source rate limited the request",
                // Без заголовка источника клиенту нужен разумный интервал
                Some(retry_after_secs.unwrap_or(60)),
            ),
            Self::Http { .. } | Self::Redirect { .. } | Self::Network | Self::CircuitOpen => {
                AppError::bad_gateway("source_upstream_failed", "source request failed")
            }
            Self::Parse { .. } => {
                AppError::bad_gateway("source_parse_failed", "cannot parse source response")
            }
            Self::Disabled => {
                AppError::service_unavailable("source_disabled", "source is disabled", None)
            }
            Self::Config(detail) => {
                tracing::warn!(detail, "source configuration error");
                AppError::service_unavailable(
                    "source_disabled",
                    "source configuration is invalid",
                    None,
                )
            }
        }
    }
}
