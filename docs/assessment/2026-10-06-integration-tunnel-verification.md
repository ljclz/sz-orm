# 集成测试隧道实测记录（2026-10-06）

- 目标：验证 G7 集成测试可通过「服务器 Docker + SSH 隧道」与「云数据库直连」在本机跑通，修正 AGENTS.md 排除清单与评估文档认知
- 版本：v9.4.0
- 结论摘要：**Redis 4/4 + ClickHouse 3/3 经 122 服务器 SSH 隧道实测通过**；MSSQL 腾讯云实例已下线（域名 NXDOMAIN），服务器 mcr 镜像拉取网络不可达，MSSQL 仍需自备实例

---

## 1. 服务就绪情况

### 1.1 Redis（✅ 实测通过 4/4）

- 本机 `127.0.0.1:6379` 已有 Redis 服务（`E:\vue\test\new-wxapp\rust\.local\redis\redis-server.exe`）。
- 同时验证服务器 Docker 路径：122.51.216.76 上 `szorm-redis-ext` 容器（映射 `6380->6379`），经 SSH 隧道 `127.0.0.1:6380 -> 服务器 6380` 访问。
- 实测命令与结果：

```bash
ssh -i ~/.ssh/szorm_server_122 -f -N -L 6380:127.0.0.1:6380 root@122.51.216.76
SZ_ORM_REDIS_URL="redis://127.0.0.1:6380/0" \
  cargo test -p sz-orm-core --test integration_redis -- --ignored --nocapture
# test result: ok. 4 passed; 0 failed
```

### 1.2 ClickHouse（✅ 实测通过 3/3）

- 122 服务器 `szorm-clickhouse` 容器（`clickhouse/clickhouse-server:latest`，MySQL 兼容端口 `9004`）。
- 启动前需重建挂载文件 `/tmp/ch_users.xml`（容器 bind mount 到 `/etc/clickhouse-server/users.d/custom-users.xml`）；该文件随服务器重启丢失，内容为 default 用户空密码 + 允许任意网络。
- 经 SSH 隧道 `127.0.0.1:19004 -> 服务器 9004` 访问。
- 实测命令与结果：

```bash
ssh -i ~/.ssh/szorm_server_122 -f -N -L 19004:127.0.0.1:9004 root@122.51.216.76
SZ_ORM_CLICKHOUSE_URL="mysql://default:@127.0.0.1:19004/default" \
  cargo test -p sz-orm-core --test integration_clickhouse -- --ignored --nocapture
# test result: ok. 3 passed; 0 failed
```

- 服务器端验证：`pymysql` 直连 `127.0.0.1:9004` 返回 `('26.8.2.7',)`，确认 MySQL 协议正常；隧道未就绪时 sqlx 报 `UnexpectedEof`（伪故障，重建隧道后消失）。

### 1.3 MSSQL（❌ 云实例已下线 + 服务器网络不可达）

#### 腾讯云实例调查

- 历史记录中的腾讯云 MSSQL 实例 `sh-mssql-adrul9nm.sql.tencentcdb.com:22527`（见 `docs/assessment/2026-09-25-gate-block-report.md:157`）：
  - `nslookup sh-mssql-adrul9nm.sql.tencentcdb.com` → `Non-existent domain`（NXDOMAIN）
  - `socket.gethostbyname` → `getaddrinfo failed`
- 结论：**实例已下线**，云数据库直连路径不可用。

#### 服务器 Docker 部署尝试（失败原因）

- 在 121.204.253.75 与 122.51.216.76 两台服务器上尝试拉取 `mcr.microsoft.com/mssql/server:2019-latest`：
  - `curl https://mcr.microsoft.com/v2/mssql/server/manifests/2019-latest` → HTTP 200（小请求可达）
  - `docker pull` → 卡死在 `Pulling fs layer`（大 blob 传输被 `connection reset by peer` 中断）
  - 代理源测试：`mcr.azk8s.cn`（403）、`docker.m.daocloud.io`（401/403）、`mcr.dockerproxy.com`（无响应）、`docker.1panel.live`（403）、`dockerproxy.net`（404）
  - 镜像约 530MB（3 层：29.9MB + 413MB + 86.3MB），网络无法稳定传输大文件
- 本机仅有 SQL Server 2019 LocalDB（命名管道，不满足 TCP 1433 + SQL 认证），无完整 SQL Server 引擎。

#### MSSQL 处置

- `integration_mssql`（8 例）继续列入本机排除清单。
- 需自备 SQL Server 实例（mcr 可达环境或云实例），通过 `SZ_ORM_MSSQL_HOST/PORT/USER/PASSWORD/DATABASE` 环境变量注入（`packages/sz-orm-core/tests/integration_mssql.rs:22-49`）。

---

## 2. 服务器与容器信息

| 项目 | 值 |
|------|-----|
| 122 服务器 | 122.51.216.76:22（SSH 密钥 `~/.ssh/szorm_server_122`） |
| 第二台服务器 | 121.204.253.75:32（SSH 密钥 `~/.ssh/id_ed25519_s253_lf`） |
| Redis 容器 | `szorm-redis-ext`（`0.0.0.0:6380->6379/tcp`） |
| ClickHouse 容器 | `szorm-clickhouse`（`0.0.0.0:9004->9004/tcp`，镜像 `clickhouse/clickhouse-server:latest`） |
| ClickHouse 挂载 | `/tmp/ch_users.xml` → `/etc/clickhouse-server/users.d/custom-users.xml` |

## 3. 隧道命令汇总

```bash
# Redis（本机 6380 -> 服务器 6380）
ssh -i ~/.ssh/szorm_server_122 -o ExitOnForwardFailure=yes -f -N -L 6380:127.0.0.1:6380 root@122.51.216.76

# ClickHouse（本机 19004 -> 服务器 9004）
ssh -i ~/.ssh/szorm_server_122 -o ExitOnForwardFailure=yes -f -N -L 19004:127.0.0.1:9004 root@122.51.216.76
```

注意：本机 `6379` 已被本机 Redis 占用、`9004` 被本地 node 进程占用，隧道需避开（用 6380/19004）。

## 4. 结论与文档影响

1. `AGENTS.md` G7 排除清单：`integration_clickhouse` 从「仅 WSL 路径」更新为「服务器 Docker + SSH 隧道可跑通（实测 3/3）」；`integration_mssql` 原因更新为「腾讯云实例已下线 + mcr 网络不可达 + 本机无完整 SQL Server」。
2. 评估文档 `2026-10-06-local-integration-services-evaluation.md` 的「可补齐路径」补充服务器隧道实测结论。
3. `integration_redis` 已确认从排除清单移除（本机与服务器隧道双路径均验证）。