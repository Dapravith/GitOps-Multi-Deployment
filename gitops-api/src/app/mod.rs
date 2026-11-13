//! HTTP kernel: everything every endpoint shares.
//!
//! * [`router`] mounts every feature module, the JSON fallbacks, and the
//!   middleware stack.
//! * [`AppState`] is the dependency container handlers extract from. With
//!   `router`, it is the only part of `app` that names module types.
//! * [`extract`] has `ApiJson` / `ApiQuery`, which reject with [`ApiError`].
//! * [`ApiError`] is the single error type a handler returns.
//! * [`middleware`] holds cross-cutting layers; modules never add their own
//!   global layers.

pub mod error;
pub mod extract;
pub mod middleware;
mod response;
mod router;
mod state;

pub use error::ApiError;
pub use middleware::HttpLimits;
pub(crate) use response::StatusBody;
pub use router::router;
pub use state::AppState;
