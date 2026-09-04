# Refinery 资料访问架构设计

## 设计背景

截至 2026 年 9 月 4 日，线上 Refinery 已能完成以下链路：

- SearXNG 搜索接口正常返回；
- 静态网页和动态网页可以通过 Jina Reader 抽取 Markdown；
- `auto` 与 `visible-content` 能显著缩短部分动态网页的返回时间；
- 原始 HTML、PDF 和未知资源可以下载；
- 公网地址校验、重定向限制和内网目标拦截有效。

上线验收同时暴露出几个结构性问题：

- 搜索 HTTP 成功不代表结果相关；
- `page=2` 返回空列表时，调用方无法判断是确实没有更多结果还是上游分页失效；
- PDF 目标可能返回 Cloudflare 挑战页，却被当作正常抽取结果；
- 原始资源可以下载，但正文不一定可抽取；
- Reader 的响应媒体类型是 `text/plain`，不能直接代表目标资源类型；
- `resource_kind`、`content_kind` 和 `content_type` 的职责边界容易混淆；
- 缺失字段的框架错误与业务错误没有统一响应结构。

这些问题不是增加若干错误码或继续延长超时可以根治的，需要重新定义资料访问的状态模型和职责边界。

## 目标与非目标

### 目标

- 为搜索、获取、抽取、下载建立清晰的职责边界；
- 区分目标资源事实、上游获取事实和抽取结果事实；
- 让智能体能够判断“可阅读、可下载、被拦截、失败、没有更多结果”等状态；
- 让超时、大小限制、SSRF 和重定向策略具有明确的责任归属；
- 保持无数据库、无持久化缓存和无任务队列的首版边界；
- 允许继续使用 SearXNG 和独立 Jina Reader，不在 Refinery 内嵌新的网页引擎。

### 非目标

- 不在本阶段实现站点专用适配器；
- 不在 Refinery 内重新实现 Curl 与 Browser 的自适应判定；
- 不引入持久化抓取任务、索引、缓存或后台队列；
- 不把搜索结果相关性伪装成固定质量保证；
- 不为每个网站建立独立数据模型。

## 分层职责

```text
调用方请求
    |
    v
策略层：协议、目标地址、预算、大小和重定向限制
    |
    +--> 搜索发现：SearXNG
    |
    +--> 目标获取：资源下载或 Reader 请求
                |
                v
        抽取编排：Reader 的 auto/curl/browser/pdf 能力
                |
                v
        结果判定：可阅读、仅可下载、被拦截、失败
                |
                v
        统一响应：事实、内容、链接、状态和诊断
```

Refinery 负责策略、编排、结果规范化和错误边界；SearXNG 负责搜索发现；Reader 负责网页/PDF 等目标的内容抽取；资源下载器负责原始字节传输。任何一层都不复制另一层的核心职责。

## 核心模型

### 目标事实

目标事实描述调用方要求访问的对象，不描述 Reader 是否成功：

```json
{
  "requested_url": "https://example.com/report.pdf",
  "final_url": "https://example.com/report.pdf",
  "resource_kind": "pdf",
  "content_type": "application/pdf"
}
```

`resource_kind` 由 URL 后缀、最终 URL 和可确认的响应媒体类型共同推导，只表示目标资源的处理类别：

- `html`
- `text`
- `pdf`
- `image`
- `unknown`

Reader 返回的 `text/plain` 或 Markdown 不能覆盖这个事实。

### 抽取结果

抽取结果描述是否形成了可供模型阅读的内容：

```json
{
  "status": "extracted",
  "engine": "reader_auto",
  "format": "markdown",
  "title": "页面标题",
  "markdown": "正文……",
  "links": []
}
```

抽取状态至少包括：

- `extracted`：形成了可阅读正文；
- `empty`：请求成功但没有有效正文；
- `blocked`：上游返回挑战页、拒绝页或明确访问阻断；
- `failed`：抽取器失败、协议错误或格式错误；
- `timed_out`：超过抽取预算。

`engine` 是观察事实，不是调用方可以伪造的目标类型。首版由 Reader 的 `auto` 负责 Curl/Browser 选择，Refinery 只固定请求策略，不复制其内部判断。

### 下载能力

下载能力独立于抽取结果：

```json
{
  "available": true,
  "resource_url": "/v1/resource?url=..."
}
```

因此可以表达：

- `extracted + available`：正文可读，也可下载原件；
- `blocked + available`：正文被目标站点拦截，但原件仍可尝试下载；
- `failed + available`：抽取失败，但资源下载仍有意义；
- `download_only`：图片、未知扩展名或不支持的二进制对象。

### 诊断信息

诊断信息只记录机器可用的有限事实：

```json
{
  "upstream_status": 403,
  "duration_ms": 183000,
  "timeout_seconds": 180,
  "warnings": [
    "upstream_challenge_page"
  ]
}
```

不得返回内部容器地址、DNS 解析细节、完整异常栈、Cookie、认证信息或正文之外的敏感上游响应。

## 统一内容响应

