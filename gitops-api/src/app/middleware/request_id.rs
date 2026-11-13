//! Request correlation id (`x-request-id`).
//!
//! An incoming id is **untrusted input** that ends up in every log line of the
//! request, so it is kept only if it is exactly one value of 1..=128 chars
//! from `[A-Za-z0-9._-]`. Anything else (missing, repeated, too long, odd
//! bytes) is replaced by a fresh UUID v4. The final id is echoed on the
//! response so a caller can quote it in a bug report.

use axum::Router;
use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue};
use axum::middleware::map_request;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};

pub const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

const MAX_LEN: usize = 128;

/// Add the three request-id layers; the sanitiser runs first.
pub(super) fn apply<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router
        .layer(PropagateRequestIdLayer::new(REQUEST_ID_HEADER))
        .layer(SetRequestIdLayer::new(REQUEST_ID_HEADER, MakeRequestUuid))
        .layer(map_request(drop_untrusted_id))
}

/// Remove the header unless it is a single acceptable value, so that
/// `SetRequestIdLayer` generates a fresh one.
async fn drop_untrusted_id(mut request: Request) -> Request {
    let headers = request.headers_mut();
    let mut values = headers.get_all(&REQUEST_ID_HEADER).iter();
    let keep = matches!((values.next(), values.next()), (Some(v), None) if is_acceptable(v));
    if !keep {
        headers.remove(&REQUEST_ID_HEADER);
    }
    request
}

fn is_acceptable(value: &HeaderValue) -> bool {
    let bytes = value.as_bytes();
    (1..=MAX_LEN).contains(&bytes.len())
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_uuid_and_simple_tokens() {
        for ok in ["0b8e5c0e-5f43-4a43-9d2b-6c2f0b8f3c11", "abc", "req_1.2-3"] {
            assert!(is_acceptable(&HeaderValue::from_static(ok)), "{ok}");
        }
    }

    #[test]
    fn rejects_empty_long_and_unsafe_values() {
        let long = "a".repeat(MAX_LEN + 1);
        for bad in ["", long.as_str(), "a b", "a\"b", "{\"x\":1}", "../etc"] {
            let value = HeaderValue::from_str(bad).unwrap();
            assert!(!is_acceptable(&value), "{bad:?}");
        }
    }
}
