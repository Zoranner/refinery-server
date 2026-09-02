# Refinery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **Status on September 1, 2026:** Superseded before execution. This plan assumes that `refinery` implements its own HTML extractor and does not reflect the approved Jina Reader integration. Do not execute this plan. The current implementation plan is `docs/superpowers/plans/2026-09-01-refinery-jina-reader-implementation.md`.

**Goal:** Build `refinery`, an internal Rust HTTP service that searches through SearXNG and reads public HTML/PDF content as link-preserving Markdown.

**Architecture:** A single Axum container exposes `/health`, `/v1/search`, and `/v1/content`. It calls SearXNG through the existing Docker network and fetches public URLs through a redirect-aware, public-address-only HTTP client; HTML and PDF processors convert content into bounded Markdown responses without retaining content between requests.

**Tech Stack:** Rust 1.88, Axum, Tokio, Reqwest with rustls, Serde, URL, `dom_smoothie`/`readabilityrs`/`trafilatura` qualification harness, selected HTML extractor, `lopdf`, Docker Compose, SearXNG.

**Spec:** `docs/superpowers/specs/2026-09-01-internal-web-search-api-design.md`

## Global Constraints

- 服务和 Docker 容器名称固定为 `refinery`，不得使用 `search-api`。
- 首版不做持久化缓存、数据库、网页归档或搜索索引；每次 `/v1/content` 重新抓取。
- 只接收和抓取绝对 `http`、`https` URL；所有首次连接和重定向目标必须拒绝非公网 IP。
- 仅使用 HTTP `GET`，不转发调用方 Cookie、认证头或请求体。
- 最多 5 次重定向；连接超时 5 秒；单次总超时 20 秒；响应体最大 10 MiB。
- 仅处理 HTML、PDF、图片；HTML/PDF 返回 Markdown，图片不返回二进制、OCR 或视觉识别结果。
- HTML 相对链接必须转换为基于最终 URL 的绝对链接；响应只列出当前 Markdown 分段中出现的正文链接。
- 搜索只经 Docker 网络中的 `searxng:8888` 调用；SearXNG 不向员工网段发布端口。
- Rust 代码完成后必须执行 `cargo fmt --all` 和 `cargo clippy --all-targets --all-features -- -D warnings`，不得使用 `#[allow(...)]` 压制警告。
- 当前 `refinery-app` 目录不是 Git 仓库。执行者不得自行初始化仓库、推送或创建远端；在用户提供 Git 仓库后，再按任务检查点创建本地提交。

---

## 文件结构

```text
Cargo.toml                                      # Rust 依赖与特性
Cargo.lock                                      # 锁定依赖版本
Dockerfile                                      # refinery 容器镜像
src/lib.rs                                      # 供集成测试和二进制入口复用的模块根
src/main.rs                                     # 进程启动、日志和路由装配
src/config.rs                                   # 环境变量与运行限制
src/error.rs                                    # API 错误模型和状态码映射
src/state.rs                                    # 只保存客户端与配置的 AppState
src/routes/mod.rs                               # 路由注册
src/routes/health.rs                            # GET /health
src/routes/search.rs                            # POST /v1/search
src/routes/content.rs                           # POST /v1/content
src/search/mod.rs                               # 搜索领域模型
src/search/searxng.rs                           # SearXNG JSON 客户端
src/fetch/mod.rs                                # 公网抓取入口和响应模型
src/fetch/policy.rs                             # URL、IP、重定向和大小策略
src/fetch/resolver.rs                           # Reqwest 实际 DNS 解析过滤
src/fetch/client.rs                             # 受限 HTTP GET 与人工重定向
src/content/mod.rs                              # 内容类型分派和分段
src/content/model.rs                            # ContentResponse、Link 与内容类型
src/content/html.rs                             # 选定 HTML 抽取器和链接标准化
src/content/pdf.rs                              # PDF 文本提取
src/content/image.rs                            # 图片元数据响应
src/content/links.rs                            # Markdown 链接收集与目标类型推断
tests/common/mod.rs                             # 测试 HTTP 服务和共享断言
tests/health_api.rs                             # 健康检查集成测试
tests/search_api.rs                             # SearXNG 代理集成测试
tests/fetch_policy.rs                           # URL 和 IP 地址策略测试
tests/content_api.rs                            # HTML、PDF、图片和分段集成测试
tests/fixtures/html/*.html                      # 固定 HTML 抽取样本
tests/fixtures/pdf/*.pdf                        # 固定 PDF 样本
examples/extractor_qualification.rs             # 临时的三个 HTML 候选固定样本比较工具，选择后删除
docs/validation/html-extractor-qualification.md # 候选评估记录
E:\Repositories\docker\services\searxng-service\docker-compose.yaml
E:\Repositories\docker\services\searxng-service\app\settings.yml
```

