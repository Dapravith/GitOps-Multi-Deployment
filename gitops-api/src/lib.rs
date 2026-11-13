//! `gitops-api`: a deliberately small HTTP service whose only job is to be
//! deployed, observed, promoted and rolled back by a GitOps pipeline.
//!
//! # Layers
//!
//! | Layer         | Owns                                                        | May depend on                     |
//! |---------------|-------------------------------------------------------------|-----------------------------------|
//! | [`bootstrap`] | config, wiring (composition root), bind / serve / drain     | everything                        |
//! | [`app`]       | HTTP kernel: router assembly, middleware, state, `ApiError` | `modules`, `platform`             |
//! | [`modules`]   | features; one directory per bounded context                 | `app::{state, error}`, `platform` |
//! | [`platform`]  | process-level concerns: telemetry, signals, build metadata  | nothing in this crate             |
//!
//! Handlers never read the environment, touch the listener or install
//! middleware. `main` only calls [`bootstrap`].
//!
//! See `docs/ARCHITECTURE.md` and `docs/ADR/` for the reasoning.

pub mod app;
pub mod bootstrap;
pub mod modules;
pub mod platform;

pub use app::{ApiError, AppState, HttpLimits};
pub use bootstrap::{Config, Error};
pub use platform::build_info::{BUILD_INFO, BuildInfo};
