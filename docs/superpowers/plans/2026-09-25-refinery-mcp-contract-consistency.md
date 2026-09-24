# Refinery MCP 契约与一致性实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 Refinery 的工具契约、错误语义、响应预算与文档承诺一致，并补上能发现链路故障的就绪探针。

**Architecture:** 契约层（`src/mcp.rs`）负责参数校验、响应投影与错误分流；领域层（`src/error.rs`）承载错误类别、阶段与可重试标记；应用层（`src/application.rs`）与适配层（`reader`、`search`、`resource`、`sitemap`）只负责表达自己那一段的事实。响应体积由 `src/material/extraction.rs` 统一裁剪，其他层不再各自决定输出多少。

**Tech Stack:** Rust 2024、Axum 0.8、Tokio、reqwest、rmcp 3.3.0、schemars 1.2.2。

**Spec:** `docs/superpowers/specs/2026-09-25-refinery-system-design.md`

## Global Constraints

- 本计划只覆盖契约与一致性范围：错误契约、工具 schema、响应预算与链接投影、`/ready` 探针、文档与版本对齐。
- 抽取主路径切换、内存缓存、搜索策略与引擎收敛不在本计划范围内，本计划不实现，也不为它们预留空字段。
- 四个工具名称保持 `web_search`、`web_read`、`web_explore`、`web_download`，`web_download` 的外部行为不变。
- 不新增 Cargo 依赖。
- Rust 改动完成后必须执行 `cargo fmt --all` 与 `cargo clippy --all-targets --all-features -- -D warnings`。
- 提交前必须完整读取 `git-commit` 技能的 SKILL.md 并按清单执行；本计划中的提交步骤只在获得提交授权后执行。
- 本机没有 Docker，SearXNG 与 Reader 的真实链路验证在部署机执行；本地验证使用测试桩。

---

### Task 1: 错误契约分层（已完成）

**Files:**
- Modify: `src/error.rs`
- Modify: `src/mcp.rs`
- Modify: `src/reader/client.rs`
- Modify: `src/resource/mod.rs`
- Modify: `src/sitemap/client.rs`
- Modify: `src/search/searxng.rs`
- Modify: `src/application.rs`
- Modify: `src/sitemap/mod.rs:212`
- Test: `tests/mcp_api.rs`

**Interfaces:**
- Consumes: `application::search`、`application::read`、`application::explore` 返回 `Result<T, ApiError>`。
- Produces: `ApiError { status, code, message, stage, retryable }`；`ApiError::with_stage(self, stage) -> Self`；`ApiError::is_protocol_error(&self) -> bool`；`mcp::payload(Value) -> CallToolResult`；`mcp::fail(ApiError) -> Result<CallToolResult, rmcp::ErrorData>`。

- [ ] **Step 1: 写失败测试，证明业务失败现在被当成协议错误**

在 `tests/mcp_api.rs` 末尾追加。注意 `post()` 会消费 `Router`，`Router` 实现了 `Clone`，需要初始化会话的地方用 `service.clone()`。

```rust
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
```

- [ ] **Step 2: 运行测试，确认失败**

```text
cargo test --test mcp_api upstream_failure_returns_tool_level_error
```

Expected: FAIL，断言 `payload.get("error").is_none()` 失败，当前实现把 SearXNG 连接失败映射成 `-32602`。

- [ ] **Step 3: 扩展 ApiError，携带类别、阶段、可重试与动态消息**

把 `src/error.rs` 的 `ApiError` 与构造器替换为下列内容，`into_response` 一并调整。

```rust
use std::borrow::Cow;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: Cow<'static, str>,
    pub stage: &'static str,
    pub retryable: bool,
}

impl ApiError {
    pub fn new(
        status: StatusCode,
        code: &'static str,
        message: impl Into<Cow<'static, str>>,
        stage: &'static str,
        retryable: bool,
    ) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            stage,
            retryable,
        }
    }

    pub fn invalid_request(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            message,
            "request",
            false,
        )
    }

    pub fn upstream_unavailable() -> Self {
        Self::new(
            StatusCode::BAD_GATEWAY,
            "upstream_unavailable",
            "upstream is unavailable",
            "upstream",
            true,
        )
    }

    pub fn upstream_timeout() -> Self {
        Self::new(
            StatusCode::GATEWAY_TIMEOUT,
            "upstream_timeout",
            "upstream timed out",
            "upstream",
            true,
        )
    }

    pub fn blocked_target() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "blocked_target",
            "target address is not allowed",
            "policy",
            false,
        )
    }

    pub fn response_too_large() -> Self {
        Self::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "response_too_large",
            "content response is too large",
            "upstream",
            false,
        )
    }

    pub fn with_message(mut self, message: impl Into<Cow<'static, str>>) -> Self {
        self.message = message.into();
        self
    }

    pub fn with_stage(mut self, stage: &'static str) -> Self {
        self.stage = stage;
        self
    }

    pub fn is_protocol_error(&self) -> bool {
        self.code == "invalid_request"
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: ErrorBody {
                    code: self.code,
                    message: self.message,
                },
            }),
        )
            .into_response()
    }
}

#[derive(Serialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: Cow<'static, str>,
}
```