### Task 1: 建立 Rust 服务骨架与健康检查

**Files:**

- Create: `Cargo.toml`
- Create: `src/lib.rs`
- Create: `src/main.rs`
- Create: `src/config.rs`
- Create: `src/error.rs`
- Create: `src/state.rs`
- Create: `src/routes/mod.rs`
- Create: `src/routes/health.rs`
- Create: `tests/health_api.rs`
- Create: `Dockerfile`

**Interfaces:**

- Produces `Config::from_env() -> Result<Config, ConfigError>`.
- Produces `AppState::new(config: Config) -> Result<AppState, AppError>`.
- Produces `routes::router(state: AppState) -> axum::Router`.
- Produces `GET /health -> 200 {"status":"ok"}`.

- [ ] **Step 1: 写出健康检查的失败测试**

```rust
#[tokio::test]
async fn health_returns_ok() {
    let app = refinery::routes::router(test_state());
    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await, json!({ "status": "ok" }));
}
```

- [ ] **Step 2: 运行失败测试**

Run: `cargo test --test health_api`

Expected: FAIL，因为 `refinery` crate、路由或测试辅助函数尚不存在。

- [ ] **Step 3: 创建最小可启动的 Axum 应用**

在 `Cargo.toml` 中加入最小运行依赖：

```toml
[package]
name = "refinery"
version = "0.1.0"
edition = "2024"

[dependencies]
axum = "0.8"
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tokio = { version = "1", features = ["macros", "net", "rt-multi-thread", "time"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
url = "2"

[dev-dependencies]
http-body-util = "0.1"
tower = { version = "0.5", features = ["util"] }
```

实现以下最小代码：

```rust
// src/routes/health.rs
pub async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "status": "ok" }))
}

// src/routes/mod.rs
pub fn router(state: AppState) -> axum::Router {
    axum::Router::new()
        .route("/health", axum::routing::get(health::health))
        .with_state(state)
}
```

`src/lib.rs` 首先导出 `config`、`error`、`routes` 和 `state` 模块；`src/main.rs` 只调用这些库模块完成启动。后续任务新增的 `search`、`fetch` 和 `content` 模块也由该文件导出，使集成测试始终通过 `refinery::` 使用真实路由。

`Config` 必须读取 `REFINERY_BIND`、`REFINERY_PORT` 和 `SEARXNG_BASE_URL`，默认值分别为 `0.0.0.0`、`8080` 与 `http://searxng:8888`。`main` 初始化 `tracing_subscriber`、构造 `AppState` 并以该绑定地址启动路由。

- [ ] **Step 4: 编写 Dockerfile**

使用 `rust:1.88-bookworm` 构建镜像，并使用 `debian:bookworm-slim` 作为运行镜像。运行镜像只复制编译后的 `refinery` 二进制，暴露 `8080`，入口为：

```dockerfile
ENTRYPOINT ["/usr/local/bin/refinery"]
```

- [ ] **Step 5: 运行通过测试与静态检查**

Run:

