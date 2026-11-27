use std::collections::{HashMap, HashSet};

use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Sqlite, Transaction};

use crate::{
    auth::session::now_unix,
    catalog::selection,
    error::{AppError, FieldError},
    source_auth::{SourceAuth, service as source_auth_service},
    sources::{
        ConfigField, FieldType, SourceDescriptor, SourceError, SourceTitle, TitleKind,
        config::{mask_config, merge_config, validate_config, validate_outbound_url},
        descriptor::Capability,
        dto::SearchRequest,
        registry,
        registry::SourceRuntime,
        text::{dedupe, normalize_name, truncate_chars},
    },
    state::AppState,
};

#[derive(Debug, Serialize)]
pub struct SourceStateDto {
    pub code: &'static str,
    pub name: &'static str,
    /// Сайт для подписи; у Remanga отличается от API-хоста `base_url`.
    pub site_url: &'static str,
    /// Стандартный адрес, который подставляется при включении источника.
    pub default_base_url: &'static str,
    /// Языковая группа: по ней источники раскладываются по колонкам админки.
    pub lang: &'static str,
    pub version: u32,
    pub capabilities: Vec<&'static str>,
    pub config_schema: Vec<Value>,
    pub filters_schema: Vec<Value>,
    pub configured: bool,
    pub enabled: bool,
    pub base_url: Option<String>,
    pub config: Value,
    pub config_error: Option<String>,
    pub last_checked_at: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SourceListResponse {
    pub sources: Vec<SourceStateDto>,
}

#[derive(Debug, Deserialize)]
pub struct SourceInput {
    pub base_url: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct ConfigSaveResponse {
    pub warnings: Vec<String>,
    pub config_error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SearchInput {
    pub query: String,
    pub cursor: Option<String>,
    pub filters: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct SearchItemDto {
    pub external_id: String,
    /// Тип из выдачи источника, если он его отдаёт.
    pub kind: Option<&'static str>,
    pub name: String,
    pub alt_names: Vec<String>,
    pub cover_ref: Option<String>,
    pub year: Option<i16>,
    pub status: Option<String>,
    pub external_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub items: Vec<SearchItemDto>,
    pub next_cursor: Option<String>,
    pub total: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ImportInput {
    pub external_id: String,
    pub title_id: Option<i64>,
    /// Сливать с существующим тайтлом по названию. Импорт закладок
    /// выключает это при конфликте одинаковых названий разных произведений.
    #[serde(default = "default_auto_link")]
    pub auto_link: bool,
}

fn default_auto_link() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct StatsDto {
    pub chapter_count: i64,
    pub available_count: i64,
    pub locked_count: i64,
    pub missing_count: i64,
}

#[derive(Debug, Serialize)]
pub struct ImportResponse {
    pub title_id: i64,
    pub source_entry_id: i64,
    pub title_created: bool,
    pub imported_chapters: i64,
    /// Сколько обложек источника удалось скачать автоматически.
    pub covers_imported: i64,
    pub stats: StatsDto,
}

#[derive(Debug, Serialize)]
pub struct ProbeResponse {
    pub status: &'static str,
    pub message: Option<String>,
    pub checked_at: i64,
}

pub async fn list_sources(state: &AppState) -> SourceListResponse {
    let registry = state.sources.read().await;
    let sources = registry::descriptors()
        .into_iter()
        .map(|descriptor| state_dto(descriptor, registry.runtime(descriptor.code).as_deref()))
        .collect();
    SourceListResponse { sources }
}

pub async fn upsert_source(
    state: &AppState,
    code: &str,
    input: SourceInput,
) -> Result<SourceStateDto, AppError> {
    let descriptor = descriptor(state, code).await?;
    let base_url = input
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalize_base_url)
        .transpose()?;

    sqlx::query(
        "INSERT INTO sources (code, name, base_url, enabled, config_json, descriptor_version, created_at) \
         VALUES (?, ?, ?, ?, '{}', ?, ?) \
         ON CONFLICT (code) DO UPDATE SET \
             name = excluded.name, \
             base_url = COALESCE(excluded.base_url, sources.base_url), \
             enabled = excluded.enabled",
    )
    .bind(descriptor.code)
    .bind(descriptor.name)
    .bind(base_url.as_deref())
    .bind(input.enabled)
    .bind(i64::from(descriptor.version))
    .bind(now_unix())
    .execute(&state.pool)
    .await?;

    state.reload_sources().await?;

    // Включение/выключение источника меняет preferred у всех его тайтлов:
    // селектор пропускает disabled-источники. Строка только что вставлена
    // или обновлена выше, поэтому fetch_one здесь не может быть пустым.
    let source_id = sqlx::query_scalar::<_, i64>("SELECT id FROM sources WHERE code = ?")
        .bind(descriptor.code)
        .fetch_one(&state.pool)
        .await?;
    selection::recompute_titles_for_source(&state.pool, source_id).await?;

    let registry = state.sources.read().await;
    Ok(state_dto(
        descriptor,
        registry.runtime(descriptor.code).as_deref(),
    ))
}

pub async fn save_config(
    state: &AppState,
    code: &str,
    patch: Value,
) -> Result<ConfigSaveResponse, AppError> {
    let descriptor = descriptor(state, code).await?;

    let row = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT config_json, base_url FROM sources WHERE code = ?",
    )
    .bind(descriptor.code)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("source_not_found", "source is not registered"))?;

    if !patch.is_null() && !patch.is_object() {
        return Err(AppError::unprocessable(
            "source_config_invalid",
            "source config must be a JSON object",
            vec![FieldError {
                path: "config".to_owned(),
                message: "expected a JSON object".to_owned(),
            }],
        ));
    }

    let current: Value = serde_json::from_str(&row.0).unwrap_or(Value::Object(Default::default()));
    let merged = merge_config(descriptor, &current, &patch);

    let validated = validate_config(descriptor, &merged).map_err(|errors| {
        AppError::unprocessable("source_config_invalid", "source config is invalid", errors)
    })?;

    let base_url = row
        .1
        .as_deref()
        .ok_or_else(|| AppError::conflict("source_not_configured", "set base_url first"))?;
    registry::validate_adapter_config(
        descriptor.code,
        base_url,
        &validated.value,
        state.metadata_client.clone(),
    )
    .map_err(|detail| {
        AppError::unprocessable(
            "source_config_invalid",
            "source config is invalid",
            vec![FieldError {
                path: "config".to_owned(),
                message: detail,
            }],
        )
    })?;

    sqlx::query(
        "UPDATE sources SET config_json = ?, descriptor_version = ?, config_error = NULL WHERE code = ?",
    )
    .bind(validated.value.to_string())
    .bind(i64::from(descriptor.version))
    .bind(descriptor.code)
    .execute(&state.pool)
    .await?;

    state.reload_sources().await?;

    // Починка битого конфига возвращает источник в пул preferred.
    let source_id = sqlx::query_scalar::<_, i64>("SELECT id FROM sources WHERE code = ?")
        .bind(descriptor.code)
        .fetch_one(&state.pool)
        .await?;
    selection::recompute_titles_for_source(&state.pool, source_id).await?;

    Ok(ConfigSaveResponse {
        warnings: validated.warnings,
        config_error: None,
    })
}

pub async fn search(
    state: &AppState,
    code: &str,
    input: SearchInput,
) -> Result<SearchResponse, AppError> {
    let descriptor = descriptor(state, code).await?;
    if !descriptor.has(Capability::Search) {
        return Err(AppError::bad_request(
            "source_bad_request",
            "source does not support search",
        ));
    }

    let filters = input.filters.unwrap_or(Value::Null);
    if !descriptor.has(Capability::SearchFilters)
        && !matches!(&filters, Value::Null)
        && !filters.as_object().is_some_and(|object| object.is_empty())
    {
        return Err(AppError::bad_request(
            "source_bad_request",
            "source does not support search filters",
        ));
    }

    let runtime = runtime(state, code).await?;
    let adapter = runtime.adapter()?.clone();
    let page = adapter
        .search(
            SearchRequest {
                query: input.query,
                cursor: input.cursor,
                filters,
            },
            None,
        )
        .await?;

    Ok(SearchResponse {
        items: page
            .items
            .into_iter()
            .map(|item| SearchItemDto {
                external_id: item.external_id.as_str().to_owned(),
                kind: item.kind.map(TitleKind::as_str),
                name: item.name,
                alt_names: item.alt_names,
                cover_ref: item.cover_ref,
                year: item.year,
                status: item.status.map(|status| status.as_str().to_owned()),
                external_url: item.external_url,
            })
            .collect(),
        next_cursor: page.next_cursor,
        total: page.total,
    })
}

pub async fn import(
    state: &AppState,
    code: &str,
    input: ImportInput,
    auth: Option<&SourceAuth>,
) -> Result<ImportResponse, AppError> {
    let descriptor = descriptor(state, code).await?;
    if !descriptor.has(Capability::Chapters) {
        return Err(AppError::bad_request(
            "source_bad_request",
            "source does not support chapters",
        ));
    }

    let runtime = runtime(state, code).await?;
    let adapter = runtime.adapter()?.clone();
    let source_id = runtime.id;
    let external_id = crate::sources::ExternalId::new(input.external_id)?;

    // Сеть до транзакции: импорт не атомарен между сетью и БД (§5).
    // Тип нового тайтла берём из метаданных источника. Ошибка источника
    // маппится с учётом того, была ли у пользователя учётка.
    let source_title = adapter
        .title(&external_id, auth)
        .await
        .map_err(|error| error.into_app(auth.is_some()))?;
    let kind = source_title.kind;
    let chapters = adapter
        .chapters(&external_id, auth)
        .await
        .map_err(|error| error.into_app(auth.is_some()))?;

    let now = now_unix();
    let mut tx = state.pool.begin().await?;

    let existing = sqlx::query_as::<_, (i64, i64)>(
        "SELECT id, title_id FROM source_entries WHERE source_id = ? AND external_id = ?",
    )
    .bind(source_id)
    .bind(external_id.as_str())
    .fetch_optional(&mut *tx)
    .await?;

    // Добавление того же произведения из другого источника должно привязать
    // его к общей карточке, а не создать дубль: без ручной привязки это
    // единственный путь получить главы из нескольких источников.
    let auto_title_id = if input.title_id.is_none() && input.auto_link {
        find_title_by_names(&mut tx, &source_title).await?
    } else {
        None
    };
    if let Some(title_id) = auto_title_id {
        tracing::info!(title_id, source = %code, "auto-linked source title by name");
    }

    let (title_id, title_created) = match existing {
        Some((_, existing_title_id)) => {
            if let Some(requested) = input.title_id
                && requested != existing_title_id
            {
                return Err(AppError::conflict(
                    "source_entry_linked",
                    "source entry already belongs to another title",
                ));
            }
            (existing_title_id, false)
        }
        None => match input.title_id.or(auto_title_id) {
            Some(title_id) => {
                sqlx::query_scalar::<_, i64>("SELECT 1 FROM titles WHERE id = ?")
                    .bind(title_id)
                    .fetch_optional(&mut *tx)
                    .await?
                    .ok_or_else(|| AppError::not_found("title_not_found", "title not found"))?;

                // preferred_source_id — source_id, поэтому два разных title
                // одного источника у одного тайтла неоднозначны.
                let already_attached = sqlx::query_scalar::<_, i64>(
                    "SELECT 1 FROM source_entries WHERE title_id = ? AND source_id = ?",
                )
                .bind(title_id)
                .bind(source_id)
                .fetch_optional(&mut *tx)
                .await?
                .is_some();
                if already_attached {
                    return Err(AppError::conflict(
                        "source_already_attached",
                        "source is already attached to this title",
                    ));
                }

                (title_id, false)
            }
            None => (
                create_title(&mut tx, kind, &source_title, source_id, now).await?,
                true,
            ),
        },
    };

    let entry_id = match existing {
        Some((entry_id, _)) => {
            sqlx::query(
                "UPDATE source_entries SET external_url = ?, raw_json = ?, adapter_version = ?, \
                        last_synced_at = ?, last_error = NULL WHERE id = ?",
            )
            .bind(source_title.external_url.as_deref())
            .bind(source_title.raw_json.as_ref().map(Value::to_string))
            .bind(i64::from(descriptor.version))
            .bind(now)
            .bind(entry_id)
            .execute(&mut *tx)
            .await?;
            entry_id
        }
        None => {
            let entry_id = sqlx::query_scalar::<_, i64>(
                "INSERT INTO source_entries (title_id, source_id, external_id, external_url, \
                                             raw_json, is_primary, adapter_version, last_synced_at) \
                 VALUES (?, ?, ?, ?, ?, 0, ?, ?) RETURNING id",
            )
            .bind(title_id)
            .bind(source_id)
            .bind(external_id.as_str())
            .bind(source_title.external_url.as_deref())
            .bind(source_title.raw_json.as_ref().map(Value::to_string))
            .bind(i64::from(descriptor.version))
            .bind(now)
            .fetch_one(&mut *tx)
            .await?;

            // Первый привязанный источник становится primary, если другого нет
            sqlx::query(
                "UPDATE source_entries SET is_primary = 1 WHERE id = ? AND NOT EXISTS \
                 (SELECT 1 FROM source_entries WHERE title_id = ? AND is_primary = 1 AND id <> ?)",
            )
            .bind(entry_id)
            .bind(title_id)
            .bind(entry_id)
            .execute(&mut *tx)
            .await?;
            entry_id
        }
    };

    // Ручной тайтл мог быть создан без языка; привязка источника заполняет
    // его, иначе сохранённый набор priorities для языка не применится.
    if let Some(lang) = source_title.lang.as_deref() {
        sqlx::query("UPDATE titles SET lang = COALESCE(lang, ?) WHERE id = ?")
            .bind(lang)
            .bind(title_id)
            .execute(&mut *tx)
            .await?;
    }

    sqlx::query("UPDATE titles SET origin_source_id = ? WHERE id = ? AND origin_source_id IS NULL")
        .bind(source_id)
        .bind(title_id)
        .execute(&mut *tx)
        .await?;

    let imported_chapters = upsert_chapters(&mut tx, entry_id, now, chapters).await?;

    tx.commit().await?;

    // Обложки — после транзакции: сеть и декодирование не держат блокировку
    // БД, а сбой загрузки не отменяет уже импортированные главы.
    let covers_imported = crate::catalog::cover::import_source_covers(
        state,
        title_id,
        source_id,
        &adapter,
        &source_title.covers,
        auth,
    )
    .await;

    let stats = selection::recompute_stats(&state.pool, entry_id).await?;
    selection::recompute_preferred(&state.pool, title_id).await?;
    // Новые главы могли попасть в существующую связку — пересобираем пары.
    if let Err(error) = crate::catalog::alias_ranges::materialize_title(&state.pool, title_id).await
    {
        tracing::warn!(title_id, %error, "cannot materialize alias ranges after import");
    }

    Ok(ImportResponse {
        title_id,
        source_entry_id: entry_id,
        title_created,
        imported_chapters,
        covers_imported,
        stats: StatsDto {
            chapter_count: stats.chapter_count,
            available_count: stats.available_count,
            locked_count: stats.locked_count,
            missing_count: stats.missing_count,
        },
    })
}

pub async fn probe(state: &AppState, code: &str) -> Result<ProbeResponse, AppError> {
    let descriptor = descriptor(state, code).await?;
    let runtime = runtime(state, code).await?;
    let adapter = runtime.adapter_unchecked()?.clone();
    let checked_at = now_unix();

    let (status, message) = match adapter
        .search(
            SearchRequest {
                query: "test".to_owned(),
                cursor: None,
                filters: Value::Null,
            },
            None,
        )
        .await
    {
        Ok(_) => ("ok", None),
        Err(SourceError::AuthRequired) => {
            ("auth", Some("source requires authentication".to_owned()))
        }
        Err(SourceError::Blocked) => (
            "cloudflare",
            Some("source blocked the request (cloudflare or region)".to_owned()),
        ),
        Err(SourceError::Parse { .. }) => {
            ("parse", Some("cannot parse source response".to_owned()))
        }
        Err(SourceError::RateLimited { .. }) => {
            ("error", Some("source rate limited the probe".to_owned()))
        }
        Err(error) => ("error", Some(error.to_string())),
    };

    sqlx::query("UPDATE sources SET last_checked_at = ?, last_error = ? WHERE code = ?")
        .bind(checked_at)
        .bind(message.as_deref())
        .bind(descriptor.code)
        .execute(&state.pool)
        .await?;
    state.reload_sources().await?;

    Ok(ProbeResponse {
        status,
        message,
        checked_at,
    })
}

#[derive(Debug, sqlx::FromRow)]
struct TitleNameRow {
    id: i64,
    name: String,
    alt_names_json: String,
}

/// Ищет уже существующий тайтл с тем же названием (или альтернативным),
/// чтобы добавление того же произведения из другого источника привязало
/// его к общей карточке, а не создало дубль. Совпадение точное после
/// нормализации; сомнительные случаи пользователь правит вручную.
async fn find_title_by_names(
    tx: &mut Transaction<'_, Sqlite>,
    source_title: &SourceTitle,
) -> Result<Option<i64>, AppError> {
    let rows = sqlx::query_as::<_, TitleNameRow>(
        "SELECT id, name, alt_names_json FROM titles ORDER BY id",
    )
    .fetch_all(&mut **tx)
    .await?;

    let mut incoming: Vec<String> = std::iter::once(source_title.name.clone())
        .chain(source_title.alt_names.iter().cloned())
        .map(|name| normalize_name(&name))
        .filter(|name| !name.is_empty())
        .collect();
    dedupe(&mut incoming);
    let incoming: HashSet<String> = incoming.into_iter().collect();

    let mut best: Option<(usize, i64)> = None;
    for row in rows {
        let mut candidate = vec![normalize_name(&row.name)];
        candidate.extend(
            decode_list(&row.alt_names_json)
                .iter()
                .map(|name| normalize_name(name)),
        );
        let score = candidate
            .iter()
            .filter(|name| incoming.contains(*name))
            .count();
        if score > 0 && best.is_none_or(|(best_score, _)| score > best_score) {
            best = Some((score, row.id));
        }
    }

    Ok(best.map(|(_, id)| id))
}

async fn create_title(
    tx: &mut Transaction<'_, Sqlite>,
    kind: TitleKind,
    source_title: &crate::sources::SourceTitle,
    source_id: i64,
    now: i64,
) -> Result<i64, AppError> {
    let name_lower = source_title.name.to_lowercase();
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO titles (kind, name, name_lower, alt_names_json, authors_json, genres_json, \
                             description, status, year, lang, rating, origin_source_id, \
                             preferred_source_id, preferred_source_updated_at, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(kind.as_str())
    .bind(&source_title.name)
    .bind(name_lower)
    .bind(json_list(&source_title.alt_names))
    .bind(json_list(&source_title.authors))
    .bind(json_list(&source_title.genres))
    .bind(source_title.description.as_deref())
    .bind(source_title.status.map(|status| status.as_str()))
    .bind(source_title.year.map(i64::from))
    .bind(source_title.lang.as_deref())
    .bind(source_title.rating.map(f64::from))
    .bind(source_id)
    .bind(source_id)
    .bind(now)
    .bind(now)
    .bind(now)
    .fetch_one(&mut **tx)
    .await
    .map_err(AppError::from)
}

pub(crate) async fn upsert_chapters(
    tx: &mut Transaction<'_, Sqlite>,
    entry_id: i64,
    now: i64,
    chapters: Vec<crate::sources::SourceChapter>,
) -> Result<i64, AppError> {
    let mut key_map: HashMap<String, String> = HashMap::new();
    let mut imported = 0_i64;

    for (ordinal, chapter) in chapters.into_iter().enumerate() {
        let external_id = chapter.external_id.as_str().to_owned();
        if key_map.contains_key(&external_id) {
            tracing::warn!(%external_id, "source returned duplicate chapter id; skipping");
            continue;
        }
        key_map.insert(external_id.clone(), chapter.chapter_key.clone());

        let locked = chapter.is_locked;
        let availability = if locked { "locked" } else { "unknown" };
        let checked_at = locked.then_some(now);

        sqlx::query(
            "INSERT INTO chapters (source_entry_id, external_id, chapter_key, number_text, \
                                   number_milli, ordinal_site, volume, chapter_kind, content_kind, \
                                   name, group_name, language, is_locked, availability, \
                                   availability_checked_at, external_url, published_at, \
                                   last_seen_at, raw_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT (source_entry_id, external_id) DO UPDATE SET \
                 number_text = excluded.number_text, \
                 number_milli = excluded.number_milli, \
                 ordinal_site = excluded.ordinal_site, \
                 volume = excluded.volume, \
                 chapter_kind = excluded.chapter_kind, \
                 content_kind = COALESCE(chapters.content_kind, excluded.content_kind), \
                 name = excluded.name, \
                 group_name = excluded.group_name, \
                 language = COALESCE(excluded.language, chapters.language), \
                 is_locked = excluded.is_locked, \
                 availability = CASE \
                     WHEN chapters.availability = 'available' THEN 'available' \
                     WHEN excluded.is_locked THEN 'locked' \
                     WHEN chapters.availability = 'locked' THEN 'unknown' \
                     ELSE chapters.availability END, \
                 availability_checked_at = CASE \
                     WHEN excluded.is_locked THEN excluded.availability_checked_at \
                     WHEN chapters.availability = 'locked' THEN NULL \
                     ELSE chapters.availability_checked_at END, \
                 external_url = excluded.external_url, \
                 published_at = COALESCE(excluded.published_at, chapters.published_at), \
                 last_seen_at = excluded.last_seen_at, \
                 raw_json = excluded.raw_json, \
                 removed_at = NULL",
        )
        .bind(entry_id)
        .bind(&external_id)
        .bind(&chapter.chapter_key)
        .bind(chapter.number_text.as_deref())
        .bind(chapter.number_milli)
        .bind(i64::try_from(ordinal).unwrap_or(i64::MAX))
        .bind(chapter.volume.map(i64::from))
        .bind(chapter.chapter_kind.as_str())
        .bind(chapter.content_kind.map(|kind| kind.as_str()))
        .bind(chapter.name.as_deref())
        .bind(chapter.group_name.as_deref())
        .bind(chapter.language.as_deref())
        .bind(locked)
        .bind(availability)
        .bind(checked_at)
        .bind(chapter.external_url.as_deref())
        .bind(chapter.published_at)
        .bind(now)
        .bind(chapter.raw_json.as_ref().map(Value::to_string))
        .execute(&mut **tx)
        .await?;

        imported += 1;
    }

    // chapter_key фиксируется при первом импорте: расхождение — повод для
    // миграции, а не молчаливая потеря прогресса
    let stored = sqlx::query_as::<_, (String, String)>(
        "SELECT external_id, chapter_key FROM chapters WHERE source_entry_id = ?",
    )
    .bind(entry_id)
    .fetch_all(&mut **tx)
    .await?;

    let drift = stored
        .iter()
        .filter(|(external_id, chapter_key)| {
            key_map
                .get(external_id)
                .is_some_and(|expected| expected != chapter_key)
        })
        .count();

    if drift > 0 {
        tracing::warn!(
            entry_id,
            drift,
            "chapter key drift detected; keeping first-import keys"
        );
        sqlx::query("UPDATE source_entries SET last_error = ? WHERE id = ?")
            .bind(format!(
                "chapter_key drift for {drift} chapters; keys kept from first import"
            ))
            .bind(entry_id)
            .execute(&mut **tx)
            .await?;
    }

    Ok(imported)
}

async fn descriptor(state: &AppState, code: &str) -> Result<&'static SourceDescriptor, AppError> {
    state
        .sources
        .read()
        .await
        .descriptor(code)
        .ok_or_else(|| AppError::not_found("source_not_found", "unknown source"))
}

async fn runtime(state: &AppState, code: &str) -> Result<std::sync::Arc<SourceRuntime>, AppError> {
    state
        .sources
        .read()
        .await
        .runtime(code)
        .ok_or_else(|| AppError::not_found("source_not_found", "source is not registered"))
}

fn normalize_base_url(raw: &str) -> Result<String, AppError> {
    let url = Url::parse(raw)
        .map_err(|_| AppError::bad_request("validation", "base_url must be an absolute URL"))?;
    validate_outbound_url(&url).map_err(|detail| AppError::bad_request("validation", detail))?;
    if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
        return Err(AppError::bad_request(
            "validation",
            "base_url must be an origin without path, query or fragment",
        ));
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

fn default_true() -> bool {
    true
}

fn json_list(values: &[String]) -> String {
    Value::from(values.to_vec()).to_string()
}

fn state_dto(
    descriptor: &'static SourceDescriptor,
    runtime: Option<&SourceRuntime>,
) -> SourceStateDto {
    SourceStateDto {
        code: descriptor.code,
        name: descriptor.name,
        site_url: descriptor.site_url,
        default_base_url: descriptor.default_base_url,
        lang: descriptor.lang,
        version: descriptor.version,
        capabilities: descriptor
            .capabilities
            .iter()
            .map(|capability| capability.as_str())
            .collect(),
        config_schema: descriptor.config.iter().map(field_schema).collect(),
        filters_schema: descriptor.filters.iter().map(field_schema).collect(),
        configured: runtime
            .and_then(|runtime| runtime.base_url.as_ref())
            .is_some(),
        enabled: runtime.is_some_and(|runtime| runtime.enabled),
        base_url: runtime.and_then(|runtime| runtime.base_url.clone()),
        config: runtime
            .map(|runtime| mask_config(descriptor, &runtime.config))
            .unwrap_or_else(|| json!({})),
        config_error: runtime.and_then(|runtime| runtime.config_error.clone()),
        last_checked_at: runtime.and_then(|runtime| runtime.last_checked_at),
        last_error: runtime.and_then(|runtime| runtime.last_error.clone()),
    }
}

fn field_schema(field: &ConfigField) -> Value {
    let mut schema = json!({
        "key": field.key,
        "kind": field.kind.kind_name(),
        "required": field.required,
        "secret": field.secret,
    });

    match field.kind {
        FieldType::Select(options) | FieldType::MultiSelect(options) => {
            schema["options"] = json!(options);
        }
        FieldType::Text { max_len } => {
            schema["max_len"] = json!(max_len);
        }
        FieldType::Integer { min, max } => {
            schema["min"] = json!(min);
            schema["max"] = json!(max);
        }
        FieldType::Range { min, max, step } => {
            schema["min"] = json!(min);
            schema["max"] = json!(max);
            schema["step"] = json!(step);
        }
        FieldType::Url
        | FieldType::UrlList
        | FieldType::HostList
        | FieldType::Bool
        | FieldType::TriState => {}
    }

    schema
}

#[derive(Debug, Serialize)]
pub struct RefreshEntryDto {
    pub source_entry_id: i64,
    pub code: String,
    pub status: &'static str,
    pub imported_chapters: i64,
    pub error: Option<String>,
    /// Машинный код ошибки: UI переводит его, а не показывает сырой текст.
    pub error_code: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub title_id: i64,
    pub entries: Vec<RefreshEntryDto>,
    pub preferred_source_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct SyncResponse {
    pub title_id: i64,
    /// false — данные ещё свежие, сеть не трогали.
    pub refreshed: bool,
    pub preferred_source_id: Option<i64>,
    pub metadata_updated: bool,
    pub entries: Vec<RefreshEntryDto>,
}

/// Снимок метаданных, который последним применился из источников.
/// Нужен, чтобы отличать автозначение от ручной правки: если поле
/// изменилось относительно снимка, синхронизация его не трогает.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct MetadataSnapshot {
    kind: Option<String>,
    name: Option<String>,
    alt_names: Vec<String>,
    authors: Vec<String>,
    genres: Vec<String>,
    description: Option<String>,
    status: Option<String>,
    year: Option<i64>,
    lang: Option<String>,
    rating: Option<f64>,
}

#[derive(Debug, Clone)]
struct SourceMetadata {
    entry_id: i64,
    source_id: i64,
    title: SourceTitle,
}

/// Автосинхронизация при открытии страницы тайтла. Ограничение по времени
/// защищает источники от запроса на каждый рендер; ручной refresh админа
/// его не учитывает.
const SYNC_TTL_SECS: i64 = 10 * 60;

/// Обновляет главы всех источников тайтла. Сеть идёт до транзакции,
/// ошибка одного источника не отменяет остальные. Учётки резолвятся для
/// действующего пользователя: администратор видит то же, что и читатель.
pub async fn refresh_title(
    state: &AppState,
    title_id: i64,
    user_id: i64,
) -> Result<RefreshResponse, AppError> {
    let (entries, _, _) = refresh_entries(state, title_id, true, user_id).await?;
    let preferred_source_id = selection::recompute_preferred(&state.pool, title_id).await?;
    Ok(RefreshResponse {
        title_id,
        entries,
        preferred_source_id,
    })
}

/// Полное обновление страницы тайтла: главы, обложки и общие метаданные.
/// Метаданные собираются из всех привязанных источников в порядке
/// приоритета: первый непустой источник заполняет поле, остальные —
/// только пропуски. Это позволяет тайтлу без Remanga получить тип,
/// описание и обложки из другого источника, даже если там их меньше.
pub async fn sync_title(
    state: &AppState,
    title_id: i64,
    user_id: i64,
) -> Result<SyncResponse, AppError> {
    let (preferred_source_id, synced_at) = sqlx::query_as::<_, (Option<i64>, Option<i64>)>(
        "SELECT preferred_source_id, metadata_synced_at FROM titles WHERE id = ?",
    )
    .bind(title_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("title_not_found", "title not found"))?;

    let now = now_unix();
    if synced_at.is_some_and(|last| now.saturating_sub(last) < SYNC_TTL_SECS) {
        return Ok(SyncResponse {
            title_id,
            refreshed: false,
            preferred_source_id,
            metadata_updated: false,
            entries: Vec::new(),
        });
    }

    let (entries, mut metadata, auth_errors) =
        refresh_entries(state, title_id, false, user_id).await?;
    let mut preferred_source_id = selection::recompute_preferred(&state.pool, title_id).await?;

    // Порядок метаданных: preferred → rank → is_primary → привязка.
    let title = selection::load_title_info(&state.pool, title_id).await?;
    let ranks = selection::load_priorities(&state.pool, title.lang.as_deref()).await?;
    metadata.sort_by_key(|source| {
        (
            Some(source.source_id) != preferred_source_id,
            selection::rank_of(&ranks, source.source_id),
            source.entry_id,
        )
    });

    let merged = merge_source_metadata(&metadata);
    let metadata_updated = match merged {
        Some(merged) => apply_source_metadata(state, title_id, &merged).await?,
        None => false,
    };

    // Язык мог измениться вместе с метаданными: набор приоритетов другой.
    let info = selection::load_title_info(&state.pool, title_id).await?;
    if info.lang != title.lang {
        preferred_source_id = selection::recompute_preferred(&state.pool, title_id).await?;
    }

    // Обложки: preferred (уже с учётом нового языка) заполняет размеры
    // первым, остальные источники — только пропуски. Ручные не трогаем.
    if let Err(error) =
        crate::catalog::cover::refresh_source_covers(state, title_id, false, user_id).await
    {
        tracing::warn!(title_id, %error, "cannot refresh source covers during sync");
    }

    if !auth_errors {
        // Auth-ошибка одного пользователя не должна «остужать» sync на
        // 10 минут: пользователь с подключённым аккаунтом повторит сразу.
        sqlx::query("UPDATE titles SET metadata_synced_at = ? WHERE id = ?")
            .bind(now)
            .bind(title_id)
            .execute(&state.pool)
            .await?;
    }

    Ok(SyncResponse {
        title_id,
        refreshed: true,
        preferred_source_id,
        metadata_updated,
        entries,
    })
}

/// Общая часть refresh/sync: главы всех entry и метаданные успешных
/// `title()`. `import_covers` включает старое поведение ручного refresh —
/// заполнить пустые размеры обложек сразу. Третий элемент — были ли
/// auth-ошибки: по нему sync не двигает TTL.
async fn refresh_entries(
    state: &AppState,
    title_id: i64,
    import_covers: bool,
    user_id: i64,
) -> Result<(Vec<RefreshEntryDto>, Vec<SourceMetadata>, bool), AppError> {
    sqlx::query_scalar::<_, i64>("SELECT 1 FROM titles WHERE id = ?")
        .bind(title_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("title_not_found", "title not found"))?;

    let entries = sqlx::query_as::<_, (i64, i64, String, String)>(
        "SELECT e.id, e.source_id, e.external_id, s.code \
         FROM source_entries e JOIN sources s ON s.id = e.source_id \
         WHERE e.title_id = ? ORDER BY e.id",
    )
    .bind(title_id)
    .fetch_all(&state.pool)
    .await?;

    let mut results = Vec::with_capacity(entries.len());
    let mut metadata = Vec::new();
    let mut auth_errors = false;

    for (entry_id, source_id, external_id, source_code) in entries {
        let runtime = {
            let registry = state.sources.read().await;
            registry.runtime_by_id(source_id)
        };
        let Some(runtime) = runtime else {
            let message = "source is not registered".to_owned();
            store_sync_error(&state.pool, entry_id, &message).await?;
            results.push(failed(entry_id, source_code, message, None));
            continue;
        };

        let version = runtime.descriptor.version;
        let adapter = match runtime.adapter() {
            Ok(adapter) => adapter.clone(),
            Err(error) => {
                let message = short_error(&error);
                store_sync_error(&state.pool, entry_id, &message).await?;
                results.push(failed(entry_id, source_code, message, Some(error.reason())));
                continue;
            }
        };
        drop(runtime);

        let external_id = match crate::sources::ExternalId::new(external_id) {
            Ok(external_id) => external_id,
            Err(error) => {
                let message = short_error(&error);
                store_sync_error(&state.pool, entry_id, &message).await?;
                results.push(failed(entry_id, source_code, message, Some(error.reason())));
                continue;
            }
        };

        // Учётка действующего пользователя. Ошибка расшифровки — это его
        // auth-проблема, а не повод перезаписать глобальный last_error.
        let auth = match source_auth_service::resolve(state, user_id, source_id).await {
            Ok(auth) => auth,
            Err(error) => {
                let message = error.short_message();
                tracing::warn!(entry_id, code = %source_code, %message, "cannot resolve source credential");
                results.push(failed(entry_id, source_code, message, Some(error.code())));
                auth_errors = true;
                continue;
            }
        };
        let auth_present = auth.is_some();

        match adapter.title(&external_id, auth.as_ref()).await {
            Ok(source_title) => {
                if !auth_present {
                    set_entry_auth_required(&state.pool, entry_id, false).await?;
                }
                if import_covers {
                    crate::catalog::cover::import_source_covers(
                        state,
                        title_id,
                        source_id,
                        &adapter,
                        &source_title.covers,
                        auth.as_ref(),
                    )
                    .await;
                }
                metadata.push(SourceMetadata {
                    entry_id,
                    source_id,
                    title: source_title,
                });
            }
            Err(error) => {
                if is_auth_error(&error) {
                    auth_errors = true;
                    if !auth_present {
                        set_entry_auth_required(&state.pool, entry_id, true).await?;
                    }
                }
                tracing::warn!(
                    entry_id,
                    code = %source_code,
                    error = %error,
                    "cannot refresh source metadata"
                );
            }
        }

        match adapter.chapters(&external_id, auth.as_ref()).await {
            Ok(chapters) => {
                let now = now_unix();
                let mut tx = state.pool.begin().await?;
                // last_error очищаем до upsert: расхождение chapter_key
                // пишется внутри и не должно быть затёрто успехом refresh.
                sqlx::query(
                    "UPDATE source_entries SET last_synced_at = ?, last_error = NULL, \
                            adapter_version = ?, auth_required = CASE WHEN ? THEN auth_required ELSE 0 END \
                     WHERE id = ?",
                )
                .bind(now)
                .bind(i64::from(version))
                .bind(auth_present)
                .bind(entry_id)
                .execute(&mut *tx)
                .await?;
                let imported = upsert_chapters(&mut tx, entry_id, now, chapters).await?;
                tx.commit().await?;

                selection::recompute_stats(&state.pool, entry_id).await?;
                results.push(RefreshEntryDto {
                    source_entry_id: entry_id,
                    code: source_code,
                    status: "ok",
                    imported_chapters: imported,
                    error: None,
                    error_code: None,
                });
            }
            Err(error) => {
                let message = short_error(&error);
                let auth_error = is_auth_error(&error);
                let mapped = error.into_app(auth_present);
                let entry_code = mapped.code();
                tracing::warn!(entry_id, code = %source_code, %message, "source refresh failed");
                if auth_error {
                    auth_errors = true;
                    if !auth_present {
                        set_entry_auth_required(&state.pool, entry_id, true).await?;
                    }
                    // Глобальный last_error не трогаем: у другого
                    // пользователя с учёткой этот источник рабочий.
                } else {
                    store_sync_error(&state.pool, entry_id, &message).await?;
                }
                results.push(failed(entry_id, source_code, message, Some(entry_code)));
            }
        }
    }

    // Один пересчёт на тайтл после обхода всех источников: иначе порядок
    // entry решал бы, какая сторона связки уже импортирована.
    if let Err(error) = crate::catalog::alias_ranges::materialize_title(&state.pool, title_id).await
    {
        tracing::warn!(title_id, %error, "cannot materialize alias ranges after refresh");
    }

    Ok((results, metadata, auth_errors))
}

fn is_auth_error(error: &SourceError) -> bool {
    matches!(
        error,
        SourceError::AuthRequired | SourceError::AuthMaybeRequired
    )
}

async fn set_entry_auth_required(
    pool: &sqlx::SqlitePool,
    entry_id: i64,
    required: bool,
) -> Result<(), AppError> {
    sqlx::query("UPDATE source_entries SET auth_required = ? WHERE id = ?")
        .bind(required)
        .bind(entry_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Приводит метаданные разных источников к одному виду: первый источник в
/// порядке приоритета заполняет поле, остальные только пропуски. Списки
/// объединяются, тип берётся первый конкретный (не `other`), числовые поля
/// проверяются по тем же ограничениям, что и схема БД.
fn merge_source_metadata(sources: &[SourceMetadata]) -> Option<MetadataSnapshot> {
    let mut name = None;
    let mut kind = TitleKind::Other;
    let mut alt_names = Vec::new();
    let mut authors = Vec::new();
    let mut genres = Vec::new();
    let mut description = None;
    let mut status = None;
    let mut year = None;
    let mut lang = None;
    let mut rating = None;

    for source in sources {
        let title = &source.title;
        if name.is_none() && !title.name.trim().is_empty() {
            name = Some(truncate_chars(&title.name, 300));
        }
        if kind == TitleKind::Other && title.kind != TitleKind::Other {
            kind = title.kind;
        }
        alt_names.extend(title.alt_names.iter().cloned());
        authors.extend(title.authors.iter().cloned());
        genres.extend(title.genres.iter().cloned());

        if description.is_none()
            && let Some(text) = title.description.as_deref()
            && !text.trim().is_empty()
        {
            description = Some(truncate_chars(text, 20_000));
        }
        if status.is_none() {
            status = title.status.map(|status| status.as_str().to_owned());
        }
        if year.is_none() {
            year = title
                .year
                .map(i64::from)
                .filter(|year| (1800..=2200).contains(year));
        }
        if lang.is_none() {
            lang = title
                .lang
                .as_deref()
                .map(str::trim)
                .filter(|lang| !lang.is_empty())
                .map(|lang| lang.chars().take(16).collect());
        }
        if rating.is_none() {
            rating = title
                .rating
                .map(f64::from)
                .filter(|rating| rating.is_finite() && (0.0..=10.0).contains(rating));
        }
    }

    let name = name?;
    for list in [&mut alt_names, &mut authors, &mut genres] {
        dedupe(list);
        list.truncate(50);
        for item in list.iter_mut() {
            *item = truncate_chars(item, 200);
        }
    }

    Some(MetadataSnapshot {
        kind: Some(kind.as_str().to_owned()),
        name: Some(name),
        alt_names,
        authors,
        genres,
        description,
        status,
        year,
        lang,
        rating,
    })
}

#[derive(Debug, sqlx::FromRow)]
struct CurrentMetadata {
    kind: String,
    name: String,
    alt_names_json: String,
    authors_json: String,
    genres_json: String,
    description: Option<String>,
    status: Option<String>,
    year: Option<i64>,
    lang: Option<String>,
    rating: Option<f64>,
    source_metadata_json: Option<String>,
}

fn decode_list(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

fn pick_value<T: PartialEq>(current: T, previous: Option<&T>, next: T) -> T {
    match previous {
        Some(previous) if current != *previous => current,
        _ => next,
    }
}

/// Применяет объединённые метаданные, сохраняя ручные правки полей.
/// Поле считается ручным, если оно отличается от прошлого снимка источника.
async fn apply_source_metadata(
    state: &AppState,
    title_id: i64,
    merged: &MetadataSnapshot,
) -> Result<bool, AppError> {
    let current = sqlx::query_as::<_, CurrentMetadata>(
        "SELECT kind, name, alt_names_json, authors_json, genres_json, description, \
                status, year, lang, rating, source_metadata_json \
         FROM titles WHERE id = ?",
    )
    .bind(title_id)
    .fetch_one(&state.pool)
    .await?;

    let previous = current
        .source_metadata_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<MetadataSnapshot>(raw).ok());

    let kind = pick_value(
        current.kind.clone(),
        previous
            .as_ref()
            .and_then(|snapshot| snapshot.kind.as_ref()),
        merged.kind.clone().unwrap_or_else(|| current.kind.clone()),
    );
    let name = pick_value(
        current.name.clone(),
        previous
            .as_ref()
            .and_then(|snapshot| snapshot.name.as_ref()),
        merged.name.clone().unwrap_or_else(|| current.name.clone()),
    );
    let alt_names = pick_value(
        decode_list(&current.alt_names_json),
        previous.as_ref().map(|snapshot| &snapshot.alt_names),
        merged.alt_names.clone(),
    );
    let authors = pick_value(
        decode_list(&current.authors_json),
        previous.as_ref().map(|snapshot| &snapshot.authors),
        merged.authors.clone(),
    );
    let genres = pick_value(
        decode_list(&current.genres_json),
        previous.as_ref().map(|snapshot| &snapshot.genres),
        merged.genres.clone(),
    );
    let description = pick_value(
        current.description.clone(),
        previous.as_ref().map(|snapshot| &snapshot.description),
        merged.description.clone(),
    );
    let status = pick_value(
        current.status.clone(),
        previous.as_ref().map(|snapshot| &snapshot.status),
        merged.status.clone(),
    );
    let year = pick_value(
        current.year,
        previous.as_ref().map(|snapshot| &snapshot.year),
        merged.year,
    );
    let lang = pick_value(
        current.lang.clone(),
        previous.as_ref().map(|snapshot| &snapshot.lang),
        merged.lang.clone(),
    );
    let rating = pick_value(
        current.rating,
        previous.as_ref().map(|snapshot| &snapshot.rating),
        merged.rating,
    );

    let changed = kind != current.kind
        || name != current.name
        || alt_names != decode_list(&current.alt_names_json)
        || authors != decode_list(&current.authors_json)
        || genres != decode_list(&current.genres_json)
        || description != current.description
        || status != current.status
        || year != current.year
        || lang != current.lang
        || rating != current.rating;

    let snapshot = serde_json::to_string(merged).ok();

    if changed {
        let name_lower = name.to_lowercase();
        sqlx::query(
            "UPDATE titles SET kind = ?, name = ?, name_lower = ?, alt_names_json = ?, \
                              authors_json = ?, genres_json = ?, description = ?, status = ?, \
                              year = ?, lang = ?, rating = ?, updated_at = max(?, updated_at + 1) \
             WHERE id = ?",
        )
        .bind(&kind)
        .bind(&name)
        .bind(name_lower)
        .bind(Value::from(alt_names).to_string())
        .bind(Value::from(authors).to_string())
        .bind(Value::from(genres).to_string())
        .bind(description.as_deref())
        .bind(status.as_deref())
        .bind(year)
        .bind(lang.as_deref())
        .bind(rating)
        .bind(now_unix())
        .bind(title_id)
        .execute(&state.pool)
        .await?;
    }

    sqlx::query("UPDATE titles SET source_metadata_json = ? WHERE id = ?")
        .bind(snapshot)
        .bind(title_id)
        .execute(&state.pool)
        .await?;

    Ok(changed)
}

fn failed(
    source_entry_id: i64,
    code: String,
    error: String,
    error_code: Option<&'static str>,
) -> RefreshEntryDto {
    RefreshEntryDto {
        source_entry_id,
        code,
        status: "error",
        imported_chapters: 0,
        error: Some(error),
        error_code,
    }
}

fn short_error(error: &SourceError) -> String {
    error.to_string().chars().take(200).collect()
}

async fn store_sync_error(
    pool: &sqlx::SqlitePool,
    entry_id: i64,
    message: &str,
) -> Result<(), AppError> {
    sqlx::query("UPDATE source_entries SET last_error = ? WHERE id = ?")
        .bind(message)
        .bind(entry_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::{ExternalId, SourceCovers, TitleStatus};

    fn source(entry_id: i64, source_id: i64, title: SourceTitle) -> SourceMetadata {
        SourceMetadata {
            entry_id,
            source_id,
            title,
        }
    }

    fn empty_title(name: &str) -> SourceTitle {
        SourceTitle {
            external_id: ExternalId::new("fixture-title").expect("external id"),
            kind: TitleKind::Other,
            name: name.to_owned(),
            alt_names: Vec::new(),
            description: None,
            covers: SourceCovers::default(),
            authors: Vec::new(),
            genres: Vec::new(),
            status: None,
            year: None,
            rating: None,
            lang: None,
            external_url: None,
            raw_json: None,
        }
    }

    #[test]
    fn merges_metadata_in_priority_order_and_fills_gaps() {
        let mut first = empty_title("First");
        first.genres = vec!["A".to_owned(), "A".to_owned()];
        first.alt_names = vec!["Alt".to_owned()];
        // Некорректные числа не должны попасть в общий тайтл.
        first.year = Some(1500);
        first.rating = Some(11.0);

        let mut second = empty_title("Second");
        second.kind = TitleKind::Manhwa;
        second.description = Some("Description".to_owned());
        second.genres = vec!["A".to_owned(), "B".to_owned()];
        second.authors = vec!["Author".to_owned()];
        second.status = Some(TitleStatus::Ongoing);
        second.year = Some(2020);
        second.rating = Some(9.5);
        second.lang = Some("ru".to_owned());

        let merged =
            merge_source_metadata(&[source(1, 10, first), source(2, 20, second)]).expect("merged");

        assert_eq!(merged.name.as_deref(), Some("First"));
        assert_eq!(merged.kind.as_deref(), Some("manhwa"));
        assert_eq!(merged.description.as_deref(), Some("Description"));
        assert_eq!(merged.genres, vec!["A", "B"]);
        assert_eq!(merged.authors, vec!["Author"]);
        assert_eq!(merged.alt_names, vec!["Alt"]);
        assert_eq!(merged.status.as_deref(), Some("ongoing"));
        assert_eq!(merged.year, Some(2020));
        assert_eq!(merged.rating, Some(9.5));
        assert_eq!(merged.lang.as_deref(), Some("ru"));
    }

    #[test]
    fn unknown_kind_stays_other_and_empty_list_has_no_snapshot() {
        assert!(merge_source_metadata(&[]).is_none());

        let merged = merge_source_metadata(&[source(1, 10, empty_title("Only"))]).expect("merged");
        assert_eq!(merged.kind.as_deref(), Some("other"));
        assert_eq!(merged.name.as_deref(), Some("Only"));
    }
}
