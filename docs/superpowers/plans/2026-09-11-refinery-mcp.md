# Refinery MCP Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Refinery 重构为只提供 Streamable HTTP MCP 和健康检查的内网资料服务。

**Architecture:** MCP transport 和工具层调用共享 application service；application service 复用现有 Reader、SearXNG、sitemap 与资源下载能力。无状态 MCP resource 在读取时重新下载，不持久化缓存。

**Tech Stack:** Rust 2024、Axum、Tokio、reqwest、rmcp 3.3.0。

**Spec:** `docs/superpowers/specs/2026-09-11-refinery-mcp-design.md`

## Global Constraints

- 工具名称固定为 `web_search`、`web_read`、`web_explore`、`web_download`。
- 仅提供 MCP 和 `/health`；删除业务 `/v1/*` 与 OpenAPI 对外入口。
- MCP 使用 Streamable HTTP，内网免认证。
- 不新增持久化缓存、数据库、文件存储或后台清理任务。
- 保留公网 URL 校验、DNS 检查、重定向校验、超时和 20 MiB 资源上限。

---

### Task 1: 应用服务层

**Files:**
- Create: `src/application/mod.rs`, `src/application/search.rs`, `src/application/read.rs`, `src/application/explore.rs`, `src/application/download.rs`
- Modify: `src/state.rs`, `src/lib.rs`
- Test: existing focused integration tests migrated to application behavior

- [ ] Write failing tests for service calls and returned domain models.
- [ ] Extract content/search/sitemap/download orchestration from route handlers.
- [ ] Make `AppState` expose the service dependencies without HTTP router coupling.
- [ ] Run focused Rust tests.

### Task 2: MCP server and resources

**Files:**
- Create: `src/mcp/mod.rs`, `src/mcp/server.rs`, `src/mcp/resources.rs`
- Modify: `Cargo.toml`, `Cargo.lock`, `src/error.rs`
- Test: `tests/mcp_api.rs`

- [ ] Add `rmcp = 3.3.0` with Streamable HTTP support.
- [ ] Write failing tests for initialization, tool listing, tool names, tool calls, resources/read and structured errors.
- [ ] Implement the MCP server with four tools and the stateless resource URI.
- [ ] Map application errors to tool errors without exposing internal service names.
- [ ] Run focused MCP tests.

### Task 3: Runtime and deployment boundary

**Files:**
- Modify: `src/main.rs`, `src/routes/mod.rs`, `deploy/docker-compose.yml`, `deploy/README.md`, `README.md`
- Delete or retire: `src/routes/content.rs`, `src/routes/search.rs`, `src/routes/sitemap.rs`, `src/routes/resource.rs`, `src/routes/openapi.rs`, `src/routes/rejection.rs`, related obsolete tests

- [ ] Write failing route tests proving only `/health` and MCP are exposed.
- [ ] Wire Streamable HTTP at `/mcp` and keep `/health`.
- [ ] Remove OpenAPI and business HTTP routes.
- [ ] Update deployment and operator documentation to MCP-only behavior.
- [ ] Run route and deployment contract checks.

### Task 4: Final verification

**Files:**
- Modify: structural checks and tests only if required by failures

- [ ] Run `cargo fmt --all`.
- [ ] Run `cargo test --all-targets --all-features`.
- [ ] Run `cargo clippy --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo build --release --locked`.
- [ ] Run `git diff --check` and scan for obsolete `/v1/` and OpenAPI references.
- [ ] Record runtime verification limits for real Docker, Reader and SearXNG.