```text
cargo test --test health_api
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: health 测试通过；格式和 Clippy 无警告。

- [ ] **Step 6: 记录 Git 检查点**

当前目录不是 Git 仓库，执行：

```text
git rev-parse --is-inside-work-tree
```

Expected: 失败并如实记录，不初始化仓库。用户提供 Git 仓库后，暂存本任务文件并提交：`初始化 refinery 健康检查服务`。

### Task 2: 修正 SearXNG Compose 基线并接入 refinery 容器

**Files:**

- Modify: `E:\Repositories\docker\services\searxng-service\docker-compose.yaml`
- Modify: `E:\Repositories\docker\services\searxng-service\app\settings.yml`

**Interfaces:**

- Consumes `refinery` 镜像与 `SEARXNG_BASE_URL=http://searxng:8888`。
- Produces同网络、可由 `refinery` 访问的 `searxng:8888`。
- Produces唯一向内网发布的 `refinery:8080`。

- [ ] **Step 1: 写出 Compose 配置前置检查**

运行以下命令并记录当前失败或不符合项：

```powershell
Test-Path 'E:\Repositories\docker\services\searxng-service\app\searxng\settings.yml'
Select-String -Path 'E:\Repositories\docker\services\searxng-service\app\settings.yml' -Pattern "bind_address: '127.0.0.1'"
```

Expected: 第一个命令为 `False`，第二个命令匹配；这证明现有挂载路径和网络监听不满足双容器通信。

- [ ] **Step 2: 修正 SearXNG 配置**

将 Compose 的挂载项改为：

```yaml
volumes:
  - ./app/settings.yml:/etc/searxng/settings.yml:ro
```

将 `app/settings.yml` 的服务器监听改为：

```yaml
bind_address: '0.0.0.0'
```

保留 `port: 8888` 和 JSON 输出格式；不为 SearXNG 增加 `ports`。

- [ ] **Step 3: 增加 refinery 服务**

在同一 `docker-compose.yaml` 加入：

```yaml
  refinery:
    build:
      context: E:\Repositories\projects\Zoranner\workflow-suite\refinery-app
    container_name: refinery
    restart: always
    environment:
      REFINERY_BIND: 0.0.0.0
      REFINERY_PORT: 8080
      SEARXNG_BASE_URL: http://searxng:8888
    ports:
      - "8080:8080"
```

该服务默认加入现有 `searxng` 外部网络。若服务器已有 8080 端口占用，必须先由部署负责人指定替代端口，不能自行选择。

- [ ] **Step 4: 验证 Compose 静态配置**

Run:

```powershell
docker compose -f 'E:\Repositories\docker\services\searxng-service\docker-compose.yaml' config
```

Expected: 两个服务都在输出中；SearXNG 使用实际存在的只读设置文件；只有 `refinery` 含 `ports`。

- [ ] **Step 5: 记录 Git 检查点**

在相应 Git 仓库存在时，分开提交 Compose 基线修正与 Rust 源码。此任务的提交标题为：`接入 refinery 容器并修正 SearXNG 配置挂载`。

### Task 3: 实现 SearXNG 搜索客户端与 `/v1/search`

**Files:**

- Create: `src/search/mod.rs`
- Create: `src/search/searxng.rs`
- Create: `src/routes/search.rs`
- Modify: `src/routes/mod.rs`
- Modify: `src/state.rs`
- Modify: `src/error.rs`
- Create: `tests/common/mod.rs`
- Create: `tests/search_api.rs`

**Interfaces:**

- Produces `SearchRequest { query, page, limit, language }`.
- Produces `SearchResponse { query, page, results }`.
- Produces `SearxngClient::search(&self, request: &SearchRequest) -> Result<SearchResponse, AppError>`.
- Produces `POST /v1/search`.

- [ ] **Step 1: 写出搜索请求校验失败测试**

```rust
#[tokio::test]
async fn search_rejects_blank_query() {
    let response = post_json(app(), "/v1/search", json!({
        "query": "  ",
        "page": 1,
        "limit": 10
    })).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(error_code(response).await, "invalid_request");
}
```

- [ ] **Step 2: 写出 SearXNG JSON 映射失败测试**

测试 HTTP 服务器返回：

```json
{
  "results": [
    {
      "title": "DOM Smoothie",
      "url": "https://example.test/dom-smoothie",
      "content": "Readable content extractor",
      "publishedDate": "2026-08-01"
    }
  ]
}
```

