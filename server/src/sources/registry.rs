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
    mangamir::{self, MangamirConfig},
    remanga::{self, RemangaConfig},
    rumix::{self, RumixConfig},
};

#[derive(Debug, sqlx::FromRow)]
struct SourceRow {
    id: i64,
    code: String,
    base_url: Option<String>,
    enabled: bool,
    config_json: String,
    config_error: Option<String>,
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
        // Строки нужны и для приоритетов: список языковой колонки строится
        // из `sources`, а не только из настроенных адаптеров. `base_url`
        // остаётся NULL: источник появится в списке, но не включится сам.
        seed_descriptors(pool).await?;

        let rows = sqlx::query_as::<_, SourceRow>(
            "SELECT id, code, base_url, enabled, config_json, config_error, \
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

            let stored_config_error = row.config_error.clone();
            let runtime = build_runtime(row, descriptor, client.clone());

            // Ошибка сборки адаптера должна быть видна и в БД: иначе
            // preferred source может указывать на нерабочий источник.
            if stored_config_error != runtime.config_error {
                sqlx::query("UPDATE sources SET config_error = ? WHERE id = ?")
                    .bind(runtime.config_error.as_deref())
                    .bind(runtime.id)
                    .execute(pool)
                    .await?;
            }

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

/// Добавляет недостающие строки для скомпилированных источников.
/// `ON CONFLICT` не трогает `base_url` и `enabled`: ручная настройка
/// администратора важнее дефолта, а пустой `default_base_url` (fixture)
/// означает, что строка появится только после явного PUT.
async fn seed_descriptors(pool: &SqlitePool) -> Result<(), AppError> {
    let now = crate::auth::session::now_unix();
    let mut langs: HashMap<&'static str, Vec<&'static str>> = HashMap::new();

    for descriptor in descriptors() {
        if descriptor.default_base_url.is_empty() {
            continue;
        }
        sqlx::query(
            "INSERT INTO sources (code, name, base_url, enabled, config_json, descriptor_version, created_at) \
             VALUES (?, ?, NULL, 0, '{}', ?, ?) \
             ON CONFLICT (code) DO UPDATE SET \
                 name = excluded.name, \
                 descriptor_version = excluded.descriptor_version",
        )
        .bind(descriptor.code)
        .bind(descriptor.name)
        .bind(i64::from(descriptor.version))
        .bind(now)
        .execute(pool)
        .await?;
        langs
            .entry(descriptor.lang)
            .or_default()
            .push(descriptor.code);
    }

    // Приоритет по умолчанию — порядок дескрипторов: Remanga → RuMix →
    // Mangamir. Существующий список администратора не перетираем.
    for (lang, codes) in langs {
        let ranked: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM source_priorities WHERE lang = ?")
                .bind(lang)
                .fetch_one(pool)
                .await?;
        if ranked > 0 {
            continue;
        }

        for (rank, code) in codes.into_iter().enumerate() {
            let source_id = sqlx::query_scalar::<_, i64>("SELECT id FROM sources WHERE code = ?")
                .bind(code)
                .fetch_one(pool)
                .await?;
            sqlx::query(
                "INSERT INTO source_priorities (lang, source_id, rank, created_at) \
                 VALUES (?, ?, ?, ?)",
            )
            .bind(lang)
            .bind(source_id)
            .bind(i64::try_from(rank).unwrap_or(i64::MAX))
            .bind(now)
            .execute(pool)
            .await?;
        }
    }

    Ok(())
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
        rumix::CODE => {
            let config = RumixConfig::from_value(config)?;
            rumix::Rumix::new(ctx, config)
                .map(SourceKind::Rumix)
                .map_err(|error| error.to_string())
        }
        mangamir::CODE => {
            let config = MangamirConfig::from_value(config)?;
            mangamir::Mangamir::new(ctx, config)
                .map(SourceKind::Mangamir)
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
    let mut list = vec![
        &remanga::DESCRIPTOR,
        &rumix::DESCRIPTOR,
        &mangamir::DESCRIPTOR,
    ];
    #[cfg(debug_assertions)]
    list.push(&crate::sources::fixture::DESCRIPTOR);
    list
}
