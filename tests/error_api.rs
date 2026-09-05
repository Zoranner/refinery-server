use std::net::SocketAddr;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    response::IntoResponse,
    routing::get,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

#[tokio::test]
async fn malformed_json_returns_machine_readable_error() {
    let response = post_raw(app(), "/v1/sitemap", "{").await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(response.headers()["content-type"], "application/json");
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn missing_required_url_returns_machine_readable_error() {
    let response = post_json(app(), "/v1/sitemap", json!({})).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(response.headers()["content-type"], "application/json");
    assert_eq!(
        json_body(response).await["error"]["code"],
        "invalid_request"
    );
}

#[tokio::test]
async fn invalid_url_preserves_invalid_request_code() {
    let response = post_json(app(), "/v1/content", json!({ "url": "not-a-valid-url" })).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json_body(response).await,
        json!({
            "error": {
                "code": "invalid_request",
                "message": "url is invalid"
            }
        })
    );
}

#[tokio::test]
async fn blocked_target_preserves_business_error_code() {
    let response = post_json(
        app(),
        "/v1/sitemap",
        json!({ "url": "http://127.0.0.1/admin" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(response).await,
        json!({
            "error": {
                "code": "blocked_target",
                "message": "target address is not allowed"
            }
        })
    );
}

#[tokio::test]
async fn upstream_business_error_remains_machine_readable() {
    let upstream = start_failing_searxng().await;
    let response = post_json(
        app_with_searxng(&format!("http://{}", upstream.address)),
        "/v1/search",
        json!({ "query": "rust" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(
        json_body(response).await,
        json!({
            "error": {
                "code": "search_upstream_failed",
                "message": "search upstream is unavailable"
            }
        })
    );
}

#[test]
fn legacy_download_only_error_helpers_are_absent_from_active_contract() {
    let forbidden = [
        concat!("resource_", "download_required"),
        concat!("unsupported_", "media_type"),
    ];
    let mut files = Vec::new();
    collect_files(std::path::Path::new("src"), &mut files);
    collect_files(std::path::Path::new("tests"), &mut files);
    files.push(std::path::PathBuf::from("README.md"));
    collect_files(std::path::Path::new("docs/superpowers/specs"), &mut files);
    collect_files(std::path::Path::new("docs/superpowers/plans"), &mut files);

    for path in files {
        if path
            == std::path::Path::new(
                "docs/reviews/2026-09-04-refinery-material-access-acceptance.md",
            )
        {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        for term in forbidden {
            assert!(!source.contains(term), "{term} found in {}", path.display());
        }
    }
}

fn collect_files(root: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_files(&path, files);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "rs" || extension == "md")
        {
            files.push(path);
        }
    }
}

fn app() -> Router {
    app_with_searxng("http://127.0.0.1:9")
}

fn app_with_searxng(searxng_base_url: &str) -> Router {
    refinery::routes::router(refinery::state::AppState::new(refinery::config::Config {
        bind: "127.0.0.1".parse().unwrap(),
        port: 0,
        searxng_base_url: searxng_base_url.to_owned(),
        reader_base_url: "http://reader.test".to_owned(),
        resource_timeout: std::time::Duration::from_secs(20),
        reader_timeout: std::time::Duration::from_secs(60),
    }))
}

async fn post_json(app: Router, path: &str, body: Value) -> axum::response::Response {
    post_raw(app, path, body.to_string()).await
}

async fn post_raw(app: Router, path: &str, body: impl Into<Body>) -> axum::response::Response {
    app.oneshot(
        Request::post(path)
            .header("content-type", "application/json")
            .body(body.into())
            .unwrap(),
    )
    .await
    .unwrap()
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

struct TestServer {
    address: SocketAddr,
}

async fn start_failing_searxng() -> TestServer {
    let router = Router::new().route("/search", get(searxng_failure));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    TestServer { address }
}

async fn searxng_failure() -> impl IntoResponse {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "fixture upstream failure",
    )
}
