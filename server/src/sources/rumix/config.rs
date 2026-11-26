use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::sources::config::{HostAllowlist, normalize_host_pattern};
use crate::sources::descriptor::{Capability, ConfigField, FieldType, SourceDescriptor};

pub const CODE: &str = "rumix";

/// RuMix отвечает 402 на UA без префикса `Mozilla/5.0`, поэтому
/// представляемся честно, но в совместимом формате.
pub const DEFAULT_USER_AGENT: &str = concat!(
    "Mozilla/5.0 (compatible; cion-reader/",
    env!("CARGO_PKG_VERSION"),
    "; +self-hosted aggregator)"
);

/// RuMix — HTML-источник на движке Grouple. `base_url` — сам сайт
/// (в отличие от Remanga, где это API-хост): поиск, тайтл и главы
/// отдаются HTML-страницами.
pub const DESCRIPTOR: SourceDescriptor = SourceDescriptor {
    code: CODE,
    name: "RuMix",
    site_url: "https://rumix.me",
    default_base_url: "https://rumix.me",
    lang: "ru",
    version: 1,
    capabilities: &[
        Capability::Search,
        Capability::Chapters,
        Capability::Images,
        Capability::Volumes,
        Capability::AltNames,
        // Картинки отдаются по подписанным ссылкам, которые протухают.
        Capability::SignedUrls,
    ],
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
pub struct RumixConfig {
    /// Хосты картинок: `fs1.rumix.me`, `fs3.rumix.me`, `static.rumix.me`.
    pub image_hosts: Vec<String>,
    /// Язык контента; на RuMix один — русский.
    pub language: String,
    /// Сайт отсекает UA без `Mozilla/5.0`, поэтому это настройка, а не константа.
    pub user_agent: String,
}

impl Default for RumixConfig {
    fn default() -> Self {
        Self {
            image_hosts: vec!["*.rumix.me".to_owned(), "rumix.me".to_owned()],
            language: "ru".to_owned(),
            user_agent: DEFAULT_USER_AGENT.to_owned(),
        }
    }
}

impl RumixConfig {
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
            return Err("user_agent must start with `Mozilla/5.0` for RuMix".to_owned());
        }

        Ok(config)
    }

    /// Сайт и CDN картинок. `base_url` сайта добавляется в адаптере.
    pub fn image_allowlist(&self) -> Result<HostAllowlist, String> {
        HostAllowlist::new(self.image_hosts.iter().cloned())
    }
}
