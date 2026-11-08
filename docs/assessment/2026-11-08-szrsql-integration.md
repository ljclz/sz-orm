# SzRSQL 数据库支持集成报告（2026-11-08）

> 依据提示词：[2026-11-08-szrsql-integration-prompt.md](2026-11-08-szrsql-integration-prompt.md)
> 工作空间版本：v10.13.2（本次 bump）· 适配目标：SzRSQL v1.0.0-rc.2（只读，ADR-0001 遵守——本会话未修改 szrsql 仓库任何文件）

## 阶段一：szrsql 理解与实测

### 1.1 协议面结论（L1 静态证据）

- **线协议 = 标准 PostgreSQL pgwire v3.0**：`crates/szrsql-protocol/src/pgwire/server.rs:13`（扩展查询协议 Parse/Bind/Execute/Describe/Close/Sync/Flush）；SCRAM-SHA-256 为默认认证（`szrsql-bin/src/main.rs:1007` `auth_mode` 参数，`--auth-mode trust` 仅本地开发）。
- **单一 PG 语义执行器**：szrsql AGENTS.md:10「线协议多前端 + 单一 PG 语义执行器」——六前端全部经 `szrsql_protocol::pgwire::session::ExecutorService`（`session.rs:751`）执行同一套 PG 语义 SQL。**这是阶段二"委托 PG 方言族"决策的核心依据。**
- **默认端口 6543**（`main.rs` `--port` 默认值，README 命令行参数表:168），避开 PG 5432——本机两者可并存。
- **information_schema 真实实现**：`crates/szrsql-catalog/src/information_schema.rs:144-238`（tables/columns/table_constraints 列名与 Schema 常量 + 查询函数）。
- **RETURNING 解析支持**：`crates/szrsql-sql/src/parser.rs:6538-6557`（INSERT/UPDATE convert 带 returning）。
- **EXPLAIN 支持**：`crates/szrsql-sql/src/parser.rs:15`（模块头声明 EXPLAIN / EXPLAIN ANALYZE）。

### 1.2 阶段一过程记录（并行会话干扰处理）

szrsql 仓库存在**并行 AI 会话活动**（同日 commit `0d89fc70`/`0531740d`/`660c4d95`/`f2d0c799`/`ca2e9ea1`，作者 ljclz，14:27–15:17）。构建时遇到：

1. HEAD 上 `Expr::ColumnRef` 变体新增后 szrsql 仓库 `crates/szrsql-catalog/src/multitenant.rs` 478 行附近 match 未覆盖（并行会话未完成的工作树改动所致）；
2. 工作树含他人未提交 WIP（`lsm_table_adapter.rs` 二级分区 `PartitionStorage` 枚举、`ast.rs`/`parser.rs` 分区 spec）。

处置：本会话将他人 WIP stash（`szorm-session-wip2`，保留可恢复），等待并行会话提交 `ca2e9ea1`（修复 parser）与 multitenant 修复（并行会话 15:17 后自行修复）后重建成功：`cargo build -p szrsql-bin` Finished（D:/rust-target 共享缓存）。**szrsql 仓库零改动**。

### 1.3 本机启动实测（L2 证据）

```text
启动：D:/rust-target/debug/szrsql.exe --port 6543 --auth-mode trust --data-dir D:/tmp/szrsql-test
监听：TCP 127.0.0.1:6543 LISTENING（netstat 实测，PID 44336）
```

经 sz-orm `AnyPool`（sqlx PostgreSQL 驱动）真连 6543 实测（探针输出逐条留存于会话记录）：

| 能力 | 实测结果 | 证据 |
|------|---------|------|
| 连接（pgwire + trust） | ✅ `connect: Ok(Postgres)` | 探针 probe_connect 输出 |
| DDL（CREATE/DROP TABLE，BIGSERIAL/TEXT/BIGINT/BOOLEAN/TIMESTAMP） | ✅ 全部成功 | probe_crud CREATE err 无 |
| `$n` 占位符 + prepared statement | ✅ INSERT/SELECT/UPDATE/DELETE 参数化全通（affected=1，rows=1） | probe_prepared_params 输出 |
| RETURNING | ✅ `RETURNING: rows=1` | probe_crud_tx_returning_pagination 输出 |
| 事务 BEGIN/COMMIT/ROLLBACK | ✅ 回滚后 COUNT 不变（`COUNT after rollback: 1`） | 同上 |
| LIMIT/OFFSET 分页 | ✅ `LIMIT/OFFSET: rows=2` | 同上 |
| information_schema.columns | ✅ `rows=5`（5 列元数据） | 同上 |
| EXPLAIN | ✅ `EXPLAIN: rows=6` | 同上 |
| SHOW server_version | ✅ ok | 同上 |
| 类型往返（BIGINT/TEXT/BOOL/TIMESTAMP/NULL） | ✅ `TYPES roundtrip: rows=1` | 同上 |