- [ ] **Step 4: 把旧构造器调用点改成新名字**

执行下列替换，`fetch_failed()` 改为 `upstream_unavailable()`，`fetch_timeout()` 改为 `upstream_timeout()`，`search_upstream_failed()` 改为 `upstream_unavailable()`。

```text
rg -l 'fetch_failed|fetch_timeout|search_upstream_failed' src
```

涉及文件：`src/application.rs`、`src/reader/client.rs`、`src/resource/mod.rs`、`src/sitemap/client.rs`、`src/sitemap/mod.rs`、`src/sitemap/parser.rs`、`src/search/searxng.rs`、`src/material/download.rs`。

同时把两处按旧码字符串判断的地方改为新码：`src/application.rs:122` 与 `src/sitemap/mod.rs:212` 中的 `error.code == "fetch_timeout"` 改为 `error.code == "upstream_timeout"`。

- [ ] **Step 5: 在四个适配层函数上标注失败阶段**

不逐个修改构造点，改为在返回边界统一标注，每个模块只改一处。

`src/reader/client.rs`：把现有 `read` 重命名为 `read_inner`，并新增包装函数。

```rust
pub async fn read(
    client: &reqwest::Client,
    base_url: &str,
    url: &Url,
    timeout: Duration,
) -> Result<ReaderDocument, ApiError> {
    read_inner(client, base_url, url, timeout)
        .await
        .map_err(|error| error.with_stage("read"))
}
```

`src/search/searxng.rs`：在 `search` 函数体外包一层同样的 `with_stage("search")`。

`src/resource/mod.rs`：在 `HttpResourceFetcher::get` 与 `get_with_timeout` 的返回处标注 `with_stage("resource")`。

`src/sitemap/client.rs`：在 `HttpSitemapFetcher::get` 的返回处标注 `with_stage("sitemap")`。

同时给 `src/reader/client.rs` 失败分支补上真实原因，让错误消息不再是无信息量的一句话。

```rust
    if !response.status().is_success() {
        return Err(ApiError::upstream_unavailable()
            .with_stage("read")
            .with_message(format!("reader returned status {}", response.status())));
    }
```

- [ ] **Step 6: 修正 mcp 层错误分流**

在 `src/mcp.rs` 增加两个辅助函数，并把四个工具的错误处理改为调用它们。

```rust
fn payload(value: serde_json::Value) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(value.to_string())])
}

fn fail(error: crate::error::ApiError) -> Result<CallToolResult, rmcp::ErrorData> {
    if error.is_protocol_error() {
        return Err(rmcp::ErrorData::invalid_params(error.message, None));
    }
    Ok(CallToolResult::structured_error(serde_json::json!({
        "error": {
            "code": error.code,
            "message": error.message,
            "stage": error.stage,
            "retryable": error.retryable,
        }
    })))
}
```

四个工具的处理体改为下列形式。

```rust
    async fn web_search(
        &self,
        Parameters(input): Parameters<SearchInput>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let request = crate::search::SearchRequest {
            query: input.query,
            page: input.page.unwrap_or(1),
            limit: input.limit.unwrap_or(10),
            language: input.language,
        };
        let result = match crate::application::search(&self.state, request).await {
            Ok(result) => result,
            Err(error) => return fail(error),
        };
        let value = serde_json::to_value(result)
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?;
        Ok(payload(value))
    }

    async fn web_read(
        &self,
        Parameters(input): Parameters<ReadInput>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let request = crate::content::ContentRequest {
            url: input.url,
            offset: input.offset.unwrap_or(0),
            max_chars: input.max_chars.unwrap_or(12000),
        };
        let result = match crate::application::read(&self.state, request).await {
            Ok(result) => result,
            Err(error) => return fail(error),
        };
        let value = serde_json::to_value(result)
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?;
        Ok(payload(value))
    }

    async fn web_explore(
        &self,
        Parameters(input): Parameters<UrlInput>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let request = crate::sitemap::SitemapRequest {
            url: input.url,
            limit: 100,
        };
        let result = match crate::application::explore(&self.state, request).await {
            Ok(result) => result,
            Err(error) => return fail(error),
        };
        let value = serde_json::to_value(result)
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?;
        Ok(payload(value))
    }
```

`web_download` 的校验错误改为 `return fail(error)`，成功路径继续返回 `resource_link`。其中 `web_search`、`web_read` 的 `unwrap_or` 默认值会在 Task 2 改为由输入结构直接承载，这里先保持现有取值方式。

注意：`payload` 只返回一份文本载荷，不再重复返回 `structuredContent`；`fail` 保留 `structured_error`，因为错误需要结构化字段。

- [ ] **Step 7: 运行测试，确认通过**

```text
cargo test --test mcp_api
```

Expected: PASS，包含新增的 `upstream_failure_returns_tool_level_error`。

