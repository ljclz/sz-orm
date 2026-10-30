# sz-orm v8.3.0 端到端测试环境变量配置
> **安全说明（2026-10-28 占位化）**：本文档中的服务器 IP 已替换为占位符；实际主机地址与凭据由运维在对应测试服务器的环境变量中提供，禁止提交真实地址/密码到仓库。

> 日期：2026-09-22 | 版本：v8.3.0
> 目标：使用第二台服务器（<remote-server-ip>）MySQL/PostgreSQL + 本机 Oracle 23ai Free 进行真实数据库端到端测试
>
> **⚠ 2026-10-27 安全整改（H-3）**：本文档曾包含真实数据库/SSH 凭据，已全部替换为占位符
> `<your-password>`。真实凭据请通过密钥管理（环境变量 / Vault）获取，禁止写入仓库。
> 历史泄露的凭据必须轮换（见 docs/assessment/2026-10-27-security-review.md）。

## 环境变量清单

### MySQL（第二台服务器 <remote-server-ip>:8802，3 库）

| 环境变量 | 连接串 |
|----------|--------|
| `SZ_ORM_E2E_MYSQL_URL_TEST` | `mysql://test:<your-password>@<remote-server-ip>:8802/test` |
| `SZ_ORM_E2E_MYSQL_URL_SHOP` | `mysql://shop:<your-password>@<remote-server-ip>:8802/shop` |
| `SZ_ORM_E2E_MYSQL_URL_NJSZJT` | `mysql://njszjt:<your-password>@<remote-server-ip>:8802/njszjt` |

### PostgreSQL（第二台服务器）

| 环境变量 | 连接串 |
|----------|--------|
| `SZ_ORM_E2E_PG_URL` | `postgres://lewuli:<your-password>@<remote-server-ip>:<port>/lewuli` |

> PostgreSQL 端口需通过 SSH 探测确定（见下方步骤），默认尝试 5432。

### Oracle（本机 23ai Free，sys/Sysdba）

| 环境变量 | 值 |
|----------|-----|
| `SZ_ORM_E2E_ORACLE_USER` | `sys` |
| `SZ_ORM_E2E_ORACLE_PASSWORD` | `test123` |
| `SZ_ORM_E2E_ORACLE_CONNECT_STRING` | `127.0.0.1:1521/freepdb1` |

### SSH（第二台服务器，用于端口探测）

| 环境变量 | 值 |
|----------|-----|
| `SZ_ORM_E2E_SSH_PASSWORD` | `<your-password>`（通过密钥管理下发，禁止写入仓库） |

## PowerShell 设置命令

```powershell
# MySQL（第二台服务器 3 库）
$env:SZ_ORM_E2E_MYSQL_URL_TEST = "mysql://test:<your-password>@<remote-server-ip>:8802/test"
$env:SZ_ORM_E2E_MYSQL_URL_SHOP = "mysql://shop:<your-password>@<remote-server-ip>:8802/shop"
$env:SZ_ORM_E2E_MYSQL_URL_NJSZJT = "mysql://njszjt:<your-password>@<remote-server-ip>:8802/njszjt"

# PostgreSQL（端口需先探测，假设为 5432）
$env:SZ_ORM_E2E_PG_URL = "postgres://lewuli:<your-password>@<remote-server-ip>:5432/lewuli"

# Oracle（本机 23ai Free）
$env:SZ_ORM_E2E_ORACLE_USER = "sys"
$env:SZ_ORM_E2E_ORACLE_PASSWORD = "test123"
$env:SZ_ORM_E2E_ORACLE_CONNECT_STRING = "127.0.0.1:1521/freepdb1"

# SSH（端口探测用）
$env:SZ_ORM_E2E_SSH_PASSWORD = "<your-password>"
```

## 执行步骤

### 步骤 1：SSH 探测 PostgreSQL 端口

```bash
node scripts/e2e_ssh_probe.js
```

输出端口号后，更新 `SZ_ORM_E2E_PG_URL` 中的端口。

### 步骤 2：数据库可达性预检

```bash
node scripts/e2e_db_precheck.js
```

输出 JSON 报告，确认 3 个数据库均 `reachable: true`。

