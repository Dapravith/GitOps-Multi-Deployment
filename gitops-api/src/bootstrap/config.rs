//! Runtime configuration, read from environment variables.
//!
//! Parsing goes through [`Config::from_lookup`] so it can be unit-tested
//! without mutating the process environment (`std::env::set_var` is `unsafe`
//! in Rust 2024).

use std::net::{Ipv4Addr, SocketAddr};
use std::ops::RangeInclusive;
use std::str::FromStr;
use std::time::Duration;

use crate::app::HttpLimits;
use crate::platform::telemetry::{LogFormat, UnknownLogFormat};

/// Names of the environment variables this service reads.
pub mod env {
    pub const PORT: &str = "PORT";
    pub const APP_ENV: &str = "APP_ENV";
    pub const LOG_FORMAT: &str = "LOG_FORMAT";
    pub const REQUEST_TIMEOUT_SECS: &str = "REQUEST_TIMEOUT_SECS";
    pub const MAX_REQUEST_BODY_BYTES: &str = "MAX_REQUEST_BODY_BYTES";
}

pub const DEFAULT_PORT: u16 = 8080;
pub const DEFAULT_ENVIRONMENT: &str = "local";

/// Accepted range for `REQUEST_TIMEOUT_SECS`. The upper bound stays well below
/// any sane `terminationGracePeriodSeconds` so a drain always completes.
const REQUEST_TIMEOUT_RANGE: RangeInclusive<u64> = 1..=60;
/// Accepted range for `MAX_REQUEST_BODY_BYTES` (1 B .. 16 MiB).
const MAX_BODY_RANGE: RangeInclusive<usize> = 1..=16 * 1024 * 1024;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("PORT must be an integer between 1 and 65535, got {0:?}")]
    InvalidPort(String),
    #[error("APP_ENV must be non-empty and contain only [a-z0-9-], got {0:?}")]
    InvalidEnvironment(String),
    #[error("LOG_FORMAT must be `json` or `pretty`, got {0:?}")]
    InvalidLogFormat(String),
    #[error("REQUEST_TIMEOUT_SECS must be an integer between 1 and 60, got {0:?}")]
    InvalidRequestTimeout(String),
    #[error("MAX_REQUEST_BODY_BYTES must be an integer between 1 and 16777216, got {0:?}")]
    InvalidMaxBody(String),
}

/// Validated runtime configuration. Unset variables take their default;
/// a variable that is set but invalid is always an error, never a fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub port: u16,
    pub environment: String,
    pub log_format: LogFormat,
    pub http: HttpLimits,
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
        let defaults = HttpLimits::default();
        Ok(Self {
            port: lookup(env::PORT)
                .map(|raw| parse_in(raw, &(1..=u16::MAX), ConfigError::InvalidPort))
                .transpose()?
                .unwrap_or(DEFAULT_PORT),
            environment: lookup(env::APP_ENV)
                .map(parse_environment)
                .transpose()?
                .unwrap_or_else(|| DEFAULT_ENVIRONMENT.to_owned()),
            log_format: lookup(env::LOG_FORMAT)
                .map(|raw| {
                    raw.parse()
                        .map_err(|UnknownLogFormat(v)| ConfigError::InvalidLogFormat(v))
                })
                .transpose()?
                .unwrap_or_default(),
            http: HttpLimits {
                request_timeout: lookup(env::REQUEST_TIMEOUT_SECS)
                    .map(|raw| {
                        parse_in(
                            raw,
                            &REQUEST_TIMEOUT_RANGE,
                            ConfigError::InvalidRequestTimeout,
                        )
                    })
                    .transpose()?
                    .map_or(defaults.request_timeout, Duration::from_secs),
                max_body_bytes: lookup(env::MAX_REQUEST_BODY_BYTES)
                    .map(|raw| parse_in(raw, &MAX_BODY_RANGE, ConfigError::InvalidMaxBody))
                    .transpose()?
                    .unwrap_or(defaults.max_body_bytes),
            },
        })
    }

    /// Bind on all interfaces: inside a container the pod network is the boundary.
    #[must_use]
    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::UNSPECIFIED, self.port))
    }
}

/// Parse an integer and require it to fall inside `range`; on any failure the
/// *original* string is reported, so the operator sees exactly what they set.
fn parse_in<T>(
    raw: String,
    range: &RangeInclusive<T>,
    err: fn(String) -> ConfigError,
) -> Result<T, ConfigError>
where
    T: FromStr + PartialOrd,
{
    match raw.trim().parse::<T>() {
        Ok(value) if range.contains(&value) => Ok(value),
        _ => Err(err(raw)),
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
        assert_eq!(cfg.http, HttpLimits::default());
        assert_eq!(cfg.bind_addr().to_string(), "0.0.0.0:8080");
    }

    #[test]
    fn reads_all_values() {
        let cfg = Config::from_lookup(lookup(&[
            ("PORT", "9000"),
            ("APP_ENV", "staging"),
            ("LOG_FORMAT", "Pretty"),
            ("REQUEST_TIMEOUT_SECS", "5"),
            ("MAX_REQUEST_BODY_BYTES", "1024"),
        ]))
        .unwrap();
        assert_eq!(cfg.port, 9000);
        assert_eq!(cfg.environment, "staging");
        assert_eq!(cfg.log_format, LogFormat::Pretty);
        assert_eq!(cfg.http.request_timeout, Duration::from_secs(5));
        assert_eq!(cfg.http.max_body_bytes, 1024);
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

    #[test]
    fn rejects_out_of_range_http_limits() {
        for bad in ["0", "61", "-1", "ten"] {
            assert_eq!(
                Config::from_lookup(lookup(&[("REQUEST_TIMEOUT_SECS", bad)])),
                Err(ConfigError::InvalidRequestTimeout(bad.to_owned())),
            );
        }
        for bad in ["0", "16777217", "1MB"] {
            assert_eq!(
                Config::from_lookup(lookup(&[("MAX_REQUEST_BODY_BYTES", bad)])),
                Err(ConfigError::InvalidMaxBody(bad.to_owned())),
            );
        }
    }
}
