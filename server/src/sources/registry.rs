//! Реестр скомпилированных источников и их состояния из БД.
//! Собирается при старте и пересобирается после изменения `sources`.

use std::{collections::HashMap, sync::Arc};

use reqwest::{Client, Url};
use serde_json::Value;
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::sources::{
    SourceKind,
    config::validate_outbound_url,
    descriptor::SourceDescriptor,
    error::SourceError,
    http::AdapterCtx,
    remanga::{self, RemangaConfig},
};

#[derive(Debug, sqlx::FromRow)]
struct SourceRow {
    id: i64,
    code: String,
    base_url: Option<String>,
    enabled: bool,
    config_json: String,
    last_checked_at: Option<i64>,
    last_error: Option<String>,
}

pub struct SourceRuntime {
    pub id: i64,
    pub code: String,
    pub descriptor: &'static SourceDescriptor,
    pub base_url: Option<String>,
    pub enabled: bool,
    pub config: Value,
    pub config_error: Option<String>,
    pub last_checked_at: Option<i64>,
    pub last_error: Option<String>,
    /// None — источник не настроен или конфиг битый.
    pub adapter: Option<SourceKind>,
}

impl SourceRuntime {
    /// Адаптер для запроса: отключённый источник отвечает `source_disabled`,
    /// а не молча ходит в сеть.
    pub fn adapter(&self) -> Result<&SourceKind, SourceError> {
        if !self.enabled {
            return Err(SourceError::Disabled);
        }
        self.adapter_unchecked()
    }

    /// Проверка конфига и probe работают и на выключенном источнике:
    /// администратор должен увидеть, рабочий ли он, до включения.
    pub fn adapter_unchecked(&self) -> Result<&SourceKind, SourceError> {
        if let Some(message) = &self.config_error {
            return Err(SourceError::Config(message.clone()));
        }
        self.adapter
            .as_ref()
            .ok_or_else(|| SourceError::Config("source is not configured".to_owned()))
    }
}

pub struct SourceRegistry {
    descriptors: Vec<&'static SourceDescriptor>,
    entries: HashMap<String, Arc<SourceRuntime>>,
    by_id: HashMap<i64, Arc<SourceRuntime>>,
}

impl SourceRegistry {
    pub async fn load(pool: &SqlitePool, client: Arc<Client>) -> Result<Self, AppError> {
        let rows = sqlx::query_as::<_, SourceRow>(
            "SELECT id, code, base_url, enabled, config_json, \
                    last_checked_at, last_error \
             FROM sources",
        )
        .fetch_all(pool)
        .await?;

        let descriptors = descriptors();
        let mut entries = HashMap::new();
        let mut by_id = HashMap::new();

        for row in rows {
            let Some(descriptor) = descriptors
                .iter()
                .copied()
                .find(|item| item.code == row.code)
            else {
                tracing::warn!(code = %row.code, "database has a source without a compiled adapter");
                continue;
            };

            let runtime = build_runtime(row, descriptor, client.clone());
            let runtime = Arc::new(runtime);
            by_id.insert(runtime.id, runtime.clone());
            entries.insert(runtime.code.clone(), runtime);
        }

        Ok(Self {
            descriptors,
            entries,
            by_id,
        })
    }

    pub fn descriptors(&self) -> &[&'static SourceDescriptor] {
        &self.descriptors
    }

    pub fn descriptor(&self, code: &str) -> Option<&'static SourceDescriptor> {
        self.descriptors
            .iter()
            .copied()
            .find(|item| item.code == code)
    }

    pub fn runtime(&self, code: &str) -> Option<Arc<SourceRuntime>> {
        self.entries.get(code).cloned()
    }

    pub fn runtime_by_id(&self, source_id: i64) -> Option<Arc<SourceRuntime>> {
        self.by_id.get(&source_id).cloned()
    }

    pub fn entries(&self) -> impl Iterator<Item = &Arc<SourceRuntime>> {
        self.entries.values()
    }
}

fn build_runtime(
    row: SourceRow,
    descriptor: &'static SourceDescriptor,
    client: Arc<Client>,
) -> SourceRuntime {
    let config = serde_json::from_str::<Value>(&row.config_json)
        .unwrap_or_else(|_| Value::Object(serde_json::Map::new()));

    let has_base_url = row.base_url.is_some();
    let (adapter, build_error) = match row.base_url.as_deref() {
        None => (None, None),
        Some(base) => match build_adapter(&row.code, base, &config, client) {
            Ok(adapter) => (Some(adapter), None),
            Err(detail) => (None, Some(detail)),
        },
    };

    SourceRuntime {
        id: row.id,
        code: row.code,
        descriptor,
        base_url: row.base_url,
        // Битый конфиг или отсутствие base_url отключают только свой источник
        enabled: row.enabled && build_error.is_none() && has_base_url,
        config,
        config_error: build_error,
        last_checked_at: row.last_checked_at,
        last_error: row.last_error,
        adapter,
    }
}

/// Проверка конфига до записи в БД: адаптер собирается, но никуда не ставится.
pub fn validate_adapter_config(
    code: &str,
    base_url: &str,
    config: &Value,
    client: Arc<Client>,
) -> Result<(), String> {
    build_adapter(code, base_url, config, client).map(|_| ())
}

fn build_adapter(
    code: &str,
    base_url: &str,
    config: &Value,
    client: Arc<Client>,
) -> Result<SourceKind, String> {
    let base = Url::parse(base_url).map_err(|_| "base_url must be an absolute URL".to_owned())?;
    validate_outbound_url(&base)?;
    let ctx = AdapterCtx::new(client, base);

    match code {
        remanga::CODE => {
            let config = RemangaConfig::from_value(config)?;
            remanga::Remanga::new(ctx, config)
                .map(SourceKind::Remanga)
                .map_err(|error| error.to_string())
        }
        #[cfg(debug_assertions)]
        crate::sources::fixture::CODE => crate::sources::fixture::Fixture::new(ctx)
            .map(SourceKind::Fixture)
            .map_err(|error| error.to_string()),
        other => Err(format!("no adapter named `{other}`")),
    }
}

pub fn descriptors() -> Vec<&'static SourceDescriptor> {
    let mut list = vec![&remanga::DESCRIPTOR];
    #[cfg(debug_assertions)]
    list.push(&crate::sources::fixture::DESCRIPTOR);
    list
}