断言 API 响应的第一项分别为 `title`、`url`、`snippet` 与 `published_at`，并且上游收到 `format=json`、`q`、`pageno`、`language` 参数。

- [ ] **Step 3: 运行失败测试**

Run:

```text
cargo test --test search_api
```

Expected: FAIL，因为搜索路由和 `SearxngClient` 尚不存在。

- [ ] **Step 4: 实现搜索模型和客户端**

在 `src/search/mod.rs` 定义：

```rust
#[derive(Deserialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_limit")]
    pub limit: u8,
    pub language: Option<String>,
}

#[derive(Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub page: u32,
    pub results: Vec<SearchResult>,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub published_at: Option<String>,
}
```

`SearchRequest::validate` 必须拒绝空白 `query`、`page == 0`、`limit == 0` 与 `limit > 20`。`SearxngClient` 用 Reqwest 调用 `{base_url}/search`，固定带 `format=json`，将上游 `content` 映射为 `snippet`，将缺失的可选字段保留为 `null`。

路由错误映射固定为：输入错误 `400/invalid_request`；SearXNG HTTP、JSON 和连接错误 `502/search_upstream_failed`。

- [ ] **Step 5: 运行通过测试与静态检查**

Run:

```text
cargo test --test search_api
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: 空查询、参数上限、上游参数映射、正常结果映射和上游错误映射测试通过。

- [ ] **Step 6: 创建 Git 检查点**

用户提供 Git 仓库后，提交标题：`实现 SearXNG 搜索接口`。

### Task 4: 实现公网 URL 策略与受限抓取器

**Files:**

- Create: `src/fetch/mod.rs`
- Create: `src/fetch/policy.rs`
- Create: `src/fetch/resolver.rs`
- Create: `src/fetch/client.rs`
- Modify: `src/config.rs`
- Modify: `src/error.rs`
- Create: `tests/fetch_policy.rs`

**Interfaces:**

- Produces `PublicUrl::parse(raw: &str) -> Result<PublicUrl, AppError>`.
- Produces `PublicResolver`, a Reqwest DNS resolver that rejects non-public resolved IP addresses.
- Produces `FetchClient::get(url: PublicUrl) -> Result<FetchedResponse, AppError>`.
- Produces `FetchedResponse { final_url, content_type, content_length, body }`.

- [ ] **Step 1: 写出 URL 与 IP 策略失败测试**

```rust
#[test]
fn public_url_rejects_non_http_schemes_and_literal_private_addresses() {
    for raw in [
        "file:///etc/passwd",
        "ftp://example.com/file",
        "http://127.0.0.1/admin",
        "http://[::1]/admin",
        "http://169.254.169.254/latest/meta-data",
        "http://192.168.1.10/",
    ] {
        assert_eq!(PublicUrl::parse(raw).unwrap_err().code(), "blocked_target");
    }
}
```

再使用可注入的固定 DNS 解析器，验证 `10.0.0.1`、`172.16.0.1`、`192.168.0.1`、`127.0.0.1`、`::1`、`fe80::1` 和多播地址都会在连接前被拒绝；`1.1.1.1` 可以通过地址策略。

- [ ] **Step 2: 写出抓取限制失败测试**

用本地测试服务器构造以下行为，并用单元级重定向决策函数验证：

- 第六次重定向返回 `fetch_failed`。
- `Content-Length: 10485761` 返回 `413/response_too_large`，不读取主体。
- 流式响应实际超过 10 MiB 返回 `413/response_too_large`。
- 连接总耗时超过 20 秒返回 `504/fetch_timeout`。

- [ ] **Step 3: 运行失败测试**

Run:

```text
cargo test --test fetch_policy
```

Expected: FAIL，因为 URL、DNS 和抓取客户端尚不存在。

- [ ] **Step 4: 实现验证与抓取**

`PublicUrl::parse` 使用 `url::Url`，要求绝对 URL、`http` 或 `https` 协议、非空主机，并立即拒绝 URL 字面量中的非公网 IP。

`PublicResolver` 包装系统 DNS 解析，在将地址交给 Reqwest 前逐一调用 `is_public_ip`。`is_public_ip` 必须明确拒绝 IPv4 的 loopback、private、link-local、unspecified、multicast、documentation、broadcast、共享地址段和保留段；IPv6 必须拒绝 loopback、unique-local、link-local、unspecified、multicast 和 documentation 段。

`FetchClient` 使用：

```rust
reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none())
    .connect_timeout(Duration::from_secs(5))
    .timeout(Duration::from_secs(20))
    .dns_resolver(Arc::new(PublicResolver::new()))
