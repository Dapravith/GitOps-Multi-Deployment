//! Operational endpoints consumed by Kubernetes and the promotion pipeline.
//!
//! * `GET /health`: liveness. 200 whenever the process is serving.
//! * `GET /ready`: readiness. 200 while accepting traffic, 503 while
//!   starting or draining, so Kubernetes stops routing before shutdown.
//! * `GET /version`: build + environment metadata, used to prove which
//!   image digest is running in each environment.
//!
//! These paths are a contract with `gitops-config` (probes) and must stay
//! unauthenticated.

mod handlers;
mod routes;

pub use routes::routes;
