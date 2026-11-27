use axum::{
    extract::{Extension, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, ETAG, IF_NONE_MATCH},
    },
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::{
    auth::session::{CurrentUser, now_unix},
    catalog::{content::replace_pages, selection},
    error::AppError,
    extract::{ApiPath, ApiQuery},
    source_auth::{SourceAuth, service as source_auth_service},
    sources::{
        Capability, ChapterKind, ContentKind, ExternalId, SourceChapter, SourceContent,
        SourceError, SourceKind, http::header_pairs,
    },
    state::AppState,
};

use super::cache::{CachedImage, ImageCache};

const CACHE_CONTROL_VALUE: &str = "private, max-age=86400";
const MAX_REFRESHES: u8 = 2;

#[derive(Debug, sqlx::FromRow)]
struct PageSourceRow {
    page_id: i64,
    idx: i64,
    chapter_id: i64,
    chapter_external_id: String,
    chapter_key: String,
    chapter_kind: String,
    content_kind: Option<String>,
    number_text: Option<String>,
    number_milli: Option<i64>,
    volume: Option<i64>,
    name: Option<String>,
    group_name: Option<String>,
    language: Option<String>,
    is_locked: bool,
    /// Глава открывается только с учёткой источника.
    auth_required: bool,
    external_url: Option<String>,
    source_entry_id: i64,
    source_id: i64,
    title_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct CoverParams {
    #[serde(rename = "ref")]
    pub reference: Option<String>,
}

pub async fn page_image(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ApiPath(page_id): ApiPath<i64>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let row = load_page(&state, page_id).await?;

    let runtime = {
        let registry = state.sources.read().await;
        registry.runtime_by_id(row.source_id)
    }
    .ok_or(SourceError::Disabled)?;
    // Только источник с per-user входом может скрывать главу за 404/401.
    let auth_possible = runtime.descriptor.has(Capability::Auth);
    let adapter = runtime.adapter()?.clone();

    let auth = source_auth_service::resolve(&state, user.id, row.source_id).await?;
    // Помеченную главу без собственной учётки не отдаём даже из кэша.
    if row.auth_required && auth.is_none() {
        return Err(source_auth_service::auth_not_configured());
    }

    // Ключ — запись источника, глава и номер страницы; подписанный URL не входит.
    // Картинки, полученные с личной учёткой, кэшируются в её пространстве.
    let key = if row.auth_required {
        format!(
            "user:{}:{}:{}:{}",
            user.id, row.source_entry_id, row.chapter_external_id, row.idx
        )
    } else {
        format!(
            "{}:{}:{}",
            row.source_entry_id, row.chapter_external_id, row.idx
        )
    };
    serve_cached(&state.image_cache, &key, &headers, || {
        fetch_page(&state, &adapter, &row, auth.as_ref(), auth_possible)
    })
    .await
}

pub async fn source_cover(
    State(state): State<AppState>,
    ApiPath(code): ApiPath<String>,
    ApiQuery(params): ApiQuery<CoverParams>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    // Обложки нужны и поиску по источникам, а не только админке: источник
    // и ref всё равно приходят из allowlist адаптера, а не от клиента напрямую.
    let reference = params
        .reference
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::bad_request("validation", "`ref` is required"))?;

    let runtime = {
        let registry = state.sources.read().await;
        registry.runtime(&code)
    }
    .ok_or_else(|| AppError::not_found("source_not_found", "source is not registered"))?;
    // Обложку можно посмотреть и у выключенного источника: поиск показывает
    // только включённые, но выдача в открытой вкладке не должна ломаться.
    let adapter = runtime.adapter_unchecked()?.clone();

    let url = adapter.resolve_cover(reference).map_err(map_image_error)?;
    let source_id = runtime.id;
    let key = format!("cover:{source_id}:{reference}");

    serve_cached(&state.image_cache, &key, &headers, || async move {
        let _permit = state
            .image_download_semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AppError::Task)?;
        let response = crate::sources::http::fetch_bytes(
            &state.image_client,
            url.clone(),
            &[],
            crate::sources::http::IMAGE_BODY_LIMIT,
        )
        .await?;
        if response.status != 200 {
            return Err(AppError::bad_gateway(
                "source_upstream_failed",
                "source cover request failed",
            ));
        }
        Ok(response.body)
    })
    .await
}

