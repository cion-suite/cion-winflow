use std::collections::HashSet;

use axum::{
    Json,
    body::Bytes,
    extract::{Extension, Multipart, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CACHE_CONTROL, CONTENT_TYPE, ETAG, IF_NONE_MATCH},
    },
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{
    auth::session::{self, CurrentUser},
    error::AppError,
    extract::{ApiPath, ApiQuery},
    sources::{
        CoverSize, SourceCovers, SourceKind,
        http::{IMAGE_BODY_LIMIT, fetch_bytes},
    },
    state::AppState,
};

/// Тело multipart-запроса ограничено чуть выше лимита файла: запас на обёртку.
pub const BODY_LIMIT: usize = 6 * 1024 * 1024;
pub const MAX_FILE_BYTES: usize = 5 * 1024 * 1024;

const CACHE: &str = "private, max-age=86400";

const fn max_dimensions(size: CoverSize) -> (u32, u32) {
    match size {
        CoverSize::Low => (320, 480),
        CoverSize::Mid => (640, 960),
        CoverSize::High => (1200, 1800),
    }
}

#[derive(Debug, Deserialize)]
pub struct CoverParams {
    pub size: Option<String>,
}

impl CoverParams {
    fn resolve(&self) -> Result<CoverSize, AppError> {
        match self.size.as_deref().map(str::trim) {
            None | Some("") => Ok(CoverSize::High),
            Some(raw) => CoverSize::from_db(&raw.to_lowercase()).ok_or_else(|| {
                AppError::bad_request(
                    "validation",
                    format!("size must be `low`, `mid` or `high`, got `{raw}`"),
                )
            }),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CoverResponse {
    pub has_cover: bool,
    pub updated_at: i64,
}

struct ProcessedCover {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
}

/// Загрузка обложки в конкретный размер: только admin, только растровые
/// форматы. Оригинал не сохраняется и клиенту не отдаётся — только WebP.
pub async fn upload_cover(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(title_id): ApiPath<i64>,
    ApiQuery(params): ApiQuery<CoverParams>,
    mut multipart: Multipart,
) -> Result<Json<CoverResponse>, AppError> {
    user.require_admin()?;
    let size = params.resolve()?;

    if !title_exists(&state.pool, title_id).await? {
        return Err(AppError::not_found("title_not_found", "title not found"));
    }

    let mut uploaded: Option<Bytes> = None;
    while let Some(field) = multipart.next_field().await.map_err(|error| {
        tracing::warn!(%error, "cannot read cover multipart");
        AppError::bad_request("multipart", "cannot read multipart body")
    })? {
        if field.name() != Some("file") {
            continue;
        }
        uploaded =
            Some(field.bytes().await.map_err(|_| {
                AppError::bad_request("cover_too_large", "cover file is too large")
            })?);
        break;
    }

    let Some(bytes) = uploaded else {
        return Err(AppError::bad_request(
            "cover_file_required",
            "multipart field `file` is required",
        ));
    };
    if bytes.len() > MAX_FILE_BYTES {
        return Err(AppError::bad_request(
            "cover_too_large",
            format!(
                "cover must be at most {} MiB",
                MAX_FILE_BYTES / (1024 * 1024)
            ),
        ));
    }

    let permit = state
        .image_semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| AppError::Task)?;
    let processed = tokio::task::spawn_blocking(move || process_cover(&bytes, size))
        .await
        .map_err(|error| {
            tracing::error!(%error, "cover processing task failed");
            AppError::Task
        })??;
    drop(permit);

    // Запись файла и строки — под локом (title, size): автоимпорт того же
    // размера не должен перетереть ручную загрузку и наоборот.
    let lock = state.cover_lock(title_id, size);
    let updated_at = {
        let _guard = lock.lock().await;
        upsert_manual_cover(&state, title_id, size, &processed).await
    };
    state.release_cover_lock(title_id, size);
    let updated_at = updated_at?;

    Ok(Json(CoverResponse {
        has_cover: true,
        updated_at,
    }))
}

async fn upsert_manual_cover(
    state: &AppState,
    title_id: i64,
    size: CoverSize,
    processed: &ProcessedCover,
) -> Result<i64, AppError> {
    let old_path = sqlx::query_scalar::<_, String>(
        "SELECT path FROM title_covers WHERE title_id = ? AND size = ?",
    )
    .bind(title_id)
    .bind(size.as_str())
    .fetch_optional(&state.pool)
    .await?;

    let relative = format!("covers/{title_id}");
    let directory = state.config.data_dir.join(&relative);
    tokio::fs::create_dir_all(&directory).await?;
    write_atomic(
        &directory,
        &format!("{}.webp", size.as_str()),
        &processed.bytes,
    )
    .await?;
    let path = format!("{relative}/{}.webp", size.as_str());

    sqlx::query(
        "INSERT INTO title_covers (title_id, size, path, width, height, source_id, updated_at) \
         VALUES (?, ?, ?, ?, ?, NULL, ?) \
         ON CONFLICT (title_id, size) DO UPDATE SET \
             path = excluded.path, width = excluded.width, height = excluded.height, \
             source_id = NULL, updated_at = excluded.updated_at",
    )
    .bind(title_id)
    .bind(size.as_str())
    .bind(&path)
    .bind(i64::from(processed.width))
    .bind(i64::from(processed.height))
    .bind(session::now_unix())
    .execute(&state.pool)
    .await?;

    // Замена мигрированной обложки меняет имя файла (thumb/full -> size):
    // старый файл удаляем, если на него больше никто не ссылается.
    if let Some(old_path) = old_path
        && old_path != path
    {
        remove_unreferenced_cover_file(state, &old_path).await;
    }

    bump_title_updated_at(&state.pool, title_id)
        .await?
        .ok_or_else(|| AppError::not_found("title_not_found", "title not found"))
}

/// Удаляет один размер. Файл стирается только когда на него больше никто
/// не ссылается: после миграции low и mid могли указывать на один thumb.
pub async fn delete_cover(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(title_id): ApiPath<i64>,
    ApiQuery(params): ApiQuery<CoverParams>,
) -> Result<Json<CoverResponse>, AppError> {
    user.require_admin()?;
    let size = params.resolve()?;

    if !title_exists(&state.pool, title_id).await? {
        return Err(AppError::not_found("title_not_found", "title not found"));
    }

    let lock = state.cover_lock(title_id, size);
    let response = {
        let _guard = lock.lock().await;
        delete_cover_locked(&state, title_id, size).await
    };
    state.release_cover_lock(title_id, size);

    Ok(Json(response?))
}

async fn delete_cover_locked(
    state: &AppState,
    title_id: i64,
    size: CoverSize,
) -> Result<CoverResponse, AppError> {
    let path = sqlx::query_scalar::<_, String>(
        "SELECT path FROM title_covers WHERE title_id = ? AND size = ?",
    )
    .bind(title_id)
    .bind(size.as_str())
    .fetch_optional(&state.pool)
    .await?;

    let updated_at = match path {
        Some(path) => {
            sqlx::query("DELETE FROM title_covers WHERE title_id = ? AND size = ?")
                .bind(title_id)
                .bind(size.as_str())
                .execute(&state.pool)
                .await?;

            remove_unreferenced_cover_file(state, &path).await;

            bump_title_updated_at(&state.pool, title_id)
                .await?
                .ok_or_else(|| AppError::not_found("title_not_found", "title not found"))?
        }
        None => {
            sqlx::query_scalar::<_, i64>("SELECT updated_at FROM titles WHERE id = ?")
                .bind(title_id)
                .fetch_one(&state.pool)
                .await?
        }
    };

    Ok(CoverResponse {
        has_cover: has_any_cover(&state.pool, title_id).await?,
        updated_at,
    })
}

/// Удаляет файл обложки, только если на него больше не ссылается ни один
/// размер: после миграции low и mid могли указывать на один thumb.webp.
async fn remove_unreferenced_cover_file(state: &AppState, path: &str) {
    let still_used =
        sqlx::query_scalar::<_, i64>("SELECT 1 FROM title_covers WHERE path = ? LIMIT 1")
            .bind(path)
            .fetch_optional(&state.pool)
            .await;
    match still_used {
        Ok(None) => {
            let file = state.config.data_dir.join(path);
            if let Err(error) = tokio::fs::remove_file(&file).await
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(path = %file.display(), %error, "cannot remove cover file");
            }
        }
        Ok(Some(_)) => {}
        Err(error) => tracing::warn!(%error, "cannot check cover file references"),
    }
}

/// Обложка отдаётся только по сессии и с ETag. Если запрошенного размера
/// у тайтла нет, отдаётся ближайший доступный — клиент всегда просит
/// семантический размер и получает картинку, пока есть хоть один вариант.
pub async fn get_cover(
    State(state): State<AppState>,
    ApiPath(title_id): ApiPath<i64>,
    ApiQuery(params): ApiQuery<CoverParams>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let size = params.resolve()?;

    let updated_at = sqlx::query_scalar::<_, i64>("SELECT updated_at FROM titles WHERE id = ?")
        .bind(title_id)
        .fetch_optional(&state.pool)
        .await?;
    let Some(updated_at) = updated_at else {
        return Err(cover_not_found());
    };

    let stored = sqlx::query_as::<_, (String, String)>(
        "SELECT size, path FROM title_covers WHERE title_id = ?",
    )
    .bind(title_id)
    .fetch_all(&state.pool)
    .await?;

    let served = size.fallback_order().into_iter().find_map(|candidate| {
        stored
            .iter()
            .find(|(stored_size, _)| stored_size.as_str() == candidate.as_str())
            .map(|(_, path)| (candidate, path.clone()))
    });
    let Some((served_size, relative)) = served else {
        return Err(cover_not_found());
    };

    let etag = format!("\"{title_id}-{updated_at}-{}\"", served_size.as_str());
    if headers
        .get(IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(',').any(|tag| tag.trim() == etag))
    {
        return Ok(not_modified(&etag));
    }

    let path = state.config.data_dir.join(&relative);
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "cover file is missing");
            return Err(cover_not_found());
        }
    };

    let mut response = bytes.into_response();
    let response_headers = response.headers_mut();
    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("image/webp"));
    response_headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE));
    response_headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    if let Ok(value) = HeaderValue::from_str(&etag) {
        response_headers.insert(ETAG, value);
    }

    Ok(response)
}

