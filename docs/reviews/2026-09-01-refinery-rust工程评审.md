# Refinery Rust 工程评审报告

## 评审范围

本次只评审 `refinery` 的 Rust 工程体系、异步服务边界、依赖与配置治理、质量门禁、部署契约和发布风险，不评价业务需求本身是否完整，也不把本地 mock 测试当作真实上游验收。

## 评审依据

- 当前 checkout 中的 `Cargo.toml`、`Cargo.lock`、`src/`、`tests/`、`Dockerfile`、`deploy/` 和 GitHub Actions 工作流。
- 当前项目 README、部署 README 和设计文档。
- 用户已确认的边界：员工内网免认证；SearXNG、Reader、Refinery 使用 Docker Compose；首版无持久化缓存；Reader 作为独立外部抽取服务。
- Rust 工程评审基线：`cargo fmt`、`cargo clippy`、测试、release 构建、异步资源边界、外部服务契约和发布前验证边界。
- Jina Reader 当前公开仓库 README 与 crawler API 源码。

## 总体结论

Rust 代码已经形成清晰的最小服务结构，搜索、内容读取、sitemap 和错误响应均有集成测试；本地 `cargo test --all-targets --all-features`、`cargo fmt --all -- --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo build --all-targets --all-features`、`cargo build --release --locked` 和 `cargo doc --all-features --no-deps` 均通过。

当前源码与发布工作流**可以进入发布流程**。Reader 集成和容器网络属于发布后的运行验收，不作为当前源码发布阻断；但这些事实仍必须保留，不能在发布前被表述为已验证：

1. `src/reader/client.rs` 的真实 Reader 请求契约没有被验证。代码发送 JSON 到 `/`，现有测试只验证自建 fake Reader；官方自托管示例展示的是向 `/` POST 表单字段 `url`，两者不能视为同一协议。
2. `AppState` 中共享 Reqwest 客户端缺少连接超时和总超时的问题已于 2026 年 9 月 2 日整改；当前统一使用 5 秒连接超时和 20 秒总超时，并有总超时回归测试。
3. 当前测试没有覆盖真实 Reader 镜像、真实 PDF、真实重定向、Reader 错误响应、Reader 大响应和容器网络路径；Compose 也未完成 Docker 运行时验证。

因此当前状态应定为：**源码本地验证通过，可以发布 `0.1.0` 候选；外部集成和容器运行验收待镜像发布后执行，不能提前表述为已通过**。

## 检查情况

已执行：

```text
cargo test --all-targets --all-features
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo build --release --locked
cargo doc --all-features --no-deps
```

结果：Rust 测试全部通过，格式检查通过，Clippy 无警告，debug/release 构建通过，文档生成通过。

已执行只读结构检查：

- `deploy/docker-compose.yml` 只有 `refinery` 映射宿主机端口。
- 三个服务均加入外部 `refinery` 网络。
- SearXNG 使用 `deploy/app/settings.yml` 只读挂载。
- 发布工作流包含 tag 版本门禁、Rust 验证、amd64/arm64 镜像、离线 tar、SHA-256 和 GitHub Release。

未执行或无法执行：

- 本机没有 Docker CLI，未执行 Compose config、镜像构建、容器启动、容器间 DNS 或真实 HTTP 联调。
- 本机没有 `actionlint`，GitHub Actions 语义校验需由 CI 运行。
- 未在有互联网权限的部署服务器拉取并固定 Jina Reader 镜像 digest，未执行真实 Reader HTML/PDF/SSRF 回归。

## 主要问题

### Reader 请求契约未完成真实验收

问题定性：发布后验收项。

证据：`src/reader/client.rs:20-29` 固定向 Reader 根路径发送 JSON 请求体 `{ "url": ... }`；`tests/content_api.rs` 的 fake Reader 也只接受这一自定义契约。真实上游仓库公开的自托管示例将 URL 作为 POST 表单字段发送，Reader API 同时还支持通过路径 URL 读取。当前测试没有证明 JSON body 会被实际镜像接受。citeturn0view0turn1view1

