# Refinery Jina Reader Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `refinery` Rust API that searches with SearXNG, extracts public HTML, known text resources and PDFs through self-hosted Jina Reader, downloads arbitrary public resources, and discovers lightweight site maps.

**Architecture:** `refinery` owns the stable intranet API, request validation, response normalization, Markdown chunking, link extraction, bounded resource downloads, sitemap discovery, size limits and error mapping. It calls SearXNG and Jina Reader over the external Docker network `refinery`; Reader is an unmodified sidecar that performs page, text and PDF extraction and is never exposed directly to staff devices.

**Tech Stack:** Rust 2024, Axum, Tokio, Reqwest/rustls, Serde, URL, `quick-xml`, Docker Compose, SearXNG, Jina Reader `ghcr.io/jina-ai/reader:oss`.

**Spec:** `docs/superpowers/specs/2026-09-01-internal-web-search-api-design.md`

## Global Constraints

- 服务和容器名称为 `refinery`；唯一向内网发布端口的是 `refinery:8080`。
- SearXNG 只用于搜索；Jina Reader 只用于单 URL 的 Markdown/PDF 抽取；两者均不发布宿主机端口。
- 首版不做持久化缓存、数据库、网页归档、任务队列、深度爬取、浏览器渲染、代理、OCR、Office 解析或 SaaS 专有能力。
- `POST /v1/content` 接受任意绝对 `http`/`https` 公网 URL；拒绝字面量回环、私有、链路本地、保留和其他非公网 IP。
- 无后缀、已知网页后缀、已知纯文本后缀和 `.pdf` 调用 Reader；已知图片后缀和未登记扩展名返回 `415 resource_download_required` 及 `/v1/resource` 地址，不调用 Reader。
- `GET /v1/resource` 下载任意公网资源，不按媒体类型拒绝；限制为 20 MiB、最多 5 次重定向，并以附件形式返回原始 `Content-Type`。
- 调用方的 Cookie、Authorization、Reader 特有请求头和请求体不得透传给外网或 Reader。
- Reader 请求固定携带无缓存、Markdown、链接保留和 20 秒超时设置；Reader 与 SearXNG 内部 HTTP 客户端使用 5 秒连接超时和 20 秒总超时；`refinery` 对 Reader 响应强制 10 MiB 上限。
- `POST /v1/sitemap` 只读取 robots.txt、sitemap.xml、sitemap index 和请求页的一层同源链接回退；不读取发现页面的正文。
- 当前 Compose 使用 `ghcr.io/jina-ai/reader:oss`。正式部署前可替换为固定 digest 并验证 HTML、链接、PDF、重定向、私网 DNS 解析、超大响应与错误响应；这不阻断 Refinery 源码构建和镜像发布。Reader 容器出站网络必须由部署环境拒绝私网/回环/链路本地目标。
- 按源 IP 的并发/速率限制和 `content_not_extractable` 细分错误仍是后续建模项，不属于当前首版实现。
- Rust 修改完成后必须执行 `cargo fmt --all` 与 `cargo clippy --all-targets --all-features -- -D warnings`，不得使用 `#[allow(...)]`。
- `refinery-app` 当前不是 Git 仓库；不得初始化仓库、推送或提交。

---

## File Structure

```text
Cargo.toml
Cargo.lock
Dockerfile
src/lib.rs
src/main.rs
src/config.rs
src/error.rs
src/http.rs
src/state.rs
src/routes/mod.rs
src/routes/health.rs
src/routes/search.rs
src/routes/content.rs
src/routes/resource.rs
src/routes/sitemap.rs
src/search/mod.rs
src/search/searxng.rs
src/reader/mod.rs
src/reader/client.rs
src/content/mod.rs
src/content/links.rs
src/content/chunk.rs
src/resource/mod.rs
src/sitemap/mod.rs
src/sitemap/parser.rs
src/sitemap/client.rs
tests/health_api.rs
tests/search_api.rs
tests/content_api.rs
tests/resource_api.rs
tests/sitemap_api.rs
README.md
deploy/README.md
deploy/docker-compose.yml
deploy/app/settings.yml
```

### 创建可测试的 Rust API 骨架

**Files:**

- Create: `Cargo.toml`, `src/lib.rs`, `src/main.rs`, `src/config.rs`, `src/error.rs`, `src/http.rs`, `src/state.rs`
- Create: `src/routes/mod.rs`, `src/routes/health.rs`, `tests/health_api.rs`, `Dockerfile`