/// Автоматически скачивает обложки источника после импорта или привязки.
/// Заполняет только отсутствующие размеры: ручную обложку не перетирает.
/// Ошибка сети или разбора не ломает импорт — она только логируется.
pub async fn import_source_covers(
    state: &AppState,
    title_id: i64,
    source_id: i64,
    adapter: &SourceKind,
    covers: &SourceCovers,
) -> i64 {
    if covers.is_empty() {
        return 0;
    }

    // Считаем размер занятым, только если файл реально на диске: строка без
    // файла (потерянный DATA_DIR) должна лечиться повторной загрузкой.
    let rows = match sqlx::query_as::<_, (String, String)>(
        "SELECT size, path FROM title_covers WHERE title_id = ?",
    )
    .bind(title_id)
    .fetch_all(&state.pool)
    .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(title_id, %error, "cannot read existing title covers");
            return 0;
        }
    };
    let mut present: HashSet<String> = HashSet::new();
    for (stored_size, stored_path) in rows {
        if tokio::fs::metadata(state.config.data_dir.join(&stored_path))
            .await
            .is_ok()
        {
            present.insert(stored_size);
        }
    }

    let mut stored = 0_i64;
    for (size, reference) in covers.entries() {
        if present.contains(size.as_str()) {
            continue;
        }
        match store_source_cover(state, title_id, source_id, adapter, size, reference).await {
            Ok(true) => stored += 1,
            Ok(false) => {}
            Err(error) => {
                tracing::warn!(
                    title_id,
                    size = %size,
                    error = %error,
                    "cannot import source cover"
                );
            }
        }
    }

    if stored > 0
        && let Err(error) = bump_title_updated_at(&state.pool, title_id).await
    {
        tracing::warn!(title_id, %error, "cannot bump title after cover import");
    }

    stored
}

