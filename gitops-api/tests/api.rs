use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use gitops_api::{AppState, BUILD_INFO, routes, serve};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tower::ServiceExt;

async fn get(state: AppState, uri: &str) -> (StatusCode, Value) {
    let response = routes::router(state)
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    assert_eq!(
        response.headers()["content-type"],
        "application/json",
        "{uri} must return JSON"
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

fn ready_state(env: &str) -> AppState {
    let state = AppState::new(env);
    state.set_ready(true);
    state
}

#[tokio::test]
async fn health_is_ok_even_when_not_ready() {
    let (status, body) = get(AppState::new("test"), "/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "status": "ok" }));
}

#[tokio::test]
async fn ready_reflects_state() {
    let state = AppState::new("test");

    let (status, body) = get(state.clone(), "/ready").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, json!({ "status": "not_ready" }));

    state.set_ready(true);
    let (status, body) = get(state.clone(), "/ready").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "status": "ready" }));

    state.set_ready(false);
    let (status, _) = get(state, "/ready").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn version_reports_build_and_environment() {
    let (status, body) = get(ready_state("staging"), "/version").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
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
    let (status, body) = get(ready_state("test"), "/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "status": "not_found" }));
}

#[tokio::test]
async fn wrong_method_is_405() {
    let response = routes::router(ready_state("test"))
        .oneshot(Request::post("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

/// Minimal HTTP/1.1 client so the test exercises the real TCP server path.
async fn raw_get(addr: std::net::SocketAddr, path: &str) -> String {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

#[tokio::test]
async fn real_server_serves_and_shuts_down_gracefully() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let state = AppState::new("test");
    let (tx, rx) = oneshot::channel::<()>();

    let server = tokio::spawn(serve(listener, state.clone(), async move {
        let _ = rx.await;
    }));

    let response = raw_get(addr, "/ready").await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(state.is_ready());

    tx.send(()).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("server did not shut down within 5s")
        .unwrap();
    assert!(result.is_ok());
    assert!(!state.is_ready(), "readiness must drop on shutdown");
    assert!(
        TcpStream::connect(addr).await.is_err(),
        "listener should be closed after shutdown"
    );
}
