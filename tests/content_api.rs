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
    assert_eq!(body["requested_url"], "https://example.test/docs/article");
    assert_eq!(body["final_url"], "https://example.test/docs/article");
    assert_eq!(body["resource_kind"], "html");
    assert_eq!(body["title"], "Example article");
    assert_eq!(
        body["links"],
        json!([{
            "text": "guide",
            "url": "https://example.test/guide",
            "kind": "html"
        }])
    );
    assert_eq!(body["offset"], 0);
    assert_eq!(body["next_offset"], 1000);
    assert_eq!(body["truncated"], true);
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
fn content_marks_known_plain_text_extensions_as_text() {
    let response = refinery::content::response(
        &refinery::content::ContentRequest {
            url: "https://example.test/notes.txt".to_owned(),
            offset: 0,
            max_chars: 1000,
        },
        "https://example.test/notes.txt".parse().unwrap(),
        "text/markdown".to_owned(),
        "plain text".to_owned(),
    );

    assert_eq!(
        serde_json::to_value(response).unwrap()["resource_kind"],
        "text"
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

#[tokio::test]
async fn content_directs_images_and_unknown_extensions_to_resource_download() {
    for url in [
        "https://example.test/diagram.png",
        "https://example.test/archive.custom",
    ] {
        let response = post_content(app("http://reader.test"), json!({ "url": url })).await;

        assert_eq!(
            response.status(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "{url}"
        );
        assert_eq!(
            json_body(response).await,
            json!({
                "error": {
                    "code": "resource_download_required",
                    "message": "该资源不支持文本抽取，请通过资源下载接口获取原文件"
                },
                "resource_url": format!("/v1/resource?url={}", urlencoding(url))
            }),
            "{url}"
        );
    }
}

fn app(reader_base_url: &str) -> Router {
    refinery::routes::router(refinery::state::AppState::new(refinery::config::Config {
        bind: "127.0.0.1".parse().unwrap(),
        port: 0,
        searxng_base_url: "http://searxng.test".to_owned(),
        reader_base_url: reader_base_url.to_owned(),
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