- [ ] **Step 8: 补参数错误的回归测试，确认仍是协议错误**

```rust
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
    assert!(payload.get("result").is_none());
}
```

```text
cargo test --test mcp_api invalid_arguments_stay_protocol_errors
```

Expected: PASS。

- [ ] **Step 9: 运行门禁并提交**

```text
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

Expected: 全部通过。获得提交授权后按 `git-commit` 技能提交 `src/error.rs`、`src/mcp.rs`、四个适配层文件与 `tests/mcp_api.rs`。

---

### Task 2: 工具 schema 与 web_explore 参数（已完成）

**Files:**
- Modify: `src/mcp.rs`
- Modify: `src/application.rs`
- Modify: `src/search/mod.rs`
- Modify: `src/content/mod.rs`
- Test: `tests/mcp_api.rs`

**Interfaces:**
- Consumes: `SearchRequest { query, page, limit, language }`、`ContentRequest { url, offset, max_chars }`、`SitemapRequest { url, limit }`。
- Produces: `SearchInput`、`ReadInput`、`ExploreInput` 三个独立输入结构，字段范围写入 JSON Schema；`web_explore` 的 `limit` 真实生效。

- [ ] **Step 1: 写失败测试，断言 schema 与参数约束**

```rust
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
```

- [ ] **Step 2: 运行测试，确认失败**

```text
cargo test --test mcp_api tool_schemas_declare_parameter_ranges
```

Expected: FAIL，当前 `web_explore` 的 schema 里没有 `limit`，`web_read` 也没有 `minimum` 与 `default`。

- [ ] **Step 3: 改写输入结构，范围与默认值进入 schema**

把 `src/mcp.rs` 的三个输入结构替换为下列定义。`range` 与 `default` 由 schemars 1.2.2 支持，`default` 通过 serde 默认值函数生效。

```rust
fn default_page() -> u32 {
    1
}

fn default_search_limit() -> u8 {
    10
}

fn default_offset() -> usize {
    0
}

fn default_max_chars() -> usize {
    8000
}