**Interfaces:**

- Produces `Config::from_env() -> Result<Config, AppError>`.
- Produces `AppState::new(config: Config) -> AppState`.
- Produces `routes::router(state: AppState) -> Router`.
- Produces `GET /health -> 200 {"status":"ok"}`.

- [ ] **Step 1: 写出失败的健康检查测试**

```rust
#[tokio::test]
async fn health_returns_ok() {
    let response = get(test_app(), "/health").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_body(response).await, json!({ "status": "ok" }));
}
```

- [ ] **Step 2: 运行测试确认其因路由缺失而失败**

Run: `cargo test --test health_api`

Expected: FAIL because the `refinery` crate and `/health` route do not exist.

- [ ] **Step 3: 用最小 Axum 实现使健康检查通过**

`Cargo.toml` must declare Axum, Tokio, Reqwest with `rustls-tls`, Serde, Serde JSON, `thiserror`, `url`, `quick-xml`, `tower` with `util`, `http-body-util`, `tracing`, and `tracing-subscriber`. `Config` defaults are `HTTP_LISTEN_ADDRESS=0.0.0.0`, `HTTP_LISTEN_PORT=8080`, `SEARXNG_BASE_URL=http://searxng:8888`, `READER_BASE_URL=http://reader:8081`, and `READER_REQUEST_TIMEOUT_SECONDS=60`.

`src/lib.rs` exports all application modules. `src/main.rs` configures tracing, creates `AppState`, builds the router and binds the configured address. `src/http.rs` constructs the shared Reader/SearXNG client with a 5-second connect timeout and 20-second total timeout. `health` returns exactly the JSON shown in Step 1.

- [ ] **Step 4: Verify green and static checks**

Run:

```text
cargo test --test health_api
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: health test passes, formatter and Clippy report no findings.

### 实现 SearXNG 搜索接口

**Files:**

- Create: `src/search/mod.rs`, `src/search/searxng.rs`, `src/routes/search.rs`, `tests/search_api.rs`
- Modify: `src/routes/mod.rs`, `src/state.rs`, `src/error.rs`

**Interfaces:**

- Produces `SearchRequest { query, page, limit, language }`.
- Produces `SearchResponse { query, page, results }`.
- Produces `SearxngClient::search(&SearchRequest) -> Result<SearchResponse, AppError>`.
- Produces `POST /v1/search`.

- [ ] **Step 1: 写出空查询和上游 JSON 映射失败测试**

Use a local Axum upstream. Test that blank `query` returns `400/invalid_request`; test a SearXNG JSON `results` item with `title`, `url`, `content`, and `publishedDate` maps to `title`, `url`, `snippet`, and `published_at`. Assert upstream receives `q`, `format=json`, `pageno`, and optional `language`.

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --test search_api`

Expected: FAIL because the endpoint and client are absent.

- [ ] **Step 3: 实现最小搜索客户端与错误映射**

Validate nonblank query, `page >= 1`, and `1 <= limit <= 20`. Call `{SEARXNG_BASE_URL}/search` with `format=json`; map upstream failure or invalid JSON to `502/search_upstream_failed`.

- [ ] **Step 4: Verify green**

Run:

```text
cargo test --test search_api
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: validation, query mapping, result mapping and upstream failure tests pass.

### 实现 Jina Reader 内容客户端与统一内容接口

**Files:**

- Create: `src/reader/mod.rs`, `src/reader/client.rs`, `src/content/mod.rs`, `src/content/chunk.rs`, `src/content/links.rs`, `src/routes/content.rs`
- Create: `tests/content_api.rs`
- Modify: `src/routes/mod.rs`, `src/state.rs`, `src/error.rs`

**Interfaces:**

- Produces `ContentRequest { url, offset, max_chars }`.
- Produces `reader::client::read(client: &reqwest::Client, base_url: &str, url: &Url) -> Result<ReaderDocument, ApiError>`.
- Produces `ContentResponse { requested_url, final_url, resource_kind, content_type, title, markdown, links, offset, next_offset, truncated, warnings }`.
- Produces `POST /v1/content`.

- [ ] **Step 1: 写出内容接口失败测试**

Use a local fake Reader. Test that:

```json
{ "url": "https://example.test/article", "offset": 0, "max_chars": 1000 }
```

causes a Reader `POST /` request with JSON `{ "url": "https://example.test/article" }`, `x-no-cache: true`, `x-respond-with: markdown`, `x-retain-links: all`, and `x-timeout: 20`. The fake Reader returns fixture Markdown containing one HTML link and one PDF link. Assert `/v1/content` returns absolute links, classifies the PDF as `pdf`, and only returns links present in the current chunk.

Add separate tests that reject `file://`, `ftp://`, `127.0.0.1`, `::1`, `169.254.169.254`, and `192.168.0.1` with `403/blocked_target`; and that reject `max_chars` outside `1000..=24000` with `400/invalid_request`.

