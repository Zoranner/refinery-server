# 资料访问统一架构实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Refinery 的搜索、获取、抽取和下载结果统一为可供智能体稳定消费的资料访问模型。

**Architecture:** 先建立独立的目标事实、抽取结果、下载能力和诊断模型，再由编排层组合内容响应；Reader 继续负责 `auto` 的 Curl/Browser 判断，Refinery 只负责策略、预算、结果判定和契约投影。搜索分页、反爬、空正文、下载可用性和框架错误都通过统一状态模型表达。

**Tech Stack:** Rust 2024、Axum、Reqwest、Serde/serde_json、Tokio、Jina Reader、SearXNG、Docker Compose、现有集成测试。

**Spec:** `docs/superpowers/specs/2026-09-04-refinery-material-access-architecture-design.md`

## Global Constraints

- 服务名称固定为 `refinery`，内网调用不做用户认证。
- `refinery`、SearXNG 和 Reader 继续作为独立服务运行；只有 `refinery` 发布宿主机端口。
- 首版不增加数据库、持久化缓存、任务队列、站点专用适配器或第二套网页抽取器。
- Reader 的 `x-engine: auto`、`x-respond-timing: visible-content` 由 Refinery 固定注入；`x-timeout` 不超过官方 180 秒。
- `resource_kind` 只表示目标资源类别；Reader 的响应 `content_type` 不得覆盖目标事实。
- 所有错误响应统一为 JSON，不泄露内部地址、Cookie、认证信息、完整上游响应或调用栈。
- SSRF、DNS、重定向、响应大小和 Reader 容器出站网络限制继续有效。
- 每个任务先写失败测试，确认失败原因后再实现；每个任务完成后独立验证并提交。
- 版本号和发布动作不在本计划中预先修改；实现完成并通过线上验收后另行决定发布版本。

---

### Task 1: 建立资料访问内部模型

**Files:**

- Create: `src/material/mod.rs`
- Create: `src/material/model.rs`
- Modify: `src/lib.rs`
- Test: `tests/material_model.rs`

**Interfaces:**

