//! Server lifecycle: bind, serve, drain.
//!
//! [`serve`] owns the readiness flag in both directions: it turns it on once
//! the router is being served and off as soon as the shutdown future resolves,
//! so `/ready` reports 503 while in-flight requests drain.

use std::future::Future;
use std::io;

use tokio::net::TcpListener;
use tracing::{info, warn};

use super::config::Config;
use super::dependencies;
use super::error::Error;
use crate::app::{self, AppState, HttpLimits};
use crate::platform::build_info::BUILD_INFO;
use crate::platform::shutdown;

/// Bind the configured address and serve until SIGTERM / SIGINT.
///
/// # Errors
/// Returns [`Error::Bind`] if the address cannot be bound and
/// [`Error::Serve`] if the server fails while running.
pub async fn run(config: &Config) -> Result<(), Error> {
    let addr = config.bind_addr();
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|source| Error::Bind { addr, source })?;
    let state = dependencies::build(config);

    info!(
        %addr,
        environment = state.environment(),
        version = BUILD_INFO.version,
        git_sha = BUILD_INFO.git_sha,
        request_timeout_ms = config.http.request_timeout.as_millis(),
        max_body_bytes = config.http.max_body_bytes,
        api_keys = config.api_keys.len(),
        "server listening"
    );
    if config.api_keys.is_empty() {
        warn!("API_KEYS is not set: every protected endpoint will return 401");
    }

    serve(listener, state, config.http, shutdown::signal())
        .await
        .map_err(Error::Serve)
}

/// Serve the API on `listener` until `shutdown` resolves, then drain in-flight
/// requests before returning.
///
/// Takes the listener and shutdown trigger as arguments so tests can drive the
/// real TCP path on an ephemeral port without sending signals.
///
/// # Errors
/// Returns any I/O error from the underlying server.
pub async fn serve<F>(
    listener: TcpListener,
    state: AppState,
    limits: HttpLimits,
    shutdown: F,
) -> io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let app = app::router(state.clone(), &limits);

    let drain = {
        let state = state.clone();
        async move {
            shutdown.await;
            state.set_ready(false);
            info!("draining connections");
        }
    };

    state.set_ready(true);
    axum::serve(listener, app)
        .with_graceful_shutdown(drain)
        .await
}
