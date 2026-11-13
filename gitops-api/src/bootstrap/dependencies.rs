//! Composition root: the one place that turns [`Config`] into live
//! dependencies and hands them to the HTTP layer.
//!
//! When a module needs an adapter (a database pool, an HTTP client, a token
//! verifier), it is constructed here, wrapped behind the module's port trait,
//! and stored in [`AppState`]. Handlers and services never construct their
//! own infrastructure, which is what keeps them testable with fakes.

use std::sync::Arc;

use super::config::Config;
use crate::app::AppState;
use crate::modules::items::application::ItemService;
use crate::modules::items::infrastructure::InMemoryItemRepository;

/// Upper bound on stored items: at <=1.1 KiB each this stays far below the
/// pod's 64 MiB memory limit.
pub const ITEM_CAPACITY: usize = 10_000;

/// Build the state shared by every request.
#[must_use]
pub fn build(config: &Config) -> AppState {
    let items = ItemService::new(Arc::new(InMemoryItemRepository::new(ITEM_CAPACITY)));
    AppState::new(config.environment.as_str(), config.api_keys.clone(), items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::items::application::ItemInput;

    #[tokio::test]
    async fn wires_config_into_state() {
        let key = "wire-0123456789abcdef0123456789abcdef";
        let config = Config::from_lookup(|k| match k {
            "APP_ENV" => Some("staging".to_owned()),
            "API_KEYS" => Some(key.to_owned()),
            _ => None,
        })
        .unwrap();

        let state = build(&config);
        assert_eq!(state.environment(), "staging");
        assert!(!state.is_ready());
        assert!(state.api_keys().verify(key));

        // A working item store is wired in.
        let items: ItemService = axum::extract::FromRef::from_ref(&state);
        let input = ItemInput {
            name: "wired".into(),
            description: String::new(),
        };
        let created = items.create(&input).await.unwrap();
        assert_eq!(items.get(&created.id().to_string()).await.unwrap(), created);
    }
}
