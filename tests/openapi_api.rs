use axum::{body::Body, http::Request};
use serde_json::Value;
use tower::ServiceExt;

#[tokio::test]
async fn serves_machine_readable_openapi_contract() {
    let state = refinery::state::AppState::new(refinery::config::Config {
        bind: "127.0.0.1".parse().unwrap(),
        port: 0,
        searxng_base_url: "http://searxng.test".to_owned(),
        reader_base_url: "http://reader.test".to_owned(),
        reader_timeout: std::time::Duration::from_secs(60),
    });

    let response = refinery::routes::router(state)
        .oneshot(Request::get("/openapi.json").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let document: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(document["openapi"], "3.0.3");
    assert!(document["paths"]["/v1/search"].is_object());
    assert!(document["paths"]["/v1/content"].is_object());
    let content_schema = &document["components"]["schemas"]["ContentResponse"];
    assert!(content_schema["properties"]["resource_kind"].is_object());
    assert!(content_schema["properties"]["content_kind"].is_null());
    assert_eq!(
        document["components"]["schemas"]["Error"]["required"][0],
        "code"
    );
}