async fn store_source_cover(
    state: &AppState,
    title_id: i64,
    source_id: i64,
    adapter: &SourceKind,
    size: CoverSize,
    reference: &str,
) -> Result<bool, AppError> {
    let url = adapter.resolve_cover(reference)?;

    let bytes = {
        let _permit = state
            .image_download_semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AppError::Task)?;
        let response = fetch_bytes(&state.image_client, url, &[], IMAGE_BODY_LIMIT).await?;
        if response.status != 200 {
            return Err(AppError::bad_gateway(
                "source_upstream_failed",
                "source cover request failed",
            ));
        }
        response.body
    };

    let processed = {
        let _permit = state
            .image_semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AppError::Task)?;
        tokio::task::spawn_blocking(move || process_cover(&bytes, size))
            .await
            .map_err(|error| {
                tracing::error!(%error, "cover processing task failed");
                AppError::Task
            })??
    };

    // Скачивание и декодирование — вне лока; сверка с ручной загрузкой
    // и запись файла со строкой — под локом (title, size).
    let lock = state.cover_lock(title_id, size);
    let stored = {
        let _guard = lock.lock().await;
        store_source_cover_locked(state, title_id, source_id, size, &processed).await
    };
    state.release_cover_lock(title_id, size);
    stored
}

