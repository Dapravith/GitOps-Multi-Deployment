use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::get;

use super::handlers;
use crate::app::AppState;
use crate::app::middleware::authentication::require_api_key;

/// Every items route requires a valid API key. `route_layer` runs the check
/// before any extractor, so an unauthenticated body is never parsed.
pub fn routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/items", get(handlers::list).post(handlers::create))
        .route(
            "/items/{id}",
            get(handlers::get)
                .put(handlers::replace)
                .delete(handlers::delete),
        )
        .route_layer(from_fn_with_state(state.api_keys(), require_api_key))
}