Add classification tests proving that `.txt`, `.md`, `.csv`, `.json`, `.xml`, `.yaml`, and `.yml` are sent to Reader and returned as `resource_kind: text`; `.pdf` is sent to Reader and returned as `pdf`; known image extensions and unregistered extensions do not call Reader and return `415/resource_download_required` with an encoded `/v1/resource` URL.

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --test content_api`

Expected: FAIL because the Reader client and content route are absent.

- [ ] **Step 3: 实现 Reader 适配与响应规范化**

The Reader client sends only fixed headers; it never forwards caller headers. It captures `x-responded-url` when Reader provides it and otherwise uses the requested URL as `final_url`. It maps Reader 4xx/5xx, malformed responses, timeout, and >10 MiB response bodies to the documented `fetch_failed`, `fetch_timeout`, and `response_too_large` errors.

Validate URL scheme/host and literal IP before classification. Classify by URL path without an extra probe request: known HTML and extensionless URLs are `html`, known text extensions are `text`, and `.pdf` is `pdf`. Send those three kinds to Reader. Known image and unknown-extension URLs return `resource_download_required` without calling Reader. Chunk by Rust character boundaries, use Markdown link parsing to collect only current-chunk links, and resolve relative URLs against `final_url`.

- [ ] **Step 4: Verify green**

Run:

```text
cargo test --test content_api
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: request headers, URL rejection, content classification, resource-download direction, chunking, link preservation, upstream error and response-size tests pass.

### 实现任意公网资源下载与轻量网站地图接口

**Files:**

- Create: `src/resource/mod.rs`, `src/routes/resource.rs`, `tests/resource_api.rs`
- Create: `src/sitemap/mod.rs`, `src/sitemap/parser.rs`, `src/sitemap/client.rs`, `src/routes/sitemap.rs`
- Create: `tests/sitemap_api.rs`
- Modify: `src/routes/mod.rs`, `src/state.rs`, `src/error.rs`

**Interfaces:**

- Produces `ResourceFetcher::get(url: &Url) -> Result<Resource, ApiError>` and `GET /v1/resource?url=...`.
- Produces `SitemapRequest { url, limit }`.
- Produces `SitemapResponse { requested_url, site_url, urls, truncated, warnings }`.
- Produces `sitemap::discover(fetcher: Arc<dyn SitemapFetcher>, request: SitemapRequest) -> Result<SitemapResponse, ApiError>`.
- Produces `POST /v1/sitemap`.

- [ ] **Step 1: 写出公网下载器、robots 和 sitemap index 的失败测试**

First write tests proving that the resource fetcher rejects non-HTTP URL schemes and literal loopback, private, link-local, unspecified, multicast and reserved IP addresses. Its `reqwest` client must disable automatic redirects; each redirect target is resolved and revalidated before the next request. Test that arbitrary response media types are returned unchanged as attachments with `X-Content-Type-Options: nosniff`; do not reject HTML, Office files, archives or unknown binary types.

Then serve the fixed `robots.txt` containing `Sitemap: https://docs.example.test/sitemap-index.xml`; serve an index linking `docs.xml`; serve `docs.xml` with three same-origin URLs and one external URL. Assert `POST /v1/sitemap` returns the three same-origin URLs, source `sitemap`, removes duplicates, and does not request any discovered URL body.

Add a fallback test with no robots declaration and a valid `/sitemap.xml`. Add a final fallback test where sitemap returns 404 and a fake Reader response contains same-origin and external Markdown links; assert only same-origin links are returned with source `page_link`.

- [ ] **Step 2: 运行测试确认失败**

Run:

```text
cargo test --test resource_api
cargo test --test sitemap_api
```

Expected: FAIL because the resource and sitemap routes and parser are absent.

- [ ] **Step 3: 实现受限 sitemap 发现**

