//! Process start-up: configuration, wiring, and the server lifecycle.
//!
//! `main` calls [`init`] (everything that must succeed before the first log
//! line) and then [`run`]. Nothing else in the crate reads the environment.

pub mod config;
mod dependencies;
mod error;
pub mod server;

pub use config::{Config, ConfigError};
pub use error::Error;
pub use server::{run, serve};

use crate::platform::telemetry;

/// Load configuration and install logging.
///
/// # Errors
/// Returns [`Error::Config`] for invalid environment variables and
/// [`Error::Telemetry`] if a global subscriber is already installed.
pub fn init() -> Result<Config, Error> {
    let config = Config::from_env()?;
    telemetry::init(config.log_format)?;
    Ok(config)
}