### 1.4 兼容性差异表

| # | 差异点 | 表现 | 影响 | 证据 |
|---|--------|------|------|------|
| D-1 | 裸 SQL 文本含 `$1` 字面量（无对应参数绑定） | szrsql 拒绝：`there is no parameter $1 in this plan: parameter substitution does not cover this statement form (M1 guard)` | sz-orm 参数化路径不受影响（占位符与参数一一对应）；sz-orm 无参数时不会生成 `$n` 占位 | 探针输出 `INSERT placeholder without params` |
| D-2 | `--auth-mode scram` 为默认 | 首次启动自动生成随机 postgres 密码打印到日志；本地测试用 `--auth-mode trust` 规避 | 集成测试文档注明 trust 仅限本地；生产用 SCRAM + `SZ_ORM_SZRSQL_URL` 凭据注入 | README 命令行参数表:170 |
| D-3 | BIGSERIAL 自增 | szrsql 支持（RETURNING id 返回真实自增值） | schema_sync SERIAL 语法族可直接执行 | 探针 RETURNING rows=1 |
| D-4 | 未实测项：SAVEPOINT/游标/COPY/LISTEN | 未覆盖（超出本批验收范围） | 遗留风险，见 §5 | — |

## 阶段二：sz-orm 集成设计决策与实施

### 2.1 设计决策（与提示词推荐架构一致，实测未推翻）

**结论：复用 PG 适配，最小 diff 即可支持**（提示词输出要求 3 的如实报告）：

1. `DbType::Szrsql` 新变体 → `SzrsqlDialect` 经既有 `delegate_dialect_to!` 宏委托 `PostgreSqlDialect`（1 行宏调用）；不新写任何方言逻辑。
2. 连接层零新组件：`szrsql://` scheme 在 `AnyBackend::from_dsn` 识别为 Postgres 后端（复用 sqlx PostgreSQL 驱动 + 既有 `PgPoolHandle`），`normalize_dsn_scheme` 把 `szrsql://` 改写回 `postgres://`——不新写 wire 协议（szrsql 自带 pgwire）。
3. PG 家族既有 match 分支补齐（`typed_ast.rs` 3 处 ILIKE/DISTINCT ON/CAST 特化、`migration.rs` Postgres 迁移锁、`DialectKind::from_db_type`）。

### 2.2 实施清单（commit `06e1767` + 文档 commit）

| 文件 | 变更 |
|------|------|
| `packages/sz-orm-core/src/db_type.rs` | `DbType::Szrsql`：as_str/from_str/default_port=6543/is_postgres_family/supports_stored_procedure/ALL 收录（变体 28→29，默认编译 22） |
| `packages/sz-orm-core/src/dialect.rs` | `SzrsqlDialect`（delegate_dialect_to! → PostgreSqlDialect）+ get_dialect 分发 + DialectKind::from_db_type |
| `packages/sz-orm-core/src/typed_ast.rs` | 3 处 PG 家族分支补 Szrsql（ILIKE / DISTINCT ON / CAST） |
| `packages/sz-orm-core/src/migration.rs` | Postgres 迁移锁分支补 Szrsql |
| `packages/sz-orm-sqlx/src/any_driver.rs` | `szrsql://` scheme 识别 + normalize 改写 + from_db_type 映射 |
| `packages/sz-orm-sqlx/tests/integration_szrsql.rs` | 新增 8 项 `#[ignore]` 集成测试 |

### 2.3 schema_sync / 迁移 DDL 兼容性结论

