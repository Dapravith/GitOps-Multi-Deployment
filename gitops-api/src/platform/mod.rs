//! Process-level concerns that know nothing about HTTP or features.
//!
//! This layer has no dependencies on the rest of the crate, so anything here
//! can be reused by a second binary (a worker, a migration job) unchanged.

pub mod build_info;
pub mod shutdown;
pub mod telemetry;
