//! Feature modules, one directory per bounded context.
//!
//! Each module exposes its routes, merged below. A module that needs state
//! at build time (e.g. for an auth `route_layer`) takes `&AppState`.
//!
//! * A **stateless** module (like [`system`]) is just `routes.rs` + `handlers.rs`.
//! * A **stateful** module grows into `domain / application / transport /
//!   infrastructure`, like [`items`] (see
//!   `docs/ADR/0001-modular-layered-architecture.md`).

pub mod items;
pub mod system;

use axum::Router;

use crate::app::AppState;

/// Every module's routes, merged. Paths must not overlap (axum panics at
/// start-up if they do, which the contract tests catch).
pub fn routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .merge(system::routes())
        .merge(items::routes(state))
}
