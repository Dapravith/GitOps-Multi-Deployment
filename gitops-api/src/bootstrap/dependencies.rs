//! Composition root: the one place that turns [`Config`] into live
//! dependencies and hands them to the HTTP layer.
//!
//! When a module needs an adapter (a database pool, an HTTP client, a token
//! verifier), it is constructed here, wrapped behind the module's port trait,
//! and stored in [`AppState`]. Handlers and services never construct their
//! own infrastructure, which is what keeps them testable with fakes.

use super::config::Config;
use crate::app::AppState;

/// Build the state shared by every request.
#[must_use]
pub fn build(config: &Config) -> AppState {
    AppState::new(config.environment.as_str())
}