```

实现人工重定向循环。每个 `Location` 都基于当前 URL 解析为绝对 URL，重新执行 `PublicUrl::parse`，并在第 6 次跳转前返回 `fetch_failed`。请求方法固定为 `GET`，只发送服务固定的 `User-Agent`，不接收或转发用户 Header。

读取响应前检查 `Content-Length`，读取时累计字节数，超过 `10 * 1024 * 1024` 立即中止。只保存当前响应的内存字节，不写磁盘、数据库或缓存。

- [ ] **Step 5: 运行通过测试与静态检查**

Run:

```text
cargo test --test fetch_policy
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: 所有受限协议、内网地址、重定向、超时和大小测试通过。

- [ ] **Step 6: 创建 Git 检查点**

用户提供 Git 仓库后，提交标题：`实现 refinery 公网抓取安全策略`。

### Task 5: 完成 HTML 抽取器资格验证并锁定唯一依赖

**Files:**

- Create: `tests/fixtures/html/article.html`
- Create: `tests/fixtures/html/documentation.html`
- Create: `tests/fixtures/html/table-and-relative-links.html`
- Create: `tests/fixtures/html/github-like-project.html`
- Create: `examples/extractor_qualification.rs`
- Create: `docs/validation/html-extractor-qualification.md`
- Modify: `Cargo.toml`

**Interfaces:**

- Produces four固定 HTML 样本的 Markdown、标题和链接结果。
- Produces `docs/validation/html-extractor-qualification.md`，记录每个候选的版本、下载量、Star、样本表现与唯一选定库。
- Produces one selected HTML extractor dependency; 删除未选中的候选依赖。

- [ ] **Step 1: 写入固定样本与期望事实**

四个样本必须分别包含：

- `article.html`：导航、文章标题、正文段落、作者日期和页脚。
- `documentation.html`：目录、代码块、二级标题和正文链接。
- `table-and-relative-links.html`：表格、`../guide` 相对链接、`/download/report.pdf` PDF 链接和图片 `alt`。
- `github-like-project.html`：仓库名称、Star、版本、下载量、README 主内容和侧栏噪声。

每个样本在文件开头以 HTML 注释写出必须保留的正文短语和必须排除的导航短语。

- [ ] **Step 2: 编写候选比较程序**

在 `examples/extractor_qualification.rs` 中为 `dom_smoothie`、`readabilityrs` 和 `trafilatura` 分别执行同一组输入，输出以下 JSON 字段：

```json
{
  "candidate": "dom_smoothie",
  "fixture": "table-and-relative-links.html",
  "title": "…",
  "markdown": "…",
  "link_count": 3,
  "contains_required_text": true,
  "contains_navigation_noise": false,
  "contains_absolute_pdf_link": true
}
```

候选均使用其 Markdown 输出模式；输入 URL 固定为 `https://fixtures.example/docs/page.html`，用以验证相对 URL 转绝对 URL。

- [ ] **Step 3: 运行资格比较并记录失败基线**

Run:

```text
cargo run --example extractor_qualification
```

Expected: 在未接入候选库前 FAIL，明确指出缺失的 crate 或比较实现。

- [ ] **Step 4: 添加三个候选并完成比较**

临时加入：

```toml
dom_smoothie = "0.18"
readabilityrs = "0.1"
trafilatura = "0.3"
```

`dom_smoothie` 使用 `Readability::new(...).parse()` 与 `TextMode::Markdown`；`readabilityrs` 读取其 `markdown_content`；`trafilatura` 使用 `extract` 与 `content_markdown()`。

