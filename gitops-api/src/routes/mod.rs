//! HTTP surface.
//!
//! * `GET /health`  – liveness: the process is up and the runtime is serving.
//! * `GET /ready`   – readiness: 200 while accepting traffic, 503 while
//!   starting or draining, so Kubernetes stops routing before shutdown.
//! * `GET /version` – build + environment metadata, used to prove which
//!   image digest is running in each environment.
//!
//! # Adding an endpoint
//!
//! 1. Create `routes/<resource>.rs` exposing `pub(super) fn router() -> Router<AppState>`.
//! 2. `.merge(<resource>::router())` it in [`router`] below.
//!
//! Cross-cutting behaviour (tracing, fallback) is applied once here, so
//! resource modules contain handlers only.

mod health;
mod response;
mod version;

use axum::Router;
use axum::http::StatusCode;
use tower_http::classify::{ServerErrorsAsFailures, SharedClassifier};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

use self::response::StatusBody;
use crate::state::AppState;

/// Assemble the full application router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(health::router())
        .merge(version::router())
        .fallback(not_found)
        .layer(trace_layer())
        .with_state(state)
}

/// One INFO span per request and one INFO event per response.
fn trace_layer() -> TraceLayer<SharedClassifier<ServerErrorsAsFailures>> {
    TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO))
}

async fn not_found() -> (StatusCode, StatusBody) {
    (StatusCode::NOT_FOUND, StatusBody::new("not_found"))
}
