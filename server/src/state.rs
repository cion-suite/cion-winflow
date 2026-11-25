use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::Semaphore;

use crate::auth::limiter::LoginLimiter;
use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub pool: SqlitePool,
    /// Ограничивает одновременные Argon2-операции, чтобы они не выедали CPU.
    pub password_semaphore: Arc<Semaphore>,
    pub login_limiter: Arc<LoginLimiter>,
}

impl AppState {
    pub fn new(config: Config, pool: SqlitePool) -> Self {
        Self {
            config,
            pool,
            password_semaphore: Arc::new(Semaphore::new(2)),
            login_limiter: Arc::new(LoginLimiter::default()),
        }
    }
}
