# 容器化缺失实例供给 × 兼容方言族矩阵验证（MZ-*）——2026-11-13

> 六环闭环批次（提示词：`2026-11-13-container-dialect-matrix-prompt.md`）。目标：Docker 供给 6 个缺失数据库实例，对 sz-orm 方言层（`packages/sz-orm-core/src/dialect.rs`，v10.13.4）的家族委托正确性与家族偏差点做真连矩阵验证，产出三分类清单。
>
> **Docker 29.9.2** 宿主；容器即实例非源码（ADR-0001 不涉）。供给脚本 `D:/tmp/docker-instances/supply.sh`（不入仓库）。

## 0. 实例供给（docker ps 实测）

| 实例 | 镜像 | 宿主端口 | 状态 | 备注 |
|------|------|---------|------|------|
| MSSQL 2022 CU28 | `mcr.microsoft.com/mssql/server:2022-latest`（16.0.4325.4） | 1433 | UP | 直连 mcr 拉取成功（daocloud 不代理 mcr 路径） |
| MariaDB 11.8.9 | `docker.m.daocloud.io/mariadb:11` | 3326 | UP | |
| TiDB v8.5.1 | `docker.m.daocloud.io/pingcap/tidb:v8.5.1`（mocktikv 单机） | 4400 | UP | root 空密码 |
| ClickHouse 26.10.3.7 | `docker.m.daocloud.io/clickhouse/clickhouse-server:latest` | 9004（MySQL 协议） | UP | 默认 user 仅限本机——挂载 `users.d/zz-open.xml` 开放 0.0.0.0 |
| DB2 Community（LUW 11.5+） | `icr.io/db2_community/db2:latest`（镜像 7.31GB） | 55000 | UP | `--privileged`；手工 `db2 create database szormdb`；**真库断言走容器内 CLP 脚本**（无 Rust 驱动，不引入 ibm-db2——项目约束） |
| openGauss 6.0.0 | `docker.m.daocloud.io/enmotech/opengauss:latest` | **15433**（原计划 5433 被 `szrsql_soak_7d.exe` 常驻进程占用，实测发现后改映射） | UP | 用户 gauss_orm；`sql_compatibility=PG` |

**二波登记（不在本批）**：OceanBase CE（≥6G 内存门槛）、PolarDB（镜像链复杂）、Kingbase/Dameng/GBase（官网 tarball docker load，无公开镜像）。

## 1. 交付物

| 项 | 内容 | 证据 |
|---|------|------|
| 测试 | 6 个既有 `integration_<db>.rs` 各追加 4 个 `matrix_mz_*` 前缀测试（共 24，不新建文件） | `packages/sz-orm-core/tests/integration_{mssql,mariadb,tidb,clickhouse,db2,gaussdb}.rs` |
| sz-orm 方言缺口修复 | **0 处**（见 §3）——六家族委托面均经真库行为验证无生成缺口 | 各套件输出（§2） |
| 引擎/环境特有登记 | 9 项（见 §3 表） | 各 matrix_mz_* 测试断言/注释 |
| 环境坑位登记 | 4 项（§4） | — |

测试落点 commit：**cc29f8a6**（并行 SQLite 矩阵线按 hunk 级隔离规则一并收纳提交，报告归属本线）。

> **复跑 env 口径说明**（红队回执 P2-1 处置）：§2 各套件 passed 数为「指定 env 全量」口径，复跑必须注入对应 env——MariaDB/TiDB：`SZ_ORM_MYSQL_URL` 分别指向 3326/4400（默认指向 8802 宝塔 MySQL，非本批容器）；ClickHouse：`SZ_ORM_CLICKHOUSE_URL` 指向 9004；openGauss：`SZ_ORM_PG_URL` 指向 15433；MSSQL：`SZ_ORM_MSSQL_PASSWORD`。DB2 套件为进程内方言面断言（非 ignored，默认 env 可跑），其真库证据经容器内 CLP——附档 `2026-11-13-container-dialect-matrix-db2-clp-evidence.md`（红队回执 P2-2 处置，原仅留容器 /tmp）。

