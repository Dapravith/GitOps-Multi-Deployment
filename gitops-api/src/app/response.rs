//! Response bodies shared across modules.

use axum::Json;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// `{"status": "<value>"}`: the body for probes and every error.
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
