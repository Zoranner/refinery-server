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
