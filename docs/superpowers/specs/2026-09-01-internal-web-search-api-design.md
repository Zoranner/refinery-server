# 内网外网搜索与内容读取 API 设计

## 目标

公司员工电脑不能访问互联网，而内网服务器具备互联网访问权限。该服务部署在内网服务器上，为内部应用提供两个能力：

- 根据关键词搜索公开互联网资料。
- 读取任意公网网页或 PDF 的可用文本内容，并保留可继续访问的链接关系。

服务只对内网开放，不做用户身份认证。访问范围由网络策略和反向代理限制到员工网段。

## 范围

首版提供：

- 基于现有 SearXNG 的网页搜索。
- 任意 `http` 或 `https` 公网 URL 的内容读取。
- HTML 正文抽取为 Markdown。
- PDF 文本提取。
- 正文链接、PDF 附件和图片链接的保留与类型标记。
- 任意原始资源的受控下载。
- 长内容的无状态分段读取。
- 受限的网站地图发现：读取 `robots.txt` 声明的 sitemap、常规 sitemap 地址及 sitemap index，不执行整站爬取。
- 抓取目标、重定向和响应体的安全限制。

首版不提供：

- 持久化缓存、数据库、网页内容归档或检索索引。
- 账号认证、权限模型和审计查询界面。
- JavaScript 浏览器渲染。
- 图片 OCR、图片视觉识别和 Office 文档解析。
- GitHub、crates.io 或其他网站的专用适配接口。

网页内容的比较、归纳和结论由调用方完成。服务只负责搜索、读取和保留来源链接。

首版不做持久化缓存。每次内容读取均以当次外网响应为准。

## 部署结构

服务在本仓 `deploy/` 的 Docker Compose 中部署 SearXNG、Jina Reader 和 `refinery` 三个容器，并使用外部 Docker 网络 `refinery`。

```text
内网调用方
    |
    v
refinery:8080
    |                |
    v                v
searxng:8888      reader:8081
    |                |
    v                v
搜索引擎          公开互联网网站
```

- `refinery` 是唯一向内网发布端口的服务。
- `searxng` 与 `reader` 仅加入 Docker 网络，不直接向员工网络发布端口。
- `refinery` 通过容器名 `searxng:8888` 调用 SearXNG 的 JSON 搜索接口。
- `refinery` 通过容器名 `reader:8081` 调用 Jina Reader 的 HTTP/1.1 接口，固定请求 Markdown 输出、关闭缓存并保留链接。
- `reader` 在容器内监听 `0.0.0.0:8081`，但 Compose 不配置 `ports`；员工访问 `refinery` 不使用认证。
- 三个服务使用外部 Docker 网络 `refinery`；SearXNG 与 Reader 都不发布宿主机端口。部署前需要执行 `docker network create refinery`，该网络只需创建一次。
- Jina Reader 固定为 Apache-2.0 开源分支的具体镜像 digest。首版只使用其单 URL 抽取能力，不使用其搜索、代理、缓存、截图、Office 文档、浏览器渲染或 SaaS 专有能力。

现有 Compose 在实施前必须完成两项基线修正：

- 挂载路径应指向实际存在的 `./app/settings.yml`，而不是不存在的 `./app/searxng/settings.yml`。
- SearXNG 的 `bind_address` 应从 `127.0.0.1` 改为 `0.0.0.0`，使同一 Docker 网络中的 API 容器可以访问它。

## HTTP 接口

### 健康检查

```text
GET /health
```

服务可接收请求时返回：

```json
{
  "status": "ok"
}
```

该接口不探测外网搜索引擎，也不触发网页抓取。

### 搜索

```text
POST /v1/search
```

请求：

```json
{
  "query": "Rust HTML 正文抽取库",
  "page": 1,
  "limit": 10,
  "language": "zh-CN"
}
```

