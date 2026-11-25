//! Дисковый кэш картинок. Ключ не зависит от подписанного URL, запись
//! атомарная, одновременные запросы одного ключа дедуплицируются.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::AppError;

#[derive(Debug)]
pub struct CachedImage {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
    pub etag: String,
}

#[derive(Serialize, Deserialize)]
struct Meta {
    content_type: String,
}

pub struct ImageCache {
    root: PathBuf,
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl ImageCache {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join("cache").join("images"),
            locks: Mutex::new(HashMap::new()),
        }
    }

    pub async fn get(&self, key: &str) -> Option<CachedImage> {
        let hash = digest(key);
        let (_, bin, meta) = self.paths(&hash);

        let meta_bytes = tokio::fs::read(&meta).await.ok()?;
        let meta: Meta = serde_json::from_slice(&meta_bytes).ok()?;
        let content_type = content_type_for(&meta.content_type)?;
        let bytes = tokio::fs::read(&bin).await.ok()?;
        if bytes.is_empty() {
            return None;
        }

        Some(CachedImage {
            bytes,
            content_type,
            etag: format!("\"{}\"", &hash[..32]),
        })
    }

    pub async fn store(&self, key: &str, bytes: Vec<u8>) -> Result<CachedImage, AppError> {
        let Some(content_type) = detect_image_type(&bytes) else {
            return Err(AppError::bad_gateway(
                "source_image_invalid",
                "source returned a non-image or unsupported format",
            ));
        };

        let hash = digest(key);
        let (directory, bin, meta) = self.paths(&hash);
        tokio::fs::create_dir_all(&directory).await?;

        let temporary = directory.join(format!(
            ".{hash}.{}.tmp",
            crate::auth::session::new_secret()?
        ));
        tokio::fs::write(&temporary, &bytes).await?;
        tokio::fs::rename(&temporary, &bin).await?;

        // Метаданные пишутся последними: файл без них считается промахом
        let meta_temporary = directory.join(format!(
            ".{hash}.meta.{}.tmp",
            crate::auth::session::new_secret()?
        ));
        let meta_body = serde_json::to_vec(&Meta {
            content_type: content_type.to_owned(),
        })
        .map_err(|_| AppError::Internal)?;
        tokio::fs::write(&meta_temporary, meta_body).await?;
        tokio::fs::rename(&meta_temporary, &meta).await?;

        Ok(CachedImage {
            bytes,
            content_type,
            etag: format!("\"{}\"", &hash[..32]),
        })
    }

    pub fn flight(&self, key: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self
            .locks
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        locks
            .entry(key.to_owned())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    /// Убирает освободившийся ключ: без этого карта растёт вместе с числом глав.
    pub fn release(&self, key: &str) {
        let mut locks = self
            .locks
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if locks
            .get(key)
            .is_some_and(|lock| Arc::strong_count(lock) == 2)
        {
            locks.remove(key);
        }
    }

    fn paths(&self, hash: &str) -> (PathBuf, PathBuf, PathBuf) {
        let directory = self.root.join(&hash[..2]);
        let bin = directory.join(format!("{hash}.bin"));
        let meta = directory.join(format!("{hash}.json"));
        (directory, bin, meta)
    }
}

pub fn digest(key: &str) -> String {
    hex::encode(Sha256::digest(key.as_bytes()))
}

/// Проверка магических байтов: SVG, HTML и прочий текст не станут картинкой.
pub fn detect_image_type(bytes: &[u8]) -> Option<&'static str> {
    match image::guess_format(bytes).ok()? {
        image::ImageFormat::Jpeg => Some("image/jpeg"),
        image::ImageFormat::Png => Some("image/png"),
        image::ImageFormat::WebP => Some("image/webp"),
        image::ImageFormat::Gif => Some("image/gif"),
        _ => None,
    }
}

fn content_type_for(value: &str) -> Option<&'static str> {
    match value {
        "image/jpeg" => Some("image/jpeg"),
        "image/png" => Some("image/png"),
        "image/webp" => Some("image/webp"),
        "image/gif" => Some("image/gif"),
        _ => None,
    }
}
