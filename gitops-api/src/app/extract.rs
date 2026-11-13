//! Extractors whose rejections are [`ApiError`]s, so malformed input gets the
//! same JSON error shape as everything else (axum's defaults reply in plain
//! text).
//!
//! Use `ApiJson<T>` / `ApiQuery<T>` instead of `Json<T>` / `Query<T>` in
//! handlers.

use axum::Json;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{FromRequest, FromRequestParts, Query, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;
use serde::de::DeserializeOwned;

use super::error::ApiError;

/// JSON body. Requires `Content-Type: application/json`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        Json::<T>::from_request(request, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(json_rejection)
    }
}

/// Query string.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApiQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Query::<T>::from_request_parts(parts, state)
            .await
            .map(|Query(value)| Self(value))
            .map_err(|rejection: QueryRejection| ApiError::BadRequest(rejection.body_text()))
    }
}

fn json_rejection(rejection: JsonRejection) -> ApiError {
    match rejection {
        JsonRejection::MissingJsonContentType(_) => ApiError::UnsupportedMediaType,
        JsonRejection::JsonSyntaxError(_) => {
            ApiError::BadRequest("request body is not valid JSON".to_owned())
        }
        // Missing/unknown fields and wrong types: serde's message names the
        // field, which is what a client needs to fix the request.
        JsonRejection::JsonDataError(e) => ApiError::Unprocessable(e.body_text()),
        other if other.status() == StatusCode::PAYLOAD_TOO_LARGE => ApiError::PayloadTooLarge,
        other => ApiError::BadRequest(other.body_text()),
    }
}