在 `docs/validation/html-extractor-qualification.md` 中记录执行日期、crate 版本、crates.io 下载量、GitHub Star、每个样本的正文保留、噪声移除、表格、绝对链接和 Markdown 结果。

选定规则固定为：

1. 四个样本均保留所有规定正文短语。
2. 四个样本均不保留规定导航短语。
3. `table-and-relative-links.html` 中 PDF 和图片链接必须是绝对 URL。
4. 满足前三项的候选中，优先选择下载量与近期维护更高者。

若没有候选同时满足前三项，停止后续 HTML 实现，更新固定样本和评估记录后请用户决定，而不是同时保留多个运行时抽取器。

- [ ] **Step 5: 删除未选中依赖与临时比较程序**

删除 `Cargo.toml` 中未选中的两个候选，并删除 `examples/extractor_qualification.rs`。资格评估结论保留在 `docs/validation/html-extractor-qualification.md`；运行时与测试不再编译三个候选库。运行：

```text
cargo test
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: 全部测试、格式和 Clippy 通过；评估记录中的选定库与 `Cargo.toml` 唯一 HTML 抽取依赖一致。

- [ ] **Step 6: 创建 Git 检查点**

用户提供 Git 仓库后，提交标题：`验证并确定 refinery 正文抽取依赖`。

### Task 6: 实现 HTML 内容模型、Markdown 链接标准化与 `/v1/content`

**Files:**

- Create: `src/content/mod.rs`
- Create: `src/content/model.rs`
- Create: `src/content/html.rs`
- Create: `src/content/links.rs`
- Create: `src/routes/content.rs`
- Modify: `src/routes/mod.rs`
- Modify: `src/state.rs`
- Modify: `src/error.rs`
- Create: `tests/content_api.rs`

**Interfaces:**

- Produces `ContentRequest { url, offset, max_chars }`.
- Produces `ContentResponse { requested_url, final_url, content_kind, content_type, title, markdown, links, offset, next_offset, truncated, warnings }`.
- Produces `content::from_fetched(request, fetched) -> Result<ContentResponse, AppError>`.
- Produces `POST /v1/content`.

- [ ] **Step 1: 写出 HTML 内容接口失败测试**

```rust
#[tokio::test]
async fn content_returns_markdown_and_only_current_chunk_links() {
    let response = post_json(app_with_fixture_fetcher("article.html"), "/v1/content", json!({
        "url": "https://fixtures.example/article",
        "offset": 0,
        "max_chars": 120
    })).await;

    let body = body_json(response).await;
    assert_eq!(body["content_kind"], "html");
    assert!(body["markdown"].as_str().unwrap().contains("正文"));
    assert_eq!(body["links"][0]["url"], "https://fixtures.example/report.pdf");
    assert_eq!(body["links"][0]["kind"], "pdf");
    assert_eq!(body["truncated"], true);
    assert_eq!(body["next_offset"], 120);
}
```

再写两个失败测试：`offset` 超过已抽取文本长度时返回空 Markdown 与 `next_offset: null`；`max_chars: 999999` 返回 `400/invalid_request`。

- [ ] **Step 2: 运行失败测试**

Run:

```text
cargo test --test content_api content_returns_markdown_and_only_current_chunk_links
```

Expected: FAIL，因为内容模型、路由和 HTML 处理尚不存在。

- [ ] **Step 3: 实现请求、响应和 HTML 处理**

在 `src/content/model.rs` 定义：

```rust
#[derive(Deserialize)]
pub struct ContentRequest {
    pub url: String,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_max_chars")]
    pub max_chars: usize,
}

#[derive(Serialize)]
pub struct Link {
    pub text: String,
    pub url: String,
    pub kind: LinkKind,
}

