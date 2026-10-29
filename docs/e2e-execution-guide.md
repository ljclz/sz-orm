# sz-orm v8.3.0 端到端测试执行指南
> **安全说明（2026-10-28 占位化）**：本文档中的服务器 IP 已替换为占位符；实际主机地址与凭据由运维在对应测试服务器的环境变量中提供，禁止提交真实地址/密码到仓库。

> 日期：2026-09-22 | 版本：v8.3.0

## 执行步骤

### 步骤 1：配置环境变量

参见 [docs/e2e-env-config.md](e2e-env-config.md)，设置以下环境变量：

```powershell
# 2026-10-27 安全整改（H-3）：凭据已从本文档移除，通过密钥管理获取（见 e2e-env-config.md）
$env:SZ_ORM_E2E_MYSQL_URL_TEST = "mysql://test:<your-password>@<remote-server-ip>:8802/test"
$env:SZ_ORM_E2E_MYSQL_URL_SHOP = "mysql://shop:<your-password>@<remote-server-ip>:8802/shop"
$env:SZ_ORM_E2E_MYSQL_URL_NJSZJT = "mysql://njszjt:<your-password>@<remote-server-ip>:8802/njszjt"
$env:SZ_ORM_E2E_PG_URL = "postgres://lewuli:<your-password>@<remote-server-ip>:5432/lewuli"
$env:SZ_ORM_E2E_ORACLE_USER = "sys"
$env:SZ_ORM_E2E_ORACLE_PASSWORD = "test123"
$env:SZ_ORM_E2E_ORACLE_CONNECT_STRING = "127.0.0.1:1521/freepdb1"
```

### 步骤 2：SSH 探测 PostgreSQL 端口

```bash
node scripts/e2e_ssh_probe.js
```

### 步骤 3：数据库可达性预检

```bash
node scripts/e2e_db_precheck.js
```

### 步骤 4：执行 e2e 测试

```bash
# 11 个 e2e 测试文件
cargo test -p sz-orm-core --features e2e-real-db --test e2e_real_db_crud
cargo test -p sz-orm-core --features e2e-real-db --test e2e_real_db_pool
cargo test -p sz-orm-core --features e2e-real-db --test e2e_real_db_transaction
cargo test -p sz-orm-core --features e2e-real-db --test e2e_real_db_migration
cargo test -p sz-orm-core --features e2e-real-db --test e2e_real_db_query
cargo test -p sz-orm-core --features e2e-real-db,multi-tenant-enhanced --test e2e_real_db_multi_tenant
cargo test -p sz-orm-core --features e2e-real-db,l1-cache --test e2e_real_db_cache
cargo test -p sz-orm-core --features e2e-real-db --test e2e_real_db_soft_delete
cargo test -p sz-orm-core --features e2e-real-db --test e2e_real_db_pagination
cargo test -p sz-orm-core --features e2e-real-db --test e2e_real_db_eager_load
cargo test -p sz-orm-core --features e2e-real-db --test e2e_real_db_dialect_behavior
```

#### 跳过记账与环境变量（R6）

e2e 套件缺库时**不再静默通过**：每个被跳过的 service 会打印一行
`E2E_SKIP[<service>]: <原因>`（service 取值：`mysql`、`mysql-test`、`mysql-shop`、
`mysql-njszjt`、`postgres`、`oracle`、`sqlite`）。

- `SZ_ORM_E2E_STRICT=1`：任何跳过都 panic（发布前全量门禁用，要求现场 DB 齐全）；
- `SZ_ORM_E2E_REQUIRED=mysql,mysql-test,postgres`：只强制列出的 service，
  其余服务允许跳过但仍输出记账行（CI 用：CI 只准备 MySQL test 库与 PostgreSQL）。

CI 断言：以 `SZ_ORM_E2E_STRICT=1 SZ_ORM_E2E_REQUIRED=mysql,mysql-test,postgres`
运行后，输出中出现 `E2E_SKIP[mysql]` / `E2E_SKIP[mysql-test]` / `E2E_SKIP[postgres]`
即判定失败（见 `.github/workflows/integration.yml`）。

URL 变量优先级：MySQL `SZ_ORM_E2E_MYSQL_URL_TEST` > `SZ_ORM_MYSQL_URL` > `MYSQL_URL`；
PostgreSQL `SZ_ORM_E2E_PG_URL` > `SZ_ORM_PG_URL` > `POSTGRES_URL`。

### 步骤 5：生成覆盖矩阵

```bash
python scripts/e2e_coverage_matrix.py
```

### 步骤 6：幻影交付扫描

```bash
python scripts/check-phantom-delivery.py
```

## v8.2.0 vs v8.3.0 对比

| 维度 | v8.2.0（本机测试） | v8.3.0（第二台服务器 + 本机 Oracle） |
|------|-------------------|--------------------------------------|
| MySQL | 127.0.0.1:3306（本机） | <remote-server-ip>:8802（3 库） |
| PostgreSQL | 127.0.0.1:5432（本机） | <remote-server-ip>:5432 |
| Oracle | 127.0.0.1:1521（本机） | 127.0.0.1:1521（本机，sys/Sysdba） |
| e2e 文件数 | 8 | 11（新增 pool/migration/query） |
| 覆盖矩阵 | 未生成 | 13×3=39 格（33 覆盖 / 1 N/A / 5 跳过） |
| 环境变量 | MYSQL_URL/POSTGRES_URL | SZ_ORM_E2E_*（旧变量兼容） |
| v8.2.0 结果 | MySQL 23/PG 18/Oracle 10 passed | — |
| v8.3.0 结果 | — | 111+ passed, 0 failed |