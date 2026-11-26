use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::sources::config::{HostAllowlist, normalize_host_pattern};
use crate::sources::descriptor::{Capability, ConfigField, FieldType, SourceDescriptor};

pub const CODE: &str = "mangamir";

/// Сайт отдаёт страницы только браузерному User-Agent; без `Mozilla/5.0`
/// он может ответить заглушкой или ошибкой.
pub const DEFAULT_USER_AGENT: &str = concat!(
    "Mozilla/5.0 (compatible; cion-reader/",
    env!("CARGO_PKG_VERSION"),
    "; +self-hosted aggregator)"
);

/// Mangamir — HTML-источник: `base_url` совпадает с сайтом, поиск, тайтл и
/// главы отдаются обычными страницами, картинки — с `img.mangamir.com`.
pub const DESCRIPTOR: SourceDescriptor = SourceDescriptor {
    code: CODE,
    name: "MangaMir",
    site_url: "https://mangamir.com",
    default_base_url: "https://mangamir.com",
    lang: "ru",
    version: 1,
    capabilities: &[Capability::Search, Capability::Chapters, Capability::Images],
    config: &[
        ConfigField {
            key: "image_hosts",
            kind: FieldType::HostList,
            required: false,
            secret: false,
        },
        ConfigField {
            key: "language",
            kind: FieldType::Select(&["ru"]),
            required: false,
            secret: false,
        },
        ConfigField {
            key: "user_agent",
            kind: FieldType::Text { max_len: 300 },
            required: false,
            secret: false,
        },
    ],
    filters: &[],
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MangamirConfig {
    /// Хосты страниц и обложек. Домен сайта добавляется в адаптере.
    pub image_hosts: Vec<String>,
    /// Язык контента; на Mangamir один — русский.
    pub language: String,
    pub user_agent: String,
}

impl Default for MangamirConfig {
    fn default() -> Self {
        Self {
            image_hosts: vec!["img.mangamir.com".to_owned()],
            language: "ru".to_owned(),
            user_agent: DEFAULT_USER_AGENT.to_owned(),
        }
    }
}

impl MangamirConfig {
    pub fn from_value(value: &Value) -> Result<Self, String> {
        let config: Self = serde_json::from_value(value.clone())
            .map_err(|error| format!("invalid config: {error}"))?;

        for host in &config.image_hosts {
            normalize_host_pattern(host)?;
        }

        if config.language.trim().is_empty() {
            return Err("language must not be empty".to_owned());
        }
        if !config.user_agent.trim().starts_with("Mozilla/5.0") {
            return Err("user_agent must start with `Mozilla/5.0` for Mangamir".to_owned());
        }

        Ok(config)
    }

    /// CDN картинок; `base_url` сайта добавляется в адаптере.
    pub fn image_allowlist(&self) -> Result<HostAllowlist, String> {
        HostAllowlist::new(self.image_hosts.iter().cloned())
    }
}
