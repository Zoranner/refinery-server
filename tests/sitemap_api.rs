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
            "status": "discovered",
            "sources": {
                "robots_txt": "discovered",
                "sitemap": "discovered",
                "page_links": "not_attempted"
            },
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
async fn sitemap_exposes_source_failures_without_hiding_discovery_state() {
    let fetcher = Arc::new(FixtureFetcher::all_timeouts());
    let reader_base_url = start_slow_reader().await;
    let response = post_sitemap_with_reader_timeout(
        app_with_reader_timeout(
            fetcher,
            &reader_base_url,
            std::time::Duration::from_millis(10),
        ),
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
            "status": "timed_out",
            "sources": {
                "robots_txt": "timed_out",
                "sitemap": "timed_out",
                "page_links": "timed_out"
            },
            "urls": [],
            "truncated": false,
            "warnings": [
                { "source": "robots_txt", "code": "source_timeout" },
                { "source": "sitemap", "code": "source_timeout" },
                { "source": "page_links", "code": "source_timeout" }
            ]
        })
    );
}

#[tokio::test]
async fn sitemap_marks_partial_when_sitemap_urls_exist_but_one_source_fails() {
    let fetcher = Arc::new(FixtureFetcher::new(HashMap::from([(
        "https://docs.example.test/sitemap.xml".to_owned(),
        r#"<?xml version="1.0"?>
            <urlset>
              <url><loc>https://docs.example.test/start</loc></url>
            </urlset>"#
            .to_owned(),
    )])));
    let response = post_sitemap(
        app(fetcher),
        json!({
            "url": "https://docs.example.test/guide/",
            "limit": 100
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "partial");
    assert_eq!(body["sources"]["robots_txt"], "failed");
    assert_eq!(body["sources"]["sitemap"], "discovered");
    assert_eq!(body["sources"]["page_links"], "not_attempted");
    assert_eq!(
        body["warnings"],
        json!([{ "source": "robots_txt", "code": "source_failed" }])
    );
    assert_eq!(
        body["urls"],
        json!([{ "url": "https://docs.example.test/start", "source": "sitemap" }])
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
    let body = json_body(response).await;
    assert_eq!(
        body["urls"],
        json!([
            {
                "url": "https://docs.example.test/start",
                "source": "page_link"
            }
        ])
    );
    assert_eq!(body["status"], "partial");
    assert_eq!(body["sources"]["robots_txt"], "failed");
    assert_eq!(body["sources"]["sitemap"], "failed");
    assert_eq!(body["sources"]["page_links"], "discovered");
}

fn app(fetcher: Arc<FixtureFetcher>) -> Router {
    app_with_reader(fetcher, "http://reader.test")
}

fn app_with_reader(fetcher: Arc<FixtureFetcher>, reader_base_url: &str) -> Router {
    app_with_reader_timeout(fetcher, reader_base_url, std::time::Duration::from_secs(60))
}

fn app_with_reader_timeout(
    fetcher: Arc<FixtureFetcher>,
    reader_base_url: &str,
    reader_timeout: std::time::Duration,
) -> Router {
    let config = refinery::config::Config {
        bind: "127.0.0.1".parse().unwrap(),
        port: 0,
        searxng_base_url: "http://searxng.test".to_owned(),
        reader_base_url: reader_base_url.to_owned(),
        resource_timeout: std::time::Duration::from_secs(20),
        reader_timeout,
    };

    refinery::routes::router(refinery::state::AppState::with_sitemap_fetcher(
        config, fetcher,
    ))
}

async fn post_sitemap_with_reader_timeout(app: Router, body: Value) -> axum::response::Response {
    post_sitemap(app, body).await
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

async fn start_slow_reader() -> String {
    let router = Router::new().route("/", post(slow_reader_response));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    format!("http://{address}")
}

async fn slow_reader_response(
    ExtractJson(_body): ExtractJson<Value>,
) -> ([(axum::http::HeaderName, &'static str); 1], String) {
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    (
        [(axum::http::header::CONTENT_TYPE, "text/markdown")],
        String::new(),
    )
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
    all_timeouts: bool,
}

impl FixtureFetcher {
    fn new(responses: HashMap<String, String>) -> Self {
        Self {
            responses,
            all_timeouts: false,
        }
    }

    fn all_timeouts() -> Self {
        Self {
            responses: HashMap::new(),
            all_timeouts: true,
        }
    }
}

#[async_trait]
impl refinery::sitemap::SitemapFetcher for FixtureFetcher {
    async fn get(&self, url: &Url) -> Result<String, refinery::error::ApiError> {
        if self.all_timeouts {
            return Err(refinery::error::ApiError::fetch_timeout());
        }
        self.responses
            .get(url.as_str())
            .cloned()
            .ok_or_else(refinery::error::ApiError::fetch_failed)
    }
}