- `query` 必填，不能为空。
- `page` 默认 `1`。
- `limit` 默认 `10`，最大 `20`。
- `language` 可选，原样传递为 SearXNG 的搜索语言参数。

响应：

```json
{
  "query": "Rust HTML 正文抽取库",
  "page": 1,
  "results": [],
  "pagination": {
    "requested_page": 1,
    "has_more": null
  },
  "diagnostics": {
    "source_status": "ok",
    "warnings": []
  }
}
```

`pagination.has_more=null` 表示上游没有提供可信的分页结束信号；空结果页不能证明已经完成。

搜索结果中的 `url` 是后续内容读取的直接输入。服务不维护搜索会话，也不签发或保存搜索结果状态。

### 内容读取

```text
POST /v1/content
```

请求：

```json
{
  "url": "https://example.com/article",
  "offset": 0,
  "max_chars": 12000
}
```

- `url` 必填，必须为绝对 `http` 或 `https` URL。
- `offset` 默认 `0`，表示已抽取文本中的起始字符位置。
- `max_chars` 默认 `12000`，范围为 `1000` 至 `24000`。
- 每次调用均重新抓取并重新抽取目标内容；`offset` 不是服务端会话或缓存令牌。

网页、纯文本或 PDF 的响应：

```json
{
  "target": {
    "requested_url": "https://example.com/article",
    "final_url": "https://example.com/article",
    "resource_kind": "html",
    "content_type": "text/html"
  },
  "extraction": {
    "status": "extracted",
    "engine": "reader_auto",
    "format": "markdown",
    "reader_content_type": "text/plain; charset=utf-8",
    "title": "页面标题",
    "markdown": "正文……[相关 PDF](https://example.com/report.pdf)",
    "links": [
      {
        "text": "相关 PDF",
        "url": "https://example.com/report.pdf",
        "kind": "pdf"
      }
    ]
  },
  "download": {
    "available": true,
    "resource_url": "/v1/resource?url=..."
  },
  "pagination": {
    "offset": 0,
    "next_offset": 12000,
    "truncated": true
  },
  "diagnostics": {
    "upstream_status": null,
    "duration_ms": null,
    "timeout_seconds": null,
    "warnings": []
  }
}
```

- 成功的文本抽取响应中，`target.resource_kind` 为 `html`、`text` 或 `pdf`。
- `extraction.markdown` 仅包含当前分段；链接保留为绝对 URL 的 Markdown 链接。
- `extraction.links` 仅列出当前分段正文中出现的链接，避免返回整页导航、页脚和站点目录。
- `kind` 根据目标 URL、链接上下文和可识别的媒体类型标记为 `html`、`pdf`、`image` 或 `unknown`。
- `next_offset` 为 `null` 时表示没有更多已抽取文本。
- 因网页动态变化而导致下一次重新抓取的内容与上次不同，是无缓存设计的已知边界。

`/v1/content` 先按 URL 路径后缀决定是否属于文本抽取对象，不为了补齐类型额外发起资源探测请求：

| URL 形式 | `resource_kind` | `/v1/content` 行为 |
|---|---|---|
| 无后缀、`.html`、`.htm`、`.xhtml`、`.php`、`.asp`、`.aspx`、`.jsp` | `html` | 调用 Reader 抽取 Markdown。 |
| `.txt`、`.md`、`.csv`、`.json`、`.xml`、`.yaml`、`.yml` | `text` | 调用 Reader 返回可读文本。 |
| `.pdf` | `pdf` | 调用 Reader 提取文本。 |
| 已知图片后缀 | `image` | 不调用 Reader，返回 HTTP 200 的 `download_only` 结果。 |
| 未登记的扩展名 | `unknown` | 不调用 Reader，返回 HTTP 200 的 `download_only` 结果。 |

已知图片后缀包括 `.avif`、`.bmp`、`.gif`、`.ico`、`.jpeg`、`.jpg`、`.png`、`.svg`、`.tif`、`.tiff`、`.webp`。