## 2. 矩阵结果逐载荷（三步 = 方言生成面 + 真库执行 + 行为断言）

### MZ-MSSQL × 2022 CU28 @1433（SqlServerDialect）
`cargo test -p sz-orm-core --test integration_mssql -- --ignored` → **12 passed; 0 failed**（既有 8 + matrix 4）。
- **mz1 OUTPUT INSERTED 回填**：`supports_returning()=true`（dialect.rs:1778-1780）语义经真库验证——`INSERT ... OUTPUT INSERTED.[id]` 回填自增值，与表内行一致。
- **mz2 [方括号] quoting**：保留字列名 `[order]` + 含 `]` 列名 `[we]]ird]`（dialect.rs:1761-1764）建表/插查往返无损。
- **mz3 分页契约**：`build_pagination` 生成 `OFFSET..ROWS FETCH NEXT..ROWS ONLY`（dialect.rs:1789-1796）；**真库契约**——无 `ORDER BY` 的 OFFSET 报 Msg 102（`Incorrect syntax near '0'`，实测）；前置 `ORDER BY` 后第 2 页正确返回。生产面已由 `requires_order_by_for_pagination()=true`（dialect.rs:2011-2013）+ `query.rs:1820` 占位排序守卫覆盖——**非缺口，契约验证**。
- **mz4 GETDATE/NVARCHAR(MAX)/JSON_VALUE**：`json_type()="NVARCHAR(MAX)"` 建列可用；`JSON_VALUE(payload,'$.user.name')` 提取无损；`DEFAULT GETDATE()` 填充 DATETIME（CONVERT 121 字符串往返验证）。

### MZ-MariaDB × 11.8.9 @3326（MariaDbDialect 委派 MySqlDialect）
`cargo test -p sz-orm-core --test integration_mariadb -- --ignored`（`SZ_ORM_MYSQL_URL` 指向 3326）→ **11 passed; 0 failed**（既有 7 + matrix 4）。
- **mz1 原生 SEQUENCE**（10.3+，MySQL 无）：`CREATE SEQUENCE`+`NEXTVAL` 真库 10→11 递增。sz-orm 不生成 SEQUENCE——**家族扩展点登记**，委托不变。
- **mz2 INSERT..RETURNING**（10.5+ 引擎扩展）：真库回填 id 成功；**裁定**——`supports_returning()` 保持 false（随 MySqlDialect，dialect.rs:2140 委派），主键回填走 `LAST_INSERT_ID()`（dialect.rs:620-622）；引擎扩展不反转委托（主线 MySQL 无此语法，开启即破坏 MySQL 兼容面）。
- **mz3 CHECK 真执行**（10.2+；MySQL 8.0.16 前 defer）：违反 CHECK 报 **4025**（`CONSTRAINT mz_chk.v failed` 实测）——家族偏差行为级证据。
- **mz4 默认 sql_mode**：`STRICT_TRANS_TABLES,ERROR_FOR_DIVISION_BY_ZERO,NO_AUTO_CREATE_USER,NO_ENGINE_SUBSTITUTION`（实测）——`NO_AUTO_CREATE_USER` 为 MySQL 8 已移除项，家族差异证据。

### MZ-TiDB × v8.5.1 @4400（TiDbDialect 委派 MySqlDialect）
`cargo test -p sz-orm-core --test integration_tidb -- --ignored` → **11 passed; 0 failed**（两轮连跑幂等验证）。
- **mz1 外键 v6.6+ 真执行**：孤儿引用报 **1452**（`Cannot add or update a child row` 实测）——旧版"静默忽略"已在 v8.5 不成立；sz-orm FK 生成面真库有效。
- **mz2 AUTO_RANDOM**（分布式主键扩展）：`AUTO_RANDOM` 建表插入生成非零 id（8646911284551352321 实测）；sz-orm 默认 `auto_increment_keyword()="AUTO_INCREMENT"`（随 MySqlDialect）在 TiDB 兼容可用——**引擎特有登记，不改委托**。
- **mz3 无 INSERT RETURNING**：`RETURNING` 报 **1064**（实测）——`supports_returning()=false` 委托正确。
- **mz4 CHECK 行为探针**：v8.5 可建 CHECK 列；违反行为引擎版本特有（测试双路径登记：拒→3819 / 放→计数断言）。
- 测试幂等性修正：seed 1062 容忍 + parent 表由 mz1 独占清理（3730 FK 依赖防止误删）。

