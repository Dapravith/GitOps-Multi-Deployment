//! Security response headers for a JSON-only API.
//!
//! Set with `if_not_present`, so a specific handler can still override one
//! (for example a cacheable resource setting its own `Cache-Control`).
//!
//! Deliberately **not** set here:
//! * `Strict-Transport-Security`: TLS terminates at the ingress, which owns HSTS.
//! * CORS: no browser client exists; add `tower_http::cors` with an explicit
//!   allow-list when one does, never a wildcard.

use axum::Router;
use axum::http::{HeaderName, HeaderValue, header};
use tower_http::set_header::SetResponseHeaderLayer;

const HEADERS: [(HeaderName, &str); 6] = [
    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    (header::X_FRAME_OPTIONS, "DENY"),
    (
        header::CONTENT_SECURITY_POLICY,
        "default-src 'none'; frame-ancestors 'none'",
    ),
    (header::REFERRER_POLICY, "no-referrer"),
    (header::CACHE_CONTROL, "no-store"),
    (
        HeaderName::from_static("cross-origin-resource-policy"),
        "same-origin",
    ),
];

pub(super) fn apply<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    HEADERS.into_iter().fold(router, |router, (name, value)| {
        router.layer(SetResponseHeaderLayer::if_not_present(
            name,
            HeaderValue::from_static(value),
        ))
    })
}
