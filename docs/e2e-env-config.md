# sz-orm v8.3.0 端到端测试环境变量配置

> 日期：2026-09-22 | 版本：v8.3.0
> 目标：使用第二台服务器（121.204.253.75）MySQL/PostgreSQL + 本机 Oracle 23ai Free 进行真实数据库端到端测试

## 环境变量清单

### MySQL（第二台服务器 121.204.253.75:8802，3 库）

| 环境变量 | 连接串 |
|----------|--------|
| `SZ_ORM_E2E_MYSQL_URL_TEST` | `mysql://test:2sJ8BcTdSxWNN4fp@121.204.253.75:8802/test` |
| `SZ_ORM_E2E_MYSQL_URL_SHOP` | `mysql://shop:JkbC2jsaWAYDe2Gz@121.204.253.75:8802/shop` |
| `SZ_ORM_E2E_MYSQL_URL_NJSZJT` | `mysql://njszjt:2jKjfFK3XCjmKEPy@121.204.253.75:8802/njszjt` |

### PostgreSQL（第二台服务器）

| 环境变量 | 连接串 |
|----------|--------|
| `SZ_ORM_E2E_PG_URL` | `postgres://lewuli:JkbC2jsaWAYDe2Gz@121.204.253.75:<port>/lewuli` |

> PostgreSQL 端口需通过 SSH 探测确定（见下方步骤），默认尝试 5432。

### Oracle（本机 23ai Free，sys/Sysdba）

| 环境变量 | 值 |
|----------|-----|
| `SZ_ORM_E2E_ORACLE_USER` | `sys` |
| `SZ_ORM_E2E_ORACLE_PASSWORD` | `test123` |
| `SZ_ORM_E2E_ORACLE_CONNECT_STRING` | `127.0.0.1:1521/freepdb1.FALSE` |

### SSH（第二台服务器，用于端口探测）

| 环境变量 | 值 |
|----------|-----|
| `SZ_ORM_E2E_SSH_PASSWORD` | `LIUJIEclz2021.` |

## PowerShell 设置命令

```powershell
# MySQL（第二台服务器 3 库）
$env:SZ_ORM_E2E_MYSQL_URL_TEST = "mysql://test:2sJ8BcTdSxWNN4fp@121.204.253.75:8802/test"
$env:SZ_ORM_E2E_MYSQL_URL_SHOP = "mysql://shop:JkbC2jsaWAYDe2Gz@121.204.253.75:8802/shop"
$env:SZ_ORM_E2E_MYSQL_URL_NJSZJT = "mysql://njszjt:2jKjfFK3XCjmKEPy@121.204.253.75:8802/njszjt"

# PostgreSQL（端口需先探测，假设为 5432）
$env:SZ_ORM_E2E_PG_URL = "postgres://lewuli:JkbC2jsaWAYDe2Gz@121.204.253.75:5432/lewuli"

# Oracle（本机 23ai Free）
$env:SZ_ORM_E2E_ORACLE_USER = "sys"
$env:SZ_ORM_E2E_ORACLE_PASSWORD = "test123"
$env:SZ_ORM_E2E_ORACLE_CONNECT_STRING = "127.0.0.1:1521/freepdb1.FALSE"

# SSH（端口探测用）
$env:SZ_ORM_E2E_SSH_PASSWORD = "LIUJIEclz2021."
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

- **禁止使用第一台服务器**（122.51.216.76）— 仅使用第二台服务器（121.204.253.75）
- **禁止使用本机 MySQL**（127.0.0.1:3306）— 仅使用第二台服务器 MySQL
- **禁止使用本机 PostgreSQL**（127.0.0.1:5432）— 仅使用第二台服务器 PostgreSQL
- **禁止硬编码 SSH 密码** — 必须通过 `SZ_ORM_E2E_SSH_PASSWORD` 环境变量传入
- **禁止使用 sshpass** — SSH 操作使用 Node.js ssh2 包