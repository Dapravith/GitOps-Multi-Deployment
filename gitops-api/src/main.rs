use std::process::ExitCode;

use gitops_api::{Config, Error, server, telemetry};
use tracing::{error, info};

#[tokio::main]
async fn main() -> ExitCode {
    let config = match bootstrap() {
        Ok(config) => config,
        Err(err) => {
            // Logging is not available yet, so report on stderr directly.
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };

    match server::run(&config).await {
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

/// Everything that must succeed before the first log line can be emitted.
fn bootstrap() -> Result<Config, Error> {
    let config = Config::from_env()?;
    telemetry::init(config.log_format)?;
    Ok(config)
}