fn default_explore_limit() -> usize {
    100
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchInput {
    /// 搜索关键词，不能为空。
    pub query: String,
    /// 页码，从 1 开始。
    #[serde(default = "default_page")]
    #[schemars(range(min = 1))]
    pub page: u32,
    /// 返回条数上限。
    #[serde(default = "default_search_limit")]
    #[schemars(range(min = 1, max = 20))]
    pub limit: u8,
    /// SearXNG 语言代码，例如 zh-CN。
    pub language: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadInput {
    /// 目标公网 URL。
    pub url: String,
    /// 已抽取正文中的起始字符位置，续读时使用上次响应的 next_offset。
    #[serde(default = "default_offset")]
    pub offset: usize,
    /// 本次返回的正文最大字符数。
    #[serde(default = "default_max_chars")]
    #[schemars(range(min = 1000, max = 24000))]
    pub max_chars: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExploreInput {
    /// 站点内任意 URL，服务只发现同源地址。
    pub url: String,
    /// 最多发现的 URL 数量。
    #[serde(default = "default_explore_limit")]
    #[schemars(range(min = 1, max = 500))]
    pub limit: usize,
}
```

`UrlInput` 保留给 `web_download`。

- [ ] **Step 4: 接线 web_explore 的 limit**

`src/mcp.rs` 的 `web_explore` 改为使用 `ExploreInput`：

```rust
    async fn web_explore(
        &self,
        Parameters(input): Parameters<ExploreInput>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let request = crate::sitemap::SitemapRequest {
            url: input.url,
            limit: input.limit,
        };
        let result = match crate::application::explore(&self.state, request).await {
            Ok(result) => result,
            Err(error) => return fail(error),
        };
        Ok(payload(
            serde_json::to_value(result)
                .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?,
        ))
    }
```

`SitemapRequest::validate` 已经校验 1–500，不需要重复实现范围判断。

- [ ] **Step 5: 同步上游请求构造**

`web_search` 与 `web_read` 的请求构造改为直接使用具体字段，不再 `unwrap_or`：

```rust
        let request = crate::search::SearchRequest {
            query: input.query,
            page: input.page,
            limit: input.limit,
            language: input.language,
        };
```

```rust
        let request = crate::content::ContentRequest {
            url: input.url,
            offset: input.offset,
            max_chars: input.max_chars,
        };
```

`src/content/mod.rs` 的 `default_max_chars()` 同步改为 8000，与 MCP 层默认值一致；`max_chars` 的运行时校验保留，schema 与运行时校验是两道不同的防线。

- [ ] **Step 6: 运行测试，确认通过**

```text
cargo test --test mcp_api tool_schemas_declare_parameter_ranges
cargo test --all-targets --all-features
```

Expected: PASS，且原有 `mcp_initializes_and_lists_the_public_tools` 不回归。

- [ ] **Step 7: 运行门禁并提交**

```text
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
```

获得提交授权后按 `git-commit` 技能提交 `src/mcp.rs`、`src/application.rs`、`src/content/mod.rs`、`src/search/mod.rs` 与 `tests/mcp_api.rs`。

---

### Task 3: 链接投影、响应预算与诊断填充（已完成）

**Files:**
- Modify: `src/content/links.rs`
- Modify: `src/content/mod.rs`
- Modify: `src/material/model.rs`
- Modify: `src/material/extraction.rs`
- Modify: `src/material/mod.rs`
- Modify: `src/application.rs`
- Modify: `src/reader/client.rs`
- Modify: `src/mcp.rs`
- Test: `src/material/extraction.rs`（新增单元测试）
- Test: `tests/material_model.rs`（两处结构体字面量补齐新字段）

**Interfaces:**
- Produces: `content::links::LinkProjection { Resources, All }`；`Link::is_resource()`；`material::OutputStats { markdown_chars, links_included, links_omitted }`；`MaterialContentResponse.stats`；`ReaderDocument.duration_ms`。
- Consumes: `ContentRequest.links: LinkProjection`。

- [ ] **Step 1: 写失败测试，固定投影、去重与上限行为**

在 `src/material/extraction.rs` 末尾新增单元测试。

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::links::LinkProjection;
    use crate::content::ResourceKind;
    use url::Url;

    fn target() -> TargetFacts {
        let url = Url::parse("https://example.com/page").unwrap();
        TargetFacts::new(url.clone(), url, ResourceKind::Html, "text/html".to_owned())
    }

    #[test]
    fn resources_projection_keeps_only_resources_and_deduplicates() {
        let markdown = "# 标题\n\n\
            [导航](https://example.com/nav)\n\
            [导航重复](https://example.com/nav)\n\
            [手册](https://example.com/report.pdf)\n\
            [图纸](https://example.com/diagram.png)\n";
        let response = normalize_reader_result_with_options(
            target(),
            markdown.to_owned(),
            "text/plain; charset=utf-8".to_owned(),
            0,
            8000,
            LinkProjection::Resources,
        );
        let urls = response
            .extraction
            .links
            .iter()
            .map(|link| link.url.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            urls,
            ["https://example.com/report.pdf", "https://example.com/diagram.png"]
        );
        assert_eq!(response.stats.links_included, 2);
        assert_eq!(response.stats.links_omitted, 0);
    }

    #[test]
    fn all_projection_caps_links_and_reports_omitted() {
        let markdown = (0..60)
            .map(|index| format!("[资源 {index}](https://example.com/file-{index}.pdf)\n"))
            .collect::<String>();
        let response = normalize_reader_result_with_options(
            target(),
            markdown,
            "text/plain; charset=utf-8".to_owned(),
            0,
            8000,
            LinkProjection::All,
        );
        assert_eq!(response.extraction.links.len(), MAX_RESPONSE_LINKS);
        assert_eq!(response.stats.links_included, MAX_RESPONSE_LINKS);
        assert_eq!(response.stats.links_omitted, 60 - MAX_RESPONSE_LINKS);
        assert_eq!(response.stats.markdown_chars > 0, true);
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

```text
cargo test --lib material::extraction
```

Expected: FAIL 编译错误，`LinkProjection`、`MAX_RESPONSE_LINKS`、`stats` 都不存在。

- [ ] **Step 3: 增加链接投影类型与去重**

`src/content/links.rs` 顶部改为：

```rust
use std::collections::HashSet;

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinkProjection {
    /// 只返回 PDF、图片和未知附件，页面链接由 Markdown 内联表达。
    Resources,
    /// 返回当前分段中的全部链接。
    All,
}

impl Default for LinkProjection {
    fn default() -> Self {
        Self::Resources
    }
}
```

`Link` 增加方法，`collect` 增加去重：

```rust
impl Link {
    pub fn is_resource(&self) -> bool {
        !matches!(self.kind, LinkKind::Html)
    }
}
```

```rust
pub fn collect(markdown: &str, base_url: &Url) -> Vec<Link> {
    let mut links = Vec::new();
    let mut seen = HashSet::new();
    let mut current: Option<(String, String)> = None;

    for event in Parser::new(markdown) {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                current = Some((String::new(), dest_url.into_string()));
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((label, _)) = &mut current {
                    label.push_str(&text);
                }
            }
            Event::End(TagEnd::Link)
                if let Some((text, target)) = current.take()
                    && let Some(url) = resolve(base_url, &target)
                    && seen.insert(url.to_string())
            {
                links.push(Link {
                    text,
                    kind: classify(&url),
                    url: url.to_string(),
                });
            }
            _ => {}
        }
    }

    links
}
```

- [ ] **Step 4: 响应模型增加统计字段并收缩空值**

`src/material/model.rs` 增加统计结构，并把 `stats` 加进响应：

```rust
#[derive(Debug, Serialize)]
pub struct OutputStats {
    pub markdown_chars: usize,
    pub links_included: usize,
    pub links_omitted: usize,
}
```

```rust
pub struct MaterialContentResponse {
    pub target: TargetFacts,
    pub extraction: ExtractionResult,
    pub download: DownloadCapability,
    pub pagination: Pagination,
    pub diagnostics: Diagnostics,
    pub stats: OutputStats,
}
```

`Diagnostics` 的三个可选字段不再序列化 `null`：

```rust
#[derive(Debug, Serialize)]
pub struct Diagnostics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}
```

`src/material/mod.rs` 的导出列表补上 `OutputStats`。

- [ ] **Step 5: 在抽取层执行投影、上限与统计**

`src/material/extraction.rs` 增加常量并把参数接进函数：

```rust
const DEFAULT_MAX_CHARS: usize = 8000;
pub(crate) const MAX_RESPONSE_LINKS: usize = 50;
```

```rust
pub(crate) fn normalize_reader_result_with_options(
    target: TargetFacts,
    markdown: String,
    reader_content_type: String,
    offset: usize,
    max_chars: usize,
    projection: LinkProjection,
) -> MaterialContentResponse {
    let (status, reason) = if let Some(reason) = challenge_reason(&markdown) {
        (MaterialStatus::Blocked, Some(reason.to_owned()))
    } else if markdown.trim().is_empty() {
        (MaterialStatus::Empty, None)
    } else {
        (MaterialStatus::Extracted, None)
    };
    let part = chunk::slice(&markdown, offset, max_chars);
    let title = title(&markdown);
    let candidates = links::collect(&part.markdown, &target.final_url)
        .into_iter()
        .filter(|link| match projection {
            LinkProjection::All => true,
            LinkProjection::Resources => link.is_resource(),
        })
        .collect::<Vec<_>>();
    let links_omitted = candidates.len().saturating_sub(MAX_RESPONSE_LINKS);
    let response_links = candidates
        .into_iter()
        .take(MAX_RESPONSE_LINKS)
        .collect::<Vec<_>>();
    let stats = OutputStats {
        markdown_chars: part.markdown.chars().count(),
        links_included: response_links.len(),
        links_omitted,
    };
    let next_offset = part.next_offset;
    let offset = part.offset;

    MaterialContentResponse {
        target,
        extraction: ExtractionResult {
            status,
            engine: "reader_auto".to_owned(),
            format: "markdown".to_owned(),
            reason,
            reader_content_type: Some(reader_content_type),
            title,
            markdown: part.markdown,
            links: response_links,
        },
        download: DownloadCapability { available: true },
        pagination: Pagination {
            offset,
            next_offset,
            truncated: next_offset.is_some(),
        },
        diagnostics: Diagnostics {
            upstream_status: None,
            duration_ms: None,
            timeout_seconds: None,
            warnings: Vec::new(),
        },
        stats,
    }
}
```

三参数的 `normalize_reader_result` 改为传入 `LinkProjection::default()`。

- [ ] **Step 6: 调整 ContentRequest 与调用方**

`src/content/mod.rs` 的请求结构增加字段，默认值是 `Resources`：

```rust
#[derive(Debug, Deserialize)]
pub struct ContentRequest {
    pub url: String,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_max_chars")]
    pub max_chars: usize,
    #[serde(default)]
    pub links: LinkProjection,
}
```

`src/mcp.rs` 的 `ReadInput` 增加同名参数，并在构造 `ContentRequest` 时传入：

```rust
    /// 链接投影模式：resources 只返回资源型链接，all 返回全部链接。
    #[serde(default)]
    pub links: LinkProjection,