async fn fetch_page(
    state: &AppState,
    adapter: &SourceKind,
    row: &PageSourceRow,
    auth: Option<&SourceAuth>,
    auth_possible: bool,
) -> Result<Vec<u8>, AppError> {
    let mut refreshes = 0_u8;

    loop {
        let reference = load_reference(state, row.page_id).await?;
        let url = adapter.resolve_image(&reference).map_err(map_image_error)?;
        // Сессионные заголовки уходят только на хосты источника, не на CDN.
        let signed = adapter.auth_headers(auth, &url);
        let headers = header_pairs(&[], &signed);

        let _permit = state
            .image_download_semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AppError::Task)?;

        let response = crate::sources::http::fetch_bytes(
            &state.image_client,
            url.clone(),
            &headers,
            crate::sources::http::IMAGE_BODY_LIMIT,
        )
        .await?;

        if response.status == 200 {
            sqlx::query("UPDATE pages SET image_url = ? WHERE id = ?")
                .bind(url.as_str())
                .bind(row.page_id)
                .execute(&state.pool)
                .await?;
            return Ok(response.body);
        }

        if matches!(response.status, 401 | 403 | 410) {
            if refreshes >= MAX_REFRESHES {
                // 401/403/410 картинки не означают, что глава закрыта для
                // всех: это может быть личная auth-проблема или блок сайта.
                return Err(image_status_error(
                    response.status,
                    auth.is_some(),
                    auth_possible,
                ));
            }
            refreshes += 1;
            refresh_pages(state, adapter, row, auth, auth_possible).await?;
            continue;
        }

        return Err(AppError::bad_gateway(
            "source_upstream_failed",
            format!(
                "source image request failed with status {}",
                response.status
            ),
        ));
    }
}

/// Ошибка скачивания картинки по HTTP-статусу. Глобальная доступность главы
/// здесь не меняется: 401/403/410 бывают и персональными, и временными.
fn image_status_error(status: u16, auth_present: bool, auth_possible: bool) -> AppError {
    match status {
        401 if auth_present => {
            AppError::bad_gateway("source_auth_invalid", "source rejected the saved account")
        }
        401 if auth_possible => source_auth_service::auth_not_configured(),
        403 => AppError::bad_gateway("source_blocked", "source blocked the image request"),
        _ => AppError::not_found(
            "source_not_found",
            "chapter images are no longer available at source",
        ),
    }
}

async fn refresh_pages(
    state: &AppState,
    adapter: &SourceKind,
    row: &PageSourceRow,
    auth: Option<&SourceAuth>,
    auth_possible: bool,
) -> Result<(), AppError> {
    let chapter = source_chapter(row)?;

    match adapter.content(&chapter, auth).await {
        Ok(SourceContent::Images { pages }) => {
            let pages = replace_pages(&state.pool, row.chapter_id, &pages).await?;
            sqlx::query(
                "UPDATE chapters SET availability = 'available', availability_checked_at = ?, \
                        content_kind = 'images', page_count = ?, is_locked = 0 WHERE id = ?",
            )
            .bind(now_unix())
            .bind(i64::try_from(pages.len()).unwrap_or(i64::MAX))
            .bind(row.chapter_id)
            .execute(&state.pool)
            .await?;
            selection::recompute_stats(&state.pool, row.source_entry_id).await?;
            Ok(())
        }
        Ok(_) => Err(AppError::bad_gateway(
            "source_parse_failed",
            "source returned a non-image chapter",
        )),
        Err(SourceError::Locked) => {
            mark_locked(state, row).await?;
            Err(AppError::forbidden(
                "source_locked",
                "chapter is locked at source",
            ))
        }
        Err(error @ SourceError::NotFound) => {
            if auth.is_none() && auth_possible {
                // 404 без входа у Remanga неоднозначен: глава может быть скрыта.
                return Err(source_auth_service::auth_not_configured());
            }
            mark_missing(state, row).await?;
            Err(error.into_app(auth.is_some()))
        }
        Err(error @ (SourceError::AuthRequired | SourceError::AuthMaybeRequired)) => {
            // Персональную ошибку учётки фиксирует путь контента; здесь
            // достаточно правильного кода ответа.
            Err(error.into_app(auth.is_some()))
        }
        Err(error) => Err(error.into_app(auth.is_some())),
    }
}

async fn mark_locked(state: &AppState, row: &PageSourceRow) -> Result<(), AppError> {
    mark(state, row, "locked").await
}

async fn mark_missing(state: &AppState, row: &PageSourceRow) -> Result<(), AppError> {
    mark(state, row, "missing").await
}

async fn mark(state: &AppState, row: &PageSourceRow, availability: &str) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE chapters SET availability = ?, availability_checked_at = ?, is_locked = ? \
         WHERE id = ?",
    )
    .bind(availability)
    .bind(now_unix())
    .bind(i64::from(availability == "locked"))
    .bind(row.chapter_id)
    .execute(&state.pool)
    .await?;
    selection::recompute_stats(&state.pool, row.source_entry_id).await?;
    selection::recompute_preferred(&state.pool, row.title_id).await?;
    Ok(())
}

