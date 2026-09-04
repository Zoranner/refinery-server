use std::{collections::HashMap, net::SocketAddr};

use axum::{
    Json, Router,
    body::Body,
    extract::Query,
    http::{Request, StatusCode},
    response::IntoResponse,
    routing::get,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

#[tokio::test]
async fn search_rejects_blank_query() {
    let response = post_json(app("http://searxng.test"), json!({ "query": "  " })).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json_body(response).await,
        json!({
            "error": {
                "code": "invalid_request",
                "message": "query must not be blank"
            }
        })
    );
}

#[tokio::test]
async fn search_maps_searxng_json_response() {
    let upstream = start_searxng().await;
    let response = post_json(
        app(&format!("http://{}", upstream.address)),
        json!({
            "query": "Rust HTML 正文抽取",
            "page": 2,
            "limit": 10,
            "language": "zh-CN"
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await,
        json!({
            "query": "Rust HTML 正文抽取",
            "page": 2,
            "results": [
                {
                    "title": "DOM Smoothie",
                    "url": "https://example.test/dom-smoothie",
                    "snippet": "Readable content extractor",
                    "published_at": "2026-08-01"
                }
            ]
        })
    );
}

fn app(searxng_base_url: &str) -> Router {
    refinery::routes::router(refinery::state::AppState::new(refinery::config::Config {
        bind: "127.0.0.1".parse().unwrap(),
        port: 0,
        searxng_base_url: searxng_base_url.to_owned(),
        reader_base_url: "http://reader.test".to_owned(),
        reader_timeout: std::time::Duration::from_secs(60),
    }))
}

async fn post_json(app: Router, body: Value) -> axum::response::Response {
    app.oneshot(
        Request::post("/v1/search")
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

struct TestServer {
    address: SocketAddr,
}

async fn start_searxng() -> TestServer {
    let router = Router::new().route("/search", get(searxng_response));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    TestServer { address }
}

async fn searxng_response(Query(query): Query<HashMap<String, String>>) -> impl IntoResponse {
    assert_eq!(query.get("q"), Some(&"Rust HTML 正文抽取".to_owned()));
    assert_eq!(query.get("format"), Some(&"json".to_owned()));
    assert_eq!(query.get("pageno"), Some(&"2".to_owned()));
    assert_eq!(query.get("language"), Some(&"zh-CN".to_owned()));

    Json(json!({
        "results": [
            {
                "title": "DOM Smoothie",
                "url": "https://example.test/dom-smoothie",
                "content": "Readable content extractor",
                "publishedDate": "2026-08-01"
            }
        ]
    }))
}
