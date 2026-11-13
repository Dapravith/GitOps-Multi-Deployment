//! Shared application state: the dependency container handlers extract from.
//!
//! Keep it a bag of cheap-to-clone handles (`Arc`s, pools, services). Each
//! dependency is exposed with `FromRef`, so a handler asks only for what it
//! uses (`State<ItemService>`), not the whole container.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::extract::FromRef;

use crate::modules::items::application::ItemService;
use crate::platform::security::api_key::ApiKeys;

/// State handed to every handler. Cloning is cheap (`Arc` bumps) and all
/// clones observe the same readiness flag.
#[derive(Debug, Clone)]
pub struct AppState {
    environment: Arc<str>,
    ready: Arc<AtomicBool>,
    api_keys: Arc<ApiKeys>,
    items: ItemService,
}

impl AppState {
    /// New state; starts **not ready** until the server begins serving.
    #[must_use]
    pub fn new(environment: impl Into<Arc<str>>, api_keys: ApiKeys, items: ItemService) -> Self {
        Self {
            environment: environment.into(),
            ready: Arc::new(AtomicBool::new(false)),
            api_keys: Arc::new(api_keys),
            items,
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

    #[must_use]
    pub fn api_keys(&self) -> Arc<ApiKeys> {
        Arc::clone(&self.api_keys)
    }
}

impl FromRef<AppState> for ItemService {
    fn from_ref(state: &AppState) -> Self {
        state.items.clone()
    }
}

impl FromRef<AppState> for Arc<ApiKeys> {
    fn from_ref(state: &AppState) -> Self {
        state.api_keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::items::infrastructure::InMemoryItemRepository;

    #[test]
    fn starts_not_ready_and_clones_share_readiness() {
        let items = ItemService::new(Arc::new(InMemoryItemRepository::new(1)));
        let state = AppState::new("test", ApiKeys::none(), items);
        let clone = state.clone();
        assert!(!state.is_ready());

        clone.set_ready(true);
        assert!(state.is_ready());
        assert_eq!(state.environment(), "test");
    }
}
