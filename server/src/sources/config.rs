//! Валидация `sources.config_json` по схеме дескриптора и allowlist хостов.
//! Конфиг — JSON-объект; неизвестные ключи дают предупреждения, а не падение.

use reqwest::Url;
use serde_json::{Map, Value};

use crate::error::FieldError;
use crate::sources::descriptor::{ConfigField, FieldType, SourceDescriptor};

#[derive(Debug)]
pub struct ValidatedConfig {
    pub value: Value,
    pub warnings: Vec<String>,
}

/// Точный домен или `*.example.com`; голый `*` и URL запрещены.
#[derive(Debug, Clone)]
pub struct HostAllowlist {
    patterns: Vec<String>,
}

impl HostAllowlist {
    pub fn new(patterns: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut normalized = Vec::new();
        for pattern in patterns {
            normalized.push(normalize_host_pattern(&pattern)?);
        }
        Ok(Self {
            patterns: normalized,
        })
    }

    pub fn allows(&self, host: &str) -> bool {
        let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
        if host.is_empty() {
            return false;
        }

        self.patterns.iter().any(|pattern| {
            if let Some(suffix) = pattern.strip_prefix("*.") {
                // `*.example.com` не совпадает с самим example.com
                host.strip_suffix(suffix)
                    .is_some_and(|prefix| prefix.ends_with('.') && prefix.len() > 1)
            } else {
                host == *pattern
            }
        })
    }
}

pub fn normalize_host_pattern(raw: &str) -> Result<String, String> {
    let pattern = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    if pattern.is_empty() || pattern == "*" {
        return Err("host must be a domain like `example.com` or `*.example.com`".to_owned());
    }
    if pattern.len() > 253 {
        return Err("host is too long".to_owned());
    }
    if pattern.contains(['/', '@', ':', ' ', '\\']) {
        return Err("host must not contain a scheme, port, path or userinfo".to_owned());
    }

    let domain = pattern.strip_prefix("*.").unwrap_or(&pattern);
    if domain.is_empty()
        || !domain.contains('.')
        || domain.starts_with('.')
        || domain.ends_with('.')
        || domain.contains("..")
        || !domain.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '.'
        })
    {
        return Err(format!("`{raw}` is not a valid host pattern"));
    }

    Ok(pattern)
}

/// URL для исходящих запросов: только http/https и порты 80/443.
pub fn validate_outbound_url(url: &Url) -> Result<(), String> {
    match url.scheme() {
        "http" | "https" => {}
        other => return Err(format!("scheme `{other}` is not allowed")),
    }
    match url.port() {
        None | Some(80) | Some(443) => {}
        // Произвольный порт на loopback нужен тестовому серверу картинок;
        // в проде loopback всё равно отбрасывает SSRF-резолвер.
        Some(_) if host_is_loopback(url) => {}
        Some(port) => return Err(format!("port {port} is not allowed")),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("userinfo in URL is not allowed".to_owned());
    }
    if url.host_str().is_none() {
        return Err("URL must have a host".to_owned());
    }
    Ok(())
}

fn host_is_loopback(url: &Url) -> bool {
    url.host_str()
        .and_then(|host| host.parse::<std::net::IpAddr>().ok())
        .is_some_and(|ip| ip.is_loopback())
}

pub fn url_allowed(url: &Url, allowlist: &HostAllowlist) -> bool {
    validate_outbound_url(url).is_ok() && url.host_str().is_some_and(|host| allowlist.allows(host))
}