风险：发布后 `/v1/content` 可能全部返回 `fetch_failed`，本地测试仍会保持通过。

建议：镜像发布后，在目标部署服务器使用固定 Reader digest 做真实请求验收，覆盖 JSON/form/path 三种入口，确认响应媒体类型、最终 URL header、Markdown、PDF 和错误状态；将最终选定的上游请求写成真实 HTTP 集成测试或验收脚本。

### Reader 与 SearXNG 传输层超时已整改

问题定性：已关闭。

证据：`src/http.rs` 统一构造共享 Reqwest 客户端，配置 5 秒连接超时和 20 秒总超时；`src/state.rs` 的全部构造路径均使用该客户端。`src/http.rs` 的回归测试验证停滞响应会触发总超时。

剩余边界：`x-timeout: 20` 仍是 Reader 自身解释的业务请求选项，真实 Reader 对该选项的处理属于目标服务器联调范围；这不影响 `refinery` 自身 20 秒传输层总超时生效。

后续要求：保留现有超时回归测试，并在真实 Reader 联调中验证超时错误映射。

### 外部 Reader 与容器网络尚未完成安全验收

问题定性：发布后验收项。

证据：设计与 README 要求 Reader 固定 digest、验证私网重定向/DNS、超大响应和 PDF；当前 `deploy/docker-compose.yml` 仍使用 `ghcr.io/jina-ai/reader:oss` 标签，且本机无 Docker CLI，未执行容器出站网络、服务监听和真实联调。

风险：不能证明 Reader 的 SSRF 防护、端口监听、镜像内容、PDF 路径和网络隔离符合已批准设计。

建议：镜像发布后，在有互联网权限的部署服务器固定 digest，执行健康检查、`refinery -> reader:8081`、`refinery -> searxng:8888`、HTML/PDF、私网目标、重定向、大小和超时验收。

### 内网免认证入口缺少应用层并发和速率限制

问题定性：发布后运行风险。

证据：路由注册没有看到 tower 限流、并发 semaphore 或按来源地址的 middleware；`/v1/content` 每次都会触发 Reader 请求。设计文档已要求对内网来源设置并发和速率限制。

风险：单个内网客户端可以并发消耗 Reader、外网连接和 Tokio 任务，影响整个服务可用性。

建议：发布后根据实际内网并发和 Reader 资源占用情况增加全局并发上限、单来源速率限制和超限错误响应；限制值应是部署配置，不开放给普通调用方。

### 内容类型判定依赖请求 URL 后缀

问题定性：建议优化，当前已暴露为契约风险。

证据：`src/content/mod.rs:80-89` 仅依据请求 URL 是否以 `.pdf` 结尾决定 `content_kind`；Reader 返回的 `Content-Type` 没有参与 HTML/PDF/图片判定。

风险：无 `.pdf` 后缀但实际返回 PDF 的 URL 会被报告为 `html`；图片和其他资源也无法准确表达，模型可能误读内容类型。

建议：在真实 Reader 响应验收后，以响应头、Reader 元数据和 URL 后缀共同判定；补充无扩展名 PDF 与图片资源测试。

## 整改优先级建议

### 发布后优先验收

- 用固定 Reader digest 验证真实请求方法、请求体、响应媒体类型和最终 URL。
- 在有 Docker 的目标环境验证 Compose、容器 DNS、Reader 出站隔离和完整 HTML/PDF/SSRF 链路。
- 增加至少一个应用层并发上限，避免无认证内网入口耗尽外部抓取资源。

### 首次发布后可安排

- 依据真实 Reader 响应完善内容类型判定。
- 增加 request id、耗时和上游错误类别的结构化日志。
- 固定 SearXNG 镜像 tag/digest，避免 `latest` 造成部署漂移。
