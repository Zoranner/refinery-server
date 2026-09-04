# Task 2 实现报告：统一内容获取与抽取判定

## 修改文件

- `src/material/extraction.rs`
  - 新增 Reader 结果归一化、挑战页判定、空正文判定、下载能力和分页组装。
- `src/material/model.rs`
  - 为 `ExtractionResult` 增加 `title`、`markdown`、`links`，使统一响应能承载正文内容。
- `src/material/mod.rs`
  - 导出归一化入口及路由内部使用的构造函数。
- `src/routes/content.rs`
  - `/v1/content` 改为返回 `MaterialContentResponse`。
  - 图片和未知扩展名不调用 Reader，直接返回 HTTP 200 的 `download_only`。
- `src/content/mod.rs`
  - 移除旧平面 `ContentResponse` 及其组装函数；保留 URL 校验、资源分类、分块和链接模块。
- `tests/content_api.rs`
  - 更新嵌套响应断言，新增挑战页、下载专用和空正文覆盖。

`src/reader/client.rs` 未需行为修改；现有固定 Reader headers 已满足本任务契约，且保留 180 秒 `x-timeout` 上限。

## 状态分类

- Reader Markdown 命中 `Just a moment...`、`cf-chl-` 或 `Target URL returned error 403`：`blocked`。
- Markdown 为空或仅空白：`empty`。
- 其他 Reader Markdown：`extracted`。
- 图片和未知扩展名：不调用 Reader，`download_only`。

目标 `resource_kind` 由目标 URL 分类，独立于 Reader 返回的 `content_type`；下载 URL 使用 URL 编码后的 `/v1/resource?url=...`。

## 验证

- `cargo fmt --all`：通过。
- `cargo test --test content_api`：8 passed。
- `cargo test --all-targets --all-features`：全部通过。
- `cargo clippy --all-targets --all-features -- -D warnings`：通过。

## Concerns

- OpenAPI 仍描述旧 `ContentResponse`，按任务边界留给 Task 6 更新。
- `Diagnostics` 当前沿用空值字段，Reader HTTP 状态和耗时采集留给后续预算/诊断任务。

## 修复轮次 1

- 修正 `TargetFacts.content_type`：按目标 URL 和 `resource_kind` 推导目标媒体类型，Reader 返回的媒体类型不再覆盖目标事实。
- 新增 `ExtractionResult.reader_content_type`，独立保留 Reader 实际响应媒体类型；未调用 Reader 的 `download_only` 响应不填该字段。
- 新增 PDF、HTML、文本、图片和未知资源的目标媒体类型映射，并补充接口断言。
- `cargo test --test content_api`：8 passed。
- `cargo test --all-targets --all-features`：全部通过。
- `cargo clippy --all-targets --all-features -- -D warnings`：通过。
