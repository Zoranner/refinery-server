# 站点发现结果与错误传播实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 sitemap 的来源失败、回退过程和发现结果统一为可观察、可解释且与 OpenAPI 一致的状态模型。

**Architecture:** Sitemap 继续采用无任务、无缓存的尽力发现模式，但不再静默吞掉来源错误；每个来源产生明确的尝试结果，编排层聚合为 `discovered`、`partial`、`empty`、`failed` 或 `timed_out` 状态。旧的内容下载 415 辅助错误路径同时删除，避免保留与新 `download_only` 模型冲突的双轨。

**Tech Stack:** Rust 2024、Axum、Reqwest、Serde/serde_json、Tokio、现有 sitemap parser、OpenAPI JSON、Rust 集成测试。

**Spec:** `docs/superpowers/specs/2026-09-04-refinery-material-access-architecture-design.md`

## Global Constraints

- 服务名称固定为 `refinery`，内网调用不做用户认证。
- 站点发现不增加数据库、缓存、任务队列、深度爬取或第二套抓取器。
- `robots.txt`、sitemap、sitemap index 和页面回退继续使用受限公网 URL、DNS、重定向、超时和大小策略。
- `/v1/sitemap` 不声明当前实现不会发出的 HTTP 504；错误必须在响应状态或机器可读 warning 中可观察。
- `resource_kind`、`extraction.status`、`download` 和诊断字段保持唯一事实来源。
- 所有框架和业务错误继续使用统一 JSON 错误结构。
- 保留现有 HTTP 二进制资源下载行为，不让 sitemap 或错误清理改变资源接口。
- 每个任务先写失败测试，确认失败原因后再实现；每个任务独立提交并审查。
- 不推送、不打 tag、不发布镜像；版本迁移在全部调用方确认后单独执行。

---

### Task 1: 建立 sitemap 来源结果与聚合状态

**Files:**

- Create: `src/sitemap/outcome.rs`
- Modify: `src/sitemap/mod.rs`
- Modify: `src/routes/sitemap.rs`
- Test: `tests/sitemap_api.rs`

**Interfaces:**

