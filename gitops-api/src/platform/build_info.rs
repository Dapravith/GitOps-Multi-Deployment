//! Build metadata baked into the binary at compile time.

use serde::Serialize;

/// Identifies exactly which build is running.
///
/// `git_sha` is injected by CI (`--build-arg GIT_SHA=...`); local builds
/// report `unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BuildInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub git_sha: &'static str,
}

pub const BUILD_INFO: BuildInfo = BuildInfo {
    name: env!("CARGO_PKG_NAME"),
    version: env!("CARGO_PKG_VERSION"),
    git_sha: match option_env!("GIT_SHA") {
        Some(sha) => sha,
        None => "unknown",
    },
};
