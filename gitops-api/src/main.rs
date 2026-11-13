use std::process::ExitCode;

use gitops_api::bootstrap;
use tracing::{error, info};

#[tokio::main]
async fn main() -> ExitCode {
    let config = match bootstrap::init() {
        Ok(config) => config,
        Err(err) => {
            // Logging is not available yet, so report on stderr directly.
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };

    match bootstrap::run(&config).await {
        Ok(()) => {
            info!("shutdown complete");
            ExitCode::SUCCESS
        }
        Err(err) => {
            error!(%err, "fatal error");
            ExitCode::FAILURE
        }
    }
}