```

- [ ] **Step 7: 填充真实诊断并测量耗时**

`src/reader/client.rs` 的 `ReaderDocument` 增加 `duration_ms: u64`，在 `read_inner` 起始记录时间：

```rust
    let started = std::time::Instant::now();
```

返回时写入：

```rust
    Ok(ReaderDocument {
        final_url: responded_url(&headers).unwrap_or_else(|| url.clone()),
        content_type: headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("text/markdown")
            .to_owned(),
        markdown,
        duration_ms: started.elapsed().as_millis() as u64,
    })
```

`src/application.rs` 的 `read` 在规范化之后补写诊断：

```rust
    let mut response = crate::material::normalize_reader_result_with_options(
        target,
        document.markdown,
        document.content_type,
        request.offset,
        request.max_chars,
        request.links,
    );
    response.diagnostics.duration_ms = Some(document.duration_ms);
    response.diagnostics.timeout_seconds = Some(state.config.reader_timeout.as_secs());
    Ok(response)
```

同一函数中 `download_only` 分支补上零值统计：

```rust
            stats: crate::material::OutputStats {
                markdown_chars: 0,
                links_included: 0,
                links_omitted: 0,
            },
```

`upstream_status` 在本次改动中保持 `None`：非 2xx 的 Reader 响应会走错误分支并携带状态码文本，成功路径没有可记录的上游状态语义。

- [ ] **Step 8: 更新既有模型测试**

`tests/material_model.rs` 的两处 `MaterialContentResponse` 字面量补字段：

```rust
        stats: refinery::material::OutputStats {
            markdown_chars: 0,
            links_included: 0,
            links_omitted: 0,
        },
