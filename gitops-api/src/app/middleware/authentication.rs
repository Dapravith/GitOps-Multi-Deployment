//! Bearer API-key authentication, applied per router with `route_layer`:
//!
//! ```ignore
//! router.route_layer(from_fn_with_state(state.api_keys(), require_api_key))
//! ```
//!
//! Probes (`/health`, `/ready`) and `/version` are never wrapped.
//!
//! * Exactly one `Authorization: Bearer <token>` header is accepted; the
//!   scheme is case-insensitive (RFC 7235).
//! * Missing, malformed and wrong credentials all get the same 401 +
//!   `WWW-Authenticate`, so a caller cannot tell which check failed.
//! * The token is never logged; `Authorization` is also marked sensitive by
//!   the global middleware stack.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tracing::debug;

use crate::app::ApiError;
use crate::platform::security::api_key::ApiKeys;

pub async fn require_api_key(
    State(keys): State<Arc<ApiKeys>>,
    request: Request,
    next: Next,
) -> Response {
    match bearer_token(request.headers()) {
        Some(token) if keys.verify(token) => next.run(request).await,
        presented => {
            debug!(credential_present = presented.is_some(), "rejected request");
            ApiError::Unauthorized.into_response()
        }
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let mut values = headers.get_all(AUTHORIZATION).iter();
    let (Some(value), None) = (values.next(), values.next()) else {
        return None;
    };
    let (scheme, token) = value.to_str().ok()?.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(values: &[&'static str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for v in values {
            headers.append(AUTHORIZATION, HeaderValue::from_static(v));
        }
        headers
    }

    #[test]
    fn extracts_a_single_bearer_token() {
        assert_eq!(bearer_token(&headers(&["Bearer abc"])), Some("abc"));
        assert_eq!(bearer_token(&headers(&["bearer abc"])), Some("abc"));
        assert_eq!(bearer_token(&headers(&["BEARER abc"])), Some("abc"));
    }

    #[test]
    fn rejects_other_shapes() {
        for bad in [
            &[][..],
            &["abc"],
            &["Bearer"],
            &["Bearer "],
            &["Basic YWxhZGRpbjpvcGVuc2VzYW1l"],
            &["Bearer abc", "Bearer abc"],
        ] {
            assert_eq!(bearer_token(&headers(bad)), None, "{bad:?}");
        }
    }
}
