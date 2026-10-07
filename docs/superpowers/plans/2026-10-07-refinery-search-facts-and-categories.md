# Refinery 搜索事实与类别契约实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 `web_search` 如实报告上游引擎状态，并让调用方能把查询路由到健康类别。

**Architecture:** 搜索域模型（`src/search/mod.rs`）定义状态四态与事实结构并提供纯函数组装；SearXNG 适配层（`src/search/searxng.rs`）只负责解析上游载荷与透传参数；MCP 契约层（`src/mcp.rs`）暴露 `categories` 与 `safesearch` 并把它翻译成请求。判定逻辑集中在域模型，适配层不含业务判断。

**Tech Stack:** Rust 2024、Axum 0.8、Tokio、reqwest、rmcp 3.3.0、schemars 1.2.2。

**Spec:** `docs/superpowers/specs/2026-09-25-refinery-system-design.md`

## Global Constraints

- 默认类别保持 `general`，不擅自为通用查询追加 `it` 或 `science`。
- 不新增 Cargo 依赖。
- 现有四个工具名不变；`web_search` 只新增可选参数，不改变既有参数的语义。
- Rust 改动完成后执行 `cargo fmt --all` 与 `cargo clippy --all-targets --all-features -- -D warnings`。
- 提交前必须完整读取 `git-commit` 技能的 SKILL.md；提交与推送分别授权。
- 引擎权重与类别构成调整不在本计划内，等本计划上线并完成测量后再单独进行。

---

### Task 1: 搜索域模型与状态四态

**Files:**
- Modify: `src/search/mod.rs`
- Test: `src/search/mod.rs`（新增单元测试）

**Interfaces:**

- Produces: `SearchSourceStatus { Ok, Partial, Empty, Failed }`、`EngineUsage { name, results }`、`UnresponsiveEngine { name, reason }`、`SearchResult.engines`、`SearchDiagnostics { source_status, engines, unresponsive_engines, upstream_results, warnings }`、`assemble(request, results, unresponsive, upstream_results) -> SearchResponse`。
- Consumes: 无。