- Consumes: `SitemapFetcher::get`, sitemap parser、Reader 页面回退结果。
- Produces: `SitemapResponse` 中的 `status`、`sources`、结构化 `warnings`，并保留现有 `requested_url`、`site_url`、`urls`、`truncated`。

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn sitemap_exposes_source_failures_without_hiding_discovery_state() {
    let response = post_sitemap(
        app_with_fetcher(FailingFetcher::timeout_for_all()),
        json!({
            "url": "https://docs.example.test/guide/",
            "limit": 20
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "timed_out");
    assert_eq!(body["urls"], json!([]));
    assert_eq!(body["sources"]["robots_txt"], "timed_out");
    assert_eq!(body["sources"]["sitemap"], "timed_out");
    assert_eq!(body["sources"]["page_links"], "timed_out");
    assert_eq!(body["warnings"][0]["code"], "source_timeout");
}
```

```rust
#[tokio::test]
async fn sitemap_marks_partial_when_sitemap_urls_exist_but_one_source_fails() {
    let response = post_sitemap(app_with_partial_fixture(), json!({
        "url": "https://docs.example.test/guide/",
        "limit": 20
    })).await;

    let body = json_body(response).await;
    assert_eq!(body["status"], "partial");
    assert_eq!(body["sources"]["sitemap"], "discovered");
    assert_eq!(body["sources"]["robots_txt"], "failed");
    assert_eq!(body["warnings"][0]["code"], "source_failed");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test sitemap_api`

Expected: FAIL because `SitemapResponse` has no status/source model and source errors are discarded.

- [ ] **Step 3: Write minimal implementation**

Create `src/sitemap/outcome.rs`:

```rust
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SitemapStatus {
    Discovered,
    Partial,
    Empty,
    Failed,
    TimedOut,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SitemapSourceStatus {
    NotAttempted,
    Discovered,
    Empty,
    Failed,
    TimedOut,
}

#[derive(Debug, Serialize)]
pub struct SitemapSources {
    pub robots_txt: SitemapSourceStatus,
    pub sitemap: SitemapSourceStatus,
    pub page_links: SitemapSourceStatus,
}

#[derive(Debug, Serialize)]
pub struct SitemapWarning {
    pub source: &'static str,
    pub code: &'static str,
}
```

Change `SitemapResponse` to include `status`, `sources`, and `Vec<SitemapWarning>`. Keep HTTP 200 for discovery responses, including all-source failure, because this endpoint is explicitly best effort; use `status` and warnings to make failure observable. Preserve validation errors as JSON 400/403.

`discover` must record each attempted source. A robots failure still permits sitemap fallback; a sitemap failure still permits page-link fallback. Do not treat an empty page as proof of “no sitemap” without recording the attempted sources.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test sitemap_api`

Expected: PASS for sitemap index, empty sites, page-link fallback, partial source failure and timeout aggregation.

- [ ] **Step 5: Commit**

```text
git add src/sitemap/outcome.rs src/sitemap/mod.rs src/routes/sitemap.rs tests/sitemap_api.rs
git commit -m "建立站点发现结果状态"
```

### Task 2: 删除旧内容下载错误双轨

**Files:**

- Modify: `src/error.rs`
- Modify: `tests/error_api.rs`
- Modify: `src/routes/openapi.rs`

**Interfaces:**

- Consumes: 当前统一 `MaterialStatus::DownloadOnly` 和 `/v1/content` 200 响应。
- Produces: 不再存在无调用方的 `resource_download_required`/`unsupported_media_type` 辅助路径，OpenAPI 只描述实际错误。

- [ ] **Step 1: Write the failing structural test**

```rust
#[test]
fn legacy_download_only_error_helpers_are_not_exported() {
    let source = std::fs::read_to_string("src/error.rs").unwrap();
    assert!(!source.contains("resource_download_required"));
    assert!(!source.contains("unsupported_media_type"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test error_api`

Expected: FAIL because the legacy helper and test still exist.

- [ ] **Step 3: Write minimal implementation**

Remove unused constructors and the test that directly serializes the old 415 shape. Remove any OpenAPI response component that is no longer referenced. Do not remove `ApiError::invalid_request`, `blocked_target`, `fetch_failed`, `fetch_timeout` or `response_too_large`.

- [ ] **Step 4: Run tests to verify it passes**

Run: `cargo test --test error_api --test openapi_api`

Expected: PASS, with no legacy helper references in `src`, `tests` or OpenAPI.

- [ ] **Step 5: Commit**

```text
git add src/error.rs tests/error_api.rs src/routes/openapi.rs
git commit -m "删除旧下载错误双轨"
```

### Task 3: 收口测试和文档事实

**Files:**

- Modify: `tests/error_api.rs`
- Modify: `tests/material_model.rs`
- Modify: `docs/superpowers/plans/2026-09-04-refinery-material-access-architecture.md`
- Modify: `README.md`
- Modify: `deploy/README.md`

**Interfaces:**

- Consumes: Task 1 的 sitemap 状态字段和 Task 2 的错误模型。
- Produces: 稳定测试、无字段示例漂移的文档，以及完整环境变量说明。

- [ ] **Step 1: Write the failing tests**

Replace the upstream error fixture that depends on `searxng.test` DNS with a local server returning HTTP 500, then assert the same `search_upstream_failed` JSON.

Update the material model test:

```rust
assert_eq!(target.content_type, "application/pdf");
assert_eq!(
    json["extraction"]["reader_content_type"],
    "text/plain; charset=utf-8"
);
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test error_api --test material_model`

Expected: FAIL until fixtures and assertions are migrated.

- [ ] **Step 3: Write minimal implementation**

Update:

- plan Task 5 example to use `diagnostics.source_status`;
- plan Task 1 example to use `application/pdf` for the PDF target and keep Reader media type separate;
- README/deploy README to list `RESOURCE_REQUEST_TIMEOUT_SECONDS`;
- README and sitemap docs to describe `status`, `sources`, structured warnings, and best-effort HTTP 200 semantics.

Do not claim sitemap emits 504 or 413 while its current implementation aggregates those failures into the response.

- [ ] **Step 4: Run tests to verify it passes**

Run: `cargo test --test error_api --test material_model --test sitemap_api`

Expected: PASS and no stale field names in the edited documents.

- [ ] **Step 5: Commit**

```text
git add tests/error_api.rs tests/material_model.rs docs/superpowers/plans/2026-09-04-refinery-material-access-architecture.md README.md deploy/README.md
git commit -m "收口资料访问测试与文档事实"
```

### Task 4: 完整契约验证和验收记录

**Files:**

- Modify: `src/routes/openapi.rs`
- Modify: `tests/openapi_api.rs`
- Modify: `docs/superpowers/specs/2026-09-04-refinery-material-access-architecture-design.md`
- Create: `docs/reviews/2026-09-05-refinery-sitemap-outcome-acceptance.md`

**Interfaces:**

- Consumes: Task 1–3 的最终响应和错误模型。
- Produces: 与实际代码一致的 OpenAPI、架构说明和本轮验证记录。

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn openapi_describes_sitemap_status_sources_and_warnings() {
    let document = openapi_document().await;

    assert!(document["components"]["schemas"]["SitemapStatus"].is_object());
    assert!(document["components"]["schemas"]["SitemapSources"].is_object());
    assert!(document["components"]["schemas"]["SitemapWarning"].is_object());
    assert_eq!(
        document["paths"]["/v1/sitemap"]["post"]["responses"]["200"]
            ["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/SitemapResponse"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test openapi_api`

Expected: FAIL because the current OpenAPI lacks the sitemap outcome schemas.

- [ ] **Step 3: Write minimal implementation**

Update the OpenAPI JSON and architecture design with the final sitemap model. Add a review document recording:

- local Rust gate commands and results;
- sitemap source failure/timeout aggregation;
- legacy helper removal;
- remaining real Reader, Docker, DNS rebinding and production traffic boundaries.

- [ ] **Step 4: Run final verification**

Run:

```text
cargo fmt --all
cargo test --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release --locked
git diff --check
```

Expected: all pass and the OpenAPI/schema/documentation scan finds no stale `resource_download_required`, `content_kind`, or sitemap 504 claims.

- [ ] **Step 5: Commit**

```text
git add src/routes/openapi.rs tests/openapi_api.rs docs/superpowers/specs/2026-09-04-refinery-material-access-architecture-design.md docs/reviews/2026-09-05-refinery-sitemap-outcome-acceptance.md
git commit -m "完成站点发现契约验收"
```

## Plan Self-Review

- Spec coverage: sitemap source state, error observability, legacy error removal, deterministic fixtures, documentation and OpenAPI are covered by Tasks 1–4.
- Placeholder scan: no `TODO`, `TBD`, “implement later” or unspecified function references remain.
- Type consistency: `SitemapStatus`, `SitemapSourceStatus`, `SitemapSources`, `SitemapWarning` are introduced in Task 1 and consumed by Tasks 3–4.
- Scope check: no new cache, database, queue, crawler, proxy or release action is introduced.
