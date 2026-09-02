use std::sync::Arc;

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
async fn resource_returns_pdf_bytes_as_attachment() {
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

fn app(fetcher: Arc<FixtureFetcher>) -> Router {
    refinery::routes::router(refinery::state::AppState::with_resource_fetcher(
        refinery::config::Config::for_test(),
        fetcher,
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
