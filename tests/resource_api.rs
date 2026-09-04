use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::ServiceExt;
use url::Url;

#[tokio::test]
async fn malformed_resource_query_returns_json_invalid_request() {
    let response = app(Arc::new(FixtureFetcher {
        content_type: "application/octet-stream".to_owned(),
        bytes: Vec::new(),
    }))
    .oneshot(
        Request::get("/v1/resource?url=%")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(response.headers()["content-type"], "application/json");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn missing_resource_query_returns_json_invalid_request() {
    let response = app(Arc::new(FixtureFetcher {
        content_type: "application/octet-stream".to_owned(),
        bytes: Vec::new(),
    }))
    .oneshot(Request::get("/v1/resource").body(Body::empty()).unwrap())
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(response.headers()["content-type"], "application/json");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn resource_response_reports_original_media_type_and_download_capability() {
    let fetcher = Arc::new(FixtureFetcher {
        content_type: "application/pdf".to_owned(),
        bytes: b"%PDF-1.7 fixture".to_vec(),
    });
    let response = app(fetcher)
        .oneshot(
            Request::get("/v1/resource?url=https%3A%2F%2Fexample.test%2Fdownload%3Fid%3D42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/pdf");
    assert_eq!(response.headers()["content-disposition"], "attachment");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(
        response.into_body().collect().await.unwrap().to_bytes(),
        b"%PDF-1.7 fixture"[..]
    );
}

#[tokio::test]
async fn resource_downloads_unknown_media_types_as_attachments() {
    let fetcher = Arc::new(FixtureFetcher {
        content_type: "text/html".to_owned(),
        bytes: b"<html>not a resource</html>".to_vec(),
    });
    let response = app(fetcher)
        .oneshot(
            Request::get("/v1/resource?url=https%3A%2F%2Fexample.test%2Fpage")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "text/html");
    assert_eq!(response.headers()["content-disposition"], "attachment");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(
        response.into_body().collect().await.unwrap().to_bytes(),
        b"<html>not a resource</html>"[..]
    );
}

#[tokio::test]
async fn resource_fetch_uses_resource_timeout_not_reader_timeout() {
    let fetcher = Arc::new(StallingFetcher {
        delay: Duration::from_millis(50),
    });
    let mut config = refinery::config::Config::for_test();
    config.resource_timeout = Duration::from_millis(10);
    config.reader_timeout = Duration::from_secs(1);

    let response = app_with_config(config, fetcher)
        .oneshot(
            Request::get("/v1/resource?url=https%3A%2F%2Fexample.test%2Fstall")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
}

fn app(fetcher: Arc<FixtureFetcher>) -> Router {
    app_with_config(refinery::config::Config::for_test(), fetcher)
}

fn app_with_config<F>(config: refinery::config::Config, fetcher: Arc<F>) -> Router
where
    F: refinery::resource::ResourceFetcher + 'static,
{
    refinery::routes::router(refinery::state::AppState::with_resource_fetcher(
        config, fetcher,
    ))
}

struct FixtureFetcher {
    content_type: String,
    bytes: Vec<u8>,
}

#[async_trait]
impl refinery::resource::ResourceFetcher for FixtureFetcher {
    async fn get(
        &self,
        url: &Url,
    ) -> Result<refinery::resource::Resource, refinery::error::ApiError> {
        assert_eq!(url.host_str(), Some("example.test"));

        Ok(refinery::resource::Resource {
            final_url: url.clone(),
            content_type: self.content_type.clone(),
            bytes: self.bytes.clone(),
        })
    }
}

struct StallingFetcher {
    delay: Duration,
}

#[async_trait]
impl refinery::resource::ResourceFetcher for StallingFetcher {
    async fn get(
        &self,
        _url: &Url,
    ) -> Result<refinery::resource::Resource, refinery::error::ApiError> {
        tokio::time::sleep(self.delay).await;
        Err(refinery::error::ApiError::fetch_failed())
    }
}
