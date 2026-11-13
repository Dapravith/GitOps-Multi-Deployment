//! The one error type handlers return.
//!
//! Every variant maps to a fixed HTTP status and a stable, machine-readable
//! `status` code. The body **never** contains internal detail: the cause of an
//! [`ApiError::Internal`] is logged (correlated by the request span's
//! `request_id`) and the client sees only `{"status":"internal_error"}`.
//!
//! A feature module defines its own domain error and converts it with
//! `impl From<DomainError> for ApiError`, so HTTP semantics stay here and the
//! domain stays HTTP-free.

use std::error::Error as StdError;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tracing::error;

use super::response::StatusBody;

type BoxError = Box<dyn StdError + Send + Sync + 'static>;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("route not found")]
    NotFound,
    #[error("method not allowed")]
    MethodNotAllowed,
    #[error("service is not ready")]
    NotReady,
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
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
            Self::NotReady => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Stable code clients may branch on. Changing one is a breaking change.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::MethodNotAllowed => "method_not_allowed",
            Self::NotReady => "not_ready",
            Self::Internal(_) => "internal_error",
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let Self::Internal(cause) = &self {
            // Emitted inside the request span, so it carries `request_id`.
            error!(error = %cause, "request failed");
        }
        (self.status_code(), StatusBody::new(self.code())).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    #[tokio::test]
    async fn internal_error_never_leaks_its_cause() {
        let response = ApiError::internal("db password rejected for user admin").into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], br#"{"status":"internal_error"}"#);
    }
}
