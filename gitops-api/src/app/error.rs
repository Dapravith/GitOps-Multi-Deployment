//! The one error type handlers return.
//!
//! Every variant maps to a fixed HTTP status and a stable, machine-readable
//! `status` code. Some variants carry a `message` written by this service
//! (validation text, conflict reason); none ever carries internal detail. The
//! cause of an [`ApiError::Internal`] is logged (correlated by the request
//! span's `request_id`) and the client sees only `{"status":"internal_error"}`.
//!
//! A feature module defines its own error and converts it with
//! `impl From<ModuleError> for ApiError` in its `transport` layer, so HTTP
//! semantics stay at the edge and the domain stays HTTP-free.

use std::error::Error as StdError;

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use tracing::error;

use super::response::ErrorBody;

type BoxError = Box<dyn StdError + Send + Sync + 'static>;

/// `WWW-Authenticate` challenge sent with every 401 (RFC 6750).
pub const BEARER_CHALLENGE: &str = r#"Bearer realm="gitops-api""#;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// The request could not be decoded (bad JSON syntax, bad query string).
    #[error("bad request: {0}")]
    BadRequest(String),
    /// Missing or invalid credentials. Deliberately says nothing about which.
    #[error("unauthorized")]
    Unauthorized,
    #[error("not found")]
    NotFound,
    #[error("method not allowed")]
    MethodNotAllowed,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("payload too large")]
    PayloadTooLarge,
    #[error("unsupported media type")]
    UnsupportedMediaType,
    /// Well-formed request that breaks a validation rule.
    #[error("validation failed: {0}")]
    Unprocessable(String),
    #[error("service is not ready")]
    NotReady,
    #[error("insufficient storage")]
    InsufficientStorage,
    #[error("internal error: {0}")]
    Internal(#[source] BoxError),
}

impl ApiError {
    /// Wrap any error as a 500 whose cause is logged, not returned.
    pub fn internal(err: impl Into<BoxError>) -> Self {
        Self::Internal(err.into())
    }

    #[must_use]
    pub const fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::UnsupportedMediaType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::Unprocessable(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::NotReady => StatusCode::SERVICE_UNAVAILABLE,
            Self::InsufficientStorage => StatusCode::INSUFFICIENT_STORAGE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Stable code clients may branch on. Changing one is a breaking change.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "bad_request",
            Self::Unauthorized => "unauthorized",
            Self::NotFound => "not_found",
            Self::MethodNotAllowed => "method_not_allowed",
            Self::Conflict(_) => "conflict",
            Self::PayloadTooLarge => "payload_too_large",
            Self::UnsupportedMediaType => "unsupported_media_type",
            Self::Unprocessable(_) => "validation_failed",
            Self::NotReady => "not_ready",
            Self::InsufficientStorage => "insufficient_storage",
            Self::Internal(_) => "internal_error",
        }
    }

    /// Client-safe detail, if any.
    fn message(&self) -> Option<&str> {
        match self {
            Self::BadRequest(m) | Self::Conflict(m) | Self::Unprocessable(m) => Some(m),
            _ => None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let Self::Internal(cause) = &self {
            // Emitted inside the request span, so it carries `request_id`.
            error!(error = %cause, "request failed");
        }
        let body = ErrorBody::new(self.code(), self.message());
        let mut response = (self.status_code(), body).into_response();
        if matches!(self, Self::Unauthorized) {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static(BEARER_CHALLENGE),
            );
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    async fn body(err: ApiError) -> (StatusCode, String) {
        let response = err.into_response();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn internal_error_never_leaks_its_cause() {
        let (status, body) = body(ApiError::internal("db password rejected for user admin")).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, r#"{"status":"internal_error"}"#);
    }

    #[tokio::test]
    async fn message_is_included_only_when_present() {
        let (status, body) = body(ApiError::Unprocessable("name: must not be empty".into())).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            body,
            r#"{"status":"validation_failed","message":"name: must not be empty"}"#
        );
        assert_eq!(body_of_not_found().await, r#"{"status":"not_found"}"#);
    }

    async fn body_of_not_found() -> String {
        body(ApiError::NotFound).await.1
    }

    #[test]
    fn unauthorized_carries_bearer_challenge() {
        let response = ApiError::Unauthorized.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers()[header::WWW_AUTHENTICATE],
            BEARER_CHALLENGE
        );
    }
}
