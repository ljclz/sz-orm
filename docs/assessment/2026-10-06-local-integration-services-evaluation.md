# 本机集成测试服务补齐评估（2026-10-06）

- 评估范围：G7 集成测试排除清单中三套件依赖服务 —— ClickHouse / SQL Server / Redis
- 评估目标：确认本机是否可补齐上述服务，消除排除项，扩大 G7 本机可执行范围
- 评估方法：实测探测端口/进程/命令/服务/安装包源，不依赖推断
- 结论摘要：**Redis 已就绪（4/4 通过）**；**ClickHouse 可经 122 服务器 Docker + SSH 隧道实测跑通（3/3 通过）**，本机无需安装；MSSQL 腾讯云实例已下线（域名 NXDOMAIN）、服务器 mcr 镜像大文件网络不可达，仍需自备 SQL Server 实例。实测细节见 `docs/assessment/2026-10-06-integration-tunnel-verification.md`。

---

## 1. 本机现状调查（实测）

### 1.1 端口监听

| 端口 | 服务 | 监听状态 | 进程 |
|------|------|---------|------|
| 6379 | Redis | ✅ LISTENING | `redis-server.exe`（PID 11444，路径 `E:\vue\test\new-wxapp\rust\.local\redis\redis-server.exe`） |
| 9004 | ClickHouse（MySQL 兼容） | ❌ 未监听 | — |
| 1433 | SQL Server TCP | ❌ 未监听 | — |
| 3306 / 5432 / 1521 | MySQL / PostgreSQL / Oracle | ✅ LISTENING | 已有 |

`redis://127.0.0.1:6379/0` 实测 PING 返回 `+PONG`，服务真实可用。

### 1.2 命令与工具

| 命令 | 状态 |
|------|------|
| `redis-server` / `redis-cli` | NOT_FOUND（PATH 中无，但服务已在运行） |
| `clickhouse` / `clickhouse-client` | NOT_FOUND |
| `sqlcmd` | ✅ FOUND（`C:\Program Files\Microsoft SQL Server\Client SDK\ODBC\170\Tools\Binn\sqlcmd`） |
| `wsl` | ✅ FOUND（WSL 2 内核需 `wsl --update`，**无已安装发行版**） |
| `docker` | NOT_FOUND |
| `winget` / `scoop` | NOT_FOUND |
| `choco` | ✅ FOUND（Chocolatey v2.7.3） |

### 1.3 SQL Server 组件

