//! Router assembly. The only place that knows which modules exist.

use axum::Router;

use super::error::ApiError;
use super::middleware::{self, HttpLimits};
use super::state::AppState;
use crate::modules;

/// Assemble the full application: module routes, JSON fallbacks, then the
/// cross-cutting middleware stack (which also wraps the fallbacks).
pub fn router(state: AppState, limits: &HttpLimits) -> Router {
    let routes = modules::routes()
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed);

    middleware::apply(routes, limits).with_state(state)
}

async fn not_found() -> ApiError {
    ApiError::NotFound
}

async fn method_not_allowed() -> ApiError {
    ApiError::MethodNotAllowed
}
