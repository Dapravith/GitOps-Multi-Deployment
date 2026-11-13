use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::app::{ApiError, AppState, StatusBody};
use crate::platform::build_info::{BUILD_INFO, BuildInfo};

/// Liveness never consults state: if this handler runs, the process is alive.
pub(super) async fn health() -> StatusBody {
    StatusBody::new("ok")
}

pub(super) async fn ready(State(state): State<AppState>) -> Result<StatusBody, ApiError> {
    if state.is_ready() {
        Ok(StatusBody::new("ready"))
    } else {
        Err(ApiError::NotReady)
    }
}

#[derive(Debug, Serialize)]
pub(super) struct VersionResponse {
    #[serde(flatten)]
    build: BuildInfo,
    environment: String,
}

pub(super) async fn version(State(state): State<AppState>) -> Json<VersionResponse> {
    Json(VersionResponse {
        build: BUILD_INFO,
        environment: state.environment().to_owned(),
    })
}