- [ ] **Step 1: 写失败测试，固定状态判定与组装行为**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn result(url: &str, engine: &str) -> SearchResult {
        SearchResult {
            title: "t".to_owned(),
            url: url.to_owned(),
            snippet: "s".to_owned(),
            published_at: None,
            engines: vec![engine.to_owned()],
        }
    }

    fn request(categories: Option<&str>) -> SearchRequest {
        SearchRequest {
            query: "q".to_owned(),
            page: 1,
            limit: 10,
            language: None,
            categories: categories.map(str::to_owned),
            safesearch: None,
        }
    }

    fn unresponsive(name: &str) -> UnresponsiveEngine {
        UnresponsiveEngine {
            name: name.to_owned(),
            reason: "验证码".to_owned(),
        }
    }

    #[test]
    fn status_is_ok_only_without_unresponsive_engines() {
        assert_eq!(derive_status(3, 0), SearchSourceStatus::Ok);
        assert_eq!(derive_status(3, 2), SearchSourceStatus::Partial);
        assert_eq!(derive_status(0, 0), SearchSourceStatus::Empty);
        assert_eq!(derive_status(0, 2), SearchSourceStatus::Failed);
    }

    #[test]
    fn assemble_deduplicates_urls_and_counts_engines() {
        let response = assemble(
            request(None),
            vec![
                result("https://a.example/1", "bing"),
                result("https://a.example/1", "bing"),
                result("https://b.example/2", "wikipedia"),
            ],
            vec![unresponsive("brave")],
            Some(42),
        );
        assert_eq!(response.results.len(), 2);
        assert_eq!(response.diagnostics.source_status, SearchSourceStatus::Partial);
        assert_eq!(response.diagnostics.engines.len(), 2);
        assert_eq!(response.diagnostics.engines[0].results, 1);
        assert_eq!(response.diagnostics.upstream_results, Some(42));
    }

    #[test]
    fn degraded_default_category_emits_warning() {
        let response = assemble(
            request(None),
            vec![result("https://a.example/1", "bing")],
            vec![unresponsive("google")],
            None,
        );
        assert!(response.diagnostics.warnings.contains(&"default_category_degraded"));

        let explicit = assemble(
            request(Some("it")),
            vec![result("https://a.example/1", "docker hub")],
            vec![unresponsive("google")],
            None,
        );
        assert!(explicit.diagnostics.warnings.is_empty());
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

```text
cargo test --lib search::tests
```

Expected: FAIL 编译错误，四态枚举、事实结构与 `assemble` 尚不存在。

- [ ] **Step 3: 实现域模型**

在 `src/search/mod.rs` 增加：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSourceStatus {
    Ok,
    Partial,
    Empty,
    Failed,
}

#[derive(Debug, Serialize)]
pub struct EngineUsage {
    pub name: String,
    pub results: usize,
}

#[derive(Debug, Serialize)]
pub struct UnresponsiveEngine {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct SearchDiagnostics {
    pub source_status: SearchSourceStatus,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub engines: Vec<EngineUsage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unresponsive_engines: Vec<UnresponsiveEngine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_results: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<&'static str>,
}
```

`SearchResult` 增加 `pub engines: Vec<String>`，`pub published_at` 增加 `#[serde(skip_serializing_if = "Option::is_none")]`；`SearchRequest` 增加 `pub categories: Option<String>` 与 `pub safesearch: Option<u8>`；`validate` 增加 `safesearch` 范围校验（0–2）与 `categories` 非空校验。

```rust
pub fn derive_status(results: usize, unresponsive: usize) -> SearchSourceStatus {
    match (results, unresponsive) {
        (0, 0) => SearchSourceStatus::Empty,
        (0, _) => SearchSourceStatus::Failed,
        (_, 0) => SearchSourceStatus::Ok,
        (_, _) => SearchSourceStatus::Partial,
    }
}

pub fn assemble(
    request: &SearchRequest,
    results: Vec<SearchResult>,
    unresponsive_engines: Vec<UnresponsiveEngine>,
    upstream_results: Option<u64>,
) -> SearchResponse {
    let mut seen = std::collections::HashSet::new();
    let mut results = results
        .into_iter()
        .filter(|item| seen.insert(item.url.clone()))
        .take(request.limit.into())
        .collect::<Vec<_>>();
    results.shrink_to_fit();

    let mut counts: Vec<(String, usize)> = Vec::new();
    for item in &results {
        for engine in &item.engines {
            match counts.iter_mut().find(|(name, _)| name == engine) {
                Some((_, count)) => *count += 1,
                None => counts.push((engine.clone(), 1)),
            }
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let status = derive_status(results.len(), unresponsive_engines.len());
    let uses_default_category = request
        .categories
        .as_deref()
        .map(|value| value.split(',').any(|part| part.trim() == "general"))
        .unwrap_or(true);
    let mut warnings = Vec::new();
    if uses_default_category && !unresponsive_engines.is_empty() {
        warnings.push("default_category_degraded");
    }

    SearchResponse {
        query: request.query.clone(),
        page: request.page,
        results,
        pagination: SearchPagination {
            requested_page: request.page,
            has_more: None,
        },
        diagnostics: SearchDiagnostics {
            source_status: status,
            engines: counts
                .into_iter()
                .map(|(name, results)| EngineUsage { name, results })
                .collect(),
            unresponsive_engines,
            upstream_results,
            warnings,
        },
    }
}
```

- [ ] **Step 4: 运行测试，确认通过**

```text
cargo test --lib search::tests
```

Expected: PASS。

---

### Task 2: SearXNG 适配层解析与透传

**Files:**
- Modify: `src/search/searxng.rs`
- Test: `src/search/searxng.rs`（新增单元测试）

**Interfaces:**

- Consumes: `SearchRequest { categories, safesearch, .. }`、`assemble(...)`。
- Produces: `query_params(request) -> Vec<(&'static str, String)>`（纯函数，便于断言透传行为）。

- [ ] **Step 1: 写失败测试，固定解析与透传**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::{SearchRequest, SearchSourceStatus};

    fn request() -> SearchRequest {
        SearchRequest {
            query: "trafilatura".to_owned(),
            page: 2,
            limit: 5,
            language: Some("zh-CN".to_owned()),
            categories: Some("it,science".to_owned()),
            safesearch: Some(1),
        }
    }

    #[test]
    fn query_params_forward_categories_and_safesearch() {
        let params = query_params(&request());
        assert!(params.contains(&("categories", "it,science".to_owned())));
        assert!(params.contains(&("safesearch", "1".to_owned())));
        assert!(params.contains(&("language", "zh-CN".to_owned())));
        assert!(params.contains(&("pageno", "2".to_owned())));
    }

    #[test]
    fn parses_engine_facts_from_upstream_payload() {
        let payload = r#"{
            "results": [
                {"title":"a","url":"https://a.example/1","content":"x","engines":["bing"]},
                {"title":"b","url":"https://b.example/2","content":"y","engines":["wikipedia"]}
            ],
            "unresponsive_engines": [["brave","暂停服务: 请求过于频繁"],["google","暂停服务: 拒绝访问"]],
            "number_of_results": 123
        }"#;
        let parsed: SearxngResponse = serde_json::from_str(payload).unwrap();
        let response = assemble_parsed(&request(), parsed);
        assert_eq!(response.results.len(), 2);
        assert_eq!(response.results[0].engines, ["bing"]);
        assert_eq!(response.diagnostics.unresponsive_engines.len(), 2);
        assert_eq!(response.diagnostics.unresponsive_engines[0].name, "brave");
        assert_eq!(response.diagnostics.source_status, SearchSourceStatus::Partial);
        assert_eq!(response.diagnostics.upstream_results, Some(123));
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

```text
cargo test --lib search::searxng::tests
```

Expected: FAIL 编译错误，`query_params` 与 `assemble_parsed` 不存在。

- [ ] **Step 3: 实现解析与透传**

`SearxngResponse` 扩展为：

```rust
#[derive(Deserialize)]
struct SearxngResponse {
    #[serde(default)]
    results: Vec<SearxngResult>,
    #[serde(default)]
    unresponsive_engines: Vec<Vec<String>>,
    number_of_results: Option<u64>,
}

#[derive(Deserialize)]
struct SearxngResult {
    title: String,
    url: String,
    #[serde(default)]
    content: String,
    #[serde(rename = "publishedDate")]
    published_date: Option<String>,
    #[serde(default)]
    engines: Vec<String>,
}
```

新增纯函数与组装函数：

```rust
pub(crate) fn query_params(request: &SearchRequest) -> Vec<(&'static str, String)> {
    let mut params = vec![
        ("q", request.query.clone()),
        ("format", "json".to_owned()),
        ("pageno", request.page.to_string()),
    ];
    if let Some(language) = &request.language {
        params.push(("language", language.clone()));
    }
    if let Some(categories) = &request.categories {
        params.push(("categories", categories.clone()));
    }
    if let Some(safesearch) = request.safesearch {
        params.push(("safesearch", safesearch.to_string()));
    }
    params
}

pub(crate) fn assemble_parsed(request: &SearchRequest, upstream: SearxngResponse) -> SearchResponse {
    let unresponsive_engines = upstream
        .unresponsive_engines
        .into_iter()
        .map(|entry| UnresponsiveEngine {
            name: entry.first().cloned().unwrap_or_default(),
            reason: entry.get(1).cloned().unwrap_or_default(),
        })
        .collect();
    let results = upstream
        .results
        .into_iter()
        .map(|result| SearchResult {
            title: result.title,
            url: result.url,
            snippet: result.content,
            published_at: result.published_date,
            engines: result.engines,
        })
        .collect();
    crate::search::assemble(request, results, unresponsive_engines, upstream.number_of_results)
}
```

`search_inner` 改为使用 `query_params` 与 `assemble_parsed`，删掉原先手写的参数数组与结果映射。

- [ ] **Step 4: 运行测试，确认通过**

```text
cargo test --lib search
```

Expected: PASS。

---

### Task 3: MCP 契约暴露 categories 与 safesearch

**Files:**
- Modify: `src/mcp.rs`
- Test: `tests/mcp_api.rs`

**Interfaces:**

- Produces: `SearchInput.categories: Option<String>`、`SearchInput.safesearch: Option<u8>`（schema 范围 0–2，默认 1）。
- Consumes: `SearchRequest { categories, safesearch }`、`SearchResponse`。

- [ ] **Step 1: 写失败测试，固定 schema 与透传**

在 `tests/mcp_api.rs` 追加：

```rust
async fn stub_searxng_capture() -> (String, std::sync::Arc<std::sync::Mutex<String>>) {
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
                let body = r#"{"results":[{"title":"a","url":"https://a.example/1","content":"x","engines":["bing"]}],"unresponsive_engines":[["google","拒绝访问"]],"number_of_results":7}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
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
    let (base, captured) = stub_searxng_capture().await;
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
    assert_eq!(document["diagnostics"]["unresponsive_engines"][0]["name"], "google");
    assert_eq!(document["diagnostics"]["engines"][0]["name"], "bing");
    assert_eq!(document["diagnostics"]["upstream_results"], 7);
    let request_line = captured.lock().unwrap().clone();
    assert!(request_line.contains("categories=it"), "{request_line}");
    assert!(request_line.contains("safesearch=1"), "{request_line}");
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
    let search = tools.iter().find(|tool| tool["name"] == "web_search").unwrap();
    assert_eq!(search["inputSchema"]["properties"]["safesearch"]["minimum"], 0);
    assert_eq!(search["inputSchema"]["properties"]["safesearch"]["maximum"], 2);
    assert_eq!(search["inputSchema"]["properties"]["safesearch"]["default"], 1);
    assert!(search["inputSchema"]["properties"]["categories"].is_object());
}
```

- [ ] **Step 2: 运行测试，确认失败**

```text
cargo test --test mcp_api web_search_forwards_categories_and_reports_engine_facts
cargo test --test mcp_api search_schema_declares_categories_and_safesearch
```

Expected: 第一个失败于透传与事实字段缺失，第二个失败于 schema 没有 `safesearch`。

- [ ] **Step 3: 扩展 MCP 输入与接线**

`SearchInput` 增加两个字段，并更新工具描述：

```rust
    /// SearXNG 类别，逗号分隔，例如 it 或 general,it；默认 general。
    pub categories: Option<String>,
    /// 安全搜索级别：0 关闭、1 中等、2 严格；默认 1。
    #[serde(default = "default_safesearch")]
    #[schemars(range(min = 0, max = 2))]
    pub safesearch: u8,
```

```rust
fn default_safesearch() -> u8 {
    1
}
```

工具描述改为：

```rust
    #[tool(
        name = "web_search",
        description = "Search public web pages. Pass categories (for example it or science) when diagnostics reports the default category as degraded."
    )]
```

请求构造传入新字段：

```rust
        let request = crate::search::SearchRequest {
            query: input.query,
            page: input.page,
            limit: input.limit,
            language: input.language,
            categories: input.categories,
            safesearch: Some(input.safesearch),
        };
```

- [ ] **Step 4: 运行测试，确认通过**

```text
cargo test --test mcp_api
cargo test --all-targets --all-features
```

Expected: PASS，且既有 MCP 测试不回归。

---

### Task 4: 文档与工具说明对齐

**Files:**
- Modify: `README.md`

**Interfaces:** 无代码接口。

- [ ] **Step 1: 更新 README 的 `web_search` 说明**

写明三个事实：`categories` 为可选参数、默认类别是 `general`；`source_status` 取 `ok`/`partial`/`empty`/`failed` 四态；`diagnostics` 里的 `engines`、`unresponsive_engines`、`upstream_results` 与 `warnings` 的含义，以及 `default_category_degraded` 时技术查询应显式传 `categories`。

- [ ] **Step 2: 校对文档与实现一致**

```text
rg -n 'source_status|categories|safesearch|unresponsive' README.md
```

Expected: 文档描述与 `src/search/mod.rs`、`src/mcp.rs` 的实际字段一致。

---

### Task 5: 部署后线上验收

**Files:** 无仓库改动。

- [ ] **Step 1: 部署新版本后确认事实可见**

用 MCP 调用 `web_search`，分别不带 `categories` 与带 `categories: "it"`，确认前者 `source_status` 为 `partial` 且带 `default_category_degraded`，后者 `unresponsive_engines` 为空、结果里出现 `github`/`pypi` 等来源引擎。

- [ ] **Step 2: 按结果决定是否进入权重调优**

若 `it` 类别仍被单一引擎（例如 docker hub）刷屏，则用同一批查询测量各引擎贡献，再单独提出权重调整方案；不在本计划内直接修改 `settings.yml`。

## 验收

- [ ] `cargo test --all-targets --all-features` 全部通过。
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` 无告警。
- [ ] `web_search` 返回 `source_status`、`engines`、`unresponsive_engines`、`upstream_results` 与 `warnings`。
- [ ] `categories` 与 `safesearch` 真实透传到 SearXNG，`safesearch` schema 范围为 0–2 且默认 1。
- [ ] 未指定 `categories` 且上游有引擎失败时出现 `default_category_degraded`；显式指定类别时不出现。

## 已知边界

本计划不修改 `deploy/app/settings.yml` 的引擎集合与权重，不解决 5 个通用引擎被反爬拦截的运维问题；服务只保证如实报告并让调用方能够换类别。

本机没有 Docker，SearXNG 的真实行为只能在部署机验证；本地用测试桩覆盖解析、透传与状态判定。

## 评测基线

后续任何搜索相关的改动（权重、引擎集合、路由映射）都要先跑这套固定查询，再决定是否保留。判定标准是"期望来源类型出现在前三"，不是凭印象看结果像不像。

实体查询（`scope=code`）：

| 查询 | 期望前三出现的来源 |
|---|---|
| `trafilatura` | github.com 上的 adbar/trafilatura |
| `python requests` | github.com 上的 psf/requests |
| `postgres index` | Postgres 索引相关项目或官方文档 |

排障查询（`scope=qa`）：

| 查询 | 期望前三出现的来源 |
|---|---|
| `postgres index not used` | stackoverflow、superuser 或 askubuntu |
| `rust tokio timeout example` | stackoverflow 或 tokio 官方文档 |
| `docker compose healthcheck retries` | docs.docker.com 或 stackoverflow |

包查询（`scope=package`）与学术查询（`scope=paper`）：

| 查询 | 期望前三出现的来源 |
|---|---|
| `trafilatura` | pypi.org、crates.io 或 npm |
| `retrieval augmented generation evaluation` | arxiv、semantic scholar 或 crossref |

通用查询（默认 `scope=web`）用于观察退化而不是判定相关：`上海天气` 不应被 stackoverflow 或 mdn 占据；当 `diagnostics.warnings` 出现 `default_category_degraded` 时记录当时可用的通用引擎。

每次测量记录每条查询的前三条来源引擎与域名。发布门槛：实体组与排障组的命中率不得低于上一版。
