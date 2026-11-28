use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::sources::config::{HostAllowlist, normalize_host_pattern, validate_outbound_url};
use crate::sources::descriptor::{Capability, ConfigField, FieldType, SourceDescriptor};

pub const CODE: &str = "exmanga";

pub const DESCRIPTOR: SourceDescriptor = SourceDescriptor {
    code: CODE,
    name: "ExManga",
    site_url: "https://exmanga.org",
    default_base_url: "https://exmanga.org",
    lang: "ru",
    version: 1,
    capabilities: &[
        Capability::Search,
        Capability::Chapters,
        Capability::Images,
        Capability::Volumes,
        Capability::AltNames,
        // ExManga — закрытый пул платных глав: без входа контент не отдаётся.
        Capability::Auth,
        // Вход по email и паролю: сервер получает JWT и хранит его как
        // обычную Bearer-учётку.
        Capability::Login,
        // Ссылки страниц подписаны и протухают; прокси перезапрашивает
        // `/api/chapter` по 401/403/410.
        Capability::SignedUrls,
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
pub struct ExmangaConfig {
    /// Сайт, а не API: нужен для обложек и относительных ссылок на страницы.
    pub site_url: String,
    /// Хосты картинок: точный домен или `*.example.com`.
    pub image_hosts: Vec<String>,
    /// Язык контента; у ExManga один — русский.
    pub language: String,
}

impl Default for ExmangaConfig {
    fn default() -> Self {
        Self {
            site_url: "https://exmanga.org".to_owned(),
            image_hosts: vec![
                "exmanga.org".to_owned(),
                "*.exmanga.org".to_owned(),
                "mirror.exmanga.org".to_owned(),
            ],
            language: "ru".to_owned(),
        }
    }
}

impl ExmangaConfig {
    pub fn from_value(value: &Value) -> Result<Self, String> {
        let config: Self = serde_json::from_value(value.clone())
            .map_err(|error| format!("invalid config: {error}"))?;

        let site = reqwest::Url::parse(&config.site_url)
            .map_err(|_| "site_url must be an absolute URL".to_owned())?;
        validate_outbound_url(&site)?;
        if site.host_str().is_some_and(is_remanga_host) {
            return Err("site_url must not point to Remanga".to_owned());
        }

        for host in &config.image_hosts {
            normalize_host_pattern(host)?;
            if is_remanga_host(host) {
                return Err(format!("image host `{host}` belongs to Remanga"));
            }
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

/// ExManga — отдельный сервис, а не прокси Remanga: даже ошибочный конфиг
/// администратора не должен направить его на сервер или CDN Remanga.
pub fn is_remanga_host(host: &str) -> bool {
    let host = host
        .trim()
        .trim_start_matches("*.")
        .trim_end_matches('.')
        .to_ascii_lowercase();
    ["remanga.org", "reimg.org", "reimg2.org"]
        .iter()
        .any(|blocked| {
            host == *blocked
                || host
                    .strip_suffix(blocked)
                    .is_some_and(|prefix| prefix.ends_with('.'))
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn rejects_remanga_site_and_image_hosts() {
        for blocked in [
            "https://remanga.org",
            "https://api.remanga.org",
            "https://img3.reimg.org",
        ] {
            let value = json!({ "site_url": blocked });
            assert!(ExmangaConfig::from_value(&value).is_err(), "{blocked}");
        }

        for blocked in ["reimg.org", "*.reimg.org", "*.remanga.org"] {
            let value = json!({ "image_hosts": [blocked] });
            assert!(ExmangaConfig::from_value(&value).is_err(), "{blocked}");
        }

        assert!(
            ExmangaConfig::default()
                .image_hosts
                .iter()
                .all(|host| { !is_remanga_host(host) })
        );
    }
}
