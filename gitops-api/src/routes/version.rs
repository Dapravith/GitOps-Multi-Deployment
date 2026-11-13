//! `/version`: which build is running, and in which environment.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::build_info::{BUILD_INFO, BuildInfo};
use crate::state::AppState;

#[derive(Debug, Serialize)]
struct VersionResponse {
    #[serde(flatten)]
    build: BuildInfo,
    environment: String,
}

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/version", get(version))
}

async fn version(State(state): State<AppState>) -> Json<VersionResponse> {
    Json(VersionResponse {
        build: BUILD_INFO,
        environment: state.environment().to_owned(),
    })
}