async fn load_page(state: &AppState, page_id: i64) -> Result<PageSourceRow, AppError> {
    sqlx::query_as::<_, PageSourceRow>(
        "SELECT p.id AS page_id, p.idx, ch.id AS chapter_id, \
                ch.external_id AS chapter_external_id, ch.chapter_key, ch.chapter_kind, \
                ch.content_kind, ch.number_text, ch.number_milli, ch.volume, ch.name, \
                ch.group_name, ch.language, ch.is_locked, ch.auth_required, ch.external_url, \
                e.id AS source_entry_id, e.source_id, t.id AS title_id \
         FROM pages p \
         JOIN chapters ch ON ch.id = p.chapter_id \
         JOIN source_entries e ON e.id = ch.source_entry_id \
         JOIN titles t ON t.id = e.title_id \
         WHERE p.id = ? AND ch.removed_at IS NULL",
    )
    .bind(page_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("page_not_found", "page not found"))
}

async fn load_reference(state: &AppState, page_id: i64) -> Result<String, AppError> {
    let row = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT image_ref, image_url FROM pages WHERE id = ?",
    )
    .bind(page_id)
    .fetch_optional(&state.pool)
    .await?;

    // Страница могла исчезнуть после перезапроса списка: это 404, а не 500
    let (image_ref, image_url) =
        row.ok_or_else(|| AppError::not_found("page_not_found", "page no longer exists"))?;

    Ok(image_url
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(image_ref))
}

async fn serve_cached<F, Fut>(
    cache: &ImageCache,
    key: &str,
    headers: &HeaderMap,
    fetch: F,
) -> Result<Response, AppError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>, AppError>>,
{
    if let Some(image) = cache.get(key).await {
        return Ok(serve(image, headers));
    }

    let flight = cache.flight(key);
    let guard = flight.lock().await;
    let result = async {
        if let Some(image) = cache.get(key).await {
            return Ok(serve(image, headers));
        }
        let bytes = fetch().await?;
        let image = cache.store(key, bytes).await?;
        Ok(serve(image, headers))
    }
    .await;
    drop(guard);
    cache.release(key);
    result
}

fn serve(image: CachedImage, headers: &HeaderMap) -> Response {
    let cache_control = HeaderValue::from_static(CACHE_CONTROL_VALUE);

    if headers
        .get(IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(',').any(|tag| tag.trim() == image.etag))
    {
        let mut response = StatusCode::NOT_MODIFIED.into_response();
        response.headers_mut().insert(CACHE_CONTROL, cache_control);
        if let Ok(value) = HeaderValue::from_str(&image.etag) {
            response.headers_mut().insert(ETAG, value);
        }
        return response;
    }

    let mut response = image.bytes.into_response();
    let response_headers = response.headers_mut();
    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static(image.content_type));
    response_headers.insert(CACHE_CONTROL, cache_control);
    response_headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_static("inline; filename=\"image\""),
    );
    response_headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    if let Ok(value) = HeaderValue::from_str(&image.etag) {
        response_headers.insert(ETAG, value);
    }
    response
}

fn source_chapter(row: &PageSourceRow) -> Result<SourceChapter, AppError> {
    Ok(SourceChapter {
        external_id: ExternalId::new(row.chapter_external_id.clone())
            .map_err(|_| AppError::Internal)?,
        chapter_key: row.chapter_key.clone(),
        number_text: row.number_text.clone(),
        number_milli: row.number_milli,
        volume: row.volume.and_then(|volume| u16::try_from(volume).ok()),
        chapter_kind: ChapterKind::from_db(&row.chapter_kind).ok_or(AppError::Internal)?,
        content_kind: row.content_kind.as_deref().and_then(ContentKind::from_db),
        name: row.name.clone(),
        group_name: row.group_name.clone(),
        language: row.language.clone(),
        is_locked: row.is_locked,
        external_url: row.external_url.clone(),
        published_at: None,
        raw_json: None,
    })
}

fn map_image_error(error: SourceError) -> AppError {
    match error {
        SourceError::Config(detail) => AppError::bad_gateway(
            "source_image_host_not_allowed",
            format!("image host is not allowed: {detail}"),
        ),
        SourceError::Parse { .. } => {
            AppError::bad_gateway("source_parse_failed", "cannot resolve source image URL")
        }
        other => other.into(),
    }
}
