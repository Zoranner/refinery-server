use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    response::IntoResponse,
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
    let response = post_json(app(), "/v1/search", json!({ "query": "rust" })).await;

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

#[tokio::test]
async fn resource_download_required_preserves_resource_url() {
    let url = url::Url::parse("https://example.test/report.pdf?download=1").unwrap();
    let response = refinery::error::ApiError::resource_download_required(&url).into_response();

    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(response.headers()["content-type"], "application/json");
    assert_eq!(
        json_body(response).await,
        json!({
            "error": {
                "code": "resource_download_required",
                "message": "该资源不支持文本抽取，请通过资源下载接口获取原文件"
            },
            "resource_url": "/v1/resource?url=https%3A%2F%2Fexample.test%2Freport.pdf%3Fdownload%3D1"
        })
    );
}

fn app() -> Router {
    refinery::routes::router(refinery::state::AppState::new(
        refinery::config::Config::for_test(),
    ))
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
