use std::{env, net::IpAddr, net::SocketAddr, path::PathBuf};

const DEV_ORIGINS: &[&str] = &[
    "http://localhost:1420",
    "http://127.0.0.1:1420",
    "http://localhost:3000",
    "http://127.0.0.1:3000",
];

#[derive(Clone, Debug)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub static_dir: PathBuf,
    pub database_url: String,
    /// Каталог данных: обложки, кэш изображений, аватары.
    pub data_dir: PathBuf,
    pub registration_mode: RegistrationMode,
    /// Origins, которым разрешены мутирующие запросы (CSRF).
    pub allowed_origins: Vec<String>,
    pub cookie_secure: bool,
    /// Прокси, чей X-Forwarded-For можно доверять (Caddy на loopback в проде).
    pub trusted_proxies: Vec<IpAddr>,
    /// Одноразовый bootstrap администратора при пустой таблице users.
    pub admin_bootstrap: Option<AdminBootstrap>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegistrationMode {
    Invite,
    Closed,
}

#[derive(Clone)]
pub struct AdminBootstrap {
    pub username: String,
    pub password: String,
}

// Пароль не должен попасть в логи через Debug конфига.
impl std::fmt::Debug for AdminBootstrap {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AdminBootstrap")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("BIND_ADDR must be a socket address like 127.0.0.1:3000: {0}")]
    BindAddr(#[from] std::net::AddrParseError),
    #[error("REGISTRATION_MODE must be `invite` or `closed`, got `{0}`")]
    RegistrationMode(String),
    #[error("COOKIE_SECURE must be true or false, got `{0}`")]
    CookieSecure(String),
    #[error("TRUSTED_PROXIES must be a comma-separated list of IP addresses: `{0}` is not an IP")]
    TrustedProxy(String),
    #[error("ADMIN_USERNAME and ADMIN_PASSWORD must be set together")]
    AdminBootstrapIncomplete,
    #[error("origin `{0}` must be an absolute http(s) origin without a path")]
    AllowedOrigin(String),
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let bind_addr = env::var("BIND_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:3000".to_owned())
            .parse()?;
        // В проде STATIC_DIR задаёт systemd; в dev dist лежит в корне репозитория
        let static_dir = env::var_os("STATIC_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("dist"));
        let database_url =
            env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://data/cion.db".to_owned());
        let data_dir = env::var_os("DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("data"));

        let registration_mode = match env::var("REGISTRATION_MODE")
            .unwrap_or_else(|_| "invite".to_owned())
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "invite" => RegistrationMode::Invite,
            "closed" => RegistrationMode::Closed,
            other => return Err(ConfigError::RegistrationMode(other.to_owned())),
        };

        let cookie_secure = parse_bool(env::var("COOKIE_SECURE").ok())?;
        let trusted_proxies = parse_ip_list(env::var("TRUSTED_PROXIES").ok())?;
        let allowed_origins = allowed_origins_from_env()?;

        let admin_bootstrap = match (env::var("ADMIN_USERNAME"), env::var("ADMIN_PASSWORD")) {
            (Ok(username), Ok(password)) => Some(AdminBootstrap { username, password }),
            (Err(_), Err(_)) => None,
            _ => return Err(ConfigError::AdminBootstrapIncomplete),
        };

        Ok(Self {
            bind_addr,
            static_dir,
            database_url,
            data_dir,
            registration_mode,
            allowed_origins,
            cookie_secure,
            trusted_proxies,
            admin_bootstrap,
        })
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], 3000)),
            static_dir: PathBuf::from("dist"),
            database_url: "sqlite://data/cion.db".to_owned(),
            data_dir: PathBuf::from("data"),
            registration_mode: RegistrationMode::Invite,
            allowed_origins: DEV_ORIGINS
                .iter()
                .map(|origin| (*origin).to_owned())
                .collect(),
            cookie_secure: false,
            trusted_proxies: Vec::new(),
            admin_bootstrap: None,
        }
    }
}

fn parse_bool(value: Option<String>) -> Result<bool, ConfigError> {
    match value.as_deref().map(str::trim) {
        None | Some("") => Ok(false),
        Some(value) if value.eq_ignore_ascii_case("true") || value == "1" => Ok(true),
        Some(value) if value.eq_ignore_ascii_case("false") || value == "0" => Ok(false),
        Some(value) => Err(ConfigError::CookieSecure(value.to_owned())),
    }
}

fn parse_ip_list(value: Option<String>) -> Result<Vec<IpAddr>, ConfigError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };

    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<IpAddr>()
                .map_err(|_| ConfigError::TrustedProxy(part.to_owned()))
        })
        .collect()
}

fn allowed_origins_from_env() -> Result<Vec<String>, ConfigError> {
    let mut origins = Vec::new();
    collect_origin(env::var("PUBLIC_ORIGIN").ok(), &mut origins)?;
    if let Ok(value) = env::var("ALLOWED_ORIGINS") {
        for part in value.split(',') {
            collect_origin(Some(part.to_owned()), &mut origins)?;
        }
    }

    // В release без PUBLIC_ORIGIN мутации запрещены: fail closed, а не тихо
    // разрешаем dev-адреса на проде.
    if origins.is_empty() && cfg!(debug_assertions) {
        origins.extend(DEV_ORIGINS.iter().map(|origin| (*origin).to_owned()));
    }

    Ok(origins)
}

fn collect_origin(value: Option<String>, origins: &mut Vec<String>) -> Result<(), ConfigError> {
    let Some(value) = value else {
        return Ok(());
    };
    let origin = value.trim().trim_end_matches('/');
    if origin.is_empty() {
        return Ok(());
    }

    let rest = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"));
    let Some(rest) = rest else {
        return Err(ConfigError::AllowedOrigin(origin.to_owned()));
    };
    if rest.is_empty() || rest.contains('/') {
        return Err(ConfigError::AllowedOrigin(origin.to_owned()));
    }

    if !origins.iter().any(|existing| existing == origin) {
        origins.push(origin.to_owned());
    }
    Ok(())
}
