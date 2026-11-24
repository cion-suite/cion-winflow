use std::{env, net::SocketAddr, path::PathBuf};

#[derive(Clone, Debug)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub static_dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("BIND_ADDR must be a socket address like 127.0.0.1:3000: {0}")]
    BindAddr(#[from] std::net::AddrParseError),
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_owned());
        // В проде STATIC_DIR задаёт systemd; в dev dist лежит в корне репозитория
        let static_dir = env::var_os("STATIC_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("dist"));

        Ok(Self {
            bind_addr: bind_addr.parse()?,
            static_dir,
        })
    }
}