- 安装目录：`C:\Program Files\Microsoft SQL Server\` 含 `150/`（SQL Server 2019）、`80/`、`90/`、`Client SDK/`
- `150/` 下仅有：`License Terms/ LocalDB/ Shared/ Tools/`，**无数据库引擎（无 `Setup Bootstrap`、无 `MSSQL` 服务目录）**
- Windows 服务：仅有 `MySQL96`、`PostgreSQL18`、`SQL Server VSS Writer`，**无 `MSSQLSERVER` 引擎服务**
- **SQL Server 2019 LocalDB 已安装**：`SqlLocalDB.exe info` 显示 `MSSQLLocalDB` 实例，版本 `15.0.4382.1`，State `Stopped`，Auto-create `Yes`

### 1.4 ClickHouse 下载源探测

| 源 | 结果 |
|----|------|
| `packages.clickhouse.com/tgz/stable/` | ✅ HTTP 200 可达 |
| GitHub `ClickHouse/ClickHouse` latest release（v26.9.11.2-stable） | 52 个资产，**全部为 Linux/macOS 包（deb/rpm/tgz/macos），无 Windows 二进制** |
| 官方安装文档 `clickhouse.com/docs/en/install` | **未提供 Windows 原生安装选项**（仅 Cloud / Linux / Docker / 源码构建） |
| 候选 `clickhouse-windows-x86_64.zip` 各镜像 URL | 403 / 404 |

结论：ClickHouse **官方不提供 Windows 原生二进制**。Windows 上的官方支持路径为 Docker 或 WSL。

---

## 2. 分项评估

### 2.1 Redis —— ✅ 已就绪（从排除清单移除）

- 本机 `127.0.0.1:6379` 已有真实 Redis 服务运行（来自 `new-wxapp` 项目本地部署）。
- 实测验证：
  ```bash
  python -c "import socket; s=socket.create_connection(('127.0.0.1',6379),timeout=3); s.sendall(b'PING\r\n'); print('REDIS_REPLY='+s.recv(100).decode().strip()); s.close()"
  # 输出：REDIS_REPLY=+PONG
  ```
- G7 实测运行：
  ```bash
  cargo test -p sz-orm-core --test integration_redis -- --ignored --nocapture
  # 输出：test result: ok. 4 passed; 0 failed
  ```
- **处置**：`integration_redis` 从排除清单移除。`AGENTS.md` G7 表已同步更新。

### 2.2 ClickHouse —— ✅ 可经服务器 Docker + SSH 隧道实测跑通（3/3 通过）

- **本机现状**：9004 端口未监听，无 `clickhouse` 命令，无 Docker，WSL 无发行版。
- **障碍**：官方 release 与安装文档均不提供 Windows 原生二进制；社区第三方 Windows 构建来源不可控，不建议用于测试基线。
- **已验证路径（2026-10-06，成本低）**：使用 122 服务器（122.51.216.76）上的 `szorm-clickhouse` Docker 容器（`clickhouse/clickhouse-server:latest`，MySQL 兼容端口 `9004`），经 SSH 隧道映射到本机：
  ```bash
  ssh -i ~/.ssh/szorm_server_122 -f -N -L 19004:127.0.0.1:9004 root@122.51.216.76
  SZ_ORM_CLICKHOUSE_URL="mysql://default:@127.0.0.1:19004/default" \
    cargo test -p sz-orm-core --test integration_clickhouse -- --ignored --nocapture
  # 输出：test result: ok. 3 passed; 0 failed
  ```
  注意：容器 bind mount `/tmp/ch_users.xml` → `/etc/clickhouse-server/users.d/custom-users.xml` 需在服务器重建（default 空密码 + 允许任意网络）；本机 `9004` 被本地进程占用，故隧道用 `19004`。
- **备选路径（WSL，成本中高）**：
  1. 启用 WSL：`wsl --update`（需管理员权限）→ `wsl --install -d Ubuntu`（需 Microsoft Store 网络）。
  2. 在 WSL 内安装 ClickHouse：官方提供 `clickhouse-common-static-26.9.11.2-amd64.tgz`（240.5 MB），解压后 `clickhouse server` 启动，配置 MySQL 兼容端口 9004。
- **预计耗时**：隧道路径约 10-20 分钟（含服务器容器准备）；WSL 路径约 30-60 分钟。
- **风险**：隧道依赖服务器可达性；WSL 更新可能要求重启；ClickHouse 内存占用（默认约 1-2GB）需与现有 MySQL/PG/Oracle 共存。

### 2.3 SQL Server（MSSQL）—— ❌ 云实例已下线 + 服务器 mcr 不可达（需自备实例）

- **本机现状**：已安装 SQL Server 2019 **LocalDB**（`MSSQLLocalDB`，15.0.4382.1），但：
  - LocalDB 默认仅支持**命名管道**连接，不监听 TCP 1433；
  - 集成测试硬编码 TCP `127.0.0.1:1433` + `AuthMethod::sql_server(user, password)`（SQL 认证），LocalDB 默认 Windows 认证，不匹配。
- **云数据库路径（已失效）**：历史使用的腾讯云 MSSQL 实例 `sh-mssql-adrul9nm.sql.tencentcdb.com`（`docs/assessment/2026-09-25-gate-block-report.md:157`）实测 `nslookup` 返回 NXDOMAIN、`socket.gethostbyname` 报 `getaddrinfo failed` —— **实例已下线**。
- **服务器 Docker 路径（已尝试，失败）**：在 121.204.253.75 与 122.51.216.76 两台服务器上 `docker pull mcr.microsoft.com/mssql/server:2019-latest`：
  - manifest 小请求可达（HTTP 200），但大 blob 传输（约 530MB，最大层 413MB）反复 `connection reset by peer` 中断；
  - 各镜像加速源（mcr.azk8s.cn / docker.m.daocloud.io / mcr.dockerproxy.com / docker.1panel.live / dockerproxy.net）分别返回 401/403/404/无响应，均不可用。
- **可补齐路径（成本高）**：
  1. **choco 一键安装**：`choco install sql-server-2019 -y`（包版本 `15.0.2000.20260902`，已确认源中存在）。安装约 1-2GB 下载，20-60 分钟，需管理员权限；安装后需启用 TCP 1433 与混合模式认证（`SECURITYMODE=SQL` + `SAPWD`）。
  2. **手动 ISO**：微软官网下载 SQL Server 2019/2022 Developer/Express，静默安装参数 `/Q /IACCEPTSQLSERVERLICENSETERMS /ACTION=Install /FEATURES=SQLENGINE /INSTANCENAME=MSSQLSERVER /TCPENABLED=1 /SECURITYMODE=SQL /SAPWD=...`。
  3. **自备云实例或可达 Docker 环境**：通过 `SZ_ORM_MSSQL_HOST/PORT/USER/PASSWORD/DATABASE` 环境变量注入（`packages/sz-orm-core/tests/integration_mssql.rs:22-49`）。
- **预计耗时**：30-90 分钟（含下载与配置）。
- **风险**：需管理员权限；安装后与现有服务共存的内存/端口开销；安装失败排查成本高。

---

## 3. 结论与建议

| 服务 | 本机现状 | 可否补齐 | 推荐方案 | 成本 |
|------|---------|---------|---------|------|
| Redis | ✅ 已运行 | **已就绪（4/4 通过）** | 直接使用现有服务 | 零 |
| ClickHouse | ❌ 未安装 | **✅ 已验证（3/3 通过）** | 122 服务器 Docker + SSH 隧道（`-L 19004:127.0.0.1:9004`） | 低（10-20 分钟） |
| SQL Server | ⚠️ 仅 LocalDB | 待自备实例 | choco 完整安装 / 自备云实例 / 可达 Docker 环境（经 `SZ_ORM_MSSQL_*` 注入） | 高（30-90 分钟） |

**建议**：
1. **Redis**：纳入 G7 本机可执行范围，`--skip redis` 不再需要。已执行并记录于本报告与隧道实测记录。
2. **ClickHouse**：排除清单保留 `--skip clickhouse` 兜底，但提供已验证的服务器 Docker + SSH 隧道执行路径（见 `docs/assessment/2026-10-06-integration-tunnel-verification.md`），需要时可临时拉起容器跑通 3/3。
3. **MSSQL**：保持排除清单。云实例已下线，服务器 mcr 拉取不可达，需由环境负责人自备 SQL Server 实例（完整安装或云实例），经环境变量注入后执行 8 例。全套件 CI 环境仍须执行。
4. 本机已运行的 `integration_redis` 4 个测试纳入后续 G7 复跑范围，相关覆盖率/集成测试计数同步更新。

## 附：验证命令汇总

```bash
# Redis 可用性
python -c "import socket; s=socket.create_connection(('127.0.0.1',6379),timeout=3); s.sendall(b'PING\r\n'); print('REDIS_REPLY='+s.recv(100).decode().strip()); s.close()"

# Redis 集成测试（G7）
cargo test -p sz-orm-core --test integration_redis -- --ignored --nocapture

# ClickHouse 官方源可达性
curl -sI --max-time 15 https://packages.clickhouse.com/tgz/stable/

# ClickHouse 经 122 服务器 Docker + SSH 隧道（G7，实测 3/3 通过）
ssh -i ~/.ssh/szorm_server_122 -f -N -L 19004:127.0.0.1:9004 root@122.51.216.76
SZ_ORM_CLICKHOUSE_URL="mysql://default:@127.0.0.1:19004/default" \
  cargo test -p sz-orm-core --test integration_clickhouse -- --ignored --nocapture

# 腾讯云 MSSQL 实例域名解析检查（实测 NXDOMAIN，实例已下线）
nslookup sh-mssql-adrul9nm.sql.tencentcdb.com

# SQL Server LocalDB 实例查询
"C:\Program Files\Microsoft SQL Server\150\Tools\Binn\SqlLocalDB.exe" info MSSQLLocalDB
```