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
use serde::Serialize;

use crate::{auth::session::CurrentUser, error::AppError, extract::ApiPath, state::AppState};

/// Тело multipart чуть больше файла: запас на обёртку.
pub const BODY_LIMIT: usize = 6 * 1024 * 1024;
pub const MAX_FILE_BYTES: usize = 5 * 1024 * 1024;

/// Аватар квадратный и небольшой: его показывают до 80 px, 512 хватает
/// для retina и не раздувает DATA_DIR.
const AVATAR_SIZE: u32 = 512;
const CACHE: &str = "private, max-age=86400";

#[derive(Debug, Serialize)]
pub struct AvatarResponse {
    /// Время загрузки: признак наличия аватара.
    pub avatar_updated_at: i64,
    /// Монотонная версия: cache-buster и ETag. Не сбрасывается при удалении.
    pub avatar_version: i64,
}

/// Загрузка своего аватара: любой пользователь, только растровые форматы.
/// Картинка обрезается по центру в квадрат, перекодируется в WebP и
/// атомарно заменяет файл; в БД хранятся время и версия, а не путь.
pub async fn upload_avatar(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    mut multipart: Multipart,
) -> Result<Json<AvatarResponse>, AppError> {
    let mut uploaded: Option<Bytes> = None;
    while let Some(field) = multipart.next_field().await.map_err(|error| {
        tracing::warn!(%error, "cannot read avatar multipart");
        AppError::bad_request("multipart", "cannot read multipart body")
    })? {
        if field.name() != Some("file") {
            continue;
        }
        uploaded =
            Some(field.bytes().await.map_err(|_| {
                AppError::bad_request("avatar_too_large", "avatar file is too large")
            })?);
        break;
    }

    let Some(bytes) = uploaded else {
        return Err(AppError::bad_request(
            "avatar_file_required",
            "multipart field `file` is required",
        ));
    };
    if bytes.len() > MAX_FILE_BYTES {
        return Err(AppError::bad_request(
            "avatar_too_large",
            format!(
                "avatar must be at most {} MiB",
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
    let processed = tokio::task::spawn_blocking(move || process_avatar(&bytes))
        .await
        .map_err(|error| {
            tracing::error!(%error, "avatar processing task failed");
            AppError::Task
        })??;
    drop(permit);

    // Файл и версия пишутся под локом пользователя: параллельное удаление
    // не должно оставить версию без файла или наоборот.
    let lock = state.avatar_lock(user.id);
    let stored = {
        let _guard = lock.lock().await;
        store_avatar(&state, user.id, &processed).await
    };
    state.release_avatar_lock(user.id);

    Ok(Json(stored?))
}

async fn store_avatar(
    state: &AppState,
    user_id: i64,
    bytes: &[u8],
) -> Result<AvatarResponse, AppError> {
    let relative = avatar_relative_path(user_id);
    let directory = state.config.data_dir.join(AVATAR_DIR);
    tokio::fs::create_dir_all(&directory).await?;
    write_atomic(&directory, &format!("{user_id}.webp"), bytes).await?;

    // updated_at — max(now, old + 1), чтобы не откатываться при часах назад;
    // version увеличивается всегда и не обнуляется удалением.
    let row = sqlx::query_as::<_, (i64, i64)>(
        "UPDATE users SET \
             avatar_updated_at = max(?, COALESCE(avatar_updated_at, 0) + 1), \
             avatar_version = avatar_version + 1 \
         WHERE id = ? RETURNING avatar_updated_at, avatar_version",
    )
    .bind(crate::auth::session::now_unix())
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("user_not_found", "user not found"))?;

    tracing::debug!(user_id, path = %relative, "avatar updated");
    Ok(AvatarResponse {
        avatar_updated_at: row.0,
        avatar_version: row.1,
    })
}

/// Удаление своего аватара. Идемпотентно: файла может уже не быть.
pub async fn delete_avatar(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<StatusCode, AppError> {
    let lock = state.avatar_lock(user.id);
    let removed = {
        let _guard = lock.lock().await;
        delete_avatar_locked(&state, user.id).await
    };
    state.release_avatar_lock(user.id);

    removed?;
    Ok(StatusCode::NO_CONTENT)
}

/// Версию не обнуляем: после удаления и новой загрузки в ту же секунду
/// ETag всё равно обязан измениться, иначе браузер покажет старый кэш.
async fn delete_avatar_locked(state: &AppState, user_id: i64) -> Result<(), AppError> {
    let updated = sqlx::query("UPDATE users SET avatar_updated_at = NULL WHERE id = ?")
        .bind(user_id)
        .execute(&state.pool)
        .await?;
    if updated.rows_affected() == 0 {
        return Err(AppError::not_found("user_not_found", "user not found"));
    }

    let path = state.config.data_dir.join(avatar_relative_path(user_id));
    if let Err(error) = tokio::fs::remove_file(&path).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(path = %path.display(), %error, "cannot remove avatar file");
    }

    Ok(())
}

/// Аватар отдаётся любой сессии: профили публичны внутри инсталляции.
/// Файл детерминированный, поэтому ETag строится из `avatar_version`.
pub async fn get_avatar(
    State(state): State<AppState>,
    ApiPath(user_id): ApiPath<i64>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let row = sqlx::query_as::<_, (Option<i64>, i64)>(
        "SELECT avatar_updated_at, avatar_version FROM users \
         WHERE id = ? AND disabled_at IS NULL",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?;

    let Some((Some(_), version)) = row else {
        return Err(avatar_not_found());
    };

    let etag = format!("\"{user_id}-{version}\"");
    if headers
        .get(IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(',').any(|tag| tag.trim() == etag))
    {
        return Ok(not_modified(&etag));
    }

    let path = state.config.data_dir.join(avatar_relative_path(user_id));
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "avatar file is missing");
            return Err(avatar_not_found());
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

const AVATAR_DIR: &str = "avatars";

fn avatar_relative_path(user_id: i64) -> String {
    format!("{AVATAR_DIR}/{user_id}.webp")
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

fn avatar_not_found() -> AppError {
    AppError::not_found("avatar_not_found", "avatar not found")
}

async fn write_atomic(
    directory: &std::path::Path,
    name: &str,
    bytes: &[u8],
) -> Result<(), AppError> {
    let target = directory.join(name);
    let temporary = directory.join(format!(
        ".{name}.{}.tmp",
        crate::auth::session::new_secret()?
    ));
    tokio::fs::write(&temporary, bytes).await?;
    tokio::fs::rename(&temporary, &target).await?;
    Ok(())
}

/// Декодирование и перекодирование — CPU-работа: только внутри
/// spawn_blocking. Лимиты защищают от «бомб», формат — из магических байтов.
fn process_avatar(bytes: &[u8]) -> Result<Vec<u8>, AppError> {
    let format = image::guess_format(bytes).map_err(|error| {
        tracing::warn!(%error, "avatar format is not recognized");
        invalid_avatar()
    })?;
    if !matches!(
        format,
        image::ImageFormat::Jpeg
            | image::ImageFormat::Png
            | image::ImageFormat::WebP
            | image::ImageFormat::Gif
    ) {
        return Err(invalid_avatar());
    }

    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(64 * 1024 * 1024);
    limits.max_image_width = Some(20_000);
    limits.max_image_height = Some(20_000);
    reader.limits(limits);

    let decoded = reader.decode().map_err(|error| {
        tracing::warn!(%error, "cannot decode avatar image");
        invalid_avatar()
    })?;

    // resize_to_fill обрезает по центру и масштабирует: круглый аватар
    // не должен зависеть от пропорций исходника.
    let fitted = decoded.resize_to_fill(
        AVATAR_SIZE,
        AVATAR_SIZE,
        image::imageops::FilterType::Lanczos3,
    );

    let mut encoded = Vec::new();
    fitted
        .write_to(
            &mut std::io::Cursor::new(&mut encoded),
            image::ImageFormat::WebP,
        )
        .map_err(|error| {
            tracing::error!(%error, "cannot encode avatar as WebP");
            AppError::Internal
        })?;
    Ok(encoded)
}

fn invalid_avatar() -> AppError {
    AppError::bad_request(
        "avatar_invalid",
        "avatar must be a JPEG, PNG, WebP or GIF image",
    )
}