#[derive(Serialize)]
pub struct ContentResponse {
    pub requested_url: String,
    pub final_url: String,
    pub content_kind: ContentKind,
    pub content_type: String,
    pub title: Option<String>,
    pub markdown: String,
    pub links: Vec<Link>,
    pub offset: usize,
    pub next_offset: Option<usize>,
    pub truncated: bool,
    pub warnings: Vec<String>,
}
```

`ContentRequest::validate` 将 `max_chars` 限定为 `1000..=24000`。`html::extract` 使用 Task 5 选定的唯一库生成主正文 Markdown，并以 `final_url` 为基准把相对 Markdown 链接替换为绝对 URL。

`links::collect_from_markdown` 只收集当前分段内的 Markdown 链接；根据 MIME、路径扩展名和图片语法推断 `html`、`pdf`、`image` 或 `unknown`。分段必须按 Rust 字符边界切片，不能用字节索引截断 UTF-8。

- [ ] **Step 4: 将 FetchClient 接入路由**

`/v1/content` 先调用 `PublicUrl::parse`，再调用 `FetchClient::get`，最后交给 `content::from_fetched`。HTML MIME 类型接受 `text/html` 与 `application/xhtml+xml`；无可抽取主内容时返回 `422/content_not_extractable`。

- [ ] **Step 5: 运行通过测试与静态检查**

Run:

```text
cargo test --test content_api
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: HTML 主文、链接标准化、UTF-8 分段、空分段和参数范围测试通过。

- [ ] **Step 6: 创建 Git 检查点**

用户提供 Git 仓库后，提交标题：`实现 refinery 网页内容读取接口`。

### Task 7: 接入 PDF 和图片内容分派

**Files:**

- Create: `src/content/pdf.rs`
- Create: `src/content/image.rs`
- Modify: `src/content/mod.rs`
- Modify: `src/content/model.rs`
- Modify: `tests/content_api.rs`
- Create: `tests/fixtures/pdf/text.pdf`
- Create: `tests/fixtures/pdf/scanned.pdf`
- Modify: `Cargo.toml`

**Interfaces:**

- Produces `pdf::extract(bytes: &[u8]) -> Result<(Option<String>, Vec<String>), AppError>`.
- Produces `image::describe(fetched: &FetchedResponse) -> ContentResponse`.
- Extends `content::from_fetched` to handle PDF and image MIME types.

- [ ] **Step 1: 写出 PDF 与图片失败测试**

```rust
#[tokio::test]
async fn content_extracts_pdf_text_in_chunks() {
    let response = post_json(app_with_fixture_fetcher("text.pdf"), "/v1/content", json!({
        "url": "https://fixtures.example/report.pdf",
        "offset": 0,
        "max_chars": 1000
    })).await;

    let body = body_json(response).await;
    assert_eq!(body["content_kind"], "pdf");
    assert!(body["markdown"].as_str().unwrap().contains("报告正文"));
}

#[tokio::test]
async fn content_marks_scanned_pdf_without_text() {
    let body = body_json(post_json(app_with_fixture_fetcher("scanned.pdf"), "/v1/content", json!({
        "url": "https://fixtures.example/scan.pdf"
    })).await).await;

    assert_eq!(body["content_kind"], "pdf");
    assert_eq!(body["warnings"], json!(["no_extractable_text"]));
}
```

再写图片测试：`image/png` 返回 `content_kind: "image"`、空 Markdown、`next_offset: null`，且不返回原始二进制字段。

- [ ] **Step 2: 运行失败测试**

Run:

```text
cargo test --test content_api content_extracts_pdf_text_in_chunks
```

Expected: FAIL，因为 PDF 和图片分派尚未实现。

- [ ] **Step 3: 实现 PDF 和图片处理**

加入 `lopdf`。`pdf::extract` 使用 `lopdf::Document::load_mem` 打开内存字节，以页号顺序调用 `extract_text`，使用双换行连接非空页文本。

PDF 无可提取文本时返回 `ContentResponse` 成功结果，`markdown` 为空，`warnings` 为 `["no_extractable_text"]`；PDF 解析损坏时返回 `422/content_not_extractable`。

`image::describe` 不读取或编码图片内容，只返回最终 URL、`content_kind: image`、原始媒体类型、空 Markdown、空 links、空 warnings 与 `next_offset: null`。

- [ ] **Step 4: 接入 MIME 内容分派**