图片和 PDF 的文本读取与原始下载分开处理。`/v1/content` 只返回可供模型阅读的文本，不直接返回二进制；HTML 页面中可识别的图片和 PDF 链接随正文链接一起返回。扫描型 PDF 或 Reader 返回空正文时，成功响应以 `extraction.status=empty` 表达并保留下载能力。Reader 抽取失败或超时由当前路由分别映射为 HTTP 502 `fetch_failed` 或 HTTP 504 `fetch_timeout`，不会发出带 `extraction.status=failed` 或 `timed_out` 的成功响应；这两个 `MaterialStatus` 值目前只保留在内部模型和契约枚举中。

### 原始资源下载

```text
GET /v1/resource?url=<encoded-public-url>
```

该接口允许下载任意公网资源：

- URL 必须经过与内容读取相同的协议、DNS、重定向和公网地址校验。
- 响应体默认最大为 20 MiB，读取过程中超过上限立即中止。
- 在 20 MiB 上限内返回原始字节，不转 Base64，不写入持久化缓存。
- 保留上游 `Content-Type`，并统一设置 `Content-Disposition: attachment` 和 `X-Content-Type-Options: nosniff`；不转发上游 Cookie、认证头或其他用户凭据。
- 图片、PDF、Office 文档、音视频、压缩包、可执行文件和未知二进制均可下载；资源下载不执行、不解压、不预览、不解析其中内容。

HTML Markdown 中的外部资源链接可以由调用方改写为对应的 `/v1/resource` 地址，使没有互联网访问权限的员工客户端仍能下载资源。资源下载不承担 OCR、视觉理解、PDF 文本提取、解压或预览职责。

当 `/v1/content` 接收到已知图片后缀或未知后缀时，响应为 HTTP 200：

```json
{
  "target": {
    "requested_url": "https://example.com/diagram.png",
    "final_url": "https://example.com/diagram.png",
    "resource_kind": "image",
    "content_type": "image/png"
  },
  "extraction": {
    "status": "download_only",
    "engine": "none",
    "format": "binary",
    "reason": "该资源不支持文本抽取，请通过资源下载接口获取原文件",
    "title": null,
    "markdown": "",
    "links": []
  },
  "download": {
    "available": true,
    "resource_url": "/v1/resource?url=..."
  },
  "pagination": {
    "offset": 0,
    "next_offset": null,
    "truncated": false
  },
  "diagnostics": {
    "upstream_status": null,
    "duration_ms": null,
    "timeout_seconds": null,
    "warnings": []
  }
}
```

`/v1/resource` 不按 `Content-Type` 决定是否允许下载；该字段只作为返回给客户端的原始资源元数据。

### 网站地图

```text
POST /v1/sitemap
```

请求：

```json
{
  "url": "https://example.com/docs/",
  "limit": 100
}
```

- `url` 必填，必须为绝对 `http` 或 `https` URL。
- `limit` 默认 `100`，范围为 `1` 至 `500`。
- 服务只发现同一站点的 URL，不抓取这些 URL 的正文，也不执行 JavaScript。

响应：

```json
{
  "requested_url": "https://example.com/docs/",
  "site_url": "https://example.com/",
  "urls": [
    {
      "url": "https://example.com/docs/getting-started",
      "source": "sitemap"
    }
  ],
  "truncated": false,
  "warnings": []
}
```

发现顺序为：

1. 读取 `robots.txt` 中的 `Sitemap:` 声明。
2. 当没有可用声明时，尝试站点根路径的 `/sitemap.xml`。
3. 递归解析 sitemap index，受 sitemap 文档数量和响应大小限制。
4. 当 sitemap 未发现 URL 时，解析请求页面正文中的同源链接作为有限回退，最多返回 `limit` 项。