### MZ-ClickHouse × 26.10.3.7 @9004 MySQL 协议（ClickHouseDialect 独立方言）
`cargo test -p sz-orm-core --test integration_clickhouse -- --ignored` → **11 passed; 0 failed**（既有 7 + matrix 4）。
- **mz1 MergeTree 引擎子句**：`build_create_table` 产物必带 `ENGINE = MergeTree()`（dialect.rs:2626-2628）真库可执行；**26.10 契约修正**——`default_table_engine=MergeTree`（system.settings 实测）使裸建表不再被拒（回落 MergeTree），sz-orm 显式生成不依赖服务器默认（正确姿势）。
- **mz2 MySQL 协议参数绑定**：**引擎/协议版本特有登记**——sqlx prepared statement 报 `prepared statement expected 0 parameters but 2 parameters were provided`（26.10 MySQL 协议通道不接受客户端绑定参数）；矩阵改走 `escape_string` 字面量形态，注入载荷 `' OR '1'='1` 命中 0 行。既有 integration_clickhouse 全部字面量形态与此一致。
- **mz3 无事务回滚契约**：显式 `BEGIN` 拒/容双路径登记（引擎版本特有）；插入立即可见（无 MVCC 隔离）——`supports_returning()=false`（dialect.rs:2560-2562）一致。
- **mz4 LIMIT offset,limit 分页**：`build_pagination` 生成 `LIMIT 2, 2`（dialect.rs:2564-2569）真库第 2 页首行 u3 正确。

### MZ-DB2 × LUW Community @55000（Db2Dialect 独立方言；CLP 脚本真库通道）
`cargo test -p sz-orm-core --test integration_db2` → **15 passed; 0 failed**（既有 11 + matrix 4，进程内断言）+ 容器 CLP 行为证据（`/tmp/mz*.sql` 脚本输出留档 §2.DB2）。
- **mz1 SYSDUMMY1 通道**：`last_insert_id_sql()=Some("SELECT IDENTITY_VAL_LOCAL() FROM SYSIBM.SYSDUMMY1")`（dialect.rs:3077-3079）；CLP 实测 `SELECT * FROM SYSIBM.SYSDUMMY1` → IBMREQD=Y 1 row、`IDENTITY_VAL_LOCAL()` 回填 identity 值。
- **mz2 OFFSET..FETCH NEXT**：`build_pagination` 生成 `OFFSET n ROWS FETCH NEXT n ROWS ONLY`（dialect.rs:3081-3087）；CLP 实测正序/倒序两形态各命中正确行（LUW 11.5+ 支持，与生成面一致）。
- **mz3 大写折叠**：双引号 quote（dialect.rs:2987-2989）阻止折叠——CLP 实测 `CREATE TABLE ... ("order" VARCHAR(20))` + `SELECT "order"` 往返无损、保留字可作列名；未 quote 裸标识符按大写折叠（同 Oracle 向，登记）。
- **mz4 IDENTITY 双通道**：`auto_increment_keyword()="GENERATED ALWAYS AS IDENTITY"`（dialect.rs:3072-3074）；CLP 实测 `IDENTITY_VAL_LOCAL()`（sz-orm 通道）与 `SELECT id FROM NEW TABLE (INSERT...)`（引擎扩展通道，sz-orm 未采用——登记）双通可用。
- 说明：DB2 无 Rust 驱动（不引入 ibm-db2/ODBC 为项目约束），真库断言以容器内 `db2 -tvf` 脚本输出为证据，进程内仅方言生成面断言（测试注释同步标注）。

