//! Startup / fatal error: everything that can stop the process from starting
//! or make it exit non-zero. Never returned to an HTTP client; for that see
//! [`crate::app::ApiError`].

use std::io;
use std::net::SocketAddr;

use tracing_subscriber::util::TryInitError;

use super::config::ConfigError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("failed to initialise logging: {0}")]
    Telemetry(#[from] TryInitError),

    #[error("failed to bind {addr}: {source}")]
    Bind {
        addr: SocketAddr,
        #[source]
        source: io::Error,
    },

    #[error("server error: {0}")]
    Serve(#[source] io::Error),
}
