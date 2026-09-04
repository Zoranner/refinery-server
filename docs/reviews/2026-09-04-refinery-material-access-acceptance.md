# Refinery 资料访问链路验收

## 验收日期与部署地址

- 验收日期：2026 年 9 月 4 日
- 部署地址：`http://192.168.2.16:18090`
- 访问约束：本次仅调用 Refinery HTTP 接口，未直接访问 SearXNG、Reader、搜索引擎或浏览器自动化。

## 本地 Rust 门禁

以下命令按任务要求在当前 checkout 执行，均退出码 0：

| 命令 | 结果 |
| --- | --- |
| `cargo fmt --all` | 通过 |
| `cargo test --all-targets --all-features` | 通过，4 + 8 + 6 + 1 + 2 + 2 + 3 + 4 + 2 = 32 项测试通过 |
| `cargo clippy --all-targets --all-features -- -D warnings` | 通过，无警告 |
| `cargo build --release --locked` | 通过 |
| `git diff --check` | 通过 |

本地门禁不证明 Docker、Reader 镜像、DNS rebinding 防护或真实 PDF 抽取行为。

## 线上场景

耗时为本次请求从发出到收到完整 HTTP 响应的毫秒数；响应摘要保留与验收边界相关的字段。

| 场景 | 请求 | 结果 | 证据 | 边界 |
| --- | --- | --- | --- | --- |
| 健康检查 | `GET /health` | 200，368 ms | `{"status":"ok"}`，`Content-Type: application/json` | 仅证明服务可达 |
| OpenAPI | `GET /openapi.json` | 200，12 ms | `openapi=3.0.3`；路径为 `/health`、`/openapi.json`、`/v1/content`、`/v1/resource`、`/v1/search`、`/v1/sitemap` | 仅证明契约文档可取 |
| 英文搜索第 2 页 | `POST /v1/search`，`{"query":"Rust web scraping libraries","page":2,"limit":5}` | 200，2553 ms | 返回 GitHub 结果；响应为旧扁平字段 `query/page/results` | 200 不证明相关性；未返回 `pagination.has_more` |
| 中文搜索第 2 页 | `POST /v1/search`，`{"query":"网页正文抽取技术","page":2,"limit":5,"language":"zh-CN"}` | 200，327 ms | `results=[]` | 空页的 `has_more` 未知 |
| 无效搜索 limit | `POST /v1/search`，`{"query":"rust","limit":21}` | 400，125 ms | JSON `error.code=invalid_request` | 仅覆盖业务校验 |
| 静态 HTML | `POST /v1/content`，`{"url":"https://example.com/","max_chars":1000}` | 200，1108 ms | `resource_kind=html`，正文含 `Example Domain`，`next_offset=null` | 线上仍返回旧扁平响应；Reader 媒体类型为 `text/plain; charset=utf-8` |
| 动态 HTML | `POST /v1/content`，`{"url":"https://www.wikipedia.org/","max_chars":1000}` | 502，33449 ms | JSON `fetch_failed` | 上游不可用边界，不能宣称动态抽取成功 |
| PDF 内容 | `POST /v1/content`，`{"url":"https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf","max_chars":1000}` | 200，1529 ms | 标题 `Just a moment...`，正文含 security verification/403；`resource_kind=pdf` | Reader 挑战页应判定 `blocked`，当前部署仍作为普通正文返回；不证明 PDF 抽取成功 |
| 图片内容 | `POST /v1/content`，`{"url":"https://www.w3.org/Icons/w3c_home.png"}` | 415，2 ms | JSON `error.code=resource_download_required`，返回 `/v1/resource` | 当前线上仍用 415，而设计目标允许 `download_only` 成功响应 |
| 未知扩展名内容 | `POST /v1/content`，`{"url":"https://example.com/file.custom"}` | 415，3 ms | JSON `resource_download_required` 和资源 URL | 同上 |
| offset 续读 | `POST /v1/content`，`{"url":"https://example.com/","offset":5,"max_chars":1000}` | 200，504 ms | `offset=5`，`next_offset=null`，仍为旧扁平响应 | 未验证长文截断；线上响应未迁移到嵌套分页模型 |
| HTML 资源下载 | `GET /v1/resource?url=https%3A%2F%2Fexample.com%2F` | 200，1949 ms | `Content-Type: text/html`；`Content-Disposition: attachment`；`X-Content-Type-Options: nosniff` | 下载成功不证明可阅读 |
| PDF 资源下载 | `GET /v1/resource?url=https%3A%2F%2Fwww.w3.org%2FWAI%2FER%2Ftests%2Fxhtml%2Ftestfiles%2Fresources%2Fpdf%2Fdummy.pdf` | 200，479 ms | `Content-Type: application/pdf`；`Content-Disposition: attachment`；`nosniff`；13264 bytes | 下载成功不证明 PDF 抽取成功 |
| 有 sitemap 的站点 | `POST /v1/sitemap`，`{"url":"https://www.sitemaps.org/","limit":20}` | 200，2212 ms | 返回 20 个 `source=sitemap` URL，`truncated=true` | 仅证明发现和截断 |
| 无 sitemap 的站点 | `POST /v1/sitemap`，`{"url":"https://example.com/","limit":20}` | 200，1945 ms | `urls=[]`，`truncated=false`，`warnings=[]` | 空结果不证明站点绝对无链接 |
| 非 HTTP 协议 | `POST /v1/content`，`{"url":"ftp://example.com/file"}` | 403，2 ms | JSON `error.code=blocked_target` | 策略拦截 |
| 私有/回环地址 | content `http://127.0.0.1/admin`、resource 同 URL、sitemap `http://192.168.1.10/` | 均 403，2–3 ms | JSON `error.code=blocked_target` | 仅覆盖字面量目标，未宣称 DNS rebinding |
| malformed JSON | `POST /v1/sitemap`，请求体 `{` | 400，4 ms | `text/plain; charset=utf-8`，框架解析错误 | 未满足统一 JSON `invalid_request` |
| 缺失必填字段 | content/search/sitemap 分别发送 `{}` | 均 422，2–3 ms | `text/plain; charset=utf-8`，缺失 `url`/`query` | 未满足统一 JSON `invalid_request` |

## 未解决问题

- 目标服务器仍返回旧扁平内容/搜索响应，尚未与设计文档中的 `target`、`extraction`、`download`、`pagination` 嵌套模型线上收敛。
- Reader 返回 Cloudflare 挑战页时，当前 `/v1/content` 返回 200 普通正文，未按 `blocked` 状态表达。
- 图片和未知资源当前返回 415 `resource_download_required`，与目标模型中的 200 `download_only` 表达不一致。
- malformed JSON 和缺失字段由 Axum 框架返回 `text/plain` 的 400/422，未统一映射为 JSON `invalid_request`。
- 动态 HTML 场景本次返回 502 `fetch_failed`，不能据此判断动态抽取链路可用。

## 非声明事项

- 未对搜索结果相关性、来源质量或排序质量作保证。
- 未把空搜索页解释为没有更多结果。
- 未把 HTTP 200、资源下载成功或 Reader 返回正文解释为内容正确或 PDF 抽取成功。
- 未验证 Docker 编排、Reader 镜像版本、DNS rebinding、真实 PDF 文本提取、持久化缓存不存在或生产流量行为。
