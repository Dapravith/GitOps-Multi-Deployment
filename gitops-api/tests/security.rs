//! Security behaviour of the shared middleware stack.

mod common;

use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode};
use axum::routing::{get as get_route, post};
use common::{app, get, ready_state, send};
use gitops_api::app::middleware;
use gitops_api::{AppState, HttpLimits};

const SECURITY_HEADERS: [(&str, &str); 6] = [
    ("x-content-type-options", "nosniff"),
    ("x-frame-options", "DENY"),
    (
        "content-security-policy",
        "default-src 'none'; frame-ancestors 'none'",
    ),
    ("referrer-policy", "no-referrer"),
    ("cache-control", "no-store"),
    ("cross-origin-resource-policy", "same-origin"),
];

fn assert_security_headers(res: &common::TestResponse) {
    for (name, value) in SECURITY_HEADERS {
        assert_eq!(res.header(name), value, "{name} on {}", res.status);
    }
}

/// Throw-away routes behind exactly the production middleware stack.
fn probe_app(limits: HttpLimits) -> Router {
    let routes = Router::new()
        .route(
            "/slow",
            get_route(|| async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                "late"
            }),
        )
        .route("/echo", post(|body: Bytes| async move { body }));
    middleware::apply(routes, &limits)
}

#[tokio::test]
async fn security_headers_on_success_and_error_responses() {
    assert_security_headers(&get(app(ready_state("test")), "/health").await);
    assert_security_headers(&get(app(AppState::new("test")), "/ready").await);
    assert_security_headers(&get(app(ready_state("test")), "/nope").await);
}

#[tokio::test]
async fn generates_request_id_when_absent() {
    let res = get(app(ready_state("test")), "/health").await;
    let id = res.header("x-request-id");
    assert_eq!(id.len(), 36, "expected a UUID, got {id:?}");
    assert_eq!(id.matches('-').count(), 4);
}

#[tokio::test]
async fn echoes_a_valid_incoming_request_id() {
    let req = Request::get("/health")
        .header("x-request-id", "trace-abc_123.4")
        .body(Body::empty())
        .unwrap();
    let res = send(app(ready_state("test")), req).await;
    assert_eq!(res.header("x-request-id"), "trace-abc_123.4");
}

#[tokio::test]
async fn replaces_untrusted_request_ids() {
    let too_long = "a".repeat(129);
    for bad in ["has space", "{\"inject\":true}", too_long.as_str()] {
        let req = Request::get("/health")
            .header("x-request-id", bad)
            .body(Body::empty())
            .unwrap();
        let res = send(app(ready_state("test")), req).await;
        let id = res.header("x-request-id");
        assert_ne!(id, bad);
        assert_eq!(id.len(), 36, "{bad:?} should be replaced by a UUID");
    }

    let req = Request::get("/health")
        .header("x-request-id", "one")
        .header("x-request-id", "two")
        .body(Body::empty())
        .unwrap();
    let res = send(app(ready_state("test")), req).await;
    assert_eq!(res.headers.get_all("x-request-id").iter().count(), 1);
    assert_eq!(res.header("x-request-id").len(), 36);
}

#[tokio::test]
async fn oversized_body_is_rejected_with_413() {
    let limits = HttpLimits {
        max_body_bytes: 16,
        ..HttpLimits::default()
    };

    let small = Request::post("/echo")
        .body(Body::from("0123456789"))
        .unwrap();
    let res = send(probe_app(limits), small).await;
    assert_eq!(res.status, StatusCode::OK);

    let big = Request::post("/echo")
        .header("content-length", "17")
        .body(Body::from("0123456789abcdefg"))
        .unwrap();
    let res = send(probe_app(limits), big).await;
    assert_eq!(res.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_security_headers(&res);
}

#[tokio::test]
async fn slow_handler_is_cut_off_with_503() {
    let limits = HttpLimits {
        request_timeout: Duration::from_millis(50),
        ..HttpLimits::default()
    };
    let res = tokio::time::timeout(Duration::from_secs(2), get(probe_app(limits), "/slow"))
        .await
        .expect("timeout layer did not fire");
    assert_eq!(res.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_security_headers(&res);
    assert!(res.headers.contains_key("x-request-id"));
}
