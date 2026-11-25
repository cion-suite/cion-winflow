use std::{
    collections::HashMap,
    sync::{Arc, Mutex as SyncMutex},
};

use reqwest::Client;
use sqlx::SqlitePool;
use tokio::sync::{RwLock, Semaphore};

use crate::auth::limiter::LoginLimiter;
use crate::config::Config;
use crate::error::AppError;
use crate::images::ImageCache;
use crate::sources::{CoverSize, http, registry::SourceRegistry};

const USER_AGENT: &str = concat!(
    "cion-reader/",
    env!("CARGO_PKG_VERSION"),
    " (+self-hosted aggregator)"
);

/// Ключ блокировки записи обложки: один тайтл и один размер.
type CoverLockKey = (i64, CoverSize);
/// Блокировка, которую держат ручная загрузка, автоимпорт и удаление.
type CoverLock = Arc<tokio::sync::Mutex<()>>;

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
    /// Сериализует запись обложки одного размера у тайтла: ручная загрузка,
    /// автоимпорт и удаление не должны перетирать файл и строку друг друга.
    cover_locks: Arc<SyncMutex<HashMap<CoverLockKey, CoverLock>>>,
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
            cover_locks: Arc::new(SyncMutex::new(HashMap::new())),
        })
    }

    /// Блокировка на пару (тайтл, размер обложки). Держится только вокруг
    /// записи файла и строки `title_covers`, а не вокруг сетевого запроса.
    pub fn cover_lock(&self, title_id: i64, size: CoverSize) -> CoverLock {
        let mut locks = self
            .cover_locks
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        locks.entry((title_id, size)).or_default().clone()
    }

    /// Убирает освободившуюся блокировку: иначе карта растёт вместе с числом
    /// тайтлов. Вызывать после `drop(guard)`, пока локальный Arc ещё жив.
    pub fn release_cover_lock(&self, title_id: i64, size: CoverSize) {
        let mut locks = self
            .cover_locks
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if locks
            .get(&(title_id, size))
            .is_some_and(|lock| Arc::strong_count(lock) == 2)
        {
            locks.remove(&(title_id, size));
        }
    }

    /// Пересобирает реестр из БД. Сетевых запросов нет; ошибка БД — 500.
    pub async fn reload_sources(&self) -> Result<(), AppError> {
        let registry = SourceRegistry::load(&self.pool, self.metadata_client.clone()).await?;
        *self.sources.write().await = registry;
        Ok(())
    }
}