```

- [ ] **Step 9: 运行测试，确认通过**

```text
cargo test --lib material::extraction
cargo test --all-targets --all-features
```

Expected: PASS，`resources_projection_keeps_only_resources_and_deduplicates` 与 `all_projection_caps_links_and_reports_omitted` 通过。

- [ ] **Step 10: 运行门禁并提交**

```text
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
```

获得提交授权后按 `git-commit` 技能提交上述文件。

---

### Task 4: 就绪探针与 Origin 校验（已完成）

**Files:**
- Modify: `src/config.rs`
- Modify: `src/mcp.rs`
- Modify: `src/routes/health.rs`
- Modify: `src/routes/mod.rs`
- Test: `tests/health_api.rs`

**Interfaces:**
- Produces: `Config.mcp_allowed_origins: Vec<String>`；`GET /ready` 返回 `{ status, checks: { searxng, reader, mcp_host } }`，全部正常为 200，否则 503。
- Consumes: `AppState { config, http_client }`。

- [ ] **Step 1: 写失败测试，固定降级与正常两条路径**

`tests/health_api.rs` 追加。`Config::for_test()` 的白名单只含 `localhost`、`127.0.0.1`、`[::1]`，因此测试通过请求头切换 Host 即可覆盖放行与拒绝两种分支。

```rust
async fn get_ready(app: axum::Router, host: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(
            Request::builder()
                .uri("/ready")
                .header("host", host)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn ready_reports_degraded_when_upstreams_are_unreachable() {
    let app = refinery::routes::router(refinery::state::AppState::for_test());
    let (status, body) = get_ready(app, "localhost").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["status"], "degraded");
    assert_eq!(body["checks"]["searxng"], "unreachable");
    assert_eq!(body["checks"]["reader"], "unreachable");
    assert_eq!(body["checks"]["mcp_host"], "allowed");
}

#[tokio::test]
async fn ready_reports_blocked_host_when_authority_is_not_whitelisted() {
    let app = refinery::routes::router(refinery::state::AppState::for_test());
    let (status, body) = get_ready(app, "search.example.com").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["checks"]["mcp_host"], "blocked");
}
```

正常路径使用本地测试桩，`tests/health_api.rs` 追加：

```rust
async fn stub_upstream() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buffer = [0_u8; 1024];
                let _ = stream.read(&mut buffer).await;
                let _ = stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok",
                    )
                    .await;
            });
        }
    });
    format!("http://{address}")
}

#[tokio::test]
async fn ready_reports_ready_when_upstreams_answer() {
    let base = stub_upstream().await;
    let mut config = refinery::config::Config::for_test();
    config.searxng_base_url = base.clone();
    config.reader_base_url = base;
    let app = refinery::routes::router(refinery::state::AppState::new(config));
    let (status, body) = get_ready(app, "localhost").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ready");
    assert_eq!(body["checks"]["searxng"], "ok");
    assert_eq!(body["checks"]["reader"], "ok");
}
```

- [ ] **Step 2: 运行测试，确认失败**

```text
cargo test --test health_api
```

Expected: FAIL，`/ready` 返回 404。

- [ ] **Step 3: 配置层增加 Origin 白名单**

`src/config.rs` 的 `Config` 增加字段，并在 `from_env` 与 `for_test` 中赋值：

```rust
    pub mcp_allowed_origins: Vec<String>,
```

```rust
            mcp_allowed_origins: env::var("MCP_ALLOWED_ORIGINS")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect(),
```

`for_test()` 使用 `mcp_allowed_origins: Vec::new()`。空列表表示不校验 `Origin`，与 rmcp 的默认行为一致。

`src/config.rs` 的既有测试补充断言与清理：

```rust
            std::env::set_var("MCP_ALLOWED_ORIGINS", "https://search.example.com");
```

```rust
        assert_eq!(
            config.mcp_allowed_origins,
            ["https://search.example.com".to_owned()]
        );
```

- [ ] **Step 4: 接线 Origin 校验**

`src/mcp.rs` 的 `http_service` 在 `with_allowed_hosts` 之后追加：

```rust
    let allowed_origins = state.config.mcp_allowed_origins.clone();
    StreamableHttpService::new(
        move || Ok(RefineryMcp::new(state.clone())),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default()
            .with_allowed_hosts(allowed_hosts)
            .with_allowed_origins(allowed_origins)
            .with_json_response(true),
    )
```

- [ ] **Step 5: 实现 /ready**

`src/routes/health.rs` 增加探针。两个上游并发探测，单个探测 2 秒超时；只要拿到 5xx 以下的响应就视为可达。

```rust
use std::time::Duration;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
};
use serde::Serialize;

use crate::state::AppState;

const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Serialize)]
pub struct ReadyChecks {
    searxng: &'static str,
    reader: &'static str,
    mcp_host: &'static str,
}

#[derive(Serialize)]
pub struct ReadyResponse {
    status: &'static str,
    checks: ReadyChecks,
}

