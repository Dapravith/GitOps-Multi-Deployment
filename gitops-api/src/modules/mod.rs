//! Feature modules, one directory per bounded context.
//!
//! Each module exposes exactly one thing to the rest of the crate:
//! `pub fn routes() -> Router<AppState>`, merged below. Everything else inside
//! a module is private to it.
//!
//! * A **stateless** module (like [`system`]) is just `routes.rs` + `handlers.rs`.
//! * A **stateful** module grows into `domain / application / transport /
//!   infrastructure` (see `docs/ADR/0001-modular-layered-architecture.md`).
//!   Add those layers when the module needs them, not before.

pub mod system;

use axum::Router;

use crate::app::AppState;

/// Every module's routes, merged. Paths must not overlap (axum panics at
/// start-up if they do, which the contract tests catch).
pub fn routes() -> Router<AppState> {
    Router::new().merge(system::routes())
}
