use axum::{
    Json,
    extract::{
        FromRequest, FromRequestParts, Path, Query, Request,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::request::Parts,
};

use crate::error::AppError;

// Экстракторы с ошибкой в формате API: axum по умолчанию отвечает plain text,
// а клиент и документация ожидают `{ code, message }`.

pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    Json<T>: FromRequest<S, Rejection = JsonRejection>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(request, state).await {
            Ok(Json(value)) => Ok(Self(value)),
            Err(rejection) => {
                tracing::warn!(%rejection, "invalid JSON request");
                Err(validation(rejection.body_text()))
            }
        }
    }
}

pub struct ApiQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    Query<T>: FromRequestParts<S, Rejection = QueryRejection>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Query::<T>::from_request_parts(parts, state).await {
            Ok(Query(value)) => Ok(Self(value)),
            Err(rejection) => {
                tracing::warn!(%rejection, "invalid query string");
                Err(validation(rejection.body_text()))
            }
        }
    }
}

pub struct ApiPath<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiPath<T>
where
    Path<T>: FromRequestParts<S, Rejection = PathRejection>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Path::<T>::from_request_parts(parts, state).await {
            Ok(Path(value)) => Ok(Self(value)),
            Err(rejection) => {
                tracing::warn!(%rejection, "invalid path parameters");
                Err(validation(rejection.body_text()))
            }
        }
    }
}

/// Детали rejection полезны в логах, но ответ не должен разрастаться.
fn validation(detail: String) -> AppError {
    let mut message: String = detail.chars().take(200).collect();
    if message.is_empty() {
        message = "invalid request".to_owned();
    }
    AppError::bad_request("validation", message)
}
