# Refinery MCP Server 最终架构设计

## 目标

将 Refinery 重构为只对外提供 MCP 和健康检查的内网资料访问服务。MCP 使用 Streamable HTTP，免认证，不持久化缓存，工具名称固定为 `web_search`、`web_read`、`web_explore`、`web_download`。

## 结构

MCP 入口只负责工具与资源协议转换；应用服务层负责业务编排；现有 search、reader、sitemap、resource 和 material 模块继续承担领域与上游适配职责。MCP handler 不调用本地 HTTP API，避免协议回环。

```text
MCP /health -> application -> upstream adapters
```

最终对外只有 `/health` 和 MCP transport endpoint。删除 OpenAPI 与业务 `/v1/*` 路由及其旧协议模型耦合。

## 工具

- `web_search`：搜索公网资料，返回结果、分页和诊断。
- `web_read`：读取网页、文本和 PDF 的 Markdown，返回目标事实、内容、链接、分段和诊断。
- `web_explore`：有限发现 robots、sitemap 和同源页面链接，不深度爬取。
- `web_download`：创建无状态 MCP resource 引用；实际 `resources/read` 时重新校验并下载公网资源，限制 20 MiB，不保存文件。

所有工具继续使用现有公网 URL 校验、DNS 检查、重定向校验、超时和响应大小限制。错误映射为结构化 MCP tool error。

## 资源

`web_download` 返回 `refinery://resource/<url-encoded-target>` 的 resource URI 和元数据。MCP `resources/read` 解析 URI 后重新执行受控下载，返回 blob、媒体类型和最终 URL。服务不保存资源、不生成缓存、不引入令牌或过期状态。

## 配置与部署

保留 HTTP_LISTEN_ADDRESS、HTTP_LISTEN_PORT、SEARXNG_BASE_URL、READER_BASE_URL、RESOURCE_REQUEST_TIMEOUT_SECONDS、READER_REQUEST_TIMEOUT_SECONDS。监听地址用于 Streamable HTTP；健康检查继续使用 HTTP GET。部署只发布 Refinery 端口，Reader 和 SearXNG 保持 Docker 内网访问。

## 验证

增加 MCP 初始化、工具列表、四个工具调用、资源引用读取和错误映射测试；删除旧业务 HTTP 路由测试和 OpenAPI 契约测试。执行 cargo fmt --all、cargo test --all-targets --all-features、cargo clippy --all-targets --all-features -- -D warnings、cargo build --release --locked，并扫描旧 `/v1/*` 与 OpenAPI 引用。
