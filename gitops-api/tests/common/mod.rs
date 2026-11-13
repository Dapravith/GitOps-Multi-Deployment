//! Helpers shared by the integration test crates. Each `tests/*.rs` file is a
//! separate crate and uses a different subset, hence the `dead_code` allow.
#![allow(dead_code)]

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use gitops_api::{AppState, HttpLimits, app};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl TestResponse {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("body is not JSON ({e}): {:?}", self.body))
    }

    pub fn header(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .unwrap_or_else(|| panic!("missing header {name}"))
            .to_str()
            .unwrap()
    }
}

/// The production router with default limits.
pub fn app(state: AppState) -> Router {
    app::router(state, &HttpLimits::default())
}

pub fn ready_state(env: &str) -> AppState {
    let state = AppState::new(env);
    state.set_ready(true);
    state
}

pub async fn send(router: Router, request: Request<Body>) -> TestResponse {
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    TestResponse {
        status,
        headers,
        body,
    }
}

pub async fn get(router: Router, uri: &str) -> TestResponse {
    send(router, Request::get(uri).body(Body::empty()).unwrap()).await
}
