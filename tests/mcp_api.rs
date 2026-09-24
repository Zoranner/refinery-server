use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

fn app() -> axum::Router {
    refinery::routes::router(refinery::state::AppState::for_test())
}

async fn post(app: axum::Router, body: Value, session: Option<&str>) -> axum::response::Response {
    let mut request = Request::post("/mcp")
        .header("host", "localhost")
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    if let Some(session) = session {
        request
            .headers_mut()
            .insert("mcp-session-id", session.parse().unwrap());
        request
            .headers_mut()
            .insert("mcp-protocol-version", "2025-03-26".parse().unwrap());
    }
    app.oneshot(request).await.unwrap()
}

async fn body(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    let payload = text
        .lines()
        .find_map(|line| {
            line.strip_prefix("data: ")
                .filter(|value| !value.is_empty())
        })
        .unwrap_or(&text);
    serde_json::from_str(payload.trim()).unwrap_or_else(|error| {
        panic!(
            "content type {:?}, body {:?}, parse error {error}",
            text, payload
        )
    })
}

#[tokio::test]
async fn mcp_initializes_and_lists_the_public_tools() {
    let service = app();
    let response = post(
        service.clone(),
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1"}
            }
        }),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let session = response.headers()["mcp-session-id"]
        .to_str()
        .unwrap()
        .to_owned();
    let initialized = body(response).await;
    assert_eq!(initialized["result"]["serverInfo"]["name"], "refinery");

    let response = post(
        service,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
        Some(&session),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let listed = body(response).await;
    let names = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        ["web_download", "web_explore", "web_read", "web_search"]
    );
}

#[tokio::test]
async fn business_http_routes_are_not_exposed() {
    let response = app()
        .oneshot(
            Request::post("/v1/search")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

fn app_with_unreachable_search() -> axum::Router {
    let mut config = refinery::config::Config::for_test();
    config.searxng_base_url = "http://127.0.0.1:9".to_owned();
    refinery::routes::router(refinery::state::AppState::new(config))
}

async fn initialize_session(service: axum::Router) -> String {
    let response = post(
        service,
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1"}
            }
        }),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response.headers()["mcp-session-id"]
        .to_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn upstream_failure_returns_tool_level_error() {
    let service = app_with_unreachable_search();
    let session = initialize_session(service.clone()).await;
    let response = post(
        service,
        json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "web_search", "arguments": {"query": "test"}}
        }),
        Some(&session),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let payload = body(response).await;
    assert!(
        payload.get("error").is_none(),
        "上游失败是工具执行失败，不是协议错误: {payload}"
    );
    let result = &payload["result"];
    assert_eq!(result["isError"], true);
    assert_eq!(
        result["structuredContent"]["error"]["code"],
        "upstream_unavailable"
    );
    assert_eq!(result["structuredContent"]["error"]["stage"], "search");
    assert_eq!(result["structuredContent"]["error"]["retryable"], true);
}

#[tokio::test]
async fn invalid_arguments_stay_protocol_errors() {
    let service = app();
    let session = initialize_session(service.clone()).await;
    let response = post(
        service,
        json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {
                "name": "web_read",
                "arguments": {"url": "https://example.com/", "max_chars": 900}
            }
        }),
        Some(&session),
    )
    .await;
    let payload = body(response).await;
    assert_eq!(payload["error"]["code"], -32602);
    assert!(payload.get("result").is_none(), "{payload}");
}

#[tokio::test]
async fn tool_schemas_declare_parameter_ranges() {
    let service = app();
    let session = initialize_session(service.clone()).await;
    let response = post(
        service,
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
        Some(&session),
    )
    .await;
    let listed = body(response).await;
    let tools = listed["result"]["tools"].as_array().unwrap();
    let schema_of = |name: &str| {
        tools
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("missing tool {name}"))["inputSchema"]
            .clone()
    };

    let read = schema_of("web_read");
    assert_eq!(read["properties"]["max_chars"]["minimum"], 1000);
    assert_eq!(read["properties"]["max_chars"]["maximum"], 24000);
    assert_eq!(read["properties"]["max_chars"]["default"], 8000);

    let search = schema_of("web_search");
    assert_eq!(search["properties"]["limit"]["maximum"], 20);

    let explore = schema_of("web_explore");
    assert_eq!(explore["properties"]["limit"]["minimum"], 1);
    assert_eq!(explore["properties"]["limit"]["maximum"], 500);
    assert_eq!(explore["properties"]["limit"]["default"], 100);
}
