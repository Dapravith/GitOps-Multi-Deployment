//! Structured logging. JSON by default (for log shippers), `pretty` for local dev.
//! Verbosity is controlled with `RUST_LOG` (default `info`).

use tracing::warn;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::{SubscriberInitExt, TryInitError};
use tracing_subscriber::{EnvFilter, fmt};

use crate::config::LogFormat;

/// Environment variable holding the `EnvFilter` directives.
pub const LOG_FILTER_ENV: &str = EnvFilter::DEFAULT_ENV;
/// Filter used when `RUST_LOG` is unset or unparsable.
pub const DEFAULT_LOG_FILTER: &str = "info";

/// Install the global `tracing` subscriber. Call once, before any logging.
///
/// An unparsable `RUST_LOG` does not stop the service: it falls back to
/// [`DEFAULT_LOG_FILTER`] and logs a warning.
///
/// # Errors
/// Returns [`TryInitError`] if a global subscriber is already installed.
pub fn init(format: LogFormat) -> Result<(), TryInitError> {
    let (filter, rejected) = env_filter(std::env::var(LOG_FILTER_ENV).ok());
    let registry = tracing_subscriber::registry().with(filter);

    match format {
        LogFormat::Json => registry
            .with(
                fmt::layer()
                    .json()
                    .flatten_event(true)
                    .with_current_span(true)
                    .with_span_list(false),
            )
            .try_init(),
        LogFormat::Pretty => registry.with(fmt::layer()).try_init(),
    }?;

    if let Some(reason) = rejected {
        warn!(
            %reason,
            fallback = DEFAULT_LOG_FILTER,
            "ignoring invalid {LOG_FILTER_ENV}"
        );
    }
    Ok(())
}

/// Build the level filter from the raw `RUST_LOG` value. The second element
/// is the parse error when the value was present but rejected.
fn env_filter(raw: Option<String>) -> (EnvFilter, Option<String>) {
    let Some(raw) = raw else {
        return (EnvFilter::new(DEFAULT_LOG_FILTER), None);
    };
    match EnvFilter::try_new(&raw) {
        Ok(filter) => (filter, None),
        Err(err) => (EnvFilter::new(DEFAULT_LOG_FILTER), Some(err.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_filter_uses_default_without_warning() {
        let (filter, rejected) = env_filter(None);
        assert_eq!(filter.to_string(), DEFAULT_LOG_FILTER);
        assert!(rejected.is_none());
    }

    #[test]
    fn valid_filter_is_used() {
        let (filter, rejected) = env_filter(Some("debug".to_owned()));
        assert_eq!(filter.to_string(), "debug");
        assert!(rejected.is_none());
    }

    #[test]
    fn invalid_filter_falls_back_and_reports() {
        let (filter, rejected) = env_filter(Some("gitops_api=loud".to_owned()));
        assert_eq!(filter.to_string(), DEFAULT_LOG_FILTER);
        assert!(rejected.is_some());
    }
}
