//! Пара учётных данных источника: `Authorization: Bearer` и/или `Cookie`.
//!
//! Значения хранятся в [`SecretString`]: `Debug` печатает только факт
//! наличия поля, поэтому случайный лог не раскрывает токен.

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

/// Расшифрованные учётные данные, которые передаются адаптеру источника.
/// Пустые строки нормализуются в `None`: отправлять пустой заголовок нельзя.
#[derive(Clone)]
pub struct SourceAuth {
    bearer: Option<SecretString>,
    cookie: Option<SecretString>,
}

/// Формат плейнтекста внутри шифротекста. Отдельная структура, чтобы схема
/// была явной и переживала добавление полей.
#[derive(Debug, Serialize, Deserialize)]
struct Payload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bearer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cookie: Option<String>,
}

impl SourceAuth {
    pub fn new(bearer: Option<String>, cookie: Option<String>) -> Self {
        Self {
            bearer: normalize(bearer),
            cookie: normalize(cookie),
        }
    }

    pub fn bearer(&self) -> Option<&str> {
        self.bearer.as_ref().map(|value| value.expose_secret())
    }

    pub fn cookie(&self) -> Option<&str> {
        self.cookie.as_ref().map(|value| value.expose_secret())
    }

    pub fn is_empty(&self) -> bool {
        self.bearer.is_none() && self.cookie.is_none()
    }

    /// Плейнтекст для шифрования. Вызывающий оборачивает результат в
    /// `Zeroizing`, чтобы буфер обнулился сразу после `seal`.
    pub fn to_payload_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&Payload {
            bearer: self.bearer().map(str::to_owned),
            cookie: self.cookie().map(str::to_owned),
        })
    }

    pub fn from_payload_json(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        let payload: Payload = serde_json::from_slice(bytes)?;
        Ok(Self::new(payload.bearer, payload.cookie))
    }
}

fn normalize(value: Option<String>) -> Option<SecretString> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(SecretString::from)
}

impl std::fmt::Debug for SourceAuth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SourceAuth")
            .field("bearer", &self.bearer.is_some())
            .field("cookie", &self.cookie.is_some())
            .finish()
    }
}
