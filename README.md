# Refinery

Refinery 为没有互联网访问权限的员工电脑提供受控的外网资料读取接口。

- `POST /v1/search`：通过同一 Docker 网络内的 SearXNG 搜索公开网页。
- `POST /v1/content`：通过同一 Docker 网络内的 Jina Reader 读取单个公网网页、可提取的 PDF 或已知纯文本资源，并返回 Markdown 与当前分段链接。
- `POST /v1/sitemap`：读取 `robots.txt`、sitemap 和 sitemap index；当站点没有可用 sitemap 时，只读取请求页一次并返回其中的同源链接。
- `GET /v1/resource`：受控下载任意公网资源原文件。
- `GET /openapi.json`：返回供智能体和工具读取的 OpenAPI 3.0 契约。

Refinery 是唯一面向内网调用方的服务。SearXNG 和 Jina Reader 不发布宿主机端口。

## 接口

```text
GET /health
```

```json
POST /v1/search
{
  "query": "Rust HTML 正文抽取",
  "page": 1,
  "limit": 10,
  "language": "zh-CN"
}
```

```json
POST /v1/content
{
  "url": "https://example.com/article",
  "offset": 0,
  "max_chars": 12000
}
```

搜索成功响应包含结果、分页和诊断信息：

```json
{
  "query": "Rust HTML 正文抽取",
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

`has_more` 为 `null` 表示搜索上游没有提供可信的结束信号；空结果页不能直接解释为没有更多结果。

内容抽取成功响应按目标事实、抽取结果、下载能力、分段和诊断分组：

```json
{
  "target": {
    "requested_url": "https://example.test/article",
    "final_url": "https://example.test/article",
    "resource_kind": "html",
    "content_type": "text/html"
  },
  "extraction": {
    "status": "extracted",
    "engine": "reader_auto",
    "format": "markdown",
    "reason": null,
    "reader_content_type": "text/plain; charset=utf-8",
    "title": "页面标题",
    "markdown": "正文",
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

```json
POST /v1/sitemap
{
  "url": "https://example.com/docs/",
  "limit": 100
}
```

```text
GET /v1/resource?url=https%3A%2F%2Fexample.com%2Fdiagram.png
```

`/v1/content` 仅接受绝对 `http` 或 `https` URL；字面量回环、私有、链路本地、保留地址和其他非公网地址会被拒绝。无后缀和已知网页后缀标记为 `html`，`.txt`、`.md`、`.csv`、`.json`、`.xml`、`.yaml`、`.yml` 标记为 `text`，`.pdf` 标记为 `pdf`，以上资源均调用 Reader。已知图片后缀和未登记扩展名不调用 Reader，而是以 HTTP 200 返回 `extraction.status=download_only` 及对应的 `/v1/resource` 地址。

`/v1/resource` 可下载任意公网资源，不按媒体类型拒绝，响应体最大 20 MiB；返回原始 `Content-Type`，并设置 `Content-Disposition: attachment` 和 `X-Content-Type-Options: nosniff`。`/v1/sitemap` 不执行深度爬取、浏览器渲染或缓存，不读取已发现 URL 的正文。

`/v1/content` 的 `target.resource_kind` 表示调用方应采用的资源处理方式，优先依据明确的响应媒体类型并结合最终 URL 后缀判断；`target.content_type` 保留目标资源媒体类型。Reader 返回的 Markdown 响应通过 `extraction.reader_content_type` 表示，不能据此覆盖目标资源类型。`extraction.status` 可为 `extracted`、`empty`、`blocked`、`failed`、`timed_out` 或 `download_only`。

## 错误响应

业务错误统一返回 JSON：

```json
{
  "error": {
    "code": "invalid_request",
    "message": "..."
  }
}
```

当前路由使用 HTTP 400（请求无效）、403（目标被阻止）、413（资源响应过大）、502（上游失败）和 504（上游超时）。HTTP 415 仅为仍需支持的非内容业务保留；`/v1/content` 的图片和未知资源使用上述 HTTP 200 的 `download_only` 响应。

## 本地验证

```text
cargo test
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
```

## 发布

推送格式为 `vX.Y.Z` 的 Git tag 会触发 GitHub Actions 发布工作流。工作流要求 tag 与 `Cargo.toml` 的版本完全一致，并依次完成 Rust 验证、amd64/arm64 镜像构建、GHCR 多架构镜像发布、离线镜像 tar 与 SHA-256 生成、GitHub Release 创建。

镜像仓库按工作流所在 GitHub 仓库 owner 动态确定：

```text
ghcr.io/<repository-owner>/refinery:<version>
```

离线候选产物为：

```text
refinery-<version>-linux-amd64.tar
refinery-<version>-linux-amd64.tar.sha256
refinery-<version>-linux-arm64.tar
refinery-<version>-linux-arm64.tar.sha256
```

发布工作流只创建镜像和 Release，不部署服务。部署服务器仍需按下文的 Reader 镜像验收和网络策略要求单独执行 Compose 更新。

## 部署前置条件

仓库中的 `deploy/` 只提供部署模板，不是实际部署工作目录。部署负责人应将其复制到固定管理电脑上的独立目录，再在该目录准备 `.env` 并运行 Compose。模板同时部署 SearXNG、Reader 与 `refinery`，并使用外部 Docker 网络 `refinery`；镜像版本和宿主机端口直接写在 Compose 文件中，Refinery 环境变量从同目录 `.env` 读取：

```text
refinery -> searxng:8888
refinery -> reader:8081
```

只有 `refinery` 发布内网端口 `8080`。SearXNG 和 Reader 在容器内部监听 `0.0.0.0`，但没有 `ports` 配置，因此不会被宿主机或员工网段直接访问。具体配置与启动方式见 [deploy/README.md](deploy/README.md)。

部署目录中的 `.env.example` 需要复制为同目录 `.env`。其中包含 HTTP 监听地址/端口、SearXNG 和 Reader 内部地址，以及 Reader 抽取超时；实际 `.env` 不应提交到仓库。

当前 Compose 使用 `ghcr.io/jina-ai/reader:oss`。这不阻断 Refinery 源码构建和镜像发布；正式部署前可将其替换为已在目标联网服务器验收的不可变 digest。验收至少覆盖：

- HTML Markdown 与正文链接保留；
- 可提取 PDF 与扫描型 PDF；
- 公网重定向与私网重定向；
- 指向私网、回环、链路本地地址的 URL；
- 响应体超过服务上限；
- Reader 的错误响应和超时。

未完成上述验收前，不应在目标服务器启动对内服务。

## 网络与安全边界

Refinery 每次内容调用均重新请求上游，不保存网页、PDF、搜索结果或 sitemap。

Refinery 固定 Reader 的无缓存、Markdown、链接保留请求头；SearXNG 内部 HTTP 客户端使用 5 秒连接超时和 20 秒总超时，Reader 请求超时由 `READER_REQUEST_TIMEOUT_SECONDS` 配置，默认 60 秒。调用方不能透传 Cookie、认证信息、代理、浏览器或 Reader 专有配置。Reader 容器必须由部署环境的出站网络策略拒绝访问回环、私有、链路本地、保留和云元数据地址。应用层的 URL 与 DNS 检查是补充，不替代这条网络策略。