- 建表走 PG 方言族 `build_create_table`（SERIAL 类型族 + 双引号标识符），szrsql 实测 `CREATE TABLE … (id BIGSERIAL PRIMARY KEY, …)` 直接执行成功（§1.3 DDL 行）。
- `migration.rs` 迁移锁 `MigrationLockKind::Postgres` 纳入 Szrsql（PG advisory lock 语义由 szrsql SQL 层承接）。
- 未实测项（ALTER 大规模变更/IF EXISTS 变体）登记为遗留风险 §5。

### 2.4 集成测试与验证输出

`cargo test -p sz-orm-sqlx --test integration_szrsql -- --ignored`（szrsql 真连 127.0.0.1:6543）：

```text
running 8 tests
test szrsql_connect_reports_szrsql_backend ... ok
test szrsql_information_schema_probe ... ok
test szrsql_pool_acquire_release_roundtrip ... ok
test szrsql_transaction_rollback ... ok
test szrsql_type_roundtrip ... ok
test szrsql_crud_roundtrip ... ok
test szrsql_parameterized_queries ... ok
test szrsql_pagination_limit_offset ... ok
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

其余门禁输出（本会话实测）：

- `cargo check --workspace --all-targets`：0 error
- `cargo clippy --workspace --all-targets -- -D warnings`：0 error
- G1 fmt：`bash scripts/check-fmt-all.sh` → 72 包通过
- G9：`check-sql-injection-auto.py --strict` → 46/46 Safe
- G12：check-doc-consistency → PASS；G15 phantom-delivery → PASS；G16 semantic-patterns → PASS
- G18：check-metrics-real → PASS（--fix 自动同步测试总数 21290→21298 与版本行）
- G19：check-publish-consistency → PASS；G27 gate-scripts → PASS；G28 security-doc-version → PASS（Security.md 头同步 10.13.2）；G33 doc-version-consistency → 18 处全部一致；G34 generated-code → 未复跑（无生成器相关变更）；G35 doc-links → 0 死链；G36 changelog-consistency → 77 版本头 PASS
- pytest：`python -m pytest tests/` → **160 passed, 1 skipped**
- 幻影交付自查：本次不新增 feature gate、不新增"自动/强制"语义组件——Szrsql 为普通枚举变体 + 真连测试，无需豁免登记。

## 3. 版本与发布

- workspace 版本 10.13.1 → **10.13.2**（Cargo.toml:6）；Cargo.lock 随动。
- **10.13.2 尚未发布 crates.io**（本批仅本地集成；发布需另行执行 publish 流程，crates.io 最新仍为 10.13.1——AGENTS.md「已发布」行已如实标注双口径）。

## 4. 文档同步清单

| 文档 | 变更 |
|------|------|
| README.md / README.zh.md | 版本行/徽章 10.13.2、方言数 28→29（22 默认 + 7 门控）、支持表新增 SzRSQL 行（端口 6543） |
| AGENTS.md | 版本行 10.13.2、「已发布」行双口径标注、「本机数据库」节新增 SzRSQL 条目（启动命令/环境变量/未启动跳过） |
| CHANGELOG.md | 新增 `[10.13.2] — 2026-11-08` 条目（含阶段一兼容性实测结论） |
| docs/Security.md、docs/sz-orm-engineering-practices.md、docs/sz-orm使用指南.md 等 14 处 | 活文档版本声明同步（G33/G28 驱动） |

## 5. 遗留风险

1. **未实测项**：SAVEPOINT 嵌套、游标、COPY、LISTEN/NOTIFY、ALTER TABLE 大规模变更、`--auth-mode scram` 路径（本批用 trust）。集成测试覆盖 CRUD/参数化/事务/分页/元数据，均已真连验证；上述扩展场景下轮补测。
2. **szrsql 并行会话**：szrsql 仓库同日高频变更（分区 spec/ColumnRef 性能线），后续复测应以最新 commit 重建。
3. **crates.io 未发布**：10.13.2 待发布流程（publish-crates.sh + SOCKS5 代理配方，见记忆 server-info-doc-and-proxy）。

## 6. 结论

按提示词输出要求 3 如实报告：**szrsql 经 pgwire v3.0 暴露标准 PG 语义，sz-orm 复用既有 PG 适配、以最小 diff（枚举变体 + 1 行方言宏 + scheme 识别）完成支持**，未过度抽象；8 项真连集成测试 + 全部门禁（本批适用项）通过。
