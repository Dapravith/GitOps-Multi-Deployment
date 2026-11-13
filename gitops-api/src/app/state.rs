//! Shared application state: the dependency container handlers extract from.
//!
//! Keep it a bag of cheap-to-clone handles (`Arc`s, pools, clients). When a
//! module needs its own dependencies, add a field holding them behind the
//! module's port trait (e.g. `Arc<dyn UserRepository>`) and expose it to that
//! module's handlers with `axum::extract::FromRef`, so handlers ask for only
//! what they use.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// State handed to every handler. Cloning is cheap (two `Arc` bumps) and all
/// clones observe the same readiness flag.
#[derive(Debug, Clone)]
pub struct AppState {
    environment: Arc<str>,
    ready: Arc<AtomicBool>,
}

impl AppState {
    /// New state; starts **not ready** until the server begins serving.
    #[must_use]
    pub fn new(environment: impl Into<Arc<str>>) -> Self {
        Self {
            environment: environment.into(),
            ready: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Deployment environment name (`dev`, `staging`, `prod`, ...).
    #[must_use]
    pub fn environment(&self) -> &str {
        &self.environment
    }

    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    pub fn set_ready(&self, ready: bool) {
        self.ready.store(ready, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_not_ready_and_clones_share_readiness() {
        let state = AppState::new("test");
        let clone = state.clone();
        assert!(!state.is_ready());

        clone.set_ready(true);
        assert!(state.is_ready());
        assert_eq!(state.environment(), "test");
    }
}