### MZ-openGauss × 6.0.0 @15433（GaussDbDialect 委派 PostgreSqlDialect）
`cargo test -p sz-orm-core --test integration_gaussdb -- --ignored`（`SZ_ORM_PG_URL` 指向 15433）→ **7 passed; 0 failed**（既有 3 + matrix 4）。
- **mz1 sql_compatibility=PG**：容器默认 PG 兼容模式（`pg_settings` 实测，gauss_orm TCP 通道）——GaussDbDialect→PG 族委托（dialect.rs:2155）前提成立。**引擎/协议特有登记**：`SHOW sql_compatibility` 经 sqlx extended protocol 返回空串（gsql 文本协议正常）——矩阵改走 `pg_settings` + 参数化 `$1`。
- **mz2 INSERT..RETURNING**：openGauss 支持 PG 形态 RETURNING，回填 serial id 成功——`supports_returning()=true` 委托正确（与 MariaDB/TiDB 的 false 形成家族差异对照）。
- **mz3 序列 NEXTVAL 通道**：**openGauss 引擎限制（与 PG 差异点）**——`nextval(pg_get_serial_sequence(...))` 报 `nextval/currval argument must be a string literal`（实测）；矩阵改字面量序列名 `nextval('<table>_id_seq')::bigint` 成功（serial 隐式命名与 PG 一致）。
- **mz4 双引号折叠**：PG 族 quote 保留小写 + 保留字列名 `"order"` 建表/往返无损（实测）；裸 `order` 列引用 gsql 报 syntax error（引擎差异双路径登记）。

## 3. 三分类清单

> 关键 file:line 证据（audit-verify 校验面）：
> - MSSQL OUTPUT 语义声明 `packages/sz-orm-core/src/dialect.rs:1778`；quote `packages/sz-orm-core/src/dialect.rs:1761`；分页生成 `packages/sz-orm-core/src/dialect.rs:1789`；ORDER BY 守卫 `packages/sz-orm-core/src/dialect.rs:2011` + `packages/sz-orm-core/src/query.rs:1820`
> - MariaDB/TiDB 委派声明 `packages/sz-orm-core/src/dialect.rs:2140`/`packages/sz-orm-core/src/dialect.rs:2143`；LAST_INSERT_ID 通道 `packages/sz-orm-core/src/dialect.rs:620`
> - ClickHouse MergeTree 子句 `packages/sz-orm-core/src/dialect.rs:2626`；LIMIT o,l `packages/sz-orm-core/src/dialect.rs:2564`；supports_returning=false `packages/sz-orm-core/src/dialect.rs:2560`
> - DB2 quote `packages/sz-orm-core/src/dialect.rs:2987`；IDENTITY 关键字 `packages/sz-orm-core/src/dialect.rs:3072`；SYSDUMMY1 通道 `packages/sz-orm-core/src/dialect.rs:3077`；分页 `packages/sz-orm-core/src/dialect.rs:3081`
> - openGauss 委派 `packages/sz-orm-core/src/dialect.rs:2155`
> - 矩阵测试落点：`packages/sz-orm-core/tests/integration_mssql.rs:634`（mz1 起）/`packages/sz-orm-core/tests/integration_mariadb.rs:262`/`packages/sz-orm-core/tests/integration_tidb.rs:235`/`packages/sz-orm-core/tests/integration_clickhouse.rs:268`/`packages/sz-orm-core/tests/integration_db2.rs:184`/`packages/sz-orm-core/tests/integration_gaussdb.rs:258`

### A. sz-orm 方言缺口（修复）
**0 处**。六家族委托选择全部正确（TiDB/MariaDB→MySQL 族、openGauss→PG 族、ClickHouse/DB2/SqlServer 独立实现），生成 SQL 真库全部按预期接受或按契约拒绝；G37 门禁 0 命中佐证。

