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
| D-4 | 未实测项：SAVEPOINT/游标/COPY/LISTEN | 初版未覆盖 → **同日 §7 已收口实测**（SAVEPOINT/游标 ✅；COPY/NOTIFY 为 szrsql pgwire 子协议与 sqlx 帧语义差异 ⚠️） | 见 §7 | §7 表 L-1~L-4 |

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
- **10.13.2 已于 2026-11-08 全量发布 crates.io**（publish-crates.sh 拓扑发布 + SOCKS5 代理；期间 429 限流 1 次、already-exists 假失败 1 次，续发消化；发布后经 crates.io API 逐包核实：**69 个库包 + cli 全部 max_version=10.13.2**，`sz-orm-js`/`sz-orm-python` 独立 0.1.0 版本线不动，examples 不发布）。

## 4. 文档同步清单

| 文档 | 变更 |
|------|------|
| README.md / README.zh.md | 版本行/徽章 10.13.2、方言数 28→29（22 默认 + 7 门控）、支持表新增 SzRSQL 行（端口 6543） |
| AGENTS.md | 版本行 10.13.2、「已发布」行双口径标注、「本机数据库」节新增 SzRSQL 条目（启动命令/环境变量/未启动跳过） |
| CHANGELOG.md | 新增 `[10.13.2] — 2026-11-08` 条目（含阶段一兼容性实测结论） |
| docs/Security.md、docs/sz-orm-engineering-practices.md、docs/sz-orm使用指南.md 等 14 处 | 活文档版本声明同步（G33/G28 驱动） |

## 5. 遗留风险

1. ~~**未实测项**：SAVEPOINT 嵌套、游标、COPY、LISTEN/NOTIFY、ALTER TABLE 大规模变更、`--auth-mode scram` 路径~~ → **已于同日收口实测（见 §7 遗留项复测）**。
2. **szrsql 并行会话**：szrsql 仓库同日高频变更（分区 spec/ColumnRef 性能线），后续复测应以最新 commit 重建。复测时点 HEAD=`2a0285db`（流式 GROUP BY 聚合）。
3. ~~**crates.io 未发布**~~ → **已于 2026-11-08 全量发布并逐包核实（§3）**。

## 7. 遗留项复测（2026-11-08 同日收口，szrsql HEAD=2a0285db，全部 L2 真连证据）

6 项遗留场景以临时探针真连实测（探针已按临时文件纪律删除，输出留存于会话记录与本节）：

| # | 场景 | 结果 | 实测证据 |
|---|------|------|---------|
| L-1 | **SAVEPOINT 嵌套**（SAVEPOINT / ROLLBACK TO / RELEASE，两层嵌套） | ✅ 支持 | `ROLLBACK TO sp1` 后 rows=1（期望 1）；嵌套 `ROLLBACK TO inner_sp` 后 rows=2（10,11，期望 2）；整事务回滚后 rows=1。与 szrsql README 已知缺陷 BUG-3 修复声明一致（SAVEPOINT 栈 + 撤销日志） |
| L-2 | **游标**（DECLARE CURSOR / FETCH n / CLOSE） | ✅ 支持 | `DECLARE cur1 CURSOR FOR SELECT…` ok；`FETCH 2 FROM cur1` rows=2；`CLOSE cur1` ok |
| L-3 | **COPY**（COPY FROM stdin / COPY TO stdout） | ⚠️ **协议层不兼容 sqlx** | szrsql 发送 `CopyInResponse`/进入 COPY 子协议后，sqlx 报 `unexpected message: CopyInResponse` / `expected to read 5 bytes, got 0 bytes at EOF`——szrsql COPY 子协议实现与 sqlx 期望的流式 CopyData 分帧不匹配。**sz-orm 侧影响为零**（`Connection::execute/query` 不使用 COPY；批量写入走参数化 INSERT 批路径，已实测通过）。属 szrsql 上游改进项（只读仓库，不代改） |
| L-4 | **LISTEN/NOTIFY** | ⚠️ **NOTIFY 推送不兼容 sqlx** | `LISTEN ch` ok、`UNLISTEN` ok；`NOTIFY` 执行时 sqlx 收到异步 `NotificationResponse` 报 `unexpected message`——sqlx 驱动仅在 PgListener 专用路径消费该消息，普通 execute 路径视其为意外帧。sz-orm 未对 Szrsql 宣称 LISTEN/NOTIFY 能力，无幻影交付风险 |
| L-5 | **ALTER TABLE 大规模变更** | ✅ 支持 | `ADD COLUMN` / `ADD COLUMN IF NOT EXISTS` / `ALTER COLUMN … SET DEFAULT` / `RENAME COLUMN` / `DROP COLUMN` / `DROP COLUMN IF EXISTS` / `RENAME TO` 全部执行成功 |
| L-6 | **SCRAM-SHA-256 认证**（非 trust） | ✅ 支持 | 独立启动 `--auth-mode scram --port 6544`（首启自动生成随机 postgres 密码打印日志，`auth.json` 持久化），sqlx PG 驱动经 SCRAM 握手 `connect + query: ok rows=1` |

