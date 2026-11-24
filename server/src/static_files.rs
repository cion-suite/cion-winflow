use axum::{
    body::Body,
    extract::State,
    http::{
        HeaderValue, Method, Request, StatusCode,
        header::{ACCEPT, CACHE_CONTROL, CONTENT_TYPE},
    },
    response::{IntoResponse, Response},
};
use tower::ServiceExt;
use tower_http::services::ServeDir;

use crate::app::AppState;

const IMMUTABLE: &str = "public, max-age=31536000, immutable";
const NO_CACHE: &str = "no-cache";

/// Штатная раздача файлов и SPA-fallback: index.html получают только
/// навигационные запросы, а отсутствующий `/assets/*` всегда 404.
pub async fn serve_static(State(state): State<AppState>, req: Request<Body>) -> Response {
    let path = req.uri().path().to_owned();
    let navigation = is_navigation(&req);
    let head = req.method() == Method::HEAD;

    match ServeDir::new(&state.config.static_dir).oneshot(req).await {
        Ok(response) if response.status() != StatusCode::NOT_FOUND => {
            // ServeDir не выставляет кэш: хешированные ассеты можно кэшировать надолго,
            // остальное (index.html, theme.js) должно переживать деплой
            let cache = if path.starts_with("/assets/") {
                IMMUTABLE
            } else {
                NO_CACHE
            };
            let (mut parts, body) = response.into_parts();
            parts
                .headers
                .insert(CACHE_CONTROL, HeaderValue::from_static(cache));
            Response::from_parts(parts, Body::new(body))
        }
        Ok(_) => {
            // Ошибка деплоя не должна маскироваться index.html: /assets/* - всегда 404
            if navigation && !path.starts_with("/assets/") {
                index_response(&state, head).await
            } else {
                StatusCode::NOT_FOUND.into_response()
            }
        }
        // ServeDir не возвращает ошибок: его Error = Infallible
        Err(never) => match never {},
    }
}

async fn index_response(state: &AppState, head: bool) -> Response {
    let index = state.config.static_dir.join("index.html");
    match tokio::fs::read(&index).await {
        Ok(bytes) => {
            let body = if head {
                Body::empty()
            } else {
                Body::from(bytes)
            };
            let mut response = body.into_response();
            response.headers_mut().insert(
                CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            );
            response
                .headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static(NO_CACHE));
            response
        }
        Err(error) => {
            tracing::error!(path = %index.display(), %error, "cannot read SPA index.html");
            (
                StatusCode::NOT_FOUND,
                "SPA build not found: run `pnpm build`",
            )
                .into_response()
        }
    }
}

/// Навигация - это адресная строка или переход по ссылке. Запрос за скриптом,
/// стилем или картинкой навигацией не считается, поэтому не получает index.html.
fn is_navigation(req: &Request<Body>) -> bool {
    let fetch_mode = req
        .headers()
        .get("sec-fetch-mode")
        .and_then(|value| value.to_str().ok());
    if fetch_mode == Some("navigate") {
        return true;
    }

    req.headers()
        .get(ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|accept| {
            accept
                .split(',')
                .any(|part| part.trim_start().starts_with("text/html"))
        })
}
