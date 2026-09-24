use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn health_returns_ok() {
    let state = refinery::state::AppState::for_test();
    let app = refinery::routes::router(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body, json!({ "status": "ok" }));
}

async fn get_ready(app: axum::Router, host: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(
            Request::builder()
                .uri("/ready")
                .header("host", host)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn stub_upstream() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buffer = [0_u8; 1024];
                let _ = stream.read(&mut buffer).await;
                let _ = stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok",
                    )
                    .await;
            });
        }
    });
    format!("http://{address}")
}

#[tokio::test]
async fn ready_reports_degraded_when_upstreams_are_unreachable() {
    let app = refinery::routes::router(refinery::state::AppState::for_test());
    let (status, body) = get_ready(app, "localhost").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["status"], "degraded");
    assert_eq!(body["checks"]["searxng"], "unreachable");
    assert_eq!(body["checks"]["reader"], "unreachable");
    assert_eq!(body["checks"]["mcp_host"], "allowed");
}

#[tokio::test]
async fn ready_reports_blocked_host_when_authority_is_not_whitelisted() {
    let app = refinery::routes::router(refinery::state::AppState::for_test());
    let (status, body) = get_ready(app, "search.example.com").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["checks"]["mcp_host"], "blocked");
}

#[tokio::test]
async fn ready_accepts_authority_whose_port_is_not_in_the_allowlist() {
    let app = refinery::routes::router(refinery::state::AppState::for_test());
    let (_, body) = get_ready(app, "localhost:18090").await;
    assert_eq!(body["checks"]["mcp_host"], "allowed");
}

#[tokio::test]
async fn ready_accepts_bracketed_ipv6_authority_with_port() {
    let app = refinery::routes::router(refinery::state::AppState::for_test());
    let (_, body) = get_ready(app, "[::1]:18090").await;
    assert_eq!(body["checks"]["mcp_host"], "allowed");
}

#[tokio::test]
async fn ready_reports_blocked_host_when_the_allowlist_requires_another_port() {
    let mut config = refinery::config::Config::for_test();
    config.mcp_allowed_hosts = vec!["localhost:18090".to_owned()];
    let app = refinery::routes::router(refinery::state::AppState::new(config));
    let (_, body) = get_ready(app, "localhost:443").await;
    assert_eq!(body["checks"]["mcp_host"], "blocked");
}

#[tokio::test]
async fn ready_reports_ready_when_upstreams_answer() {
    let base = stub_upstream().await;
    let mut config = refinery::config::Config::for_test();
    config.searxng_base_url = base.clone();
    config.reader_base_url = base;
    let app = refinery::routes::router(refinery::state::AppState::new(config));
    let (status, body) = get_ready(app, "localhost").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ready");
    assert_eq!(body["checks"]["searxng"], "ok");
    assert_eq!(body["checks"]["reader"], "ok");
}