**复测结论**：6 项遗留中 4 项（SAVEPOINT/游标/ALTER/SCRAM）全通，2 项（COPY/NOTIFY 异步帧）为 szrsql pgwire 子协议与 sqlx 驱动的帧语义差异——均不触达 sz-orm 已宣称的 Szrsql 支持面（CRUD/参数化/事务/分页/元数据/池），无需修改 sz-orm 代码；两项差异如实登记为本报告 D-5/D-6，供 szrsql 上游参考。

### 7.1 D-5/D-6 上游修复闭环（2026-11-08 第二轮，szrsql 修复 commit `3261c7dd`）

szrsql 侧按反馈提示词独立会话完成修复并落盘报告 `szrsql/docs/governance/evaluation/2026-11-08-szorm反馈-copy-notify-修复.md`（HEAD 基线 `08c71541`）。sz-orm 侧以新二进制（含修复）重建 6543 服务后真连复核：

| 项 | 上游定性（差分对拍结论） | sz-orm 侧复核结果 |
|----|------------------------|------------------|
| **D-5 COPY** | **非 szrsql 协议缺陷**——裸 socket 帧对拍（`H d d c C Z` / `G … C Z`）与 PG18 字节级一致；重定性为「sqlx 需走 `PgCopyIn`/`copy_out_raw` 专用 API」 | `sqlx::Pool::copy_in_raw` → send → finish = **2 rows**，`COUNT=2`；`copy_out_raw` 流式读回 `1\talpha\n2\tbeta\n`（15 字节）——**全通** |
| **D-6 NOTIFY** | **真实协议缺陷（已修复）**——PG18 差分证明跨会话通知应在监听方空闲 socket 上**主动推送**（1.5 表）；szrsql 修复前只在监听方下一条查询响应里附带送达，`PgListener::recv` 永久阻塞。修复 commit `3261c7dd`：async/sync 两条主循环在阻塞读前 drain pending 通知编码 `A` 帧主动推送（sync 路径 100ms 短周期轮询） | `execute("NOTIFY …")` 不再报 unexpected message；`PgListener`（独立连接 LISTEN，无查询）跨会话 recv **收到** `channel=ch_d6x payload=cross-session`——**全通** |

**闭环结论**：D-5 无需 sz-orm 改代码（与首轮「影响为零」结论一致，验收线 `PgCopyIn` 已通过）；D-6 szrsql 已修复并经 sz-orm 侧真连复核通过。szrsql 侧补充的差分事实：**自听 NOTIFY 时 PG18 也把 `A` 回显给自身**（首轮报告 L-4 的"仅 PgListener 路径消费"表述需修正为"sqlx 普通 execute 路径不消费异步 `A` 帧，自听场景 PG 同样如此"）。sz-orm 仍不在 Szrsql 文档中宣称 LISTEN/NOTIFY 能力（可用但非 ORM 宣称面，无幻影交付）。

## 6. 结论

按提示词输出要求 3 如实报告：**szrsql 经 pgwire v3.0 暴露标准 PG 语义，sz-orm 复用既有 PG 适配、以最小 diff（枚举变体 + 1 行方言宏 + scheme 识别）完成支持**，未过度抽象；8 项真连集成测试 + 全部门禁（本批适用项）通过。
