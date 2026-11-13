//! Response bodies shared by more than one route module.

use axum::Json;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// `{"status": "<value>"}` — the body for probes and simple errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) struct StatusBody {
    status: &'static str,
}

impl StatusBody {
    pub(super) const fn new(status: &'static str) -> Self {
        Self { status }
    }
}

impl IntoResponse for StatusBody {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}
