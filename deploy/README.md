# 部署

`deploy/` 目录只提供部署模板。`docker-compose.yml` 编排 SearXNG、Jina Reader 和 Refinery，三个服务使用外部 Docker 网络 `refinery` 通信。实际部署时，将本目录复制到固定管理电脑上的独立目录；不要直接把源码仓库目录作为运行目录。

## 当前接口

Refinery 是 MCP-only 服务，使用 Streamable HTTP：

```text
POST /mcp
GET  /mcp
GET  /health
```

服务免认证，不提供 stdio、`/v1/*` HTTP API 或 `/openapi.json`。四个 MCP 工具为 `web_search`、`web_read`、`web_explore` 和 `web_download`。工具参数、结构化错误、站点发现的部分/空/超时结果、`web_read` 的 `target`/`extraction`/`download`/`pagination`/`diagnostics` 结构，以及 `web_download` 后必须使用 `resources/read` 的资源读取规则，以仓库根目录 [README.md](../README.md) 为准。

`web_download` 不对应 HTTP 文件下载端点。普通 HTTP GET 或 `curl` 不能读取 `refinery://resource/...`；调用方必须使用 MCP `resources/read`，再自行解码 base64 保存。资源读取实际受控下载单个最大 20 MiB，并返回实际 MIME 类型和大小元数据。

## 部署前置条件

- 目标环境已按实际网络完成 Reader 固定镜像的 HTML、PDF、重定向、SSRF、超大响应、错误和超时验收。
- 出站网络策略阻断 Reader 访问回环、私有、链路本地、保留和云元数据地址。
- 本次 MCP 源代码已在独立部署目录构建为 candidate image，例如 `refinery:mcp-local`；不要把当前 `0.2.0` 旧 HTTP 镜像误称为 MCP 版本。

仓库模板本身不执行构建或部署。当前模板端口为：

```text
18090:8080
```

只有 Refinery 发布宿主机端口。不要为 SearXNG 或 Reader 添加 `ports`。

## 配置

镜像版本和宿主机端口直接写在 `docker-compose.yml` 中。将模板复制到实际部署目录后准备环境文件：

```powershell
Copy-Item .env.example .env
```

`.env` 只保留在实际部署目录中。已有环境变量保持不变；新增：

```text
MCP_ALLOWED_HOSTS=localhost,127.0.0.1,[::1]
```

该变量是逗号分隔的 Host authority 列表。内网部署必须填写实际 authority，例如：

```text
MCP_ALLOWED_HOSTS=192.168.2.16:18090
```

默认拒绝所有带 `Origin` 的浏览器请求；原生 MCP client 不带 `Origin`。服务不增加认证，也不支持 browser CORS。

## 启动

```powershell
docker network create refinery
docker compose up -d
```

共享网络只需创建一次。启动前确认 Compose 的 Refinery `image` 已替换为本次本地 candidate image，例如 `refinery:mcp-local`。本文不记录已通过验证或生产可用结论。

## 发布与离线镜像

现有发布工作流继续执行 tag 与 Cargo 版本校验、Rust 验证、多架构 GHCR 镜像、离线 tar、SHA-256 和 GitHub Release；它只创建镜像和 Release，不部署服务。离线服务器可导入与目标架构一致的发布产物：

```powershell
docker image load --input refinery-<version>-linux-amd64.tar
```

导入后将实际镜像 tag 写入 `docker-compose.yml`，并核对 tar 与同名 `.sha256` 文件。Reader 镜像仍需使用已完成目标环境验收的固定 tag 或 digest。

## 网络与验收边界

Refinery 不持久化网页、搜索结果、站点发现结果或下载内容。SearXNG 和 Reader 通过 Docker 网络供 Refinery 使用，员工和内网应用只访问 Refinery。Reader 的出站限制必须由部署环境落实，应用层检查不能替代网络策略。

正式对内启动前，仍需在目标环境实际核对 MCP `/mcp` 调用、`/health`、Reader、SearXNG、重定向、私网阻断、20 MiB 上限、`resources/read` 下载和实际 MIME/大小元数据。本文保留这些验收边界，不把本地代码状态、旧镜像状态或模板内容写成完成证明。