async fn reachable(client: &reqwest::Client, base_url: &str) -> bool {
    let url = format!("{}/", base_url.trim_end_matches('/'));
    match client.get(url).timeout(PROBE_TIMEOUT).send().await {
        Ok(response) => response.status().as_u16() < 500,
        Err(_) => false,
    }
}

fn host_allowed(state: &AppState, headers: &HeaderMap) -> bool {
    if state.config.mcp_allowed_hosts.is_empty() {
        return true;
    }

    let Some(raw) = headers.get(header::HOST).and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let Some((host, port)) = parse_authority(raw) else {
        return false;
    };
    state.config.mcp_allowed_hosts.iter().any(|allowed| {
        parse_authority(allowed).is_some_and(|(allowed_host, allowed_port)| {
            allowed_host == host && (allowed_port.is_none() || allowed_port == port)
        })
    })
}

fn normalize_host(host: &str) -> String {
    host.trim_matches(['[', ']']).to_ascii_lowercase()
}

fn parse_authority(raw: &str) -> Option<(String, Option<u16>)> {
    let authority = axum::http::uri::Authority::try_from(raw.trim()).ok()?;
    Some((normalize_host(authority.host()), authority.port_u16()))
}

pub async fn ready(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> (StatusCode, Json<ReadyResponse>) {
    let (searxng, reader) = tokio::join!(
        reachable(&state.http_client, &state.config.searxng_base_url),
        reachable(&state.http_client, &state.config.reader_base_url),
    );
    let checks = ReadyChecks {
        searxng: if searxng { "ok" } else { "unreachable" },
        reader: if reader { "ok" } else { "unreachable" },
        mcp_host: if host_allowed(&state, &headers) {
            "allowed"
        } else {
            "blocked"
        },
    };
    let ready =
        checks.searxng == "ok" && checks.reader == "ok" && checks.mcp_host == "allowed";
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(ReadyResponse {
            status: if ready { "ready" } else { "degraded" },
            checks,
        }),
    )
}
```

`host_allowed` 按 rmcp 的语义解析 authority：忽略大小写与 IPv6 方括号，允许项不带端口时匹配任意端口，白名单为空表示不校验；探针用于提前发现 09-24 那类域名未加入白名单的问题，不替代 rmcp 的校验。rmcp 若调整该校验语义，探针需要同步更新。

`src/routes/mod.rs` 注册路由：

```rust
    Router::new()
        .route("/health", get(health::health))
        .route("/ready", get(health::ready))
        .nest_service("/mcp", mcp::http_service(state.clone()))
        .with_state(state)
```

- [ ] **Step 6: 运行测试，确认通过**

```text
cargo test --test health_api
```

Expected: PASS，`health_returns_ok` 不回归。

- [ ] **Step 7: 运行门禁并提交**

```text
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

获得提交授权后按 `git-commit` 技能提交 `src/config.rs`、`src/mcp.rs`、`src/routes/health.rs`、`src/routes/mod.rs`、`tests/health_api.rs`。

---

### Task 5: 资源元数据与文档对齐（已完成）

**Files:**
- Modify: `src/mcp.rs`
- Modify: `README.md`
- Modify: `deploy/README.md`
- Modify: `E:\Repositories\agents\refinery-mcp\README.md`

**Interfaces:**
- Produces: `resources/read` 的 `_meta.final_url` 与 `_meta.size_bytes`；`src/mcp.rs` 内部的纯函数 `resource_meta(final_url: &str, size_bytes: usize) -> MetaObject`。

- [ ] **Step 1: 写失败测试，固定元数据内容**