该接口不是深度爬取器：不读取发现 URL 的正文、不进入二级页面、不使用浏览器、不保存任务、缓存或站点索引。
robots.txt 和 sitemap 文档必须通过与内容接口相同的公网 URL 校验、DNS 解析检查、重定向检查、超时与大小限制；不得使用未受限的 HTTP 客户端作为网站地图下载通道。

## 内容处理链路

```text
输入 URL
  -> 协议与主机校验
  -> DNS 解析与目标地址校验
  -> 根据 URL 后缀分类
  -> 文本对象：调用 Jina Reader 抽取并分段
  -> 图片或未知对象：返回 download_only 与 /v1/resource 下载指引
```

`refinery` 先执行自身的 URL 策略和路径分类。只有 `html`、`text`、`pdf` 会交给 Jina Reader；`refinery` 通过内部 HTTP POST 调用 Reader，Reader 对公网目标执行下载和抽取。Reader 返回的 Markdown 是 HTML、纯文本和可提取 PDF 的权威内容；`refinery` 不返回原始 HTML、脚本、样式或页面布局，并把保留下来的相对链接按最终 URL 转为绝对 URL。

PDF 支持必须以固定 PDF 样本验证 Reader 当前固定镜像的实际输出后才纳入；抽取为空时返回带 `target.resource_kind=pdf` 的成功响应并将状态设为 `empty`；抽取失败或超时则返回 HTTP 502/504 错误体，不伪装为成功响应。首版不识别扫描件中的图片文字，也不尝试恢复复杂版式。

图片仅作为可发现资源保留，首版不做视觉内容理解。若后续出现明确需求，再增加独立的 OCR 或视觉处理能力。

## 抓取安全与资源边界

内容接口允许任意公网 URL，但 `refinery` 与 Reader 组合必须执行以下限制：

- 仅接受 `http` 和 `https`。
- 每次首次连接和每次重定向后都解析主机地址，拒绝回环、私有、链路本地、未指定、多播、保留和其他非公网 IP 地址。
- 不发送调用方 Cookie、认证头或其他用户凭据。`refinery` 对外网资源、robots.txt 和 sitemap 只使用 HTTP `GET`；`refinery` 到 Reader 的内部调用使用固定 HTTP `POST`。`refinery` 只向 Reader 发送固定的 Markdown、无缓存、保留链接和超时请求头，绝不接受调用方透传的 Reader 请求头。
- 最多跟随 5 次重定向。
- 连接超时为 5 秒，单次请求总超时为 20 秒。
- `refinery` 对 Reader 响应执行 10 MiB 上限和 20 秒总超时；Reader 对外网原始响应的大小、超时和重定向限制必须以固定镜像的实测结果记录。若 Reader 无法满足安全验证，不能上线该镜像。
- `/v1/content` 只对 HTML、已知纯文本和 PDF 调用 Reader；图片与未知扩展名返回 HTTP 200 的 `download_only` 结果。`/v1/resource` 可以下载任意媒体类型，不按类型拒绝。
- 对每个内网源 IP 设置并发和速率限制，避免无认证服务被内部滥用。
- 日志只记录脱敏后的 URL 主机、状态、耗时、字节数和错误类别；不记录正文和完整查询参数。

网络地址策略应在真正建立连接时生效，而不只检查 URL 字符串或首次 DNS 结果，以避免重定向和域名解析变化绕过限制。Reader 容器的出站网络策略也必须拒绝访问内网保留地址，不能只依赖 `refinery` 的前置校验。

## 错误响应

所有错误响应采用：

```json
{
  "error": {
    "code": "blocked_target",
    "message": "目标地址不允许访问"
  }
}
```

| 状态码 | `code` | 含义 |
|---|---|---|
| 400 | `invalid_request` | 请求字段缺失、格式错误或 URL 不是 HTTP/HTTPS。 |
| 403 | `blocked_target` | 目标或重定向目标不符合公网地址策略。 |
| 413 | `response_too_large` | `/v1/content` 或 `/v1/resource` 的响应体超过配置上限；`/v1/sitemap` 当前吞并抓取错误或转为 warning，不对外发出 413。 |
| 502 | `search_upstream_failed` | SearXNG 不可用或返回无效搜索结果。 |
| 502 | `fetch_failed` | 外部站点连接、TLS 或响应协议失败。 |
| 504 | `fetch_timeout` | 外部站点在时限内未完成响应。 |