- Consumes: 当前 `content::ResourceKind`、Reader 响应元数据、资源下载能力。
- Produces: `TargetFacts`、`ExtractionResult`、`DownloadCapability`、`Diagnostics`、`MaterialStatus`，供后续路由和适配层使用。

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn material_model_keeps_target_kind_separate_from_extraction_media_type() {
    let target = refinery::material::TargetFacts::new(
        "https://example.test/report.pdf".parse().unwrap(),
        "https://example.test/report.pdf".parse().unwrap(),
        refinery::content::ResourceKind::Pdf,
        "text/plain; charset=utf-8".to_owned(),
    );
    let extraction = refinery::material::ExtractionResult::blocked(
        "reader_auto",
        "目标站点返回挑战页".to_owned(),
    );

    assert_eq!(target.resource_kind, refinery::content::ResourceKind::Pdf);
    assert_eq!(target.content_type, "text/plain; charset=utf-8");
    assert_eq!(extraction.status, refinery::material::MaterialStatus::Blocked);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test material_model`

Expected: FAIL because the `material` module and model types do not exist.

- [ ] **Step 3: Write minimal implementation**

Create `src/material/model.rs` with:

```rust
pub struct TargetFacts {
    pub requested_url: Url,
    pub final_url: Url,
    pub resource_kind: ResourceKind,
    pub content_type: String,
}

pub enum MaterialStatus {
    Extracted,
    Empty,
    Blocked,
    Failed,
    TimedOut,
    DownloadOnly,
}

pub struct ExtractionResult {
    pub status: MaterialStatus,
    pub engine: String,
    pub format: String,
    pub reason: Option<String>,
}

pub struct DownloadCapability {
    pub available: bool,
    pub resource_url: Option<String>,
}

pub struct Diagnostics {
    pub upstream_status: Option<u16>,
    pub duration_ms: Option<u64>,
    pub timeout_seconds: Option<u64>,
    pub warnings: Vec<String>,
}

pub struct Pagination {
    pub offset: usize,
    pub next_offset: Option<usize>,
    pub truncated: bool,
}

pub struct MaterialContentResponse {
    pub target: TargetFacts,
    pub extraction: ExtractionResult,
    pub download: DownloadCapability,
    pub pagination: Pagination,
    pub diagnostics: Diagnostics,
}
```

Add constructors that make invalid combinations difficult to create, including `blocked` and `download_only`. Export the module from `src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test material_model`

Expected: PASS.

- [ ] **Step 5: Commit**

```text
git add src/material src/lib.rs tests/material_model.rs
git commit -m "建立资料访问内部模型"
```

### Task 2: 统一内容获取与抽取判定

**Files:**

- Create: `src/material/extraction.rs`
- Modify: `src/reader/client.rs`
- Modify: `src/routes/content.rs`
- Modify: `src/content/mod.rs`
- Test: `tests/content_api.rs`

**Interfaces:**

- Consumes: `TargetFacts`、`ExtractionResult`、现有 Reader HTTP 客户端。
- Produces: `MaterialContentResponse`，明确区分目标类型、正文状态、下载能力和诊断信息。

- [ ] **Step 1: Write the failing tests**

Add tests for three distinct outcomes:

```rust
#[tokio::test]
async fn content_marks_reader_challenge_as_blocked_instead_of_success() {
    let response = post_content_with_reader_body(
        "https://example.test/report.pdf",
        "Title: Just a moment...\n\nTarget URL returned error 403: Forbidden",
        "text/plain",
    )
    .await;

    assert_eq!(response.status(), 200);
    let body = json_body(response).await;
    assert_eq!(body["target"]["resource_kind"], "pdf");
    assert_eq!(body["extraction"]["status"], "blocked");
    assert_eq!(body["download"]["available"], true);
}
```

```rust
#[tokio::test]
async fn content_reports_download_only_without_calling_reader() {
    let response = post_content("https://example.test/file.bin").await;

    assert_eq!(response.status(), 200);
    let body = json_body(response).await;
    assert_eq!(body["extraction"]["status"], "download_only");
    assert_eq!(body["download"]["available"], true);
}
```

`download_only` is a normal `200` material response in the new contract. The old `415 resource_download_required` response is removed with the flat response shape; callers use `extraction.status` and `download.resource_url` instead.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test content_api`

Expected: FAIL because the current flat response has no `target`, `extraction`, `download`, or `diagnostics` objects and treats Reader challenge text as ordinary Markdown.

- [ ] **Step 3: Write minimal implementation**

Implement one normalization function in `src/material/extraction.rs`:

```rust
pub fn normalize_reader_result(
    target: TargetFacts,
    markdown: String,
    reader_content_type: String,
) -> MaterialContentResponse
```

The function must:

- preserve `target.resource_kind`;
- classify known Reader challenge markers and upstream 403 text as `Blocked`;
- classify empty or whitespace-only Markdown as `Empty`;
- classify ordinary Reader Markdown as `Extracted`;
- expose `/v1/resource` capability without downloading the resource a second time;
- keep `markdown`, absolute links, and pagination under `extraction`;
- retain the existing `max_chars` and `offset` limits.

The Reader client must continue sending `x-engine: auto`, `x-respond-timing: visible-content`, `x-respond-with: markdown`, `x-retain-links: all`, and a capped `x-timeout`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test content_api`

Expected: PASS, including static HTML, dynamic HTML, text, PDF, challenge-page, image and unknown-resource cases.

- [ ] **Step 5: Commit**

```text
git add src/material/extraction.rs src/reader/client.rs src/routes/content.rs src/content/mod.rs tests/content_api.rs
git commit -m "统一内容获取与抽取结果"
```

### Task 3: 统一资源下载结果与分层预算

**Files:**

- Create: `src/material/download.rs`
- Modify: `src/resource/mod.rs`
- Modify: `src/routes/resource.rs`
- Modify: `src/config.rs`
- Modify: `deploy/.env.example`
- Test: `tests/resource_api.rs`

**Interfaces:**

- Consumes: `TargetFacts`、资源下载器和运行时预算配置。
- Produces: 统一的下载能力对象，以及可区分资源连接超时、Reader 抽取超时和 Refinery 总预算超时的诊断信息。

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn resource_response_reports_original_media_type_and_download_capability() {
    let response = get_resource("https://example.test/file.bin").await;

    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-disposition"], "attachment");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
}
```

Add a timeout test using a fixture fetcher that stalls after connection and assert the normalized diagnostic stage is `resource_fetch`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test resource_api`

Expected: FAIL because the current resource route returns bytes and headers but has no shared download capability or stage diagnostics.

- [ ] **Step 3: Write minimal implementation**

Add explicit budget fields to `Config`:

```rust
pub resource_timeout: Duration,
pub reader_timeout: Duration,
```

Keep the current 20 MiB resource limit and public-address checks. Return the original upstream `Content-Type`, and create a `DownloadCapability` for content responses without performing a duplicate request. Keep resource bytes as a binary response; only JSON metadata uses the material model.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test resource_api`

Expected: PASS, with resource and Reader budgets independently asserted.

- [ ] **Step 5: Commit**

```text
git add src/material/download.rs src/resource src/routes/resource.rs src/config.rs deploy/.env.example tests/resource_api.rs
git commit -m "分离资源下载与抽取预算"
```

### Task 4: 增加统一 JSON 错误边界

**Files:**

- Create: `src/routes/rejection.rs`
- Modify: `src/routes/mod.rs`
- Modify: `src/error.rs`
- Test: `tests/error_api.rs`

**Interfaces:**

- Consumes: Axum JSON extractor rejections and existing `ApiError`.
- Produces: 所有业务和框架输入错误均为统一 JSON `ErrorResponse`。

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn malformed_json_returns_machine_readable_error() {
    let response = post_raw("/v1/sitemap", "{").await;

    assert_eq!(response.status(), 400);
    assert_eq!(response.headers()["content-type"], "application/json");
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "invalid_request");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test error_api`

Expected: FAIL because Axum currently emits a framework-generated `422 text/plain` response.

- [ ] **Step 3: Write minimal implementation**

Configure `JsonRejection` handling at the route layer. Map malformed JSON and missing required fields to `ApiError::invalid_request`, preserve existing business errors, and ensure `Content-Type: application/json`.

- [ ] **Step 4: Run tests to verify it passes**

Run: `cargo test --test error_api`

Expected: PASS for malformed JSON, missing fields, invalid URL, blocked target and upstream errors.

- [ ] **Step 5: Commit**

```text
git add src/routes/rejection.rs src/routes/mod.rs src/error.rs tests/error_api.rs
git commit -m "统一接口错误响应格式"
```

### Task 5: 明确搜索分页和来源状态

**Files:**

- Modify: `src/search/mod.rs`
- Modify: `src/search/client.rs`
- Modify: `src/routes/search.rs`
- Modify: `src/routes/openapi.rs`
- Test: `tests/search_api.rs`

**Interfaces:**

- Consumes: SearXNG JSON 响应和请求页码。
- Produces: 带 `pagination.has_more`、`pagination.requested_page` 和 `source_status` 的搜索响应。

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn empty_second_page_does_not_claim_end_of_results() {
    let response = post_search(json!({
        "query": "web scraping",
        "page": 2,
        "limit": 3
    }))
    .await;

    assert_eq!(response.status(), 200);
    let body = json_body(response).await;
    assert_eq!(body["results"].as_array().unwrap().len(), 0);
    assert_eq!(body["pagination"]["has_more"], serde_json::Value::Null);
    assert_eq!(body["pagination"]["source_status"], "ok");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test search_api`

Expected: FAIL because the current response has no pagination state.

- [ ] **Step 3: Write minimal implementation**

Preserve the existing `results` array and add:

```rust
pub struct SearchPagination {
    pub requested_page: usize,
    pub has_more: Option<bool>,
}

pub struct SearchDiagnostics {
    pub source_status: String,
    pub warnings: Vec<String>,
}
```

Set `has_more` to `None` unless SearXNG provides a trustworthy signal. Do not infer “no more results” solely from an empty page.

- [ ] **Step 4: Run tests to verify it passes**

Run: `cargo test --test search_api`

Expected: PASS for valid queries, empty pages, invalid limits and upstream failures.

- [ ] **Step 5: Commit**

```text
git add src/search src/routes/search.rs src/routes/openapi.rs tests/search_api.rs
git commit -m "明确搜索分页状态"
```

### Task 6: 让 OpenAPI、文档和线上契约一致

**Files:**

- Modify: `src/routes/openapi.rs`
- Modify: `README.md`
- Modify: `deploy/README.md`
- Modify: `docs/superpowers/specs/2026-09-01-internal-web-search-api-design.md`
- Test: `tests/openapi_api.rs`

**Interfaces:**

- Consumes: 前五项产生的统一响应类型和错误状态。
- Produces: 与实际 JSON 完全一致的 OpenAPI 3.0.3 文档和智能体使用说明。

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn openapi_describes_material_response_and_error_states() {
    let document = openapi_document().await;

    assert!(document["components"]["schemas"]["TargetFacts"].is_object());
    assert!(document["components"]["schemas"]["ExtractionResult"].is_object());
    assert!(document["components"]["schemas"]["DownloadCapability"].is_object());
    assert_eq!(
        document["components"]["schemas"]["MaterialStatus"]["enum"][2],
        "blocked"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test openapi_api`

Expected: FAIL because the current OpenAPI document describes the old flat response.

- [ ] **Step 3: Write minimal implementation**

Regenerate the static JSON document from the final public response shape. Document:

- `target.resource_kind`;
- `extraction.status`, `engine`, `format`, `markdown`, `links`;
- `download.available` and `resource_url`;
- pagination and diagnostics;
- JSON errors for all documented status codes.

Update README and design documentation to remove contradictory `content_kind`/`resource_kind` descriptions and old timeout claims.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test openapi_api`

Expected: PASS, with every public response field represented exactly once.

- [ ] **Step 5: Commit**

```text
git add src/routes/openapi.rs README.md deploy/README.md docs/superpowers/specs/2026-09-01-internal-web-search-api-design.md tests/openapi_api.rs
git commit -m "同步智能体接口契约"
```

### Task 7: 全链路回归与目标服务器验收

**Files:**

- Modify: `docs/superpowers/specs/2026-09-04-refinery-material-access-architecture-design.md`
- Create: `docs/reviews/2026-09-04-refinery-material-access-acceptance.md`
- Test: `tests/content_api.rs`, `tests/resource_api.rs`, `tests/search_api.rs`, `tests/sitemap_api.rs`, `tests/openapi_api.rs`

**Interfaces:**

- Consumes: 前六项的统一接口。
- Produces: 可审查的本地验证记录和目标服务器线上验收记录。

- [ ] **Step 1: Run local verification**

Run:

```text
cargo fmt --all
cargo test --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release --locked
git diff --check
```

Expected: 全部通过；任何未覆盖的真实 Reader、Docker 或公网行为必须单独列为验证缺口。

- [ ] **Step 2: Run target-server scenarios**

Using only Refinery HTTP endpoints, record:

- static HTML and dynamic HTML;
- plain text and PDF, including one blocked/challenge PDF;
- image and unknown extension download-only flow;
- resource download headers and large/slow resource behavior;
- search page 1, empty page 2, invalid limit;
- sitemap discovery and fallback;
- malformed JSON and blocked target errors;
- `/openapi.json` against every observed response.

- [ ] **Step 3: Write acceptance record**

The acceptance document must record exact date, URL, request shape, status, elapsed time, response state and remaining boundary. It must not convert a successful HTTP status into a claim that content quality or search relevance is guaranteed.

- [ ] **Step 4: Commit**

```text
git add docs/superpowers/specs/2026-09-04-refinery-material-access-architecture-design.md docs/reviews/2026-09-04-refinery-material-access-acceptance.md tests
git commit -m "记录资料访问链路验收"
```

## Plan Self-Review

- Spec coverage: target facts, extraction states, download capability, diagnostics, search pagination, timeout budgets, unified errors, migration and acceptance are covered by Tasks 1–7.
- Placeholder scan: no `TODO`, `TBD`, “implement later” or unspecified function references remain.
- Type consistency: `TargetFacts`, `ExtractionResult`, `DownloadCapability`, `Diagnostics` and `MaterialStatus` are introduced in Task 1 and consumed by later tasks; route and OpenAPI changes are deferred until the model exists.
- Scope check: no database, cache, queue, second extractor, site-specific adapter or release action was added.