/// Проверяет конфиг по схеме. Ошибки — 422 для `PUT .../config`;
/// неизвестные ключи попадают в `warnings`.
pub fn validate_config(
    descriptor: &SourceDescriptor,
    raw: &Value,
) -> Result<ValidatedConfig, Vec<FieldError>> {
    let object = match raw {
        Value::Null => Map::new(),
        Value::Object(object) => object.clone(),
        _ => {
            return Err(vec![FieldError {
                path: "config".to_owned(),
                message: "config must be a JSON object".to_owned(),
            }]);
        }
    };

    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let known: Vec<&str> = descriptor.config.iter().map(|field| field.key).collect();

    for key in object.keys() {
        if !known.contains(&key.as_str()) {
            warnings.push(format!("unknown config key `{key}` ignored"));
        }
    }

    for field in descriptor.config {
        let Some(value) = object.get(field.key) else {
            if field.required {
                errors.push(FieldError {
                    path: field.key.to_owned(),
                    message: "field is required".to_owned(),
                });
            }
            continue;
        };

        if value.is_null() {
            if field.required {
                errors.push(FieldError {
                    path: field.key.to_owned(),
                    message: "field is required and cannot be null".to_owned(),
                });
            }
            continue;
        }

        if let Err(message) = validate_field(field, value) {
            errors.push(FieldError {
                path: field.key.to_owned(),
                message,
            });
        }
    }

    if errors.is_empty() {
        Ok(ValidatedConfig {
            value: Value::Object(object),
            warnings,
        })
    } else {
        Err(errors)
    }
}

fn validate_field(field: &ConfigField, value: &Value) -> Result<(), String> {
    match field.kind {
        FieldType::Text { max_len } => {
            let text = expect_str(value)?;
            if text.chars().count() > max_len {
                return Err(format!("value must be at most {max_len} characters"));
            }
            Ok(())
        }
        FieldType::Url => {
            let url = parse_url(expect_str(value)?)?;
            validate_outbound_url(&url)
        }
        FieldType::UrlList => {
            let list = expect_list(value)?;
            if list.len() > 20 {
                return Err("list may contain at most 20 URLs".to_owned());
            }
            for item in list {
                let url = parse_url(expect_str(item)?)?;
                validate_outbound_url(&url)?;
            }
            Ok(())
        }
        FieldType::HostList => {
            let list = expect_list(value)?;
            if list.len() > 50 {
                return Err("list may contain at most 50 hosts".to_owned());
            }
            for item in list {
                normalize_host_pattern(expect_str(item)?)?;
            }
            Ok(())
        }
        FieldType::Integer { min, max } => {
            let number = expect_i64(value)?;
            if !(min..=max).contains(&number) {
                return Err(format!("value must be between {min} and {max}"));
            }
            Ok(())
        }
        FieldType::Bool => {
            if value.is_boolean() {
                Ok(())
            } else {
                Err("value must be a boolean".to_owned())
            }
        }
        FieldType::Select(options) => {
            let text = expect_str(value)?;
            if options.contains(&text) {
                Ok(())
            } else {
                Err(format!("value must be one of: {}", options.join(", ")))
            }
        }
        FieldType::MultiSelect(options) => {
            let list = expect_list(value)?;
            for item in list {
                let text = expect_str(item)?;
                if !options.contains(&text) {
                    return Err(format!("value must be one of: {}", options.join(", ")));
                }
            }
            Ok(())
        }
        FieldType::Range { min, max, step } => {
            let number = expect_i64(value)?;
            if !(min..=max).contains(&number) {
                return Err(format!("value must be between {min} and {max}"));
            }
            if step > 0 && (number - min) % step != 0 {
                return Err(format!(
                    "value must be a multiple of {step} starting at {min}"
                ));
            }
            Ok(())
        }
        FieldType::TriState => {
            if value.is_null() || value.is_boolean() {
                Ok(())
            } else {
                Err("value must be true, false or null".to_owned())
            }
        }
    }
}

fn expect_str(value: &Value) -> Result<&str, String> {
    value
        .as_str()
        .ok_or_else(|| "value must be a string".to_owned())
}

fn expect_list(value: &Value) -> Result<&Vec<Value>, String> {
    value
        .as_array()
        .ok_or_else(|| "value must be an array".to_owned())
}

fn expect_i64(value: &Value) -> Result<i64, String> {
    if let Some(number) = value.as_i64() {
        return Ok(number);
    }
    if let Some(number) = value.as_f64()
        && number.fract() == 0.0
        && number >= i64::MIN as f64
        && number <= i64::MAX as f64
    {
        return Ok(number as i64);
    }
    Err("value must be an integer".to_owned())
}