async fn store_source_cover_locked(
    state: &AppState,
    title_id: i64,
    source_id: i64,
    size: CoverSize,
    processed: &ProcessedCover,
) -> Result<bool, AppError> {
    let existing = sqlx::query_scalar::<_, String>(
        "SELECT path FROM title_covers WHERE title_id = ? AND size = ?",
    )
    .bind(title_id)
    .bind(size.as_str())
    .fetch_optional(&state.pool)
    .await?;

    // Пока шло скачивание, размер могла занять ручная загрузка или другой
    // источник. Существующий файл не перетираем; отсутствующий — лечим.
    if let Some(path) = &existing
        && tokio::fs::metadata(state.config.data_dir.join(path))
            .await
            .is_ok()
    {
        return Ok(false);
    }

    let relative = format!("covers/{title_id}");
    let directory = state.config.data_dir.join(&relative);
    tokio::fs::create_dir_all(&directory).await?;
    write_atomic(
        &directory,
        &format!("{}.webp", size.as_str()),
        &processed.bytes,
    )
    .await?;
    let path = format!("{relative}/{}.webp", size.as_str());

    sqlx::query(
        "INSERT INTO title_covers (title_id, size, path, width, height, source_id, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT (title_id, size) DO UPDATE SET \
             path = excluded.path, width = excluded.width, height = excluded.height, \
             source_id = excluded.source_id, updated_at = excluded.updated_at",
    )
    .bind(title_id)
    .bind(size.as_str())
    .bind(&path)
    .bind(i64::from(processed.width))
    .bind(i64::from(processed.height))
    .bind(source_id)
    .bind(session::now_unix())
    .execute(&state.pool)
    .await?;

    if let Some(old_path) = existing
        && old_path != path
    {
        remove_unreferenced_cover_file(state, &old_path).await;
    }

    Ok(true)
}