`POST /v1/content` 的成功响应应围绕目标、抽取和下载三个部分组织，而不是让一个字段同时承担三种含义：

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
    "title": "页面标题",
    "markdown": "正文……",
    "links": []
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
    "warnings": []
  }
}
```

这是目标契约，不要求一次性保持旧字段形状。兼容迁移时可以短期提供旧字段，但旧字段必须由新模型单向投影，不能形成两套独立事实来源。

对于图片、未知扩展名或明确不可抽取资源，响应应表达 `download.available=true` 与 `extraction.status=download_only`，而不是把“没有正文”压缩成普通 HTTP 失败。

对于 Reader 返回挑战页的情况，目标类型仍可为 `pdf` 或 `html`，但抽取状态应为 `blocked`，并保留下载能力和可诊断 warning。

## 搜索模型

搜索是发现能力，不是内容质量保证。`POST /v1/search` 应明确返回：

```json
{
  "query": "Rust web scraping libraries",
  "page": 1,
  "results": [],
  "pagination": {
    "requested_page": 1,
    "has_more": null,
    "source_status": "ok"
  },
  "diagnostics": {
    "warnings": []
  }
}
```

`has_more=null` 表示上游未提供可信的分页结束信息，不能把空结果直接解释为“没有更多结果”。搜索源质量属于 SearXNG 配置和来源策略；Refinery 只负责保留排序、来源和上游状态，不对相关性作无证据承诺。

后续如需提升资料调研质量，应在搜索层引入可审查的来源策略（例如来源白名单、域名限定或搜索配置档），而不是继续向全局配置堆叠引擎。

## 超时与预算

每次外部操作使用独立预算，并由请求总预算约束：

```text
请求总预算
├─ SearXNG 搜索预算
├─ 原始资源下载预算
├─ Reader 抽取预算
│  ├─ Reader 业务上限：180 秒
│  └─ Refinery 传输预算：略大于 Reader 上限
└─ 响应序列化与传输余量
```

- Reader 的 `x-timeout` 不超过官方 180 秒；
- `x-engine: auto` 和 `x-respond-timing: visible-content` 由 Refinery 固定注入；
- Refinery 不把 300 秒直接转发给 Reader；
- 资源下载和 Reader 抽取使用不同预算，不用一个全局常数掩盖责任差异；
- 超时结果必须携带明确的阶段信息，区分“目标连接超时”“浏览器抽取超时”和“Refinery 等待上限”。

## 安全与失败语义

安全策略仍由 Refinery 在首次连接、重定向和 DNS 解析处执行，Reader 容器的出站策略作为第二道边界。

失败判断按因果顺序分层：

1. 目标地址不符合策略：`blocked_target`；
2. 原始请求失败：`fetch_failed` 或 `fetch_timeout`；
3. 上游明确拒绝或返回挑战页：`upstream_blocked`；
4. 获取成功但正文为空：`content_empty`；
5. Reader 无法生成可读结果：`content_not_extractable`；
6. 目标只能下载：`download_only`。

这些状态不能都压缩为 `fetch_failed`，也不能用 HTTP 200 掩盖正文不可用。

所有框架层参数错误也应经过统一 JSON 错误映射，避免业务错误返回 JSON、反序列化错误却返回 `text/plain`。

## 迁移原则

迁移必须先建立新模型，再移除冲突字段：

1. 先实现内部 `TargetFacts`、`ExtractionResult`、`DownloadCapability` 和 `Diagnostics`；
2. 让内容、资源、sitemap 和搜索客户端分别产出自己的事实；
3. 由统一编排层组合结果；
4. 由响应适配层生成 OpenAPI 契约；
5. 更新智能体使用文档和集成测试；
6. 确认调用方已使用新响应后，再删除旧的平面字段。

不允许通过同时保留 `content_kind` 和 `resource_kind`、同时维护两套响应结构或在路由层追加特殊例外来维持不一致。

## 验收标准

- 静态 HTML、动态 HTML、纯文本、可提取 PDF、被拦截 PDF、图片和未知资源都能表达不同状态；
- 调用方能区分可阅读、可下载、仅下载、被拦截、为空和超时；
- `resource_kind` 不再被 Reader 的 `content_type` 覆盖；
- Reader 的 `auto`、Curl/Browser 选择仍由 Reader 负责；
- 搜索空页不会被错误解释为分页结束；
- 所有错误响应均为统一 JSON；
- SSRF、重定向、大小限制和无持久化边界保持不变；
- OpenAPI、README、测试和实际线上 JSON 结构一致。

## 当前不变项

- 服务名称仍为 `refinery`；
- 内网调用不做用户认证；
- Compose 仍包含 SearXNG、Reader 和 Refinery；
- 只有 Refinery 发布宿主机端口；
- Reader 仍作为独立服务运行；
- 首版不增加数据库、缓存、任务队列或站点专用适配器。

## 验收边界记录

2026 年 9 月 4 日已完成本地 Rust 门禁和目标服务器 HTTP 场景验收。`TargetFacts`、`ExtractionResult`、`DownloadCapability`、统一错误码、搜索分页未知态以及资源下载安全响应均有本地测试覆盖；目标服务器已验证健康检查、OpenAPI 路径、搜索、内容、资源、sitemap、SSRF 拦截和响应头边界。

验收结论仅覆盖实际观测到的事实：HTTP 200 只证明传输成功；空搜索页的 `has_more` 未知；资源下载成功不证明正文抽取成功；Reader 挑战页、上游超时或不可用均按线上边界记录。目标服务器当前仍返回旧扁平内容响应，且 malformed JSON/缺失字段存在 `text/plain` 框架错误，因此嵌套资料模型和统一 `invalid_request` 响应尚未完成线上收敛。
