//! `gitops-api`: a deliberately small HTTP service whose only job is to be
//! deployed, observed, promoted and rolled back by a GitOps pipeline.
//!
//! # Layout
//!
//! | Module         | Responsibility                                            |
//! |----------------|-----------------------------------------------------------|
//! | [`config`]     | Parse and validate environment variables                  |
//! | [`telemetry`]  | Install the global `tracing` subscriber                   |
//! | [`server`]     | Bind, serve, and own the readiness lifecycle              |
//! | [`shutdown`]   | Wait for SIGTERM / SIGINT                                 |
//! | [`routes`]     | HTTP surface: one submodule per resource, merged in `mod` |
//! | [`state`]      | Shared state handed to every handler                      |
//! | [`build_info`] | Compile-time metadata (`name`, `version`, `git_sha`)      |
//! | [`error`]      | Top-level error returned to `main`                        |
//!
//! Dependencies point one way: `main` → `server` → `routes` → `state`.
//! Handlers never read the environment or touch the listener.

pub mod build_info;
pub mod config;
pub mod error;
pub mod routes;
pub mod server;
pub mod shutdown;
pub mod state;
pub mod telemetry;

pub use build_info::{BUILD_INFO, BuildInfo};
pub use config::Config;
pub use error::Error;
pub use server::{run, serve};
pub use state::AppState;
