use axum::{
    Json,
    body::Body,
    extract::{Extension, Multipart, State},
    http::{
        StatusCode,
        header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE},
    },
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::{auth::session::CurrentUser, error::AppError, state::AppState};

use super::{
    export,
    service::{self, ImportJobDto, MAX_FILE_BYTES},
};

#[derive(Debug, Serialize)]
pub struct ImportJobResponse {
    /// Последнее задание пользователя; `None` — импортов ещё не было.
    pub job: Option<ImportJobDto>,
}

#[derive(Debug, Serialize)]
pub struct StartedImportResponse {
    pub job: ImportJobDto,
}

/// Состояние последнего импорта: профиль опрашивает его, пока задание
/// выполняется, и показывает итог после завершения.
pub async fn latest(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ImportJobResponse>, AppError> {
    Ok(Json(ImportJobResponse {
        job: state.bookmark_imports.latest(user.id),
    }))
}

/// Скачивание библиотеки в формате закладок Remanga. Ответ — файл,
/// поэтому обработчик отдаёт байты и заголовки, а не JSON-экстрактор.
pub async fn export(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Response, AppError> {
    let bytes = export::export_bookmarks(&state.pool, user.id, service::SOURCE_CODE).await?;
    Ok((
        [
            (CONTENT_TYPE, "application/json; charset=utf-8"),
            (
                CONTENT_DISPOSITION,
                "attachment; filename=\"bookmarks.json\"",
            ),
            (CACHE_CONTROL, "no-store"),
        ],
        Body::from(bytes),
    )
        .into_response())
}

/// Запуск импорта закладок из JSON-экспорта. Файл только разбирается и
/// ставит фоновое задание; ответ не ждёт сотни запросов к источнику.
pub async fn start(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<StartedImportResponse>), AppError> {
    let mut uploaded = None;
    while let Some(field) = multipart.next_field().await.map_err(|error| {
        tracing::warn!(%error, "cannot read bookmarks multipart");
        AppError::bad_request("multipart", "cannot read multipart body")
    })? {
        if field.name() != Some("file") {
            continue;
        }
        uploaded = Some(field.bytes().await.map_err(|_| {
            AppError::bad_request("bookmarks_too_large", "bookmarks file is too large")
        })?);
        break;
    }

    let Some(bytes) = uploaded else {
        return Err(AppError::bad_request(
            "bookmarks_file_required",
            "multipart field `file` is required",
        ));
    };
    if bytes.len() > MAX_FILE_BYTES {
        return Err(AppError::bad_request(
            "bookmarks_too_large",
            format!(
                "bookmarks file must be at most {} MiB",
                MAX_FILE_BYTES / (1024 * 1024)
            ),
        ));
    }

    let (job, _handle) = service::start(&state, user.id, &bytes, service::SOURCE_CODE).await?;
    Ok((StatusCode::ACCEPTED, Json(StartedImportResponse { job })))
}