### 步骤 3：执行 e2e 测试

```bash
cargo test -p sz-orm-core --features e2e-real-db --test e2e_real_db_crud
cargo test -p sz-orm-core --features e2e-real-db --test e2e_real_db_pool
cargo test -p sz-orm-core --features e2e-real-db --test e2e_real_db_transaction
# ... 其他 e2e 测试文件
```

## 旧变量兼容

以下旧环境变量仍可读取（v8.2.0 既有测试不破坏）：

| 旧变量 | 新变量 |
|--------|--------|
| `MYSQL_URL` | `SZ_ORM_E2E_MYSQL_URL_TEST` |
| `POSTGRES_URL` | `SZ_ORM_E2E_PG_URL` |
| `SZ_ORM_ORACLE_USER` | `SZ_ORM_E2E_ORACLE_USER` |
| `SZ_ORM_ORACLE_PASSWORD` | `SZ_ORM_E2E_ORACLE_PASSWORD` |
| `SZ_ORM_ORACLE_CONNECT_STRING` | `SZ_ORM_E2E_ORACLE_CONNECT_STRING` |

## 禁止项

- **禁止使用第一台服务器**（<legacy-server-ip>）— 仅使用第二台服务器（<remote-server-ip>）
- **禁止使用本机 MySQL**（127.0.0.1:3306）— 仅使用第二台服务器 MySQL
- **禁止使用本机 PostgreSQL**（127.0.0.1:5432）— 仅使用第二台服务器 PostgreSQL
- **禁止硬编码 SSH 密码** — 必须通过 `SZ_ORM_E2E_SSH_PASSWORD` 环境变量传入
- **禁止使用 sshpass** — SSH 操作使用 Node.js ssh2 包

## 统一入口（2026-10-30）

仓库里长期存在两族凭据入口，此前没有单一入口，容易"配了 A 跑 B"：

| 变量族 | 消费方 | 未配置时的行为 |
|--------|--------|----------------|
| `SZ_ORM_PG_URL` / `SZ_ORM_MYSQL_URL` | `sz-orm-sqlx/tests/{real_db_pool_tests,real_db_jepsen,any_pg_test,any_mysql_test}`、`sz-orm-core/tests/{prewarm_integration,smart_eager_integration_pg,smart_eager_integration_mysql}` | 回退到源码内默认连接串（`test123`），连不上即测试失败——**不**跳过 |
| `SZ_ORM_E2E_PG_URL` / `SZ_ORM_E2E_MYSQL_URL_{TEST,SHOP,NJSZJT}` / `SZ_ORM_E2E_ORACLE_*` | `sz-orm-core/tests/e2e_*.rs`（18 个文件，经 `tests/common/e2e_env.rs`） | 输出 `E2E_SKIP[<service>]: <原因>` 并跳过；`SZ_ORM_E2E_STRICT=1` 或 `SZ_ORM_E2E_REQUIRED=...` 下跳过即 panic |
| `SZ_ORM_ORACLE_PASSWORD` / `SZ_ORM_MSSQL_PASSWORD` | `sz-orm-oracle` / `sz-orm-mssql` 的 `--ignored` 套件 | 缺变量即连接失败 |

统一入口（推荐）：

```bash
cp docs/env.real-db.example .env.real-db   # 填入真实连接串；.env.* 已被 .gitignore 覆盖
bash scripts/run-real-db-tests.sh pg mysql e2e oracle
```

脚本先把套件按变量存在性分类：**缺变量 = 显式 SKIP 并记账**，有变量才运行，最后输出
`运行 N 组 / 失败 F / 跳过 S`，全跳过时退出码 2（禁止把"什么都没跑"当作通过）。
日志默认写入 `docs/assessment/real-db-run-<UTC 时间戳>.log`。

> **2026-10-30 口径更正**：下方「禁止使用本机 MySQL/PostgreSQL」是 v8.3.0 阶段的临时约束，
> 已被 `AGENTS.md`「本机数据库」小节取代——本机 MySQL 8.0.29:8802、PostgreSQL 18:5432、
> Oracle 26ai:1521、Redis:6379 均可用于本地验证；凭据只能经环境变量注入，不得入库。