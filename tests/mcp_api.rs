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

async fn stub_reader(markdown: &'static str) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buffer = [0_u8; 2048];
                let _ = stream.read(&mut buffer).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/plain; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    markdown.len(),
                    markdown
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    format!("http://{address}")
}

#[tokio::test]
async fn web_read_projects_links_and_reports_stats() {
    let reader = stub_reader(
        "# 标题\n\n[导航](https://example.com/nav)\n[手册](https://example.com/report.pdf)\n",
    )
    .await;
    let mut config = refinery::config::Config::for_test();
    config.reader_base_url = reader;
    let service = refinery::routes::router(refinery::state::AppState::new(config));
    let session = initialize_session(service.clone()).await;
    let response = post(
        service,
        json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "web_read", "arguments": {"url": "https://example.com/page"}}
        }),
        Some(&session),
    )
    .await;
    let payload = body(response).await;
    assert_eq!(payload["result"]["isError"], false);
    assert!(
        payload["result"].get("structuredContent").is_none(),
        "成功结果只返回一份载荷: {payload}"
    );
    let text = payload["result"]["content"][0]["text"].as_str().unwrap();
    let document: Value = serde_json::from_str(text).unwrap();
    let links = document["extraction"]["links"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["url"], "https://example.com/report.pdf");
    assert!(
        document["extraction"]["markdown"]
            .as_str()
            .unwrap()
            .contains("https://example.com/nav"),
        "默认投影不改变 Markdown 内联链接: {document}"
    );
    assert_eq!(document["stats"]["links_included"], 1);
    assert_eq!(document["stats"]["links_omitted"], 0);
    assert!(document["diagnostics"]["duration_ms"].is_number());
    assert!(document["diagnostics"]["timeout_seconds"].is_number());
}

async fn stub_searxng(
    status: u16,
    body: &'static str,
) -> (String, std::sync::Arc<std::sync::Mutex<String>>) {
    use std::sync::{Arc, Mutex};

    let captured = Arc::new(Mutex::new(String::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let sink = Arc::clone(&captured);
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let sink = Arc::clone(&sink);
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buffer = [0_u8; 4096];
                let read = stream.read(&mut buffer).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                if let Some(line) = request.lines().next() {
                    *sink.lock().unwrap() = line.to_owned();
                }
                let response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    (format!("http://{address}"), captured)
}

#[tokio::test]
async fn web_search_forwards_categories_and_reports_engine_facts() {
    let (base, captured) = stub_searxng(
        200,
        r#"{"results":[{"title":"a","url":"https://a.example/1","content":"x","engines":["bing"]}],"unresponsive_engines":[["google","拒绝访问"]],"number_of_results":7}"#,
    )
    .await;
    let mut config = refinery::config::Config::for_test();
    config.searxng_base_url = base;
    let service = refinery::routes::router(refinery::state::AppState::new(config));
    let session = initialize_session(service.clone()).await;
    let response = post(
        service,
        json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "web_search", "arguments": {
                "query": "trafilatura", "categories": "it", "safesearch": 1
            }}
        }),
        Some(&session),
    )
    .await;
    let payload = body(response).await;
    let text = payload["result"]["content"][0]["text"].as_str().unwrap();
    let document: Value = serde_json::from_str(text).unwrap();

    assert_eq!(document["diagnostics"]["source_status"], "partial");
    assert_eq!(
        document["diagnostics"]["unresponsive_engines"][0]["name"],
        "google"
    );
    assert_eq!(document["diagnostics"]["engines"][0]["name"], "bing");
    assert_eq!(document["diagnostics"]["upstream_results"], 7);
    assert!(
        document["diagnostics"].get("warnings").is_none(),
        "显式指定类别时不应给出默认类别退化提示: {document}"
    );

    let request_line = captured.lock().unwrap().clone();
    assert!(request_line.contains("categories=it"), "{request_line}");
    assert!(request_line.contains("safesearch=1"), "{request_line}");
}

#[tokio::test]
async fn search_scope_routes_to_the_mapped_category() {
    let (base, captured) = stub_searxng(
        200,
        r#"{"results":[{"title":"a","url":"https://a.example/1","content":"x","engines":["arxiv"]}]}"#,
    )
    .await;
    let mut config = refinery::config::Config::for_test();
    config.searxng_base_url = base;
    let service = refinery::routes::router(refinery::state::AppState::new(config));
    let session = initialize_session(service.clone()).await;
    let response = post(
        service,
        json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "web_search", "arguments": {"query": "trafilatura", "scope": "paper"}}
        }),
        Some(&session),
    )
    .await;
    let payload = body(response).await;
    let text = payload["result"]["content"][0]["text"].as_str().unwrap();
    let document: Value = serde_json::from_str(text).unwrap();

    assert_eq!(document["diagnostics"]["routing"]["scope"], "paper");
    assert_eq!(document["diagnostics"]["routing"]["categories"], "science");
    let request_line = captured.lock().unwrap().clone();
    assert!(
        request_line.contains("categories=science"),
        "{request_line}"
    );
}

#[tokio::test]
async fn search_reports_upstream_status_in_the_error() {
    let (base, _captured) = stub_searxng(400, r#"{"error":"bad category"}"#).await;
    let mut config = refinery::config::Config::for_test();
    config.searxng_base_url = base;
    let service = refinery::routes::router(refinery::state::AppState::new(config));
    let session = initialize_session(service.clone()).await;
    let response = post(
        service,
        json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "web_search", "arguments": {"query": "trafilatura"}}
        }),
        Some(&session),
    )
    .await;
    let payload = body(response).await;
    let error = &payload["result"]["structuredContent"]["error"];

    assert_eq!(payload["result"]["isError"], true);
    assert_eq!(error["code"], "upstream_unavailable");
    assert!(
        error["message"].as_str().unwrap().contains("400"),
        "{error}"
    );
}

#[tokio::test]
async fn search_schema_declares_categories_and_safesearch() {
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
    let search = tools
        .iter()
        .find(|tool| tool["name"] == "web_search")
        .unwrap();

    assert_eq!(
        search["inputSchema"]["properties"]["safesearch"]["minimum"],
        0
    );
    assert_eq!(
        search["inputSchema"]["properties"]["safesearch"]["maximum"],
        2
    );
    assert_eq!(
        search["inputSchema"]["properties"]["safesearch"]["default"],
        1
    );
    assert!(search["inputSchema"]["properties"]["categories"].is_object());
    let scope = search["inputSchema"]["properties"]["scope"].to_string();
    for expected in ["web", "code", "qa", "package", "paper"] {
        assert!(
            scope.contains(expected),
            "scope schema 缺少 {expected}: {scope}"
        );
    }
}
