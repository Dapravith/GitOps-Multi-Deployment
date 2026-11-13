//! Runtime configuration, read from environment variables.
//!
//! Parsing goes through [`Config::from_lookup`] so it can be unit-tested
//! without mutating the process environment (`std::env::set_var` is `unsafe`
//! in Rust 2024).

use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;

/// Names of the environment variables this service reads.
pub mod env {
    pub const PORT: &str = "PORT";
    pub const APP_ENV: &str = "APP_ENV";
    pub const LOG_FORMAT: &str = "LOG_FORMAT";
}

pub const DEFAULT_PORT: u16 = 8080;
pub const DEFAULT_ENVIRONMENT: &str = "local";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogFormat {
    #[default]
    Json,
    Pretty,
}

impl FromStr for LogFormat {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "pretty" | "text" => Ok(Self::Pretty),
            other => Err(ConfigError::InvalidLogFormat(other.to_owned())),
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("PORT must be an integer between 1 and 65535, got {0:?}")]
    InvalidPort(String),
    #[error("APP_ENV must be non-empty and contain only [a-z0-9-], got {0:?}")]
    InvalidEnvironment(String),
    #[error("LOG_FORMAT must be `json` or `pretty`, got {0:?}")]
    InvalidLogFormat(String),
}

/// Validated runtime configuration. Unset variables take their default;
/// a variable that is set but invalid is always an error, never a fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub port: u16,
    pub environment: String,
    pub log_format: LogFormat,
}

impl Config {
    /// Read configuration from the process environment.
    ///
    /// # Errors
    /// Returns [`ConfigError`] when a variable is present but invalid.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Build configuration from an arbitrary key lookup.
    ///
    /// # Errors
    /// Returns [`ConfigError`] when a variable is present but invalid.
    pub fn from_lookup<F>(lookup: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        Ok(Self {
            port: lookup(env::PORT)
                .map(parse_port)
                .transpose()?
                .unwrap_or(DEFAULT_PORT),
            environment: lookup(env::APP_ENV)
                .map(parse_environment)
                .transpose()?
                .unwrap_or_else(|| DEFAULT_ENVIRONMENT.to_owned()),
            log_format: lookup(env::LOG_FORMAT)
                .map(|raw| raw.parse())
                .transpose()?
                .unwrap_or_default(),
        })
    }

    /// Bind on all interfaces: inside a container the pod network is the boundary.
    #[must_use]
    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::UNSPECIFIED, self.port))
    }
}

fn parse_port(raw: String) -> Result<u16, ConfigError> {
    match raw.trim().parse::<u16>() {
        Ok(port) if port != 0 => Ok(port),
        _ => Err(ConfigError::InvalidPort(raw)),
    }
}

/// The environment name ends up in logs and `/version`, so it is restricted
/// to the same alphabet as a Kubernetes namespace.
fn parse_environment(raw: String) -> Result<String, ConfigError> {
    let valid = !raw.is_empty()
        && raw
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if valid {
        Ok(raw)
    } else {
        Err(ConfigError::InvalidEnvironment(raw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        let map: HashMap<_, _> = pairs.iter().copied().collect();
        move |k| map.get(k).map(|v| (*v).to_owned())
    }

    #[test]
    fn defaults_when_unset() {
        let cfg = Config::from_lookup(lookup(&[])).unwrap();
        assert_eq!(cfg.port, DEFAULT_PORT);
        assert_eq!(cfg.environment, DEFAULT_ENVIRONMENT);
        assert_eq!(cfg.log_format, LogFormat::Json);
        assert_eq!(cfg.bind_addr().to_string(), "0.0.0.0:8080");
    }

    #[test]
    fn reads_all_values() {
        let cfg = Config::from_lookup(lookup(&[
            ("PORT", "9000"),
            ("APP_ENV", "staging"),
            ("LOG_FORMAT", "Pretty"),
        ]))
        .unwrap();
        assert_eq!(cfg.port, 9000);
        assert_eq!(cfg.environment, "staging");
        assert_eq!(cfg.log_format, LogFormat::Pretty);
    }

    #[test]
    fn rejects_invalid_port() {
        for bad in ["0", "70000", "abc", ""] {
            assert_eq!(
                Config::from_lookup(lookup(&[("PORT", bad)])),
                Err(ConfigError::InvalidPort(bad.to_owned())),
                "port {bad:?}"
            );
        }
    }

    #[test]
    fn rejects_invalid_environment() {
        for bad in ["", "Prod", "prod env", "prod;rm"] {
            assert!(matches!(
                Config::from_lookup(lookup(&[("APP_ENV", bad)])),
                Err(ConfigError::InvalidEnvironment(_))
            ));
        }
    }

    #[test]
    fn rejects_invalid_log_format() {
        assert_eq!(
            Config::from_lookup(lookup(&[("LOG_FORMAT", "xml")])),
            Err(ConfigError::InvalidLogFormat("xml".to_owned()))
        );
    }
}
