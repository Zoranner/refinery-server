# Refinery

Refinery 是面向内网调用方的 MCP-only 网关，用于受控访问公开网页资料。当前用户接口是 Streamable HTTP MCP：

```text
POST /mcp
GET  /mcp
GET  /health
```

服务免认证，不提供 stdio、`/v1/*` HTTP API 或 `/openapi.json`。服务不持久化网页、搜索结果、站点发现结果或下载内容。上游继续使用同一 Docker 网络中的 Reader 与 SearXNG；网络出站限制仍由部署环境负责落实。

## MCP 工具

服务提供以下四个工具：

- `web_search(query, page=1, limit=10, language?)`：通过 SearXNG 搜索公开网页。`limit` 范围为 1–20。`pagination.has_more=null` 表示上游没有提供可信的结束信号，不能把它解释为已经结束；空结果页也不能单独证明没有更多结果。
- `web_read(url, offset=0, max_chars=12000)`：通过 Reader 读取网页、可提取 PDF 或已知纯文本资源。`max_chars` 范围为 1000–24000。成功结果保留 `target`、`extraction`、`download`、`pagination` 和 `diagnostics` 五个对象；`download.available=true` 表示服务具备资源读取能力，不包含 `resource_url`。
- `web_explore(url, limit=100)`：尽力发现站点地图和页面链接，`limit` 范围为 1–500。站点发现部分成功、空结果或超时都属于正常的结构化工具结果，应结合状态、来源和 warnings 判断是否需要人工复核。
- `web_download(url)`：只校验 URL 并生成资源 URI，不探测公网，也不声明资源 available、文件真实类型或大小已经验证。返回 `requested_url`、`resource_uri` 以及原生 MCP `resource_link`。

所有工具业务错误使用 `isError=true`，并在 `structuredContent.error` 中提供 `code` 和 `message`。MCP 协议错误由 SDK 处理。

## MCP 资源下载

`web_download` 返回的 `resource_uri` 形如：

```text
refinery://resource/<percent-encoded-url>
```

调用方必须通过 MCP `resources/read` 按 URI 读取资源。服务在实际读取时执行受控下载，单个响应体上限为 20 MiB，并返回实际 `mimeType`、base64 `blob`，以及 `_meta.final_url` 和 `_meta.size_bytes`。服务没有 HTTP 文件下载端点；对普通 HTTP URL 使用 `curl` 不能下载 MCP resource URI。调用方需要自行解码 base64 并保存文件。

## 运行边界

Refinery 仅允许受控的 URL 访问，并依赖部署环境限制 Reader 的出站网络。Reader 必须由网络策略阻断回环、私有、链路本地、保留地址和云元数据地址；应用层 URL/DNS 检查是补充。服务没有认证，也不支持 browser CORS；默认拒绝所有带 `Origin` 的浏览器请求，原生 MCP client 通常不发送 `Origin`。

通过 `MCP_ALLOWED_HOSTS` 配置 MCP 请求允许的 Host，值为逗号分隔列表，默认值为：

```text
localhost,127.0.0.1,[::1]
```

内网部署必须将实际主机 authority 加入列表，例如 `192.168.2.16:18090`。已有环境变量继续保留，具体配置见 `deploy/.env.example`。

## 部署与发布

`deploy/` 只提供部署模板，不是实际部署目录。模板编排 Refinery、SearXNG 和 Reader，使用外部 Docker 网络 `refinery`，只向内网发布 Refinery 的 `18090:8080`；SearXNG 和 Reader 不发布宿主机端口。实际部署应将模板复制到独立目录，在该目录准备 `.env`，并按 [deploy/README.md](deploy/README.md) 操作。

模板固定的 `ghcr.io/zoranner/refinery:0.2.0` 是本版 MCP-only 发布镜像。`0.1.x` 及更早的镜像是旧 HTTP 接口，不能按本文的调用方式使用。离线或本地试用时，将 Compose 中的 `image` 替换为实际导入或构建的 tag；本文不执行构建或部署。

发布工作流负责 tag 与 Cargo 版本校验、Rust 门禁、多架构 GHCR 镜像、离线 tar、SHA-256 和 GitHub Release。工作流只创建镜像和 Release，不部署服务。正式部署前仍需在目标环境验收 MCP `/mcp` 调用、`/health`、Reader 固定镜像的 HTML/PDF、重定向、SSRF、超大响应、错误和超时，以及 SearXNG 与实际网络出站策略。当前文档不把这些边界写成已通过验证或生产可用。
