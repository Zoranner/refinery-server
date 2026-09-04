# 部署

`deploy/` 目录只提供部署模板：`docker-compose.yml` 同时编排 SearXNG、Jina Reader 和 Refinery，三个服务使用外部 Docker 网络 `refinery` 通信。实际部署时，应将该目录复制到固定管理电脑上的独立部署目录，不要求也不建议直接在源码仓库目录运行。

## 部署前置条件

- 正式启动对内服务前，已完成 Jina Reader 固定镜像 digest 的 HTML、PDF、重定向、SSRF、超大响应和错误响应验收；该验收不阻断 Refinery 源码构建和镜像发布。
- 部署环境的出站网络策略阻断 Reader 访问回环、私有、链路本地、保留和云元数据地址。
- `refinery` 镜像已通过版本 tag 发布，或已从离线 tar 导入。

首次部署前创建共享网络：

```powershell
docker network create refinery
```

该网络只需创建一次。

## 配置

镜像版本和宿主机发布端口直接写在 `docker-compose.yml` 中；Refinery 容器环境变量从同目录 `.env` 读取：

- `ghcr.io/zoranner/refinery:0.1.0`：已发布的 Refinery 镜像版本；升级时修改 Compose 中的 tag。
- `searxng/searxng:latest`：SearXNG 镜像；需要可复现部署时应改为已验证的固定 tag 或 digest。
- `ghcr.io/jina-ai/reader:oss`：当前 Compose 使用的 Jina Reader 镜像；正式部署前可替换为已经完成安全验收的不可变 digest。
- `8080:8080`：向内网发布的 Refinery 端口，端口变更时直接修改 Compose。

将模板复制到实际部署目录后，在该目录复制环境变量示例：

```powershell
Copy-Item .env.example .env
```

`.env` 只保留在实际部署目录中，不应回写源码仓库。变量说明见 `.env.example`；不要把 SearXNG 专属配置或代理配置写入 Refinery 的环境文件。

不要为 SearXNG 或 Reader 添加 `ports`。员工和内网应用只访问 Refinery；SearXNG 的配置位于 `deploy/app/settings.yml`。

## 启动

```powershell
docker compose up -d
```

## 离线镜像

如果部署服务器无法访问 GHCR，先导入与目标架构一致的 Refinery 发布产物：

```powershell
docker image load --input refinery-<version>-linux-amd64.tar
```

导入后，将导入的镜像 tag 直接写入 `deploy/docker-compose.yml`。离线 tar 的 SHA-256 必须与同名 `.sha256` 文件核对一致。
