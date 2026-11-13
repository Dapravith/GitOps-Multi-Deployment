//! Kubernetes probes: `/health` (liveness) and `/ready` (readiness).

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;

use super::response::StatusBody;
use crate::state::AppState;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
}

/// Liveness never consults state: if this handler runs, the process is alive.
async fn health() -> StatusBody {
    StatusBody::new("ok")
}

async fn ready(State(state): State<AppState>) -> (StatusCode, StatusBody) {
    if state.is_ready() {
        (StatusCode::OK, StatusBody::new("ready"))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            StatusBody::new("not_ready"),
        )
    }
}
