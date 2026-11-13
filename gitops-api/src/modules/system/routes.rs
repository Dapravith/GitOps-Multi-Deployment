use axum::Router;
use axum::routing::get;

use super::handlers;
use crate::app::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/ready", get(handlers::ready))
        .route("/version", get(handlers::version))
}
