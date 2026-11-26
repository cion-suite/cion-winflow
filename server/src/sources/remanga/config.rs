use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::sources::config::{HostAllowlist, normalize_host_pattern, validate_outbound_url};
use crate::sources::descriptor::{Capability, ConfigField, FieldType, SourceDescriptor};

pub const CODE: &str = "remanga";

pub const DESCRIPTOR: SourceDescriptor = SourceDescriptor {
    code: CODE,
    name: "Remanga",
    site_url: "https://remanga.org",
    default_base_url: "https://api.remanga.org",
    lang: "ru",
    version: 1,
    capabilities: &[
        Capability::Search,
        Capability::Chapters,
        Capability::Images,
        Capability::Volumes,
        Capability::AltNames,
    ],
    config: &[
        ConfigField {
            key: "site_url",
            kind: FieldType::Url,
            required: false,
            secret: false,
        },
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
    ],
    filters: &[],
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RemangaConfig {
    /// Сайт, а не API: нужен для обложек и ссылок «открыть на источнике».
    pub site_url: String,
    /// Хосты картинок: точный домен или `*.example.com`.
    pub image_hosts: Vec<String>,
    /// Язык контента; у Remanga один — русский.
    pub language: String,
}

impl Default for RemangaConfig {
    fn default() -> Self {
        Self {
            site_url: "https://remanga.org".to_owned(),
            image_hosts: vec![
                "*.reimg.org".to_owned(),
                "*.reimg2.org".to_owned(),
                "remanga.org".to_owned(),
                "*.remanga.org".to_owned(),
            ],
            language: "ru".to_owned(),
        }
    }
}

impl RemangaConfig {
    pub fn from_value(value: &Value) -> Result<Self, String> {
        let config: Self = serde_json::from_value(value.clone())
            .map_err(|error| format!("invalid config: {error}"))?;

        let site = reqwest::Url::parse(&config.site_url)
            .map_err(|_| "site_url must be an absolute URL".to_owned())?;
        validate_outbound_url(&site)?;

        for host in &config.image_hosts {
            normalize_host_pattern(host)?;
        }

        if config.language.trim().is_empty() {
            return Err("language must not be empty".to_owned());
        }

        Ok(config)
    }

    pub fn site(&self) -> Result<reqwest::Url, String> {
        let mut site = reqwest::Url::parse(&self.site_url)
            .map_err(|_| "site_url must be an absolute URL".to_owned())?;
        if !site.path().ends_with('/') {
            let path = format!("{}/", site.path());
            site.set_path(&path);
        }
        Ok(site)
    }

    /// Сайт и CDN картинок. `base_url` API добавляется в адаптере.
    pub fn image_allowlist(&self) -> Result<HostAllowlist, String> {
        HostAllowlist::new(self.image_hosts.iter().cloned())
    }
}
