//! The HTTP contract `gitops-config` and the promotion scripts rely on.
//! A failure here is a breaking change, not a test to update casually.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{app, get, ready_state, send};
use gitops_api::{AppState, BUILD_INFO};
use serde_json::json;

#[tokio::test]
async fn health_is_ok_even_when_not_ready() {
    let res = get(app(AppState::new("test")), "/health").await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.header("content-type"), "application/json");
    assert_eq!(res.json(), json!({ "status": "ok" }));
}

#[tokio::test]
async fn ready_reflects_state() {
    let state = AppState::new("test");

    let res = get(app(state.clone()), "/ready").await;
    assert_eq!(res.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(res.json(), json!({ "status": "not_ready" }));

    state.set_ready(true);
    let res = get(app(state.clone()), "/ready").await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.json(), json!({ "status": "ready" }));

    state.set_ready(false);
    let res = get(app(state), "/ready").await;
    assert_eq!(res.status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn version_reports_build_and_environment() {
    let res = get(app(ready_state("staging")), "/version").await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(
        res.json(),
        json!({
            "name": "gitops-api",
            "version": env!("CARGO_PKG_VERSION"),
            "git_sha": BUILD_INFO.git_sha,
            "environment": "staging",
        })
    );
}

#[tokio::test]
async fn unknown_route_is_json_404() {
    let res = get(app(ready_state("test")), "/nope").await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_eq!(res.header("content-type"), "application/json");
    assert_eq!(res.json(), json!({ "status": "not_found" }));
}

#[tokio::test]
async fn wrong_method_is_json_405() {
    let res = send(
        app(ready_state("test")),
        Request::post("/health").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(res.status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(res.json(), json!({ "status": "method_not_allowed" }));
}
