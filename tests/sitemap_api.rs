use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    extract::Json as ExtractJson,
    http::{Request, StatusCode},
    routing::post,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use url::Url;

#[tokio::test]
async fn sitemap_discovers_same_origin_urls_from_robots_and_index() {
    let fetcher = Arc::new(FixtureFetcher::new(HashMap::from([
        (
            "https://docs.example.test/robots.txt".to_owned(),
            "User-agent: *\nSitemap: https://docs.example.test/sitemap-index.xml\n".to_owned(),
        ),
        (
            "https://docs.example.test/sitemap-index.xml".to_owned(),
            r#"<?xml version="1.0"?>
            <sitemapindex>
              <sitemap><loc>https://docs.example.test/docs.xml</loc></sitemap>
            </sitemapindex>"#
                .to_owned(),
        ),
        (
            "https://docs.example.test/docs.xml".to_owned(),
            r#"<?xml version="1.0"?>
            <urlset>
              <url><loc>https://docs.example.test/start</loc></url>
              <url><loc>https://docs.example.test/api</loc></url>
              <url><loc>https://other.example.test/ignored</loc></url>
              <url><loc>https://docs.example.test/start</loc></url>
            </urlset>"#
                .to_owned(),
        ),
    ])));
    let response = post_sitemap(
        app(fetcher),
        json!({
            "url": "https://docs.example.test/guide/",
            "limit": 100
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await,
        json!({
            "requested_url": "https://docs.example.test/guide/",
            "site_url": "https://docs.example.test/",
            "urls": [
                { "url": "https://docs.example.test/start", "source": "sitemap" },
                { "url": "https://docs.example.test/api", "source": "sitemap" }
            ],
            "truncated": false,
            "warnings": []
        })
    );
}

#[tokio::test]
async fn sitemap_falls_back_to_same_origin_links_from_requested_page() {
    let reader_base_url = start_reader().await;
    let response = post_sitemap(
        app_with_reader(
            Arc::new(FixtureFetcher::new(HashMap::new())),
            &reader_base_url,
        ),
        json!({
            "url": "https://docs.example.test/guide/",
            "limit": 100
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await["urls"],
        json!([
            {
                "url": "https://docs.example.test/start",
                "source": "page_link"
            }
        ])
    );
}

fn app(fetcher: Arc<FixtureFetcher>) -> Router {
    app_with_reader(fetcher, "http://reader.test")
}

fn app_with_reader(fetcher: Arc<FixtureFetcher>, reader_base_url: &str) -> Router {
    let config = refinery::config::Config {
        bind: "127.0.0.1".parse().unwrap(),
        port: 0,
        searxng_base_url: "http://searxng.test".to_owned(),
        reader_base_url: reader_base_url.to_owned(),
        reader_timeout: std::time::Duration::from_secs(60),
    };

    refinery::routes::router(refinery::state::AppState::with_sitemap_fetcher(
        config, fetcher,
    ))
}

async fn start_reader() -> String {
    let router = Router::new().route("/", post(reader_response));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    format!("http://{address}")
}

async fn reader_response(
    ExtractJson(body): ExtractJson<Value>,
) -> ([(axum::http::header::HeaderName, &'static str); 1], String) {
    assert_eq!(body, json!({ "url": "https://docs.example.test/guide/" }));

    (
        [(axum::http::header::CONTENT_TYPE, "text/markdown")],
        "[start](/start)\n[external](https://other.example.test/ignored)".to_owned(),
    )
}

async fn post_sitemap(app: Router, body: Value) -> axum::response::Response {
    app.oneshot(
        Request::post("/v1/sitemap")
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

struct FixtureFetcher {
    responses: HashMap<String, String>,
}

impl FixtureFetcher {
    fn new(responses: HashMap<String, String>) -> Self {
        Self { responses }
    }
}

#[async_trait]
impl refinery::sitemap::SitemapFetcher for FixtureFetcher {
    async fn get(&self, url: &Url) -> Result<String, refinery::error::ApiError> {
        self.responses
            .get(url.as_str())
            .cloned()
            .ok_or_else(refinery::error::ApiError::fetch_failed)
    }
}
