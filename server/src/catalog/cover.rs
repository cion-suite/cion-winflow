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

use crate::{
    auth::session::{self, CurrentUser},
    error::AppError,
    extract::{ApiPath, ApiQuery},
    state::AppState,
};

/// Тело multipart-запроса ограничено чуть выше лимита файла: запас на обёртку.
pub const BODY_LIMIT: usize = 6 * 1024 * 1024;
pub const MAX_FILE_BYTES: usize = 5 * 1024 * 1024;

const FULL_MAX: (u32, u32) = (1200, 1800);
const THUMB_MAX: (u32, u32) = (320, 480);
const CACHE: &str = "private, max-age=86400";

#[derive(Debug, Deserialize)]
pub struct CoverParams {
    pub size: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CoverResponse {
    pub has_cover: bool,
    pub updated_at: i64,
}

struct ProcessedCover {
    full: Vec<u8>,
    thumb: Vec<u8>,
}

/// Загрузка обложки: только admin, только растровые форматы. Оригинал
/// не сохраняется и клиенту не отдаётся — только пережатые WebP.
pub async fn upload_cover(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(title_id): ApiPath<i64>,
    mut multipart: Multipart,
) -> Result<Json<CoverResponse>, AppError> {
    user.require_admin()?;

    let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM titles WHERE id = ?")
        .bind(title_id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
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
    let processed = tokio::task::spawn_blocking(move || process_cover(&bytes))
        .await
        .map_err(|error| {
            tracing::error!(%error, "cover processing task failed");
            AppError::Task
        })??;
    drop(permit);

    let relative = format!("covers/{title_id}");
    let directory = state.config.data_dir.join(&relative);
    tokio::fs::create_dir_all(&directory).await?;
    write_atomic(&directory, "full.webp", &processed.full).await?;
    write_atomic(&directory, "thumb.webp", &processed.thumb).await?;

    // max(now, updated_at + 1): ETag и `v=` меняются даже при двух загрузках
    // в одну секунду, а часы назад не откатывают версию.
    let updated_at = sqlx::query_scalar::<_, i64>(
        "UPDATE titles SET cover_path = ?, updated_at = max(?, updated_at + 1) \
         WHERE id = ? RETURNING updated_at",
    )
    .bind(&relative)
    .bind(session::now_unix())
    .bind(title_id)
    .fetch_optional(&state.pool)
    .await?;
    let Some(updated_at) = updated_at else {
        return Err(AppError::not_found("title_not_found", "title not found"));
    };

    Ok(Json(CoverResponse {
        has_cover: true,
        updated_at,
    }))
}

/// Обложка отдаётся только по сессии и с ETag: перезагрузка после замены
/// файла видна сразу, а повторные показы не читают диск заново.
pub async fn get_cover(
    State(state): State<AppState>,
    ApiPath(title_id): ApiPath<i64>,
    ApiQuery(params): ApiQuery<CoverParams>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let size = match params.size.as_deref().unwrap_or("thumb") {
        "thumb" => "thumb",
        "full" => "full",
        other => {
            return Err(AppError::bad_request(
                "validation",
                format!("size must be `thumb` or `full`, got `{other}`"),
            ));
        }
    };

    let row = sqlx::query_as::<_, (Option<String>, i64)>(
        "SELECT cover_path, updated_at FROM titles WHERE id = ?",
    )
    .bind(title_id)
    .fetch_optional(&state.pool)
    .await?;

    let Some((cover_path, updated_at)) = row else {
        return Err(cover_not_found());
    };
    let Some(relative) = cover_path else {
        return Err(cover_not_found());
    };

    let etag = format!("\"{title_id}-{updated_at}-{size}\"");
    if headers
        .get(IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(',').any(|tag| tag.trim() == etag))
    {
        return Ok(not_modified(&etag));
    }

    let path = state
        .config
        .data_dir
        .join(&relative)
        .join(format!("{size}.webp"));
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
fn process_cover(bytes: &[u8]) -> Result<ProcessedCover, AppError> {
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

    let full = fit(decoded.clone(), FULL_MAX);
    let thumb = fit(decoded, THUMB_MAX);

    Ok(ProcessedCover {
        full: encode_webp(&full)?,
        thumb: encode_webp(&thumb)?,
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
