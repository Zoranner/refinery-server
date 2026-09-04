use std::net::SocketAddr;

use axum::{
    Router,
    body::Body,
    extract::{Json as ExtractJson, State},
    http::{HeaderMap, Request, StatusCode},
    routing::post,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

#[tokio::test]
async fn content_forwards_only_fixed_reader_options_and_returns_current_chunk_links() {
    let reader = start_reader().await;
    let response = post_content(
        app(&format!("http://{}", reader.address)),
        json!({
            "url": "https://example.test/docs/article",
            "offset": 0,
            "max_chars": 1000
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(
        body["target"]["requested_url"],
        "https://example.test/docs/article"
    );
    assert_eq!(
        body["target"]["final_url"],
        "https://example.test/docs/article"
    );
    assert_eq!(body["target"]["resource_kind"], "html");
    assert_eq!(body["target"]["content_type"], "text/html");
    assert_eq!(body["extraction"]["title"], "Example article");
    assert_eq!(body["extraction"]["reader_content_type"], "text/markdown");
    assert_eq!(
        body["extraction"]["links"],
        json!([{
            "text": "guide",
            "url": "https://example.test/guide",
            "kind": "html"
        }])
    );
    assert_eq!(body["extraction"]["status"], "extracted");
    assert_eq!(body["extraction"]["engine"], "reader_auto");
    assert_eq!(body["extraction"]["format"], "markdown");
    assert_eq!(body["pagination"]["offset"], 0);
    assert_eq!(body["pagination"]["next_offset"], 1000);
    assert_eq!(body["pagination"]["truncated"], true);
    assert_eq!(body["download"]["available"], true);
}

#[tokio::test]
async fn content_rejects_non_public_or_non_http_urls() {
    for url in [
        "file:///etc/passwd",
        "ftp://example.test/file",
        "http://127.0.0.1/admin",
        "http://[::1]/admin",
        "http://169.254.169.254/latest/meta-data",
        "http://192.168.1.10/",
    ] {
        let response = post_content(app("http://reader.test"), json!({ "url": url })).await;

        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{url}");
        assert_eq!(
            json_body(response).await["error"]["code"],
            "blocked_target",
            "{url}"
        );
    }
}

#[tokio::test]
async fn content_rejects_out_of_range_max_chars() {
    let response = post_content(
        app("http://reader.test"),
        json!({
            "url": "https://example.test/article",
            "max_chars": 999
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json_body(response).await["error"]["code"],
        "invalid_request"
    );
}

#[test]
fn content_uses_unambiguous_response_media_types() {
    let url = url::Url::parse("https://example.test/download").unwrap();

    assert_eq!(
        refinery::content::kind_for_response(&url, "application/pdf"),
        refinery::content::ResourceKind::Pdf
    );
    assert_eq!(
        refinery::content::kind_for_response(&url, "image/png"),
        refinery::content::ResourceKind::Image
    );
    assert_eq!(
        refinery::content::kind_for_response(&url, "application/json"),
        refinery::content::ResourceKind::Text
    );
}

#[test]
fn content_normalizes_empty_reader_markdown_without_losing_target_kind() {
    let target = refinery::material::TargetFacts::new(
        "https://example.test/notes.txt".parse().unwrap(),
        "https://example.test/notes.txt".parse().unwrap(),
        refinery::content::ResourceKind::Text,
        "text/plain".to_owned(),
    );

    let response = refinery::material::normalize_reader_result(
        target,
        " \n\t".to_owned(),
        "text/plain".to_owned(),
    );
    let json = serde_json::to_value(response).unwrap();

    assert_eq!(json["target"]["resource_kind"], "text");
    assert_eq!(json["extraction"]["status"], "empty");
    assert_eq!(json["extraction"]["markdown"], " \n\t");
    assert_eq!(json["extraction"]["links"], json!([]));
}

#[tokio::test]
async fn content_marks_reader_challenge_as_blocked_instead_of_success() {
    let reader = start_reader_body(
        "Title: Just a moment...\n\ncf-chl- challenge\n\nTarget URL returned error 403: Forbidden",
    )
    .await;
    let response = post_content(
        app(&format!("http://{}", reader.address)),
        json!({ "url": "https://example.test/report.pdf" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["target"]["resource_kind"], "pdf");
    assert_eq!(body["target"]["content_type"], "application/pdf");
    assert_eq!(body["extraction"]["reader_content_type"], "text/plain");
    assert_eq!(body["extraction"]["status"], "blocked");
    assert_eq!(body["download"]["available"], true);
}

#[tokio::test]
async fn content_reports_download_only_without_calling_reader() {
    let response = post_content(
        app("http://127.0.0.1:9"),
        json!({ "url": "https://example.test/file.bin" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["target"]["resource_kind"], "unknown");
    assert_eq!(body["extraction"]["status"], "download_only");
    assert_eq!(body["extraction"]["engine"], "none");
    assert_eq!(body["download"]["available"], true);
}

#[tokio::test]
async fn content_reports_image_download_only_without_calling_reader() {
    for url in [
        "https://example.test/diagram.png",
        "https://example.test/archive.custom",
    ] {
        let response = post_content(app("http://reader.test"), json!({ "url": url })).await;

        assert_eq!(response.status(), StatusCode::OK, "{url}");
        let body = json_body(response).await;
        assert_eq!(
            body["target"]["resource_kind"],
            if url.ends_with(".png") {
                "image"
            } else {
                "unknown"
            }
        );
        assert_eq!(
            body["target"]["content_type"],
            if url.ends_with(".png") {
                "image/png"
            } else {
                "application/octet-stream"
            }
        );
        assert_eq!(body["extraction"]["status"], "download_only", "{url}");
        assert_eq!(body["extraction"]["engine"], "none", "{url}");
        assert_eq!(body["download"]["available"], true, "{url}");
        assert_eq!(
            body["download"]["resource_url"],
            format!("/v1/resource?url={}", urlencoding(url)),
            "{url}"
        );
    }
}

#[tokio::test]
async fn content_uses_redirected_resource_type_for_target_facts() {
    let reader = start_redirect_reader().await;
    let response = post_content(
        app(&format!("http://{}", reader.address)),
        json!({ "url": "https://example.test/download" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(
        body["target"]["final_url"],
        "https://example.test/report.pdf"
    );
    assert_eq!(body["target"]["resource_kind"], "pdf");
    assert_eq!(body["target"]["content_type"], "application/pdf");
}

fn app(reader_base_url: &str) -> Router {
    refinery::routes::router(refinery::state::AppState::new(refinery::config::Config {
        bind: "127.0.0.1".parse().unwrap(),
        port: 0,
        searxng_base_url: "http://searxng.test".to_owned(),
        reader_base_url: reader_base_url.to_owned(),
        resource_timeout: std::time::Duration::from_secs(20),
        reader_timeout: std::time::Duration::from_secs(60),
    }))
}

fn urlencoding(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

async fn post_content(app: Router, body: Value) -> axum::response::Response {
    app.oneshot(
        Request::post("/v1/content")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
    .unwrap()
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

struct TestReader {
    address: SocketAddr,
}

#[derive(Clone)]
struct ReaderState {
    article: String,
}

async fn start_reader() -> TestReader {
    let state = ReaderState {
        article: format!(
            "# Example article\n\n[guide](../guide)\n\n{}\n\n[report](/report.pdf)",
            "正文".repeat(600)
        ),
    };
    let router = Router::new()
        .route("/", post(reader_response))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    TestReader { address }
}

async fn start_reader_body(body: &str) -> TestReader {
    let state = ReaderState {
        article: body.to_owned(),
    };
    let router = Router::new()
        .route("/", post(reader_body_response))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    TestReader { address }
}

async fn start_redirect_reader() -> TestReader {
    let router = Router::new().route("/", post(redirect_reader_response));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    TestReader { address }
}

async fn redirect_reader_response(
    headers: HeaderMap,
    ExtractJson(_body): ExtractJson<Value>,
) -> (HeaderMap, String) {
    assert_eq!(headers["x-engine"], "auto");
    let mut response_headers = HeaderMap::new();
    response_headers.insert("content-type", "application/pdf".parse().unwrap());
    response_headers.insert(
        "x-responded-url",
        "https://example.test/report.pdf".parse().unwrap(),
    );
    (response_headers, "PDF content".to_owned())
}

async fn reader_body_response(
    State(state): State<ReaderState>,
    headers: HeaderMap,
    ExtractJson(_body): ExtractJson<Value>,
) -> (HeaderMap, String) {
    assert_eq!(headers["x-engine"], "auto");
    assert_eq!(headers["x-respond-timing"], "visible-content");
    assert_eq!(headers["x-respond-with"], "markdown");
    assert_eq!(headers["x-retain-links"], "all");
    let mut response_headers = HeaderMap::new();
    response_headers.insert("content-type", "text/plain".parse().unwrap());
    (response_headers, state.article)
}

async fn reader_response(
    State(state): State<ReaderState>,
    headers: HeaderMap,
    ExtractJson(body): ExtractJson<Value>,
) -> (HeaderMap, String) {
    assert_eq!(body, json!({ "url": "https://example.test/docs/article" }));
    assert_eq!(headers["x-no-cache"], "true");
    assert_eq!(headers["x-engine"], "auto");
    assert_eq!(headers["x-respond-timing"], "visible-content");
    assert_eq!(headers["x-respond-with"], "markdown");
    assert_eq!(headers["x-retain-links"], "all");
    assert_eq!(headers["x-timeout"], "60");

    let mut response_headers = HeaderMap::new();
    response_headers.insert("content-type", "text/markdown".parse().unwrap());
    response_headers.insert(
        "x-responded-url",
        "https://example.test/docs/article".parse().unwrap(),
    );

    (response_headers, state.article)
}
