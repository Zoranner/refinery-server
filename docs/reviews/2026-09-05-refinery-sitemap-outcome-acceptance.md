# Refinery 站点发现结果契约验收

## 验收日期

- 验收日期：2026-09-05
- 验收范围：当前 checkout 的 sitemap 状态模型、OpenAPI 契约、架构说明和本地 Rust 门禁。
- 未修改：sitemap 运行逻辑，以及 2026 年 9 月 4 日历史验收记录。

## 本地 Rust 门禁

以下命令均在本 checkout 执行：

| 命令 | 结果 |
| --- | --- |
| `cargo fmt --all` | 通过 |
| `cargo test --all-targets --all-features` | 通过，44 项测试通过 |
| `cargo clippy --all-targets --all-features -- -D warnings` | 通过，无警告 |
| `cargo build --release --locked` | 通过 |
| `git diff --check` | 通过 |

本地门禁只证明 Rust 源码、集成测试和契约文档在当前 checkout 的结果，不证明真实 Docker 编排、Reader 镜像行为、生产网络或公网 DNS rebinding 防护。

## Sitemap fixture 覆盖

| fixture 场景 | 观测结果 |
| --- | --- |
| discovered | robots.txt 声明 sitemap index，index 指向同源文档，返回同源 URL；`status=discovered`，来源为 `robots_txt=discovered`、`sitemap=discovered`。 |
| partial | robots.txt 获取失败但 sitemap.xml 返回同源 URL；仍返回 HTTP 200，`status=partial`，并记录 `source_failed` warning。 |
| empty | 所有来源均未发现 URL 时，响应保留 `status=empty`、各来源状态和空 `urls`；空结果不被解释为站点绝对无链接。 |
| failed | sitemap 来源失败且页面回退也失败时，响应仍为 HTTP 200，`status=failed`，并通过来源状态、`source_failed` 和 `fallback_failed` warning 暴露失败。 |
| timed_out | robots、sitemap 和页面回退均超时时，响应仍为 HTTP 200，`status=timed_out`，三个来源均为 `timed_out`，warning code 为 `source_timeout`。 |
| page-link fallback | robots 与 sitemap 未发现 URL 时，只调用请求页一次，保留同源 Markdown 链接并过滤外部链接；来源为 `page_links=discovered`。 |

来源失败不会中断后续有限回退；当前实现不把 sitemap 来源超时提升为 sitemap HTTP 504，也不声明 sitemap HTTP 413。

## OpenAPI 契约检查

- `SitemapStatus`、`SitemapSourceStatus`、`SitemapSources`、`SitemapWarning` 和 `SitemapResponse` 均有 schema。
- `SitemapResponse` 必须包含 `status`、`sources`、`warnings`，并保留请求地址、站点地址、URL 列表和截断事实。
- `SitemapWarning.code` 枚举包含运行时的 `source_failed`、`source_timeout` 和 `fallback_failed`。
- `POST /v1/sitemap` 的 HTTP 200 JSON schema `$ref` 为 `#/components/schemas/SitemapResponse`。
- `/v1/sitemap` 没有 413 或 504 response entry；内容、资源、搜索及统一 JSON 错误 schema 保持存在。
- OpenAPI 集成测试先按 TDD 观察到缺少 `SitemapStatus` schema 的失败，补充契约后通过。

## 活动引用扫描

沿 `tests/error_api.rs` 的扫描范围检查 `src`、`tests`、`README.md`、`docs/superpowers/specs` 和 `docs/superpowers/plans`，未发现已移除的旧仅下载错误 helper 或旧内容字段活动引用。`docs/reviews/2026-09-04-refinery-material-access-acceptance.md` 保留原线上证据，按测试约定排除在扫描之外。

## 仍存在的真实边界

- 未在本轮启动或验证真实 Docker、Reader 服务和 Reader 镜像版本。
- 未执行真实公网 sitemap、DNS rebinding、重定向链和部署侧出站防火墙验证。
- 未把 Reader 页面回退、真实站点超时和生产流量表现扩大解释为本地 fixture 之外的稳定性保证。
- 未修改或重写历史线上验收结论；历史记录中的旧内容响应、框架错误和资源边界仍按原日期事实保留。