在 `src/mcp.rs` 末尾新增单元测试。这里只测纯函数，完整 `resources/read` 链路在部署机冒烟验证。

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_meta_carries_final_url_and_size() {
        let meta = resource_meta("https://example.com/final.pdf", 2048);
        assert_eq!(
            meta.0["final_url"],
            serde_json::json!("https://example.com/final.pdf")
        );
        assert_eq!(meta.0["size_bytes"], serde_json::json!(2048));
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

```text
cargo test --lib mcp::tests
```

Expected: FAIL 编译错误，`resource_meta` 不存在。

- [ ] **Step 3: 实现元数据并接入 resources/read**

`src/mcp.rs` 增加纯函数，导入 `MetaObject`：

```rust
use rmcp::model::MetaObject;

fn resource_meta(final_url: &str, size_bytes: usize) -> MetaObject {
    let mut meta = serde_json::Map::new();
    meta.insert(
        "final_url".to_owned(),
        serde_json::Value::String(final_url.to_owned()),
    );
    meta.insert(
        "size_bytes".to_owned(),
        serde_json::Value::from(size_bytes),
    );
    MetaObject::from(meta)
}
```

`read_resource` 在编码前记录长度，并把元数据挂到资源内容上：

```rust
        let size_bytes = resource.bytes.len();
        let blob = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(resource.bytes)
        };
        Ok(ReadResourceResponse::Complete(ReadResourceResult::new(
            vec![
                ResourceContents::blob(blob, request.uri)
                    .with_mime_type(resource.content_type)
                    .with_meta(resource_meta(resource.final_url.as_str(), size_bytes)),
            ],
        )))
```

- [ ] **Step 4: 运行测试，确认通过**

```text
cargo test --lib mcp::tests
cargo test --all-targets --all-features
```

Expected: PASS。

- [ ] **Step 5: 对齐 README 与实现**

`README.md` 的四处承诺按当前实现改写，改完后逐条复核。

- 工具参数一节：`web_read` 补 `links` 参数与默认 8000 的说明、新增 `stats` 对象说明；`web_explore` 保留 `limit` 说明，因为实现已真实暴露。
- 错误一节：写明参数非法返回协议错误 `-32602`，上游失败、超时、目标阻断返回 `isError=true` 且携带 `code`、`message`、`stage`、`retryable`。
- 资源一节：保留 `_meta.final_url` 与 `_meta.size_bytes`，因为实现已补齐；同时写明 20 MiB 上限会在后续期复评。
- 安全一节：改为"配置 `MCP_ALLOWED_ORIGINS` 后拒绝不匹配的 `Origin`；未配置时不校验"，不再声称默认拒绝。

`deploy/README.md` 的环境变量清单补充：

```text
MCP_ALLOWED_ORIGINS=https://search.rd.kim
```

并说明未配置时 `Origin` 不校验、`/ready` 会同时检查 Host 白名单与两个上游可达性。

- [ ] **Step 6: 对齐扩展包版本与冒烟步骤**

`E:\Repositories\agents\refinery-mcp\README.md` 中"来源与版本"一节的服务版本改为与 `refinery-app/Cargo.toml` 一致，并增加安装后冒烟检查：

```text
curl -sS https://search.rd.kim/ready
```

期望返回 `status` 为 `ready`。如果无法直连，则在客户端里调用一次 `web_search`，确认返回结果而不是工具缺失。

- [ ] **Step 7: 校对文档与实现一致**

```text
rg -n '0\.3\.0|默认拒绝所有带|_meta\.final_url|_meta\.size_bytes|limit=100|max_chars' README.md deploy\README.md
rg -n '0\.3\.0|0\.2\.0' E:\Repositories\agents\refinery-mcp\README.md
```

Expected: `refinery-app/README.md` 不再出现与实现矛盾的表述；扩展包 README 的版本与 `Cargo.toml` 一致。

- [ ] **Step 8: 运行门禁并提交**

```text
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

获得提交授权后按 `git-commit` 技能分别提交 `refinery-app` 与 `refinery-mcp` 两个仓库的变更。

---

## 验收

- [x] `cargo test --all-targets --all-features` 全部通过。
- [x] `cargo clippy --all-targets --all-features -- -D warnings` 无告警。
- [x] 业务失败返回 `isError=true` 并携带 `code`、`stage`、`retryable`；参数错误仍是 `-32602`。
- [x] `tools/list` 中 `web_read.max_chars` 有 1000–24000 与默认 8000，`web_explore.limit` 真实存在且默认 100。
- [x] `web_read` 默认只返回资源型链接，`stats` 报告正文字符数与链接省略数。
- [x] `/ready` 在宿主机名未加入白名单时报告 `mcp_host: blocked`，在两个上游不可达时报告 `degraded`。
- [ ] `resources/read` 返回 `_meta.final_url` 与 `_meta.size_bytes`。
- [ ] 部署机冒烟：`/ready` 返回 `ready`，并复现一次 `web_search` 与 `web_read`。

未勾选两项的当前证据边界：`resource_meta` 已有单元测试，但完整 `resources/read` 链路需要桩替换 `ResourceFetcher`，而集成测试无法直接使用 `async-trait`，因此该链路留到部署机冒烟验证；部署机项受限于本机没有 Docker。

## 已知边界

本计划不改变抽取质量、不引入缓存、不调整搜索策略，09-24 会话中观察到的导航噪声、重复抓取超时和搜索结果污染仍会存在。未知后缀改为响应裁决、部署自检脚本、镜像固定 digest 与 SearXNG 密钥外置同样不在本次范围内。

`upstream_status` 在本次改动后仍不会出现在成功响应的 `diagnostics` 中；非 2xx 场景通过错误消息携带真实状态码。

本次改动的错误类别覆盖 `invalid_request`、`blocked_target`、`upstream_unavailable`、`upstream_timeout`、`response_too_large`；`busy` 与并发限流尚未实现，`internal` 由 MCP 层在序列化失败时产生。

本机没有 Docker，`/ready` 的真实探针行为与 MCP 客户端行为需要在部署机复核，本地只覆盖测试桩场景。

## Execution Handoff

本计划的执行方式二选一：

1. Subagent-Driven（推荐）：每个任务派发独立子智能体，任务之间做两阶段评审。
2. Inline Execution：在当前会话内按任务顺序执行，批次之间设检查点。