`HttpResourceFetcher` and `HttpSitemapFetcher` must validate the URL before every connection, reject any DNS result that is not public, set `redirect(Policy::none())`, use a 5-second connect timeout and 20-second total timeout, and follow at most five manually validated redirects. Resource responses stop after 20 MiB and preserve the upstream `Content-Type`; sitemap documents stop after 5 MiB. Deployment-owned egress policy remains mandatory to protect against DNS rebinding and route-level access that application checks cannot fully prevent.

Parse `Sitemap:` case-insensitively; fall back to root `/sitemap.xml`; parse `urlset` and `sitemapindex` using `quick-xml`; traverse at most 20 sitemap documents and return at most `limit` same-origin URLs. If sitemap discovery returns no URLs, call Reader once for the requested page and collect one level of same-origin links. Do not read discovered page bodies or invoke browser rendering.

- [ ] **Step 4: Verify green**

Run:

```text
cargo test --test sitemap_api
cargo test --test resource_api
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: arbitrary resource attachment, robots, sitemap index, fallback and same-origin tests pass.

### 接入本仓三容器 Compose 并记录 Reader 验收边界

**Files:**

- Create: `deploy/docker-compose.yml`, `deploy/.env.example`, `deploy/app/settings.yml`, `deploy/README.md`
- Create: `README.md`

**Interfaces:**

- Produces `searxng`, `reader`, and `refinery` services on the external Docker network `refinery`, with only `refinery` publishing host port 8080.
- Keeps `ghcr.io/jina-ai/reader:oss` in the source Compose; replacing it with a verified digest is a target-server deployment action, not a source-release prerequisite.

- [ ] **Step 1: 写出 Compose 静态断言**

Assert that all three services use the external network named `refinery`; only Refinery contains `ports`; SearXNG mounts `./app/settings.yml`; the settings file binds `0.0.0.0:8888`; and the Compose file contains no `${...}` references.

- [ ] **Step 2: 记录 Reader 版本准入**

Keep `ghcr.io/jina-ai/reader:oss` in the current Compose. On the intended internet-enabled deployment server, the deployment owner may resolve and record an immutable digest before startup. The target-server acceptance record must include observed behavior for HTML Markdown links, PDF text, Reader response headers, redirect to public target, redirect to private target, DNS/private target, and oversized body.

If any SSRF or response-size case violates the spec, do not start the service for intranet users. This runtime acceptance does not block building or publishing Refinery source and images.

- [ ] **Step 3: 修改 Compose**

Create the three services without `${...}` references. Load Refinery variables from `./.env`, mount SearXNG from `./app/settings.yml:/etc/searxng/settings.yml:ro`, set its `bind_address` to `0.0.0.0`, and do not publish SearXNG or Reader ports. The resulting structure is:

```yaml
  reader:
    image: ghcr.io/jina-ai/reader:oss
    restart: always

  refinery:
    image: ghcr.io/zoranner/refinery:0.1.1
    restart: always
    env_file:
      - ./.env
    ports:
      - 8080:8080
```

Do not choose an alternative host port if 8080 is occupied; obtain deployment-owner direction.

- [ ] **Step 4: 写明部署与安全要求**

README must document API examples; Jina Reader's separate internal role; no cache behavior; `sitemap` limits; the requirement that a deployment-owned egress firewall blocks Reader access to loopback, private, link-local, and metadata addresses; and the required local/online validation commands.

- [ ] **Step 5: Verify integration configuration**

Run:

```text
cargo test
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
docker compose -f deploy/docker-compose.yml config
```

Expected: Rust tests, formatting, Clippy, and Compose configuration pass. If Docker CLI is unavailable, record that verification gap. Do not start containers in the local development workstation unless the user explicitly requests it.

## Self-Review

- Spec coverage: Task 1 creates the service boundary and shared HTTP timeout policy; Task 2 covers SearXNG; Task 3 covers Reader HTML/text/PDF classification and resource-download direction; Task 4 covers arbitrary resource download and the deliberately limited sitemap; Task 5 covers local Docker deployment and target-server Reader acceptance boundaries.
- Placeholder check: No implementation task depends on an unspecified function or a future generic validation step.
- Interface consistency: every public endpoint is registered by `routes::router`; all upstream dependencies enter through `AppState`; `ContentRequest`, `SearchRequest`, and `SitemapRequest` are validated before upstream access.
