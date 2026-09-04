# Task 1 实现报告

## 修改文件

- `src/material/mod.rs`：新增资料访问模型模块并导出模型类型。
- `src/material/model.rs`：新增目标事实、抽取结果、下载能力、分页、诊断和统一资料内容响应模型。
- `src/lib.rs`：注册并公开 `material` 模块。
- `tests/material_model.rs`：覆盖目标资源类型与抽取媒体类型分离，以及完整响应结构序列化。

## 构造函数和序列化设计

- `TargetFacts::new(...)` 统一构造请求 URL、最终 URL、目标资源类型和响应媒体类型。
- `ExtractionResult::blocked(...)` 构造 `blocked` 状态并保留抽取引擎和原因。
- `ExtractionResult::download_only(...)` 构造 `download_only` 状态并保留抽取引擎和原因。
- `MaterialStatus` 使用 `snake_case` 序列化名称。
- 资料模型结构均实现 `Serialize`；`TargetFacts` 对 `url::Url` 使用字符串形式序列化，避免改变现有 `url` 依赖特性。

## 测试命令及结果

- `cargo test --test material_model`：2 passed，0 failed。
- `cargo fmt --all`：通过。
- `cargo test --all-targets --all-features`：全部通过（19 passed，0 failed）。
- `cargo clippy --all-targets --all-features -- -D warnings`：通过。

## 未完成事项

- 未修改任何路由、现有响应、数据库、缓存、队列或网页抽取器；后续任务负责接入该内部模型。