async fn title_exists(pool: &SqlitePool, title_id: i64) -> Result<bool, AppError> {
    let row = sqlx::query_scalar::<_, i64>("SELECT 1 FROM titles WHERE id = ?")
        .bind(title_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

async fn has_any_cover(pool: &SqlitePool, title_id: i64) -> Result<bool, AppError> {
    let row = sqlx::query_scalar::<_, i64>("SELECT 1 FROM title_covers WHERE title_id = ? LIMIT 1")
        .bind(title_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

async fn bump_title_updated_at(pool: &SqlitePool, title_id: i64) -> Result<Option<i64>, AppError> {
    // max(now, updated_at + 1): ETag и `v=` меняются даже при двух загрузках
    // в одну секунду, а часы назад не откатывают версию.
    Ok(sqlx::query_scalar::<_, i64>(
        "UPDATE titles SET updated_at = max(?, updated_at + 1) WHERE id = ? RETURNING updated_at",
    )
    .bind(session::now_unix())
    .bind(title_id)
    .fetch_optional(pool)
    .await?)
}

fn not_modified(etag: &str) -> Response {
    let mut response = StatusCode::NOT_MODIFIED.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static(CACHE));
    if let Ok(value) = HeaderValue::from_str(etag) {
        response.headers_mut().insert(ETAG, value);
    }
    response
}

fn cover_not_found() -> AppError {
    AppError::not_found("cover_not_found", "cover not found")
}

async fn write_atomic(
    directory: &std::path::Path,
    name: &str,
    bytes: &[u8],
) -> Result<(), AppError> {
    let target = directory.join(name);
    // Уникальный суффикс: две одновременные загрузки не перетрут tmp друг друга
    let temporary = directory.join(format!(".{name}.{}.tmp", session::new_secret()?));
    tokio::fs::write(&temporary, bytes).await?;
    tokio::fs::rename(&temporary, &target).await?;
    Ok(())
}

/// Декодирование, ресайз и WebP-перекодирование — CPU-работа: вызывается
/// только внутри spawn_blocking, лимиты защищают от «бомб».
fn process_cover(bytes: &[u8], size: CoverSize) -> Result<ProcessedCover, AppError> {
    let format = image::guess_format(bytes).map_err(|error| {
        tracing::warn!(%error, "cover format is not recognized");
        invalid_cover()
    })?;
    if !matches!(
        format,
        image::ImageFormat::Jpeg
            | image::ImageFormat::Png
            | image::ImageFormat::WebP
            | image::ImageFormat::Gif
    ) {
        return Err(invalid_cover());
    }

    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(64 * 1024 * 1024);
    limits.max_image_width = Some(20_000);
    limits.max_image_height = Some(20_000);
    reader.limits(limits);

    let decoded = reader.decode().map_err(|error| {
        tracing::warn!(%error, "cannot decode cover image");
        invalid_cover()
    })?;

    let fitted = fit(decoded, max_dimensions(size));
    let width = fitted.width();
    let height = fitted.height();

    Ok(ProcessedCover {
        bytes: encode_webp(&fitted)?,
        width,
        height,
    })
}

fn fit(image: image::DynamicImage, (max_width, max_height): (u32, u32)) -> image::DynamicImage {
    if image.width() > max_width || image.height() > max_height {
        image.resize(max_width, max_height, image::imageops::FilterType::Lanczos3)
    } else {
        image
    }
}

fn encode_webp(image: &image::DynamicImage) -> Result<Vec<u8>, AppError> {
    let mut encoded = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut encoded),
            image::ImageFormat::WebP,
        )
        .map_err(|error| {
            tracing::error!(%error, "cannot encode cover as WebP");
            AppError::Internal
        })?;
    Ok(encoded)
}

fn invalid_cover() -> AppError {
    AppError::bad_request(
        "cover_invalid",
        "cover must be a JPEG, PNG, WebP or GIF image",
    )
}
