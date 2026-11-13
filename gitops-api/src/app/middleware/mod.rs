//! Cross-cutting middleware, applied once to the whole router.
//!
//! Request flow, outermost first:
//!
//! 1. **sensitive headers**: `Authorization`, `Cookie`, ... are marked
//!    sensitive so no layer can log them by accident.
//! 2. **request id**: an untrusted `x-request-id` is validated or replaced,
//!    then echoed on the response ([`request_id`]).
//! 3. **trace**: one span per request carrying `method`, `path` (never the
//!    query string) and `request_id`.
//! 4. **security headers**: added to *every* response, including the
//!    timeout and body-limit rejections produced further in ([`security`]).
//! 5. **timeout**: a handler that exceeds [`HttpLimits::request_timeout`] is
//!    cancelled with 503, which also bounds how long a drain can take.
//! 6. **body limit**: bodies above [`HttpLimits::max_body_bytes`] get 413.
//!
//! `Router::layer` wraps what is already there, so [`apply`] adds the layers
//! innermost first: read it bottom-up to see the request order.

pub mod authentication;
mod request_id;
mod security;

use std::time::Duration;

use axum::Router;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::http::header::{AUTHORIZATION, COOKIE, PROXY_AUTHORIZATION, SET_COOKIE};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::sensitive_headers::SetSensitiveHeadersLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::{Level, Span, info_span};

pub use request_id::REQUEST_ID_HEADER;

pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_MAX_BODY_BYTES: usize = 64 * 1024;

/// Per-request resource limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpLimits {
    pub request_timeout: Duration,
    pub max_body_bytes: usize,
}

impl Default for HttpLimits {
    fn default() -> Self {
        Self {
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,
        }
    }
}

/// Wrap `router` (routes *and* fallbacks) in the standard middleware stack.
///
/// Public and generic over state so tests can put throw-away routes behind
/// exactly the production stack.
pub fn apply<S>(router: Router<S>, limits: &HttpLimits) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let router = router
        .layer(RequestBodyLimitLayer::new(limits.max_body_bytes))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::SERVICE_UNAVAILABLE,
            limits.request_timeout,
        ));
    let router = security::apply(router);
    let router = router.layer(
        TraceLayer::new_for_http()
            .make_span_with(request_span)
            .on_response(DefaultOnResponse::new().level(Level::INFO)),
    );
    let router = request_id::apply(router);
    router.layer(SetSensitiveHeadersLayer::new([
        AUTHORIZATION,
        PROXY_AUTHORIZATION,
        COOKIE,
        SET_COOKIE,
    ]))
}

/// Span fields are chosen, not defaulted: the query string and headers are
/// left out because they are where tokens and PII leak into logs.
fn request_span(request: &Request) -> Span {
    let request_id = request
        .headers()
        .get(&REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    info_span!(
        "request",
        method = %request.method(),
        path = request.uri().path(),
        request_id,
    )
}