HTTP 415 仅为仍需支持的非内容业务保留。框架参数错误也统一映射为 400 JSON，不暴露 `422 text/plain`。

错误不泄露内部 IP、容器地址、完整上游响应或调用栈。

## Rust 实现边界

API 服务使用 Rust 实现，职责保持为：

- HTTP API 与输入输出校验。
- SearXNG JSON 客户端。
- Jina Reader HTTP 客户端与固定请求头注入。
- URL 前置校验、内容响应规范化、分段、链接标准化和错误映射。
- Markdown 分段和链接标准化。

Jina Reader 以未修改的独立 Docker 服务运行，承担 HTML 与可提取 PDF 的正文抽取；`refinery` 不嵌入其代码。Reader 是 Node.js 服务，这一语言边界仅存在于独立的外部抽取容器；`refinery` 仍是 Rust 服务，用户指定保留的 SearXNG 是独立既有搜索服务。

Reader 必须固定到明确的上游镜像 digest，并通过健康检查、HTML 链接保留、PDF 内容、响应大小、重定向目标、SSRF 和错误语义验证后才能进入部署 Compose。`refinery` 运行时只调用该固定 Reader 服务，不维护 Spider、Webclaw 或其他并行网页抽取器。

## 验证

### 可重复自动验证

- 使用本地 HTTP 测试服务器模拟 SearXNG JSON 响应。
- 使用固定 HTML 样本验证正文抽取、Markdown 链接、相对链接补全、长内容分段和图片 `alt` 保留。
- 使用固定 PDF 样本验证可提取文本、空文本扫描 PDF 和分段读取。
- 使用本地地址、重定向链、超大响应和错误媒体类型验证抓取安全策略。
- 使用 Reader 当前固定镜像，加上本项目固定中文、表格、相对链接、复杂网页与 PDF 样本，验证抽取质量、链接保留与安全边界。

### 联网验收场景

在具备互联网权限的内网服务器上执行“Rust 网页正文抽取库调研”：

1. 以“Rust HTML 正文抽取库”等关键词调用搜索接口。
2. 读取候选项目的 GitHub 仓库页。
3. 沿页面链接读取 crates.io、文档和许可证页面。
4. 由调用方基于返回内容比较功能、版本、下载量、GitHub Star、维护活跃度和限制。

该场景验证搜索、复杂网页正文抽取、跨页面跳转、链接保留和长内容读取的完整主链路。下载量、Star 等动态数据只记录验收当天的观察结果，不作为固定自动化断言。

## 非目标和后续扩展

出现明确业务需求后，才考虑以下扩展：

- 有时效边界的内存或持久化缓存。
- JavaScript 渲染兜底。
- 图片 OCR 和视觉内容提取。
- Office 文档内容提取。
- 站点专用解析器。
- 认证、访问审计和更细粒度的网络策略。

## 发布

`refinery` 使用 tag 驱动的 GitHub Actions 发布工作流。只有格式为 `vX.Y.Z`、且与 `Cargo.toml` 版本严格一致的 tag 可以进入发布。

发布顺序为：

1. 执行 Rust 格式、Clippy 和完整测试。
2. 分别构建 linux/amd64 与 linux/arm64 镜像，并推送为 GHCR digest。
3. 导出每个平台的离线 Docker tar 和 SHA-256。
4. 合并多架构版本 tag。
5. 创建包含离线镜像与校验文件的 GitHub Release。

工作流只发布 `refinery` 镜像，不部署 Compose，不拉取或更新 Jina Reader，也不改变任何内网服务器状态。