fn parse_url(raw: &str) -> Result<Url, String> {
    Url::parse(raw.trim()).map_err(|_| "value must be an absolute URL".to_owned())
}

/// Маскирует секретные поля: клиент не должен получать cookie и токены.
pub fn mask_config(descriptor: &SourceDescriptor, value: &Value) -> Value {
    let Value::Object(mut object) = value.clone() else {
        return Value::Object(Map::new());
    };

    for field in descriptor.config {
        if field.secret
            && object
                .get(field.key)
                .is_some_and(|value| !value.is_null() && value != "")
        {
            object.insert(field.key.to_owned(), Value::String("***".to_owned()));
        }
    }

    Value::Object(object)
}

/// Семантика PUT: отсутствующее поле — «не менять», null — очистить,
/// `***` в секретном поле — оставить текущее значение.
pub fn merge_config(descriptor: &SourceDescriptor, current: &Value, patch: &Value) -> Value {
    let mut merged = match current {
        Value::Object(object) => object.clone(),
        _ => Map::new(),
    };
    let Value::Object(patch) = patch else {
        return Value::Object(merged);
    };

    for (key, value) in patch {
        let secret = descriptor
            .config
            .iter()
            .any(|field| field.key == key && field.secret);

        if value.is_null() {
            merged.remove(key);
        } else if secret && value.as_str() == Some("***") {
            // Маскированный ответ можно безопасно отправить обратно
        } else {
            merged.insert(key.clone(), value.clone());
        }
    }

    Value::Object(merged)
}

/// Пример конфига для админки и `contract_suite`.
pub fn config_example(descriptor: &SourceDescriptor) -> Value {
    let mut object = Map::new();
    for field in descriptor.config {
        let value = match field.kind {
            FieldType::Text { .. } => Value::String(String::new()),
            FieldType::Url => Value::String("https://example.com".to_owned()),
            FieldType::UrlList => Value::Array(Vec::new()),
            FieldType::HostList => Value::Array(vec![Value::String("example.com".to_owned())]),
            FieldType::Integer { min, .. } => Value::from(min),
            FieldType::Bool => Value::Bool(false),
            FieldType::Select(options) => options
                .first()
                .map_or(Value::Null, |value| Value::String((*value).to_owned())),
            FieldType::MultiSelect(_) => Value::Array(Vec::new()),
            FieldType::Range { min, .. } => Value::from(min),
            FieldType::TriState => Value::Null,
        };
        object.insert(field.key.to_owned(), value);
    }
    Value::Object(object)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_patterns_match_subdomains_only() {
        let list = HostAllowlist::new(["remanga.org".to_owned(), "*.reimg2.org".to_owned()])
            .expect("allowlist");

        assert!(list.allows("remanga.org"));
        assert!(list.allows("img3.reimg2.org"));
        assert!(!list.allows("reimg2.org"));
        assert!(!list.allows("evilreimg2.org"));
        assert!(!list.allows("remanga.org.evil.example"));
    }

    #[test]
    fn host_patterns_reject_unsafe_values() {
        for pattern in ["*", "", "https://x", "x:443", "a@b", "a/b", "no-dot"] {
            assert!(normalize_host_pattern(pattern).is_err(), "{pattern}");
        }
    }

    #[test]
    fn outbound_url_restricts_scheme_and_port() {
        assert!(validate_outbound_url(&Url::parse("https://x.example/a").expect("url")).is_ok());
        assert!(validate_outbound_url(&Url::parse("http://x.example:80/a").expect("url")).is_ok());
        assert!(validate_outbound_url(&Url::parse("ftp://x.example/a").expect("url")).is_err());
        assert!(
            validate_outbound_url(&Url::parse("https://x.example:8443/a").expect("url")).is_err()
        );
        assert!(
            validate_outbound_url(&Url::parse("https://u:p@x.example/").expect("url")).is_err()
        );
    }
}
