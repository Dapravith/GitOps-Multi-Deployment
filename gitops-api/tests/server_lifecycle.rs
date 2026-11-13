//! Real TCP server: serve, flip readiness, drain, close the listener.

use std::time::Duration;

use gitops_api::bootstrap::serve;
use gitops_api::{AppState, HttpLimits};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

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

    let server = tokio::spawn(serve(
        listener,
        state.clone(),
        HttpLimits::default(),
        async move {
            let _ = rx.await;
        },
    ));

    let response = raw_get(addr, "/ready").await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(
        response.to_ascii_lowercase().contains("x-request-id:"),
        "{response}"
    );
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
