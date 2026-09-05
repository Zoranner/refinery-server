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
        resource_timeout: std::time::Duration::from_secs(20),
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
    let content_schema = &document["components"]["schemas"]["MaterialContentResponse"];
    assert!(content_schema.is_object());
    assert_eq!(
        document["paths"]["/v1/content"]["post"]["responses"]["200"]["content"]["application/json"]
            ["schema"]["$ref"],
        "#/components/schemas/MaterialContentResponse"
    );
    assert!(document["components"]["schemas"]["ContentResponse"].is_null());
    assert_eq!(
        document["components"]["schemas"]["Error"]["required"][0],
        "code"
    );
}

#[tokio::test]
async fn openapi_describes_material_response_and_search_pagination() {
    let state = refinery::state::AppState::new(refinery::config::Config {
        bind: "127.0.0.1".parse().unwrap(),
        port: 0,
        searxng_base_url: "http://searxng.test".to_owned(),
        reader_base_url: "http://reader.test".to_owned(),
        resource_timeout: std::time::Duration::from_secs(20),
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

    assert!(document["components"]["schemas"]["TargetFacts"].is_object());
    assert!(document["components"]["schemas"]["ExtractionResult"].is_object());
    assert!(document["components"]["schemas"]["DownloadCapability"].is_object());
    assert_eq!(
        document["components"]["schemas"]["MaterialStatus"]["enum"][2],
        "blocked"
    );
    assert!(document["components"]["schemas"]["SearchPagination"].is_object());
    assert_eq!(
        document["components"]["schemas"]["SearchPagination"]["properties"]["has_more"]["nullable"],
        true
    );
    assert_eq!(
        document["components"]["schemas"]["ExtractionResult"]["properties"]["reader_content_type"]
            ["nullable"],
        true
    );
    assert_eq!(
        document["paths"]["/v1/content"]["post"]["responses"]["413"]["$ref"],
        "#/components/responses/ResponseTooLarge"
    );
    assert!(document["paths"]["/v1/sitemap"].is_object());
    assert!(document["paths"]["/v1/sitemap"]["post"].is_object());
    assert!(document["paths"]["/v1/sitemap"]["post"]["responses"].is_object());
    assert!(document["paths"]["/v1/sitemap"]["post"]["responses"]["413"].is_null());
    assert!(document["paths"]["/v1/sitemap"]["post"]["responses"]["504"].is_null());
}

#[tokio::test]
async fn openapi_describes_sitemap_status_sources_and_warnings() {
    let state = refinery::state::AppState::new(refinery::config::Config {
        bind: "127.0.0.1".parse().unwrap(),
        port: 0,
        searxng_base_url: "http://searxng.test".to_owned(),
        reader_base_url: "http://reader.test".to_owned(),
        resource_timeout: std::time::Duration::from_secs(20),
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

    assert!(document["components"]["schemas"]["SitemapStatus"].is_object());
    assert!(document["components"]["schemas"]["SitemapSourceStatus"].is_object());
    assert!(document["components"]["schemas"]["SitemapSources"].is_object());
    assert!(document["components"]["schemas"]["SitemapWarning"].is_object());
    assert_eq!(
        document["paths"]["/v1/sitemap"]["post"]["responses"]["200"]["content"]["application/json"]
            ["schema"]["$ref"],
        "#/components/schemas/SitemapResponse"
    );
    assert!(document["paths"]["/v1/sitemap"]["post"]["responses"]["413"].is_null());
    assert!(document["paths"]["/v1/sitemap"]["post"]["responses"]["504"].is_null());
    assert_eq!(
        document["components"]["schemas"]["SitemapResponse"]["required"],
        serde_json::json!([
            "requested_url",
            "site_url",
            "status",
            "sources",
            "urls",
            "truncated",
            "warnings"
        ])
    );
    assert_eq!(
        document["components"]["schemas"]["SitemapWarning"]["required"],
        serde_json::json!(["source", "code"])
    );
}