### B. 引擎/环境版本特有（登记）

| # | 条目 | 证据 |
|---|------|------|
| B1 | MSSQL：无 ORDER BY 的 OFFSET 报 Msg 102（生产面已被 `requires_order_by_for_pagination` 守卫覆盖） | integration_mssql.rs mz3 + query.rs:1820 |
| B2 | MariaDB：原生 SEQUENCE / INSERT RETURNING / CHECK 4025 / sql_mode NO_AUTO_CREATE_USER——引擎扩展不反转 MySQL 委托 | integration_mariadb.rs mz1~mz4 |
| B3 | TiDB：FK v6.6+ 真执行（1452）/ AUTO_RANDOM / 无 RETURNING（1064）/ CHECK 行为版本相关 | integration_tidb.rs mz1~mz4 |
| B4 | ClickHouse：default_table_engine=MergeTree 使裸建表合法化（26.x）；MySQL 协议通道不接受 prepared 绑定参数；BEGIN 行为版本相关 | integration_clickhouse.rs mz1~mz3 |
| B5 | DB2：IDENTITY 回填引擎扩展通道 `NEW TABLE (INSERT)`（sz-orm 未采用）；未 quote 标识符大写折叠 | integration_db2.rs mz3/mz4 注释 CLP 证据 |
| B6 | openGauss：`nextval` 参数必须字面量（不接受函数结果）；SHOW 经 extended protocol 返回空；裸保留字列引用双路径 | integration_gaussdb.rs mz1/mz3/mz4 |
| B7 | 端口占用：宿主 5433 被 szrsql_soak_7d.exe 常驻占用，openGauss 改映射 15433 | netstat 实测（§0） |
| B8 | DB2 真库断言经容器内 CLP（无 Rust 驱动为项目约束） | integration_db2.rs 头注释 |
| B9 | 二波实例（OceanBase/PolarDB/Kingbase/Dameng/GBase）留待补批 | 提示词 §二波登记 |

### C. 文档措辞（登记）
1. `integration_clickhouse.rs` 真库测试与 MySQL 协议绑定限制一致（全字面量形态）——无需改动，本报告登记即可。
2. openGauss 测试文件头部沿用「PG 5432 默认」注释——矩阵用 `SZ_ORM_PG_URL` 覆盖，无文档漂移。

## 4. 环境坑位（本批新登记）

1. **daocloud 镜像代理不覆盖 mcr.microsoft.com**（`invalid repository` 报错）——mcr 镜像须直连拉取（本机直连可用）。
2. **enmotech/opengauss** 可经 daocloud 代理（`opengauss/opengauss` 官方镜像不在白名单）。
3. **ClickHouse 默认 user 仅限本机网络**——MySQL 协议跨容器访问须挂载 `users.d` 配置开放网络（`//d/...` 双斜杠路径防 git-bash 路径改写）。
4. **git-bash 下 `docker exec` 内绝对路径被改写**（`/opt/...` → `D:/git/opt/...`）——容器内命令须 `bash -c '...'` 包裹或 `MSYS_NO_PATHCONV=1`。
5. **DB2 首启须手工建库**（容器无自动建库；`db2 create database szormdb`）；`sqlcmd` 在 MSSQL 容器内路径 `/opt/mssql-tools18/bin/`（需 `-C` 信任自签证书）。

## 5. 门禁与回归

- 六套件真连全绿（§2）；既有测试零回归（12/11/11/11/15/7 passed 含既有项）。
- 本线未改 `src/` 生产代码（0 缺口），fmt/clippy 由并行线 cc29f8a6 门禁记录覆盖（G1/G2/G3 全过）。
- 测试新增均 `#[ignore]`（env 注入跳过模式），默认 `cargo test --workspace` 不受影响。

## 6. 复评与红队（六环第 4/5 环）

- 复评：六套件连跑两轮全绿（TiDB 套件幂等性修正后 2×11 passed）。
- 红队独立复核：本报告落盘后由独立会话执行（回执补充于本节之后）。
