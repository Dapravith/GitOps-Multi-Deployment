//! Response bodies shared across modules.

use axum::Json;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// `{"status": "<value>"}`: the body for probes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct StatusBody {
    status: &'static str,
}

impl StatusBody {
    pub(crate) const fn new(status: &'static str) -> Self {
        Self { status }
    }
}

impl IntoResponse for StatusBody {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

/// `{"status": "<code>", "message": "<detail>"?}`: the body for every error.
/// `message` is omitted when absent, so simple errors stay `{"status": ...}`.
#[derive(Debug, Serialize)]
pub(crate) struct ErrorBody<'a> {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'a str>,
}

impl<'a> ErrorBody<'a> {
    pub(crate) const fn new(status: &'static str, message: Option<&'a str>) -> Self {
        Self { status, message }
    }
}

impl IntoResponse for ErrorBody<'_> {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}
