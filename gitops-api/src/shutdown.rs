//! Termination signals: SIGTERM (Kubernetes) or SIGINT (Ctrl-C).
//!
//! This module only *waits*; what happens next (flipping readiness, draining)
//! is decided by [`crate::server::serve`].

use std::future::pending;

use tracing::{error, info};

/// Resolves when the first termination signal arrives.
///
/// If a handler cannot be installed the failure is logged and that signal is
/// treated as never arriving, rather than shutting the server down at once.
pub async fn signal() {
    let received = tokio::select! {
        () = interrupt() => "SIGINT",
        () = terminate() => "SIGTERM",
    };
    info!(signal = received, "shutdown signal received");
}

async fn interrupt() {
    if let Err(err) = tokio::signal::ctrl_c().await {
        error!(%err, "failed to install SIGINT handler");
        pending::<()>().await;
    }
}

#[cfg(unix)]
async fn terminate() {
    use tokio::signal::unix::{SignalKind, signal};

    match signal(SignalKind::terminate()) {
        Ok(mut stream) => {
            stream.recv().await;
        }
        Err(err) => {
            error!(%err, "failed to install SIGTERM handler");
            pending::<()>().await;
        }
    }
}

#[cfg(not(unix))]
async fn terminate() {
    pending::<()>().await;
}
