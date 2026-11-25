use std::sync::Arc;

use reqwest::Client;
use sqlx::SqlitePool;
use tokio::sync::{RwLock, Semaphore};

use crate::auth::limiter::LoginLimiter;
use crate::config::Config;
use crate::error::AppError;
use crate::images::ImageCache;
use crate::sources::{http, registry::SourceRegistry};

const USER_AGENT: &str = concat!(
    "cion-reader/",
    env!("CARGO_PKG_VERSION"),
    " (+self-hosted aggregator)"
);

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub pool: SqlitePool,
    /// Ограничивает одновременные Argon2-операции, чтобы они не выедали CPU.
    pub password_semaphore: Arc<Semaphore>,
    /// Ограничивает одновременное декодирование и перекодирование картинок.
    pub image_semaphore: Arc<Semaphore>,
    pub login_limiter: Arc<LoginLimiter>,
    /// Скомпилированные адаптеры и их состояние; пересобирается после
    /// изменения строк `sources`, guard никогда не держится через `.await`.
    pub sources: Arc<RwLock<SourceRegistry>>,
    /// Исходящие запросы метаданных: общий пул, SSRF-резолвер, таймауты.
    pub metadata_client: Arc<Client>,
    /// Исходящие запросы картинок: без редиректов, отдельный пул.
    pub image_client: Arc<Client>,
    /// Дисковый кэш страниц и обложек.
    pub image_cache: Arc<ImageCache>,
    /// Пул скачивания страниц: не больше 16 одновременных исходящих картинок.
    pub image_download_semaphore: Arc<Semaphore>,
}

impl AppState {
    pub async fn new(config: Config, pool: SqlitePool) -> Result<Self, AppError> {
        let metadata_client = Arc::new(http::metadata_client(
            USER_AGENT,
            config.allow_loopback_images,
        )?);
        let image_client = Arc::new(http::image_client(
            config.allow_loopback_images,
            USER_AGENT,
        )?);
        let registry = SourceRegistry::load(&pool, metadata_client.clone()).await?;
        let image_cache = Arc::new(ImageCache::new(&config.data_dir));

        Ok(Self {
            config,
            pool,
            password_semaphore: Arc::new(Semaphore::new(2)),
            image_semaphore: Arc::new(Semaphore::new(2)),
            login_limiter: Arc::new(LoginLimiter::default()),
            sources: Arc::new(RwLock::new(registry)),
            metadata_client,
            image_client,
            image_cache,
            image_download_semaphore: Arc::new(Semaphore::new(16)),
        })
    }

    /// Пересобирает реестр из БД. Сетевых запросов нет; ошибка БД — 500.
    pub async fn reload_sources(&self) -> Result<(), AppError> {
        let registry = SourceRegistry::load(&self.pool, self.metadata_client.clone()).await?;
        *self.sources.write().await = registry;
        Ok(())
    }
}
