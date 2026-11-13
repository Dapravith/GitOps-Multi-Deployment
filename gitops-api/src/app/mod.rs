//! HTTP kernel: everything every endpoint shares.
//!
//! * [`router`] mounts every feature module, the JSON fallbacks, and the
//!   middleware stack. It is the only file in `app` that knows `modules`.
//! * [`AppState`] is the dependency container handlers extract from.
//! * [`ApiError`] is the single error type a handler returns.
//! * [`middleware`] holds cross-cutting layers; modules never add their own
//!   global layers.

pub mod error;
pub mod middleware;
mod response;
mod router;
mod state;

pub use error::ApiError;
pub use middleware::HttpLimits;
pub(crate) use response::StatusBody;
pub use router::router;
pub use state::AppState;