`content::from_fetched` 按响应 `Content-Type` 分派：

```text
text/html, application/xhtml+xml -> HTML
application/pdf                 -> PDF
image/*                         -> image
其他                             -> 415/unsupported_content_type
```

- [ ] **Step 5: 运行通过测试与静态检查**

Run:

```text
cargo test --test content_api
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: PDF 文本、扫描 PDF 警告、图片无二进制响应和不支持媒体类型测试通过。

- [ ] **Step 6: 创建 Git 检查点**

用户提供 Git 仓库后，提交标题：`支持 refinery PDF 与图片资源读取`。

### Task 8: 完成端到端测试、运行说明与联网验收

**Files:**

- Create: `README.md`
- Modify: `tests/search_api.rs`
- Modify: `tests/content_api.rs`
- Modify: `docs/validation/html-extractor-qualification.md`

**Interfaces:**

- Produces本地构建、Compose 启动和接口调用说明。
- Produces可重复的单元与集成验证命令。
- Produces一次在有外网权限服务器执行的联网验收记录。

- [ ] **Step 1: 写出端到端失败测试**

测试服务链路使用本地模拟 SearXNG 与本地公开样本服务器：

1. `/v1/search` 返回固定结果 URL。
2. 将该 URL 传给 `/v1/content`。
3. 响应含正文 Markdown 和 PDF 链接。
4. 将 PDF 链接传给 `/v1/content`。
5. 响应返回 PDF 文本。

断言每一步都只使用前一步输出的原始 URL，不依赖缓存、数据库或服务端会话。

- [ ] **Step 2: 运行失败测试**

Run:

```text
cargo test end_to_end_search_then_read_then_pdf
```

Expected: 在链路尚未装配完成前 FAIL；完成 Task 3 至 Task 7 后 PASS。

- [ ] **Step 3: 编写 README**

README 必须包括：

- 服务职责与非目标。
- `docker compose -f E:\Repositories\docker\services\searxng-service\docker-compose.yaml up --build -d` 的部署命令，并注明只应在有互联网权限的内网服务器执行。
- `/health`、`/v1/search`、`/v1/content` 的请求与响应示例。
- HTML、PDF、图片、扫描 PDF、无缓存和动态网页重读边界。
- 外网抓取限制与“仅反向代理/防火墙允许员工网段访问 refinery”的部署要求。
- 验证命令与联网验收场景。

- [ ] **Step 4: 运行完整本地验证**

Run:

```text
cargo test
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
docker compose -f E:\Repositories\docker\services\searxng-service\docker-compose.yaml config
```

Expected: Rust 测试、格式、Clippy 和 Compose 静态配置均通过。若 Cargo 因用户进程占用、权限或构建锁失败，保留具体命令和原始错误，不终止任何进程、不设置 `CARGO_TARGET_DIR`。

- [ ] **Step 5: 执行有外网权限服务器的联网验收**

在部署服务后，按设计文档的“Rust 网页正文抽取库调研”场景执行：

```text
POST /v1/search   query=Rust HTML 正文抽取库
POST /v1/content  读取候选仓库页面
POST /v1/content  读取该页面链接到的 crates.io、文档或许可证页面
```

记录验收日期、搜索结果 URL、HTML/PDF 读取是否成功、链接是否可继续访问，以及当日观察到的版本、下载量、Star 和抽取质量。动态数字只作为当日观察，不写入固定断言。

- [ ] **Step 6: 创建 Git 检查点**

用户提供 Git 仓库后，提交标题：`完成 refinery 搜索与内容读取服务`。

## 自检结果

- 规格覆盖：八个任务分别覆盖服务骨架、Compose 基线、搜索、SSRF/资源限制、抽取器选择、HTML、PDF/图片、测试与联网验收；没有未映射的规格要求。
- 占位符检查：计划没有未决标记、未指定的错误处理或未定义接口。
- 类型一致性：搜索使用 `SearchRequest/SearchResponse`；内容使用 `ContentRequest/ContentResponse`；所有路由均依赖同一 `AppState`；抓取层统一返回 `FetchedResponse`。
