# sz-orm v3.7.0 技术设计文档

> 版本：v3.7.0（真实数据库端到端测试体系 + 对比分析重评估与文档同步 + v3.6.0 探索能力成熟化 + 方言扩展延续 + 云数仓真实验证 + 工程规范化）
> 基线：v3.6.0（已完成 M1-M5：15 新表达式 + 195 missing_docs 全补齐 + QueryBuilder 渐进合并 47 测试 + Snowflake/Redshift 方言 32 测试 + async trait 重评估保持方案 C；workspace.package.version = "3.6.0"）
> 日期：2026-08-10
> 文档定位：技术设计（How to build），对应需求规格 `docs/spec/v3.7.0/spec.md`（6 方向 / 28 条 EARS 需求 / 6 组 REQ-E2E/REQ-REEVAL/REQ-MAT/REQ-DIALECT/REQ-CLOUD/REQ-ENG）
> 设计约束：Rust 2021 Edition / rust-version 1.81 / API 向后兼容（无 Breaking Change）/ 禁止占位实现 / unsafe 零容忍 / 参数化查询铁律 / Feature 隔离 / 五方言行为一致 / ADR-0001 严禁修改下游/上游仓库 / 编译时 `$env:RUST_MIN_STACK="67108864"` + `$env:CARGO_INCREMENTAL=0` / 测试 `cargo test --workspace -j 2 --no-fail-fast`
> 优先级声明：六项能力按"真实 DB e2e 测试体系(1,最高,补 v3.6.0 最大缺口) → 对比分析重评估(2,高,纯文档工作) → 探索能力成熟化(3,高,补齐测试文档) → 方言扩展延续(4,中,需 Rust 驱动成熟) → 云数仓真实验证(5,中,需云实例可用) → 工程规范化(6,低但必须,git 入库 + Prisma 评估落地)"的收益/风险序推进；方向 1 为最高收益中风险（建立真实 DB e2e 测试体系，需 feature gate 隔离 + 本机 DB 实例 + 复用既有 adapter），方向 2 为高收益低风险（纯文档工作不引入新代码依赖），方向 3 为高收益低风险（既有探索实现仅补齐测试与文档），方向 4 为中收益中风险（新方言实现需 Rust 驱动 + 行为一致验证），方向 5 为中收益中风险（需云实例可用，无实例时输出缺口报告），方向 6 为低收益低风险（git 提交 + 门禁 + Prisma 评估落地）
> 缺陷来源：v3.6.0 端到端测试缺口（96 e2e 测试全用 InMemoryDb，63 真实 DB 测试全 `#[ignore]`）+ 对比分析文档滞后（停留 v3.5.0 基线）+ v3.6.0 探索性质能力（typed relation / 自定义诊断 / proc-macro SQL 验证）成熟度不足 + v3.5.0 方言扩展路线图 v3.7.0 候选（Informix/SAP HANA/Firebird）+ v3.6.0 Snowflake/Redshift 无真实云数据库验证 + 213 文件未提交 git

---

# 一、需求与存量功能关系分析

## 1.1 需求功能与存量功能对比

v3.7.0 的六项优化任务与 v3.6.0 已交付代码的关系如下。v3.6.0 已完成 M1-M5 五个里程碑：编译期类型安全深入优化（15 新表达式 CTE 3 + Window Frame 6 + JSON 6 + typed relation + 自定义诊断 + proc-macro SQL 验证）、313 pub API 文档补齐（195 missing_docs 全补齐 + 全局 `#![warn(missing_docs)]` 启用）、QueryBuilder 渐进合并（lint + fix + 差分测试 47 测试）、方言扩展（Snowflake + Redshift 20 种方言 32 测试）、async trait 重评估（保持方案 C），workspace 版本 3.6.0。本版本在此基础上向"真实 DB e2e 测试可信度、文档与代码一致性、探索能力正式化、方言覆盖度、云数仓验证完整性、工程规范"六个维度能力突破，所有新增能力以扩展模块 + feature gate 方式提供，不修改 sz-orm-core / sz-orm-macros / 扩展包既有公开 API 签名（满足 spec §4.5 兼容性约束：既有公开 API 完全向后兼容）。

### 1.1.1 已实现功能

| 需求功能 | 存量功能 | 代码位置 | 匹配度 |
|---------|---------|---------|--------|
| 既有 e2e 测试基础设施（方向 1 基础） | 5 个 `e2e_*.rs` 文件 96 个测试用 `InMemoryDb`（逻辑端到端，验证 SQL 下推 + 真实执行 + 行为验证），覆盖事务/批 upsert/keyset 分页/migration/eager cycle | [packages/sz-orm-core/tests/e2e_transaction.rs:27](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/e2e_transaction.rs#L27)（`use common::{InMemoryDb, TransactionalConnection}`）+ e2e_batch_upsert.rs:20 + e2e_eager_cycle.rs:20 + e2e_keyset.rs:20 + e2e_migration.rs:20 | 75%（96 测试用 InMemoryDb 已验证业务路径，缺真实 DB 执行链路） |
| 既有 integration 测试基础设施（方向 1 基础） | 7 个 `integration_*.rs` 文件 63 个测试全 `#[ignore]`（需真实 DB），覆盖 MySQL/PG/SQLite/Oracle/MSSQL/Redis/DuckDB | [packages/sz-orm-core/tests/integration_mysql.rs:113](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/integration_mysql.rs#L113)（`#[ignore = "需要 MySQL 9.6.0 运行于 127.0.0.1:3306"]`）+ integration_pg.rs:18 + integration_oracle.rs:10 + integration_mssql.rs:8 + integration_redis.rs:4 | 50%（63 测试连真实 DB 但全 `#[ignore]`，需 feature gate 启用时默认运行） |
| 既有 tests/common/ adapter 基础设施（方向 1 基础） | `tests/common/` 含 sqlx_pg_adapter / sqlx_mysql_adapter / rusqlite_adapter / schema_builder / equivalence / soak / InMemoryDb / TransactionalConnection，复用既有连接管理与占位符转换 | [packages/sz-orm-core/tests/common/mod.rs:7](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/common/mod.rs#L7)（pub mod equivalence; rusqlite_adapter; schema_builder; soak; sqlx_mysql_adapter; sqlx_pg_adapter）+ [common/sqlx_pg_adapter.rs:15](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/common/sqlx_pg_adapter.rs#L15)（SqlxPgAdapter） | 100%（adapter 完整可复用，真实 DB e2e 测试直接复用） |
| 既有 feature gate 体系（方向 1/3/4 基础） | sz-orm-core 已有 25+ feature（default=["redis"]，含 testing/db-verify/redis/circuit-breaker/rate-limit/auto-prewarm/plan-cache/zero-copy/simd/multi-tenant-enhanced/dist-cache/test-coverage/arch-improvement/doc-completion/perf-smallstring/perf-enum-dispatch/perf-zero-copy-l2/perf-box-str/type-safe-columns/typed-column/typed-dsl/l1-cache/dialect-cockroachdb/dialect-yugabytedb/migration-guide/typed-relation/sql-verify-proc/qb-migration-tool/dialect-snowflake/dialect-redshift） | [packages/sz-orm-core/Cargo.toml:13](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L13)（[features]）+ [Cargo.toml:58](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L58)（typed-relation）+ [Cargo.toml:60](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L60)（sql-verify-proc）+ [Cargo.toml:62](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L62)（qb-migration-tool）+ [Cargo.toml:70](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L70)（dialect-snowflake）+ [Cargo.toml:72](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L72)（dialect-redshift） | 100%（25+ feature 完整，新增 e2e-real-db/dialect-informix/dialect-saphana/dialect-firebird feature 即可） |
| typed relation 探索实现（方向 3 基础） | v3.6.0 M1 已实现 typed_relation.rs（BelongsTo/HasMany/HasOne + TypedTable trait + Relation trait + 编译期外键类型校验），通过 `typed-relation` feature gate 隔离 | [packages/sz-orm-core/src/typed_relation.rs:35](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_relation.rs#L35)（TypedTable trait）+ [typed_relation.rs:45](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_relation.rs#L45)（Relation trait）+ [lib.rs:494](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L494)（`#[cfg(feature = "typed-relation")] pub mod typed_relation`） | 75%（探索实现已有，缺正式 feature 标注 + 测试覆盖下限 10 + 文档完整 + 稳定性标注） |
| 自定义编译期诊断探索实现（方向 3 基础） | v3.6.0 M1 已实现 diagnostic.rs（TypeMismatchDiagnostic 结构 + suggestions 预设 + format_error 方法），通过 `typed-dsl` feature gate 隔离 | [packages/sz-orm-macros/src/diagnostic.rs:10](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/src/diagnostic.rs#L10)（TypeMismatchDiagnostic）+ [diagnostic.rs:33](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/src/diagnostic.rs#L33)（format_error）+ [sz-orm-macros/Cargo.toml:37](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/Cargo.toml#L37)（typed-dsl feature）+ [sz-orm-macros/tests/typed_ast_diagnostic_test.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/tests/typed_ast_diagnostic_test.rs) | 75%（探索实现已有，缺独立 `custom-diagnostic` feature gate + 测试覆盖下限 10 + 文档完整） |
| proc-macro SQL 验证探索实现（方向 3 基础） | v3.6.0 M1 已实现 sql_verify.rs（VerifyResult + SQL 解析 + 表/列存在性校验 + 类型匹配 + EXPLAIN only + 缓存），通过 `sql-verify-proc` feature gate 隔离；既有 `query!` 宏 db-verify feature（连真 DB EXPLAIN） | [packages/sz-orm-core/src/sql_verify.rs:22](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/sql_verify.rs#L22)（VerifyResult）+ [lib.rs:496](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L496)（`#[cfg(feature = "sql-verify-proc")] pub mod sql_verify`）+ [sz-orm-macros/Cargo.toml:31](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/Cargo.toml#L31)（db-verify feature）+ [sz-orm-macros/src/lib.rs:464](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/src/lib.rs#L464)（query! 宏） | 75%（探索实现已有，缺连真 DB EXPLAIN 覆盖所有 QueryBuilder 路径 + 测试覆盖下限 10 + 文档完整） |
| SnowflakeDialect 实现（方向 5 基础） | v3.6.0 M4 已实现 SnowflakeDialect（Dialect trait + VARIANT/OBJECT/ARRAY + COPY INTO + TIME TRAVEL），通过 `dialect-snowflake` feature gate 隔离，32 测试均为 SQL 生成测试 | [packages/sz-orm-core/src/dialect.rs:1567](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1567)（SnowflakeDialect）+ [dialect.rs:1750](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1750)（Snowflake 特有方法）+ [tests/dialect_snowflake_test.rs:5](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/dialect_snowflake_test.rs#L5)（`#![cfg(feature = "dialect-snowflake")]`） | 75%（SQL 生成测试已有，缺真实云数据库行为一致性验证） |
| RedshiftDialect 实现（方向 5 基础） | v3.6.0 M4 已实现 RedshiftDialect（委派 PostgreSqlDialect + COPY/UNLOAD 特性扩展），通过 `dialect-redshift` feature gate 隔离 | [packages/sz-orm-core/src/dialect.rs:1794](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1794)（`delegate_dialect_to!(RedshiftDialect, PostgreSqlDialect, DbType::Redshift)`）+ [dialect.rs:1798](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1798)（Redshift 特有方法）+ [tests/dialect_redshift_test.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/dialect_redshift_test.rs) | 75%（SQL 生成测试已有，缺真实云数据库行为一致性验证） |
| DbType 枚举 23 变体（方向 4 基础） | DbType 枚举含 23 变体（v3.6.0 在 v3.5.0 的 21 变体基础上新增 Snowflake + Redshift），`#[non_exhaustive]` 允许扩展 | [packages/sz-orm-core/src/db_type.rs:11](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/db_type.rs#L11)（DbType 枚举）+ [db_type.rs:61](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/db_type.rs#L61)（Snowflake）+ [db_type.rs:65](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/db_type.rs#L65)（Redshift） | 100%（#[non_exhaustive] 允许新增 Informix/SAP HANA/Firebird 变体） |
| 20 种方言实现（方向 4 基础） | 9 独立方言（MySqlDialect/PostgreSqlDialect/SqliteDialect/OracleDialect/SqlServerDialect/SnowflakeDialect/ClickHouseDialect/DuckDBDialect/Db2Dialect）+ 11 委派方言（MariaDB/TiDB→MySQL、KingbaseES/PolarDB/GaussDB/CockroachDB/YugabyteDB/Redshift→PG、Dameng→Oracle、Sybase/GBase→SQL Server） | [packages/sz-orm-core/src/dialect.rs:228](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L228)（MySqlDialect）至 [dialect.rs:2292](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L2292)（Db2Dialect）+ 11 delegate_dialect_to! 宏调用 | 100%（20 方言完整，需补 Informix/SAP HANA/Firebird 达 23 种） |
| Dialect trait + delegate_dialect_to 宏（方向 4 基础） | Dialect trait（quote/escape_string/build_pagination/supports_returning/json_extract/full_text_search/build_create_table 等）+ delegate_dialect_to! 宏（委派基础方言） | [packages/sz-orm-core/src/dialect.rs:23](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L23)（Dialect trait）+ [dialect.rs:1429](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1429)（delegate_dialect_to 宏） | 100%（trait + 宏完整，新方言可独立实现或委派） |
| 全局 missing_docs lint（v3.6.0 已补齐） | v3.6.0 M2 已补齐 195 missing_docs，全局 `#![warn(missing_docs)]` 启用，`cargo doc --workspace --no-deps --all-features` 零警告 | [packages/sz-orm-core/src/lib.rs:404](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L404)（`#![warn(missing_docs)]`） | 100%（文档完整度已对齐竞品） |
| Prisma 方言兼容评估文档（方向 6 基础） | v3.6.0 M4 已输出 Prisma 评估文档（79 行，含 Schema DSL 映射 + 查询语法映射 + 跨生态可行性 + 推荐方案） | [docs/prisma-dialect-evaluation.md:1](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/prisma-dialect-evaluation.md#L1) | 100%（评估文档已有，需确认结论落地） |
| 对比分析文档（方向 2 基础） | v3.5.0 已完成对比分析文档（963 行，13 维度评分 + §5 独特优势 + §6 已知不足 + §7 结论与建议），但头部标注 `版本：v3.5.0 | 日期：2026-08-09`，文档滞后一个版本 | [docs/sz-orm与同类产品对比分析.md:3](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/sz-orm与同类产品对比分析.md#L3)（`版本：v3.5.0 | 日期：2026-08-09`） | 50%（文档结构完整，但版本基线滞后 v3.6.0，需重评估 13 维度评分 + 更新 §6 已知不足 + §7 结论） |
| workspace 版本集中管理 | `workspace.package.version = "3.6.0"`，edition="2021"，rust-version="1.81" | [Cargo.toml:6](file:///E:/vue/test/鲜视达/rust/sz-orm/Cargo.toml#L6) | 100%（需升级到 3.7.0） |
| sz-pay 生产依赖证据 | sz-pay 从 crates.io 拉取 sz-orm-core/sqlx/config/auth/macros/queue 6 个包 | `E:\vue\test\sz-pay\server\sz-rust\Cargo.toml` | 100%（需升级到 3.7.0 零回归验证） |
| 14 道门禁脚本（方向 6 基础） | scripts/ 已有 gate.ps1/gate.sh + check-sql-injection.ps1 + check-doc-consistency.py + check-doc-sync.py + audit-verify.sh/ps1 + check-upstream-unmodified.sh/ps1 + audit-api-changes.ps1 等 30+ 脚本 | [scripts/gate.ps1](file:///E:/vue/test/鲜视达/rust/sz-orm/scripts/gate.ps1) + [scripts/check-sql-injection.ps1](file:///E:/vue/test/鲜视达/rust/sz-orm/scripts/check-sql-injection.ps1) + [scripts/audit-verify.sh](file:///E:/vue/test/鲜视达/rust/sz-orm/scripts/audit-verify.sh) | 100%（门禁脚本完整，需运行验证全通过） |
| 本机数据库连接信息（方向 1 基础） | MySQL 9.6（`mysql://root:test123@127.0.0.1:3306/sz_orm_test`）+ PostgreSQL 18（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）+ Oracle 23ai Free（`127.0.0.1:1521/freepdb1`，用户 sys，密码 test123，Sysdba 权限）+ SQLite（文件型） | AGENTS.md 记录 + spec.md §1.2.8 | 100%（本机 DB 实例可用，真实 DB e2e 测试可连） |

### 1.1.2 需要扩展的功能

| 需求功能 | 存量功能 | 差异说明 | 扩展方向 |
|---------|---------|---------|---------|
| 真实 DB e2e 测试套件（REQ-E2E-001/002） | 既有 96 e2e 测试用 InMemoryDb（逻辑端到端）+ 63 integration 测试全 `#[ignore]` | 执行链路差异：InMemoryDb 不连真实 DB，无法验证真实 SQL 行为（UPSERT/行锁/标识符引用/方言特有语法）；运行层次差异：integration 测试全 `#[ignore]` 不默认运行，缺"feature gate 启用时默认运行"层次；覆盖路径差异：缺 8 类核心业务路径（CRUD/事务/预加载/分页/软删除/多租户/缓存/方言行为）的真实 DB e2e 测试 | 新增 `tests/e2e_real_db_*.rs` 真实 DB e2e 测试套件，复用 `tests/common/` 既有 adapter（SqlxPgAdapter/SqlxMysqlAdapter/RusqliteAdapter），覆盖 8 类核心业务路径，通过 `e2e-real-db` feature gate 隔离（新增），默认关闭，CI 中启用，本地有 DB 时可手动启用；既有 96 e2e + 63 integration 测试保留不替换 |
| e2e-real-db feature gate 隔离（REQ-E2E-003） | 既有 25+ feature 无 `e2e-real-db` | 隔离差异：缺真实 DB e2e 测试的 feature gate，无 DB 环境编译失败风险 | sz-orm-core/Cargo.toml 新增 `e2e-real-db = []` feature（默认关闭），真实 DB e2e 测试用 `#[cfg(feature = "e2e-real-db")]` 条件编译隔离，无 DB 环境不编译 |
| 真实 DB e2e 测试幂等且隔离（REQ-E2E-004） | 既有 integration 测试无清理机制（全 `#[ignore]`，未考虑重复运行） | 幂等差异：缺 DROP TABLE IF EXISTS + 重建清理机制；隔离差异：缺独立表名或独立事务回滚 | 真实 DB e2e 测试套件实现清理机制（每次运行前 DROP TABLE IF EXISTS + 重建），每个测试用独立表名（如 `e2e_test_crud_<uuid>`）或独立事务回滚，保证重复运行结果一致 |
| 对比分析文档更新到 v3.6.0 基线（REQ-REEVAL-001） | 对比分析文档头部标注 `版本：v3.5.0 | 日期：2026-08-09`，文档滞后一个版本 | 版本差异：文档基线 v3.5.0，代码基线 v3.6.0，滞后一个版本；能力差异：v3.6.0 新增 15 表达式 + typed relation + 自定义诊断 + proc-macro SQL 验证 + Snowflake/Redshift 方言 + 195 missing_docs 补齐 + QueryBuilder 迁移工具，文档未反映 | 更新 [docs/sz-orm与同类产品对比分析.md:3](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/sz-orm与同类产品对比分析.md#L3) 头部为 `版本：v3.6.0 | 日期：2026-08-10`，代码基线更新为 `Cargo.toml:6 workspace.package.version = "3.6.0"`，能力清单反映 v3.6.0 实际能力 |
| 逐维度重新评分附证据（REQ-REEVAL-002） | v3.5.0 评分基于 v3.5.0 代码证据，未反映 v3.6.0 改进 | 评分差异：13 维度（异步/类型安全/连接池/方言/查询API/事务/缓存/N+1/安全/性能/宏/文档生态/生产就绪）需基于 v3.6.0 实际能力重新评分；证据差异：每条变更须附 v3.6.0 file:line 证据 | 逐维度重新评分，每条变更附 v3.6.0 file:line 证据（如类型安全维度更新为"61 种表达式 + typed relation + 自定义诊断"，附 [typed_ast.rs:397](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L397) + [typed_relation.rs:35](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_relation.rs#L35) 证据） |
| 已知不足标注 v3.6.0 改进状态（REQ-REEVAL-003） | §6 各子节未标注 v3.6.0 是否已改进 | 标注差异：§6 各子节须标注 v3.6.0 改进状态（✅ 已改进 / ⚠️ 部分改进 / ❌ 未改进）+ 证据 | §6 各子节标注 v3.6.0 改进状态，如 §6.1 生态成熟度标注"⚠️ 部分改进（v3.6.0 已补齐 195 missing_docs，但社区规模未扩展）"，附 [lib.rs:404](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L404) 证据 |
| typed relation 转正式 feature（REQ-MAT-001） | v3.6.0 探索实现通过 `typed-relation` feature gate 隔离，缺正式 feature 标注 + 测试覆盖下限 10 + 文档完整 | 成熟度差异：缺稳定性标注（Cargo.toml 注释 "stable"）+ 测试覆盖下限 10 + 文档完整（迁移指南 + 适用场景） | typed-relation feature 标注 "stable"（[Cargo.toml:58](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L58) 注释更新），补齐测试覆盖至 ≥10 用例（编译期校验 + 运行时查询 + 外键类型匹配 + 表归属 + 与 EagerLoader 协作），补齐文档（迁移指南 + 适用场景 + escape hatch 说明） |
| 自定义编译期诊断转正式 feature（REQ-MAT-002） | v3.6.0 探索实现通过 `typed-dsl` feature gate 隔离（在 sz-orm-macros），缺独立 `custom-diagnostic` feature gate | 隔离差异：自定义诊断与 typed-dsl 共用 feature gate，缺独立 feature 便于按需启用；成熟度差异：缺测试覆盖下限 10 + 文档完整 | sz-orm-macros/Cargo.toml 新增 `custom-diagnostic = []` feature（默认关闭），自定义诊断用 `#[cfg(feature = "custom-diagnostic")]` 条件编译隔离，补齐测试覆盖至 ≥10 用例（错误位置 + 期望类型 + 实际类型 + 修复建议 + 各诊断场景），补齐文档 |
| proc-macro SQL 验证转正式 feature（REQ-MAT-003） | v3.6.0 探索实现通过 `sql-verify-proc` feature gate 隔离，缺连真 DB EXPLAIN 覆盖所有 QueryBuilder 路径 | 覆盖差异：缺连真 DB EXPLAIN 覆盖所有 QueryBuilder 路径（SELECT/INSERT/UPDATE/DELETE/JOIN/子查询/CTE/窗口）；成熟度差异：缺测试覆盖下限 10 + 文档完整 | sql-verify-proc feature 标注 "stable"，扩展连真 DB EXPLAIN 覆盖所有 QueryBuilder 路径（SELECT/INSERT/UPDATE/DELETE/JOIN/子查询/CTE/窗口），补齐测试覆盖至 ≥10 用例，补齐文档（启用方式 + DATABASE_URL 配置 + 降级模式说明） |
| Informix 方言按需实现（REQ-DIALECT-001） | 20 种方言无 Informix | 方言差异：Informix 特性（SERIAL/ROW 类型、PUT 语句），需 Rust 驱动成熟；路线图差异：v3.5.0 路线图建议 v3.7.0 实现 | 评估 Rust Informix 驱动成熟度，如成熟则实现 InformixDialect（Dialect trait + DbType::Informix 变体 + 方言测试），通过 `dialect-informix` feature gate 隔离（新增）；如不成熟则标注"SQL generation only, no real DB driver"，仅实现 SQL 生成方言 |
| SAP HANA 方言按需实现（REQ-DIALECT-002） | 20 种方言无 SAP HANA | 方言差异：SAP HANA 特性（计算列、CE 函数），需 Rust 驱动成熟 + 企业需求 | 评估 Rust SAP HANA 驱动成熟度 + 企业需求，如成熟且需求出现则实现 SapHanaDialect，通过 `dialect-saphana` feature gate 隔离（新增）；如不成熟或无需求则标注暂缓 |
| Firebird 方言按需实现（REQ-DIALECT-003） | 20 种方言无 Firebird | 方言差异：Firebird 特性（GENERATOR/SEQUENCE、EXECUTE BLOCK），需用户需求出现 | 评估用户需求，如需求出现则实现 FirebirdDialect，通过 `dialect-firebird` feature gate 隔离（新增）；如无需求则标注暂缓 |
| Snowflake 真实云数据库验证（REQ-CLOUD-001） | v3.6.0 SnowflakeDialect 32 测试均为 SQL 生成测试，未连真实云数据库 | 验证差异：缺真实 Snowflake 实例验证 SQL 行为一致性（UPSERT/TIME TRAVEL/VARIANT 类型等） | 评估 Snowflake 云实例可用性，如有可用云实例则连真实云验证行为一致性，输出验证报告；如无可用云实例则输出验证缺口报告 + 替代方案（如本地 Snowflake 模拟或仅 SQL 生成 + 人工审核） |
| Redshift 真实云数据库验证（REQ-CLOUD-002） | v3.6.0 RedshiftDialect 测试均为 SQL 生成测试，未连真实云数据库 | 验证差异：缺真实 Redshift 实例验证 SQL 行为一致性（COPY/UNLOAD/PG 兼容性等） | 评估 Redshift 云实例可用性（AWS Redshift Serverless 或 Provisioned），如有可用云实例则连真实云验证，输出验证报告；如无可用云实例则输出验证缺口报告 |
| v3.6.0 未提交工作入库（REQ-ENG-001） | git status 显示 213 个未提交改动文件，git log 只到 v3.3.0（`ce58e7c feat(v3.3.0): 企业级数据治理...`），v3.6.0 工作未入库 | 入库差异：213 文件未提交，v3.6.0 五个里程碑工作未入库；规范差异：提交信息须遵循既有风格（feat/docs/refactor 前缀） | 按 v3.6.0 五个里程碑分组提交 git（M1 编译期类型安全 / M2 文档补齐 / M3 QueryBuilder 合并 / M4 方言扩展 / M5 async trait 重评估），提交信息遵循既有风格，每次提交后运行门禁验证 |
| Prisma 评估结论落地（REQ-ENG-002） | v3.6.0 已输出 Prisma 评估文档（79 行），但结论未正式落地 | 落地差异：评估文档已有但结论未标注正式落地（可行性/推荐方案/实现计划） | 审查 [docs/prisma-dialect-evaluation.md](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/prisma-dialect-evaluation.md) 结论，标注正式落地（如评估为不可行则标注"不实施，跨生态兼容难度高收益低"+ 理由；如可行则输出实现计划） |

### 1.1.3 需要新增的功能或接口

按业务模块分组，以下功能在存量代码中完全没有对应实现，需新增。

**模块 A：真实 DB e2e 测试套件（对应 REQ-E2E-001/002/004/005）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| e2e_real_db_crud.rs | DATABASE_URL + 真实 DB 实例 | CRUD 测试通过/失败 | 连真实 DB（复用 SqlxPgAdapter/SqlxMysqlAdapter/RusqliteAdapter），建表 + 准备数据 + 执行 CRUD（insert/select/update/delete）+ 断言结果 + 清理 | 既有 tests/common/ adapter |
| e2e_real_db_transaction.rs | DATABASE_URL + 真实 DB 实例 | 事务测试通过/失败 | 连真实 DB，验证事务 commit/rollback/savepoint 真实行为 | 既有 tests/common/ adapter |
| e2e_real_db_eager_load.rs | DATABASE_URL + 真实 DB 实例 | 预加载测试通过/失败 | 连真实 DB，验证 eager load（BelongsTo/HasMany/HasOne）真实行为 + N+1 检测 | 既有 tests/common/ adapter + EagerLoader |
| e2e_real_db_pagination.rs | DATABASE_URL + 真实 DB 实例 | 分页测试通过/失败 | 连真实 DB，验证 offset/limit + keyset 分页真实行为 | 既有 tests/common/ adapter |
| e2e_real_db_soft_delete.rs | DATABASE_URL + 真实 DB 实例 | 软删除测试通过/失败 | 连真实 DB，验证软删除（deleted_at 字段 + 过滤）真实行为 | 既有 tests/common/ adapter |
| e2e_real_db_multi_tenant.rs | DATABASE_URL + 真实 DB 实例 | 多租户测试通过/失败 | 连真实 DB，验证多租户（tenant_id 隔离 + 行级安全）真实行为 | 既有 tests/common/ adapter + tenant_context |
| e2e_real_db_cache.rs | DATABASE_URL + 真实 DB 实例 | 缓存测试通过/失败 | 连真实 DB，验证 L1/L2 缓存真实行为（缓存命中/失效/一致性） | 既有 tests/common/ adapter + l1_cache + l2_cache |
| e2e_real_db_dialect_behavior.rs | DATABASE_URL + 真实 DB 实例 | 方言行为一致性测试通过/失败 | 连真实 DB，验证方言行为一致性（UPSERT/行锁/标识符引用/方言特有语法） | 既有 tests/common/ adapter + Dialect trait |
| 测试清理机制 | 真实 DB 连接 | 清理完成 | 每次运行前 DROP TABLE IF EXISTS + 重建，每个测试用独立表名（如 `e2e_test_crud_<uuid>`）或独立事务回滚 | 既有 tests/common/ schema_builder |
| 测试超时机制 | 测试执行 | 超时标记 | 单方言 60 秒超时，全方言 300 秒超时，超时标记失败并输出卡点 | tokio::time::timeout |

**模块 B：对比分析文档重评估（对应 REQ-REEVAL-001/002/003/004）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| 文档头部更新 | v3.6.0 代码基线 | 更新后的文档头部 | 更新版本号为 v3.6.0，日期为 2026-08-10，代码基线为 Cargo.toml:6 workspace.package.version = "3.6.0" | 无（纯文档） |
| 13 维度重新评分 | v3.6.0 代码证据 | 更新后的评分矩阵 | 逐维度基于 v3.6.0 实际能力重新评分，每条变更附 v3.6.0 file:line 证据 | v3.6.0 代码库 |
| §6 已知不足更新 | v3.6.0 改进状态 | 更新后的 §6 | 各子节标注 v3.6.0 改进状态（✅/⚠️/❌）+ 证据 | v3.6.0 代码库 |
| §7 结论与建议更新 | v3.6.0 后真实状态 | 更新后的 §7 | 综合结论、定位建议、改进建议反映 v3.6.0 后真实状态，建议反映 v3.7.0+ 方向 | v3.6.0 代码库 |
| §5 独特优势更新 | v3.6.0 新增项 | 更新后的 §5 | 新增 v3.6.0 独特优势项（15 新表达式 + typed relation + 自定义诊断 + proc-macro SQL 验证 + Snowflake/Redshift 方言 + QueryBuilder 迁移工具） | v3.6.0 代码库 |

**模块 C：探索能力成熟化（对应 REQ-MAT-001/002/003）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| typed relation 测试补齐 | typed_relation.rs 既有实现 | ≥10 测试用例 | 补齐测试覆盖：编译期外键类型校验 + 运行时关联查询 + 表归属校验 + 与 EagerLoader 协作 + escape hatch | 既有 typed_relation.rs + EagerLoader |
| typed relation 文档补齐 | typed_relation.rs 既有实现 | 完整文档 | 补齐文档：迁移指南（从 EagerLoader 到 typed relation）+ 适用场景 + escape hatch 说明 + 稳定性标注 | 既有 typed_relation.rs |
| 自定义诊断独立 feature gate | diagnostic.rs 既有实现 | custom-diagnostic feature | sz-orm-macros/Cargo.toml 新增 `custom-diagnostic = []` feature，自定义诊断用 `#[cfg(feature = "custom-diagnostic")]` 条件编译隔离 | 既有 diagnostic.rs |
| 自定义诊断测试补齐 | diagnostic.rs 既有实现 | ≥10 测试用例 | 补齐测试覆盖：错误位置 + 期望类型 + 实际类型 + 修复建议 + 各诊断场景（Eq/And/Or/filter 跨表） | 既有 diagnostic.rs |
| proc-macro SQL 验证连真 DB EXPLAIN 覆盖 | sql_verify.rs 既有实现 | 所有 QueryBuilder 路径覆盖 | 扩展连真 DB EXPLAIN 覆盖所有 QueryBuilder 路径（SELECT/INSERT/UPDATE/DELETE/JOIN/子查询/CTE/窗口），复用既有 db-verify 连真 DB 逻辑 | 既有 sql_verify.rs + db-verify feature |
| proc-macro SQL 验证测试补齐 | sql_verify.rs 既有实现 | ≥10 测试用例 | 补齐测试覆盖：SQL 解析 + 表/列存在性 + 类型匹配 + EXPLAIN only + 缓存 + 降级模式 | 既有 sql_verify.rs |

**模块 D：方言扩展（对应 REQ-DIALECT-001/002/003）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| InformixDialect 实现 | SQL 构造请求 | Informix SQL | 评估 Rust Informix 驱动成熟度，如成熟则实现 Dialect trait（quote/escape_string/build_pagination/SERIAL/ROW 类型/PUT 语句），如不成熟则仅实现 SQL 生成方言 | 既有 Dialect trait |
| SapHanaDialect 实现 | SQL 构造请求 | SAP HANA SQL | 评估 Rust SAP HANA 驱动成熟度 + 企业需求，如成熟且需求出现则实现 Dialect trait（计算列/CE 函数） | 既有 Dialect trait |
| FirebirdDialect 实现 | SQL 构造请求 | Firebird SQL | 评估用户需求，如需求出现则实现 Dialect trait（GENERATOR/SEQUENCE/EXECUTE BLOCK） | 既有 Dialect trait |
| DbType 新增变体 | 无 | DbType 枚举变体 | db_type.rs 新增 Informix/SapHana/Firebird 变体（#[non_exhaustive] 允许） | 既有 DbType |
| 方言扩展路线图更新 | v3.7.0 实现状态 | 更新后的路线图 | 路线图标注 v3.7.0 已实现/暂缓状态 | 既有路线图 |

**模块 E：云数仓真实验证（对应 REQ-CLOUD-001/002/003）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| Snowflake 真实云验证 | Snowflake 云实例（如可用） | 验证报告 | 评估 Snowflake 云实例可用性，如有则连真实云验证行为一致性（UPSERT/TIME TRAVEL/VARIANT 类型），如无则输出缺口报告 + 替代方案 | 既有 SnowflakeDialect |
| Redshift 真实云验证 | Redshift 云实例（如可用） | 验证报告 | 评估 Redshift 云实例可用性，如有则连真实云验证行为一致性（COPY/UNLOAD/PG 兼容性），如无则输出缺口报告 | 既有 RedshiftDialect |
| 验证报告文档化 | 验证结果 | 验证报告文档 | 验证报告附测试用例、结果、与 SQL 生成测试的差异（如有） | 无 |

**模块 F：工程规范化（对应 REQ-ENG-001/002/003/004）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| v3.6.0 未提交工作入库 | 213 个未提交文件 | git 提交完成 | 按 v3.6.0 五个里程碑分组提交 git，提交信息遵循既有风格（feat/docs/refactor 前缀），每次提交后运行门禁验证 | git |
| Prisma 评估结论落地 | 既有 Prisma 评估文档 | 正式落地文档 | 审查评估结论，标注正式落地（可行性/推荐方案/实现计划或不可行理由） | 既有 [docs/prisma-dialect-evaluation.md](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/prisma-dialect-evaluation.md) |
| 14 道门禁全通过 | v3.7.0 交付 | 门禁通过 | 运行 14 道门禁（fmt/check/clippy/test/doc/audit/integration/占位检查/SQL注入/feature全组合/上游未改/文档一致性/审计证据/文档同步），全部通过才允许交付 | 既有 scripts/ 门禁脚本 |

## 1.2 存量功能详细分析

### 1.2.1 既有 e2e 测试基础设施（e2e_*.rs + InMemoryDb，96 测试）

- **接口契约**：
  - `InMemoryDb`（[tests/common/mod.rs:29](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/common/mod.rs#L29)）：内存数据库状态，`tables: HashMap<String, Vec<HashMap<String, Value>>>`，实现 Clone 支持事务快照。
  - `TransactionalConnection`（[tests/common/mod.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/common/mod.rs)）：事务连接包装，begin_transaction 时克隆 InMemoryDb 状态，rollback 时恢复快照，commit 时丢弃快照。
  - 5 个 e2e 测试文件：e2e_transaction.rs（16 测试）+ e2e_batch_upsert.rs（20 测试）+ e2e_eager_cycle.rs（20 测试）+ e2e_keyset.rs（20 测试）+ e2e_migration.rs（20 测试）= 96 测试。
- **业务规则**：所有 e2e 测试用 InMemoryDb 验证完整业务流程（SQL 下推 + 真实执行 + 行为验证），不连真实数据库，随 `cargo test` 默认运行。InMemoryDb 实现基本 INSERT/SELECT/UPDATE/DELETE 语义，支持事务快照（Clone + 恢复）。
- **扩展点**：v3.7.0 新增真实 DB e2e 测试套件（`tests/e2e_real_db_*.rs`），复用既有 tests/common/ adapter（SqlxPgAdapter/SqlxMysqlAdapter/RusqliteAdapter），连真实 DB 验证 8 类核心业务路径，通过 `e2e-real-db` feature gate 隔离。既有 96 e2e 测试保留不替换。
- **约束**：
  - 不替换既有 InMemoryDb e2e 测试：既有 96 测试保留，真实 DB e2e 测试为新增补充（spec §1.4.3）。
  - 不连生产数据库：真实 DB e2e 测试必须使用 `sz_orm_test` 测试库（spec §4.3.1）。
  - 测试幂等且隔离：每次运行清理残留，测试间互不干扰（spec §4.2.1/4.2.2）。

### 1.2.2 既有 integration 测试基础设施（integration_*.rs，63 测试全 `#[ignore]`）

- **接口契约**：
  - 7 个 integration 测试文件：integration_mysql.rs（23 ignored）+ integration_pg.rs（18 ignored）+ integration_oracle.rs（10 ignored）+ integration_mssql.rs（8 ignored）+ integration_redis.rs（4 ignored）+ integration_sqlite.rs（0 ignored）+ integration_duckdb.rs（0 ignored）= 63 ignored。
  - 每个测试标注 `#[ignore = "需要 X 运行于 127.0.0.1:port"]`（如 [integration_mysql.rs:113](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/integration_mysql.rs#L113) `#[ignore = "需要 MySQL 9.6.0 运行于 127.0.0.1:3306"]`）。
  - 运行方式：`cargo test --package sz-orm-core --test integration_mysql -- --ignored --nocapture`（[integration_mysql.rs:10](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/integration_mysql.rs#L10)）。
- **业务规则**：integration 测试连真实 DB（MySQL/PG/SQLite/Oracle/MSSQL/Redis/DuckDB），但全 `#[ignore]` 不默认运行，需手动 `--ignored` 标志运行。覆盖各方言的 CRUD/事务/连接池/方言特性。
- **扩展点**：v3.7.0 新增真实 DB e2e 测试套件为"默认运行（feature gate 启用时）"层次，覆盖更广业务路径（8 类核心路径）。既有 63 integration 测试保留不替换。
- **约束**：
  - 不替换既有 integration 测试：既有 63 测试保留，真实 DB e2e 测试为新增层次（spec §1.4.4）。
  - 复用既有 adapter：真实 DB e2e 测试复用 tests/common/ 既有 adapter，不重复实现连接管理（spec §4.4.1）。

### 1.2.3 既有 tests/common/ adapter 基础设施（sqlx_pg_adapter / sqlx_mysql_adapter / rusqlite_adapter / schema_builder / equivalence / soak）

- **接口契约**：
  - `SqlxPgAdapter`（[tests/common/sqlx_pg_adapter.rs:15](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/common/sqlx_pg_adapter.rs#L15)）：sqlx PostgreSQL 到 sz_orm_core::Connection 的适配器，`convert_placeholders` 将 `?` 占位符转换为 PostgreSQL 的 `$N` 格式，`row_to_map` 将 PgRow 转换为 HashMap<String, Value>，`try_get_value` 按类型尝试解码，`bind_value` 按 Value 类型绑定参数。
  - `SqlxMysqlAdapter`（[tests/common/sqlx_mysql_adapter.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/common/sqlx_mysql_adapter.rs)）：sqlx MySQL 到 sz_orm_core::Connection 的适配器，类似 SqlxPgAdapter。
  - `RusqliteAdapter`（[tests/common/rusqlite_adapter.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/common/rusqlite_adapter.rs)）：rusqlite SQLite 到 sz_orm_core::Connection 的适配器。
  - `schema_builder`（[tests/common/schema_builder.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/common/schema_builder.rs)）：测试 schema 构建器，用于建表/准备数据。
  - `equivalence`（[tests/common/equivalence.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/common/equivalence.rs)）：等价性验证工具，验证不同实现生成相同结果。
  - `soak`（[tests/common/soak.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/common/soak.rs)）：soak 测试基础设施。
- **业务规则**：adapter 将 sz-orm 的 `?` 占位符转换为各方言原生占位符（PG `$N`/MySQL `?`/SQLite `?`），按 Value 类型绑定参数，按列类型解码值。schema_builder 提供建表/准备数据工具。equivalence 验证不同实现等价性。
- **扩展点**：v3.7.0 真实 DB e2e 测试套件直接复用既有 adapter，不重复实现连接管理与占位符转换。新增 Oracle/MSSQL adapter（如本机可用）或复用既有 integration_oracle.rs/integration_mssql.rs 连接逻辑。
- **约束**：
  - 复用既有 adapter：不重复造轮子（spec §4.4.1）。
  - 占位符转换正确：adapter 必须正确转换 `?` 到各方言原生占位符。
  - 类型解码正确：adapter 必须按列类型正确解码值（i32/i64/f64/String/bool/bytes）。

### 1.2.4 typed_relation.rs 探索实现（typed_relation.rs:35-397）

- **接口契约**：
  - `TypedTable` trait（[typed_relation.rs:35](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_relation.rs#L35)）：关联查询基础，`const NAME: &'static str`（表名）+ `type PrimaryKey: Clone + Debug`（主键类型）+ `type ForeignKey: Clone + Debug`（外键类型）。
  - `Relation` trait（[typed_relation.rs:45](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_relation.rs#L45)）：关联类型标记，`type Child: TypedTable` + `type Parent: TypedTable` + `const KIND: RelationKind`。
  - `RelationKind` 枚举（[typed_relation.rs:56](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_relation.rs#L56)）：BelongsTo/HasMany/HasOne。
  - `BelongsTo<Child, Parent, FK>`/`HasMany<Parent, Child, FK>`/`HasOne<Parent, Child, FK>`：ZST 关联类型，通过关联类型约束在编译期校验外键类型匹配。
  - 通过 `typed-relation` feature gate 隔离（[lib.rs:494](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L494) `#[cfg(feature = "typed-relation")] pub mod typed_relation`）。
- **业务规则**：所有关联类型为 ZST（零大小类型），仅在编译期携带类型信息。编译期校验外键类型匹配（Child::ForeignKey == Parent::PrimaryKey）+ 表归属校验（外键属于 Child 表）。
- **扩展点**：v3.7.0 成熟化 typed relation：补齐测试覆盖至 ≥10 用例（编译期校验 + 运行时查询 + 外键类型匹配 + 表归属 + 与 EagerLoader 协作 + escape hatch），补齐文档（迁移指南 + 适用场景 + 稳定性标注），Cargo.toml feature 注释标注 "stable"。
- **约束**：
  - 不破坏 v3.6.0 既有 API：探索期 API 如已发布须保持向后兼容，仅补齐测试与文档（spec §5.3.1.4）。
  - 无运行时开销：均为编译期工作，运行时零开销（spec §5.3.1.5）。
  - 与 EagerLoader 协作：typed relation 提供编译期类型安全，EagerLoader 提供运行时执行，复杂关联回退 EagerLoader（escape hatch）。

### 1.2.5 自定义编译期诊断探索实现（diagnostic.rs:10-66）

- **接口契约**：
  - `TypeMismatchDiagnostic` 结构（[diagnostic.rs:10](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/src/diagnostic.rs#L10)）：`location: String`（错误位置）+ `expected: String`（期望 SqlType）+ `found: String`（实际 SqlType）+ `suggestion: String`（修复建议）。
  - `TypeMismatchDiagnostic::new`（[diagnostic.rs:23](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/src/diagnostic.rs#L23)）：创建新的类型不匹配诊断。
  - `TypeMismatchDiagnostic::format_error`（[diagnostic.rs:33](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/src/diagnostic.rs#L33)）：格式化为编译器友好的错误信息（`类型不匹配：{location} 期望 `{expected}`，但发现 `{found}`\n  help: {suggestion}`）。
  - `suggestions` 模块（[diagnostic.rs:42](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/src/diagnostic.rs#L42)）：常见诊断场景预设建议（TYPE_MISMATCH_EQ/NON_BOOLEAN_LOGIC/CROSS_TABLE_REFERENCE）。
  - 通过 `typed-dsl` feature gate 隔离（[sz-orm-macros/Cargo.toml:37](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/Cargo.toml#L37) typed-dsl feature）。
  - 测试：[sz-orm-macros/tests/typed_ast_diagnostic_test.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/tests/typed_ast_diagnostic_test.rs)（required-features = ["typed-dsl"]）。
- **业务规则**：自定义诊断信息包含错误位置 + 期望类型 + 实际类型 + 修复建议，比 Rust 默认类型不匹配错误信息更清晰。通过 proc-macro 的 Diagnostic API 或 compile_error! 生成。
- **扩展点**：v3.7.0 成熟化自定义诊断：新增独立 `custom-diagnostic` feature gate（与 typed-dsl 解耦），补齐测试覆盖至 ≥10 用例（错误位置 + 期望类型 + 实际类型 + 修复建议 + 各诊断场景），补齐文档。
- **约束**：
  - 不破坏 v3.6.0 既有 API：探索期 API 须保持向后兼容，仅补齐测试与文档。
  - 无运行时开销：均为编译期工作。

### 1.2.6 proc-macro SQL 验证探索实现（sql_verify.rs:22-170 + db-verify feature）

- **接口契约**：
  - `VerifyResult` 结构（[sql_verify.rs:22](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/sql_verify.rs#L22)）：`is_valid: bool` + `errors: Vec<String>` + `sql: String`。
  - `VerifyResult::ok`/`VerifyResult::fail`（[sql_verify.rs:32/42](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/sql_verify.rs#L32)）：创建成功/失败验证结果。
  - 通过 `sql-verify-proc` feature gate 隔离（[lib.rs:496](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L496) `#[cfg(feature = "sql-verify-proc")] pub mod sql_verify`）。
  - 既有 `query!` 宏 db-verify feature（[sz-orm-macros/Cargo.toml:31](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/Cargo.toml#L31) `db-verify = ["dep:sqlx", "dep:tokio", "dep:serde_json"]`）：连真 DB 执行 EXPLAIN 验证，通过 `SZ_ORM_QUERY_VERIFY=1` + `DATABASE_URL` 环境变量启用。
  - `query!` 宏（[sz-orm-macros/src/lib.rs:464](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/src/lib.rs#L464)）：proc-macro，解析 SQL 字符串，校验 SQL 语法 + 表/列存在性（连真 DB EXPLAIN）+ 类型匹配，支持离线缓存模式（`SZ_ORM_QUERY_VERIFY=cache` + `SZ_ORM_SQLX_CACHE=.sz-orm/query-cache.json`）。
- **业务规则**：sql_verify 扩展 db-verify 到 QueryBuilder 生态，通过 sqlparser 在编译期解析 SQL 字符串，校验 SQL 语法 + 表/列存在性（连真 DB EXPLAIN）+ 类型匹配，仅执行 EXPLAIN 不执行修改 SQL，缓存验证结果（按 SQL 哈希缓存）。
- **扩展点**：v3.7.0 成熟化 proc-macro SQL 验证：扩展连真 DB EXPLAIN 覆盖所有 QueryBuilder 路径（SELECT/INSERT/UPDATE/DELETE/JOIN/子查询/CTE/窗口），补齐测试覆盖至 ≥10 用例，补齐文档（启用方式 + DATABASE_URL 配置 + 降级模式说明），sql-verify-proc feature 标注 "stable"。
- **约束**：
  - 不破坏 v3.6.0 既有 API：探索期 API 须保持向后兼容。
  - 无运行时开销：均为编译期工作。
  - 仅执行 EXPLAIN 不执行修改 SQL：安全约束，避免编译期修改 DB 数据。
  - 降级模式：DATABASE_URL 未设置时回退到仅语法校验，输出降级警告。

### 1.2.7 SnowflakeDialect/RedshiftDialect 实现（dialect.rs:1567-1794 + dialect_*_test.rs）

- **接口契约**：
  - `SnowflakeDialect`（[dialect.rs:1567](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1567)）：独立实现 Dialect trait，支持 VARIANT/OBJECT/ARRAY 半结构化类型 + COPY INTO 数据加载 + TIME TRAVEL 时间旅行查询（`AT(TIMESTAMP => ...)`/`BEFORE(...)`/`AT(OFFSET => ...)`）。
  - `RedshiftDialect`（[dialect.rs:1794](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1794)）：委派 PostgreSqlDialect + Redshift 特性扩展（COPY 数据加载 + UNLOAD 数据卸载 + Redshift 特有函数）。
  - `DbType::Snowflake`/`DbType::Redshift`（[db_type.rs:61/65](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/db_type.rs#L61)）：通过 `dialect-snowflake`/`dialect-redshift` feature gate 隔离。
  - 测试：[tests/dialect_snowflake_test.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/dialect_snowflake_test.rs)（`#![cfg(feature = "dialect-snowflake")]`，32 测试均为 SQL 生成测试）+ [tests/dialect_redshift_test.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/dialect_redshift_test.rs)。
- **业务规则**：SnowflakeDialect 独立实现（无合适基础方言可委派），RedshiftDialect 委派 PostgreSqlDialect + 覆盖不兼容构造。32 测试均为 SQL 生成测试，未连真实云数据库验证行为一致性。
- **扩展点**：v3.7.0 补齐真实云数据库验证：评估 Snowflake/Redshift 云实例可用性，如有则连真实云验证行为一致性，输出验证报告；如无则输出验证缺口报告 + 替代方案。
- **约束**：
  - 不破坏既有 20 种方言：新能力通过 feature gate 隔离，既有 20 种方言不变（spec §5.4.1.4）。
  - 云实例不可用时不阻断交付：输出验证缺口报告 + 替代方案（spec §5.5.3.1）。

### 1.2.8 对比分析文档（sz-orm与同类产品对比分析.md，963 行，停留 v3.5.0 基线）

- **接口契约**：
  - 文档头部（[docs/sz-orm与同类产品对比分析.md:3](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/sz-orm与同类产品对比分析.md#L3)）：`版本：v3.5.0 | 日期：2026-08-09 | 基于实际代码分析`。
  - 代码基线（[docs/sz-orm与同类产品对比分析.md:6](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/sz-orm与同类产品对比分析.md#L6)）：`Cargo.toml workspace.package.version = "3.5.0"`。
  - 13 维度评分矩阵：异步/类型安全/连接池/方言/查询API/事务/缓存/N+1/安全/性能/宏/文档生态/生产就绪。
  - §5 独特优势 + §6 已知不足 + §7 结论与建议。
  - 评估方法（[docs/sz-orm与同类产品对比分析.md:12](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/sz-orm与同类产品对比分析.md#L12)）：代码证据铁律（每条 SZ-ORM 能力结论附 file:line 证据）+ 竞品信息来源 + 客观标注 + 禁止主观臆断 + 版本对齐。
- **业务规则**：对比分析文档基于 v3.5.0 代码基线，逐维度评分 13 个维度，每条结论附 file:line 证据。文档滞后一个版本（v3.6.0 已发布，文档仍停留 v3.5.0）。
- **扩展点**：v3.7.0 更新对比分析文档到 v3.6.0 基线：更新头部版本/日期/代码基线，逐维度重新评分附 v3.6.0 file:line 证据，§6 已知不足标注 v3.6.0 改进状态，§7 结论与建议更新，§5 独特优势更新（v3.6.0 新增项）。
- **约束**：
  - 纯文档不改变代码：文档更新不引入任何 .rs 代码变更（spec §5.2.1.5）。
  - 每条评分变更附 v3.6.0 file:line 证据：证据真实存在，可验证（spec §5.2.1.2）。
  - 审计验证脚本通过：所有 file:line 证据真实存在（spec §9.2）。

---

# 二、增量设计方案

## 2.0 架构总览

### 2.0.1 v3.7.0 整体架构图

v3.7.0 在 v3.6.0 现有 workspace 基础上，不新增独立包，而是在 sz-orm-core 内新增真实 DB e2e 测试套件（tests/e2e_real_db_*.rs）+ Informix/SAP HANA/Firebird 方言（按需）+ 探索能力成熟化（补齐测试与文档），在 sz-orm-macros 内新增 custom-diagnostic 独立 feature gate，在 docs/ 内更新对比分析文档到 v3.6.0 基线 + Snowflake/Redshift 真实云验证报告 + Prisma 评估结论落地文档，在 scripts/ 内运行 14 道门禁验证，通过 4 个新增 feature gate（e2e-real-db/dialect-informix/dialect-saphana/dialect-firebird）+ 1 个 sz-orm-macros 新增 feature gate（custom-diagnostic）+ 既有 feature 体系（typed-relation/sql-verify-proc/dialect-snowflake/dialect-redshift）隔离，复用既有 tests/common/ adapter/typed_relation.rs/diagnostic.rs/sql_verify.rs/Dialect trait/delegate_dialect_to 宏/db-verify feature 基础设施。整体架构如下：

```plantuml
@startuml
!theme plain
title sz-orm v3.7.0 整体架构（增量视图）

rectangle "sz-orm v3.7.0 workspace" as WS {

  rectangle "核心层 sz-orm-core (扩展)" as Core {
    rectangle "既有: typed_relation.rs [typed-relation] (v3.6.0 探索)" as RelOld
    rectangle "既有: sql_verify.rs [sql-verify-proc] (v3.6.0 探索)" as VerifyOld
    rectangle "既有: dialect.rs (20 种方言, v3.6.0)" as DialectOld
    rectangle "既有: SnowflakeDialect/RedshiftDialect (v3.6.0)" as CloudOld
    rectangle "既有: db_type.rs (23 变体, v3.6.0)" as DbTypeOld

    rectangle "typed relation 成熟化 <<new>>\n[typed-relation stable]" as RelMature
    rectangle "proc-macro SQL 验证成熟化 <<new>>\n[sql-verify-proc stable]" as VerifyMature
    rectangle "InformixDialect/SapHanaDialect/FirebirdDialect <<new>>\n[dialect-informix/saphana/firebird]" as DialectNew
  }

  rectangle "sz-orm-core tests/ (扩展)" as Tests {
    rectangle "既有: e2e_*.rs (96 InMemoryDb, v3.6.0)" as E2eOld
    rectangle "既有: integration_*.rs (63 #[ignore], v3.6.0)" as IntOld
    rectangle "既有: common/ adapter (v3.6.0)" as AdapterOld

    rectangle "真实 DB e2e 测试套件 <<new>>\n[e2e-real-db]" as E2eRealDb
  }

  rectangle "sz-orm-macros (扩展)" as Macros {
    rectangle "既有: diagnostic.rs [typed-dsl] (v3.6.0 探索)" as DiagOld
    rectangle "既有: query! + db-verify (v3.6.0)" as QueryMacro

    rectangle "自定义诊断独立 feature gate <<new>>\n[custom-diagnostic stable]" as DiagMature
  }

  rectangle "docs/ (扩展)" as Docs {
    rectangle "既有: sz-orm与同类产品对比分析.md (v3.5.0 基线)" as CmpOld
    rectangle "既有: prisma-dialect-evaluation.md (v3.6.0)" as PrismaOld

    rectangle "对比分析文档更新到 v3.6.0 基线 <<new>>" as CmpNew
    rectangle "Snowflake/Redshift 真实云验证报告 <<new>>" as CloudReport
    rectangle "Prisma 评估结论落地 <<new>>" as PrismaLand
  }

  rectangle "scripts/ (既有)" as Scripts {
    rectangle "既有: gate.ps1 + 14 道门禁脚本" as Gate
  }
}

cloud "crates.io" as CratesIo
database "MySQL/PG/SQLite/Oracle/MSSQL" as DB
database "Snowflake" as Snowflake
database "AWS Redshift" as Redshift
rectangle "sz-pay 生产项目" as SzPay
rectangle "Diesel (对标基准)" as Diesel

E2eRealDb ..> AdapterOld : 复用 SqlxPgAdapter/SqlxMysqlAdapter/RusqliteAdapter
E2eRealDb --> DB : 连真实 DB 验证 8 类核心路径
RelMature ..> RelOld : 补齐测试 + 文档 + 稳定性标注
VerifyMature ..> VerifyOld : 扩展连真 DB EXPLAIN 覆盖所有路径
DiagMature ..> DiagOld : 独立 feature gate + 补齐测试 + 文档
DialectNew ..> DialectOld : 新增 Informix/SAP HANA/Firebird (按需)
CmpNew ..> CmpOld : 更新到 v3.6.0 基线 + 重新评分
CloudReport --> Snowflake : 真实云验证 (或缺口报告)
CloudReport --> Redshift : 真实云验证 (或缺口报告)
CratesIo --> SzPay : sz-pay 升级到 3.7.0 (零回归验证)
CmpNew ..> Diesel : 评分反映 v3.6.0 后真实状态

@enduml
```

### 2.0.2 6 大方向在 workspace 中的定位

| 方向 | 需求组 | 包名 | 形态 | feature gate | 在 workspace 中的位置 | 依赖关系 |
|------|--------|------|------|-------------|---------------------|---------|
| 1 真实 DB e2e 测试体系 | REQ-E2E-001~006 | sz-orm-core | **新增 tests/e2e_real_db_*.rs 真实 DB e2e 测试套件** | `e2e-real-db`（新增） | `packages/sz-orm-core/tests/e2e_real_db_*.rs` + `#[cfg(feature = "e2e-real-db")]` 条件编译 | 复用既有 tests/common/ adapter（SqlxPgAdapter/SqlxMysqlAdapter/RusqliteAdapter）+ 本机 DB 实例 |
| 2 对比分析重评估 | REQ-REEVAL-001~005 | docs/ | **纯文档更新** | 无（纯文档） | `docs/sz-orm与同类产品对比分析.md` 更新 | 无新增（纯文档） |
| 3 探索能力成熟化 | REQ-MAT-001~005 | sz-orm-core + sz-orm-macros | **补齐测试 + 文档 + 稳定性标注 + 独立 feature gate** | `typed-relation`（既有，标注 stable）+ `sql-verify-proc`（既有，标注 stable）+ `custom-diagnostic`（sz-orm-macros 新增） | `packages/sz-orm-core/src/typed_relation.rs` 补齐测试文档 + `packages/sz-orm-core/src/sql_verify.rs` 扩展连真 DB EXPLAIN + `packages/sz-orm-macros/src/diagnostic.rs` 独立 feature gate | 复用既有 typed_relation.rs/sql_verify.rs/diagnostic.rs/db-verify feature |
| 4 方言扩展延续 | REQ-DIALECT-001~005 | sz-orm-core | **Informix/SAP HANA/Firebird 方言按需实现** | `dialect-informix`（新增）+ `dialect-saphana`（新增）+ `dialect-firebird`（新增） | `packages/sz-orm-core/src/dialect.rs` + `packages/sz-orm-core/src/db_type.rs` | 既有 Dialect trait + delegate_dialect_to 宏 |
| 5 云数仓真实验证 | REQ-CLOUD-001~003 | docs/ + sz-orm-core | **Snowflake/Redshift 真实云验证报告** | 无（验证报告，不改变代码） | `docs/snowflake-cloud-verification.md` + `docs/redshift-cloud-verification.md` | 既有 SnowflakeDialect/RedshiftDialect + 云实例（如可用） |
| 6 工程规范化 | REQ-ENG-001~004 | git + docs/ + scripts/ | **v3.6.0 未提交工作入库 + Prisma 评估落地 + 14 道门禁** | 无（git 提交 + 门禁） | git commit + `docs/prisma-dialect-evaluation.md` 落地 + `scripts/gate.ps1` 运行 | git + 既有 scripts/ 门禁脚本 |

### 2.0.3 与 v3.6.0 现有架构的演进关系

```plantuml
@startuml
!theme plain
title v3.6.0 → v3.7.0 架构演进

rectangle "v3.6.0 (15 新表达式 + 195 missing_docs 补齐 + QueryBuilder 合并 47 测试 + Snowflake/Redshift 20 方言 32 测试 + async trait 重评估)" as V36 {
  rectangle "sz-orm-core\n(query/pool/dialect 20 种/typed_ast 61 种/typed_relation/sql_verify/eager_loader\n+typed-dsl/typed-relation/sql-verify-proc/dialect-snowflake/dialect-redshift/qb-migration-tool)" as Core36
  rectangle "sz-orm-macros (query! + db-verify + diagnostic [typed-dsl])" as Macros36
  rectangle "sz-orm-core tests/ (e2e 96 InMemoryDb + integration 63 #[ignore] + common/ adapter)" as Tests36
  rectangle "docs/ (对比分析 v3.5.0 + Prisma 评估 + async-trait-eval + query-builder-guide)" as Docs36
  rectangle "scripts/ (gate + 14 道门禁脚本)" as Scripts36
}

rectangle "v3.7.0 增量 (5 feature gate 隔离 + 纯文档/测试/工程)" as V37 {
  rectangle "真实 DB e2e 测试套件 (8 类核心路径)\n[e2e-real-db]" as E2e37
  rectangle "对比分析文档更新到 v3.6.0 基线\n(纯文档)" as Cmp37
  rectangle "探索能力成熟化 (typed-relation/sql-verify-proc stable + custom-diagnostic 独立)\n[typed-relation/sql-verify-proc/custom-diagnostic]" as Mature37
  rectangle "Informix/SAP HANA/Firebird 方言 (按需)\n[dialect-informix/saphana/firebird]" as Dialect37
  rectangle "Snowflake/Redshift 真实云验证报告\n(纯文档 + 可选真实云验证)" as Cloud37
  rectangle "工程规范化 (213 文件入库 + Prisma 落地 + 14 门禁)\n(git + 门禁)" as Eng37
}

Core36 ..> Mature37 : 补齐测试文档 + 稳定性标注
Core36 ..> Dialect37 : 新增 Informix/SAP HANA/Firebird (按需)
Tests36 ..> E2e37 : 复用 common/ adapter 连真实 DB
Docs36 ..> Cmp37 : 更新到 v3.6.0 基线
Docs36 ..> Cloud37 : Snowflake/Redshift 验证报告
Docs36 ..> Eng37 : Prisma 评估结论落地

note bottom of V37
  **演进原则**：
  1. 无 Breaking Change（现有公开 API 签名不变）
  2. 新增能力通过 feature gate 隔离（默认关闭）
  3. 不新增独立包，全部在既有包内扩展
  4. 复用现有基础设施（tests/common/ adapter/typed_relation/sql_verify/diagnostic/Dialect trait/db-verify）
  5. 五方言行为一致（增强在 core 层统一，不触碰方言驱动）
  6. ADR-0001 严禁修改下游/上游仓库（sz-pay 仅验证不改）
  7. 测试覆盖不回退（v3.6.0 基线仅增不减）
  8. 既有 96 e2e + 63 integration 测试保留不替换
end note

@enduml
```

**演进关键决策**：

| 决策点 | 选项 | 选择 | 理由 |
|--------|------|------|------|
| 真实 DB e2e 测试隔离方式 | A. 既有 testing feature / B. 新增 e2e-real-db feature | B | testing feature 用于 tokio/full 运行时，e2e-real-db 专注真实 DB e2e 测试，语义不同，独立 feature 便于按需启用 |
| 真实 DB e2e 测试复用方式 | A. 修改既有 e2e_*.rs / B. 新增 e2e_real_db_*.rs | B | 既有 96 e2e 测试用 InMemoryDb 保留不替换（spec §1.4.3），真实 DB e2e 测试为新增补充 |
| 真实 DB e2e 测试覆盖路径 | A. 复刻既有 96 e2e / B. 覆盖 8 类核心路径 | B | 8 类核心路径（CRUD/事务/预加载/分页/软删除/多租户/缓存/方言行为）覆盖更广业务路径，避免简单复刻 |
| 自定义诊断隔离方式 | A. 既有 typed-dsl / B. 新增 custom-diagnostic | B | 自定义诊断是独立能力（编译期诊断信息），与 typed-dsl 表达式不同，独立 feature 便于按需启用 |
| 探索能力成熟化方式 | A. 重写实现 / B. 补齐测试文档 + 标注 stable | B | v3.6.0 探索实现已有，仅需补齐测试覆盖至 ≥10 + 文档完整 + 稳定性标注，不重写 |
| Informix/SAP HANA/Firebird 实现策略 | A. 全部实现 / B. 按需实现（驱动成熟 + 用户需求） | B | spec §5.4.1 要求按需实现，Rust 驱动成熟度 + 用户需求是触发条件，避免实现无驱动支撑的方言 |
| Snowflake/Redshift 真实云验证策略 | A. 强制连真实云 / B. 评估可用性，无则输出缺口报告 | B | spec §5.5.3.1 要求云实例不可用时不阻断交付，输出缺口报告 + 替代方案 |
| 对比分析文档更新方式 | A. 重写文档 / B. 更新到 v3.6.0 基线 + 重新评分 | B | 文档结构完整（963 行），仅需更新头部 + 重新评分 + 更新 §6/§7，不重写 |
| v3.6.0 未提交工作入库策略 | A. 一次性提交 213 文件 / B. 按里程碑分组提交 | B | 按里程碑分组提交便于追溯 + 每次提交后运行门禁验证，避免一次性提交引入未发现问题 |
| crates.io 发布版本号 | A. 3.7.0 minor / B. 4.0.0 major | A | v3.7.0 通过 feature gate 隔离新能力，默认 feature 行为不变，向后兼容，SemVer minor 升级（spec §4.5.2） |

## 2.1 实现模型

### 2.1.1 上下文视图

```plantuml
@startuml
!theme plain
title sz-orm v3.7.0 上下文视图

actor "ORM 库维护者" as Dev
actor "CI 环境" as CI
actor "sz-pay 开发者" as Downstream

rectangle "sz-orm v3.7.0" as ORM {
  rectangle "真实 DB e2e 测试体系 [e2e-real-db]" as E2e
  rectangle "对比分析文档更新 (纯文档)" as Doc
  rectangle "探索能力成熟化 [typed-relation/sql-verify-proc/custom-diagnostic]" as Mature
  rectangle "方言扩展 [dialect-informix/saphana/firebird]" as Dialect
  rectangle "云数仓验证 (纯文档 + 可选真实云)" as Cloud
  rectangle "工程规范化 (git + 门禁)" as Eng
}

database "MySQL 9.6" as MySQL
database "PostgreSQL 18" as PG
database "Oracle 23ai" as Oracle
database "SQLite" as SQLite
database "MSSQL" as MSSQL
database "Snowflake" as Snowflake
database "Redshift" as Redshift
cloud "crates.io" as CratesIo

Dev --> ORM : 开发/验证/测试
CI --> E2e : 启用 e2e-real-db 运行真实 DB e2e
E2e --> MySQL : 真实 DB 测试
E2e --> PG : 真实 DB 测试
E2e --> Oracle : 真实 DB 测试
E2e --> SQLite : 真实 DB 测试
E2e --> MSSQL : 真实 DB 测试
Cloud --> Snowflake : 真实云验证 (或缺口报告)
Cloud --> Redshift : 真实云验证 (或缺口报告)
Downstream --> ORM : API 兼容性验证
ORM --> CratesIo : v3.7.0 发布 (如发布)

@enduml
```

### 2.1.2 服务/组件总体架构

```plantuml
@startuml
!theme plain
title sz-orm v3.7.0 服务/组件总体架构

rectangle "sz-orm-core" as Core {
  rectangle "src/typed_relation.rs [typed-relation stable]" as Rel
  rectangle "src/sql_verify.rs [sql-verify-proc stable]" as Verify
  rectangle "src/dialect.rs (20 + Informix/SAP HANA/Firebird)" as Dialect
  rectangle "src/db_type.rs (23 + 新变体)" as DbType
  rectangle "tests/common/ adapter (复用)" as Adapter
  rectangle "tests/e2e_real_db_*.rs [e2e-real-db]" as E2eReal
}

rectangle "sz-orm-macros" as Macros {
  rectangle "src/diagnostic.rs [custom-diagnostic stable]" as Diag
  rectangle "src/lib.rs query! + db-verify (既有)" as QueryMacro
}

rectangle "docs/" as Docs {
  rectangle "sz-orm与同类产品对比分析.md (v3.6.0 基线)" as Cmp
  rectangle "snowflake-cloud-verification.md" as SfReport
  rectangle "redshift-cloud-verification.md" as RsReport
  rectangle "prisma-dialect-evaluation.md (落地)" as Prisma
}

rectangle "scripts/" as Scripts {
  rectangle "gate.ps1 (14 道门禁)" as Gate
}

rectangle "外部依赖" as Ext {
  database "MySQL/PG/SQLite/Oracle/MSSQL" as DB
  database "Snowflake/Redshift 云" as CloudDB
  cloud "crates.io" as CratesIo
  rectangle "sz-pay" as SzPay
}

Rel --> Adapter : typed relation 测试复用 adapter
Verify --> QueryMacro : 扩展 db-verify 到 QueryBuilder 生态
E2eReal --> Adapter : 复用 SqlxPgAdapter/SqlxMysqlAdapter/RusqliteAdapter
E2eReal --> DB : 连真实 DB 验证 8 类核心路径
Dialect --> DbType : 新方言新增 DbType 变体
Diag --> Macros : 独立 feature gate
Cmp --> Core : 评分附 v3.6.0 file:line 证据
SfReport --> CloudDB : Snowflake 真实云验证
RsReport --> CloudDB : Redshift 真实云验证
Gate --> Core : 14 道门禁验证
Gate --> Macros : 14 道门禁验证
CratesIo --> SzPay : sz-pay 升级到 3.7.0 (零回归)

@enduml
```

### 2.1.3 实现设计文档

```plantuml
@startuml
!theme plain
title sz-orm v3.7.0 实现流程总览

start

partition "M1 真实 DB e2e 测试体系 (最高优先级, 3 周)" {
  :新增 e2e-real-db feature gate;
  :实现真实 DB e2e 测试套件 (8 类核心路径);
  :实现测试清理机制 (DROP TABLE IF EXISTS + 独立表名);
  :实现测试超时机制 (单方言 60s, 全方言 300s);
  :复用 tests/common/ adapter;
  :运行 cargo test --features e2e-real-db;
  if (全通过 + 既有 96 e2e + 63 integration 不回退?) then (是)
    :M1 验收通过;
  else (否)
    :修复 + 重新测试;
  endif
}

partition "M2 对比分析重评估 (高优先级, 1 周)" {
  :更新文档头部到 v3.6.0 基线;
  :逐维度重新评分附 v3.6.0 file:line 证据;
  :更新 §6 已知不足 (标注 v3.6.0 改进状态);
  :更新 §7 结论与建议;
  :更新 §5 独特优势 (v3.6.0 新增项);
  :运行审计验证脚本;
  if (所有 file:line 证据真实存在?) then (是)
    :M2 验收通过;
  else (否)
    :修正证据 + 重新验证;
  endif
}

partition "M3 探索能力成熟化 (高优先级, 2 周)" {
  :typed relation 补齐测试 (≥10) + 文档 + 标注 stable;
  :自定义诊断独立 custom-diagnostic feature + 补齐测试 (≥10) + 文档;
  :proc-macro SQL 验证扩展连真 DB EXPLAIN 覆盖所有路径 + 补齐测试 (≥10) + 文档;
  :运行 cargo test --features typed-relation,custom-diagnostic,sql-verify-proc;
  if (全通过 + v3.6.0 既有 API 兼容?) then (是)
    :M3 验收通过;
  else (否)
    :修复 + 重新测试;
  endif
}

partition "M4 方言扩展延续 (中优先级, 2 周)" {
  :评估 Rust Informix/SAP HANA/Firebird 驱动成熟度 + 用户需求;
  :按需实现 InformixDialect/SapHanaDialect/FirebirdDialect;
  :新增 DbType 变体 + 路线图更新;
  :运行 cargo test --features dialect-informix,dialect-saphana,dialect-firebird;
  if (新方言测试通过 + 既有 20 种不变?) then (是)
    :M4 验收通过;
  else (否)
    :修复 + 重新测试;
  endif
}

partition "M5 云数仓真实验证 (中优先级, 1 周)" {
  :评估 Snowflake/Redshift 云实例可用性;
  if (云实例可用?) then (是)
    :连真实云验证行为一致性;
    :输出验证报告 (用例/结果/差异);
  else (否)
    :输出验证缺口报告 + 替代方案;
  endif
  :M5 验收通过;
}

partition "M6 工程规范化 (低但必须, 1 周)" {
  :按里程碑分组提交 v3.6.0 未提交工作 (213 文件);
  :Prisma 评估结论落地;
  :运行 14 道门禁;
  if (14 道门禁全通过?) then (是)
    :M6 验收通过;
  else (否)
    :修复失败门禁 + 重新运行;
  endif
}

:sz-pay 零回归验证 (升级到 3.7.0);
if (sz-pay cargo check + cargo test 零回归?) then (是)
  :v3.7.0 交付完成;
else (否)
  :回退 + 分析失败原因;
endif

stop

@enduml
```

## 2.2 接口设计

### 2.2.1 总体设计

| 接口分类 | 接口数量 | 稳定性等级 | 变更策略 |
|---------|---------|-----------|---------|
| 真实 DB e2e 测试接口（新增） | 8 个测试套件 | 实验（feature gate 隔离） | 通过 `e2e-real-db` feature gate 隔离，默认关闭 |
| 探索能力成熟化接口（既有 + 补齐） | typed_relation + custom-diagnostic + sql-verify-proc | 稳定（成熟化后） | 既有 API 向后兼容，仅补齐测试与文档 |
| 方言扩展接口（新增） | InformixDialect + SapHanaDialect + FirebirdDialect | 实验（feature gate 隔离） | 通过 `dialect-informix`/`dialect-saphana`/`dialect-firebird` feature gate 隔离 |
| 对比分析文档接口（纯文档） | 无代码接口 | 稳定（纯文档） | 文档更新不改变代码行为 |
| 云数仓验证接口（纯文档 + 可选真实云） | 验证报告文档 | 稳定（纯文档） | 验证报告不改变代码行为 |
| 工程规范化接口（git + 门禁） | 无代码接口 | 稳定（git + 门禁） | git 提交 + 门禁验证 |

### 2.2.2 接口清单

#### 2.2.2.1 真实 DB e2e 测试套件接口（REQ-E2E-001/002）

**接口签名**（测试套件，非 Rust API）：

```rust
// packages/sz-orm-core/tests/e2e_real_db_crud.rs
#![cfg(feature = "e2e-real-db")]

#[tokio::test]
async fn e2e_real_db_crud_insert_select_update_delete() {
# Example: DATABASE_URL=mysql://root:test123@127.0.0.1:3306/sz_orm_test
```

**业务说明**：连真实 DB 验证 CRUD 完整链路（应用层 API → QueryBuilder → SQL 下推 → 真实 DB 执行 → 结果回填 → 断言）。

**前置条件**：
- `e2e-real-db` feature gate 启用
- `DATABASE_URL` 环境变量指向真实 DB（`sz_orm_test` 测试库）
- 真实 DB 实例可达

**后置条件**：测试完成后清理所有测试数据与表（DROP TABLE IF EXISTS），不残留。

**异常映射**：
- DATABASE_URL 未设置：测试跳过并输出 "skipped: DATABASE_URL not set"
- DB 不可达：测试跳过并输出 "skipped: DB unreachable"
- 测试超时（>60s）：标记超时失败，输出 "timeout: >60s, stuck at [test_name]"
- 清理失败：输出警告 "warning: cleanup failed for [table], manual cleanup required"

**8 类核心路径测试套件**：

| 测试套件 | 文件 | 覆盖路径 | 测试数量 |
|---------|------|---------|---------|
| CRUD | e2e_real_db_crud.rs | insert/select/update/delete + 批量操作 | ≥10 |
| 事务 | e2e_real_db_transaction.rs | commit/rollback/savepoint + 嵌套事务 | ≥8 |
| 预加载 | e2e_real_db_eager_load.rs | BelongsTo/HasMany/HasOne + N+1 检测 | ≥8 |
| 分页 | e2e_real_db_pagination.rs | offset/limit + keyset 分页 | ≥6 |
| 软删除 | e2e_real_db_soft_delete.rs | deleted_at 字段 + 过滤 + 恢复 | ≥5 |
| 多租户 | e2e_real_db_multi_tenant.rs | tenant_id 隔离 + 行级安全 | ≥5 |
| 缓存 | e2e_real_db_cache.rs | L1/L2 缓存命中/失效/一致性 | ≥6 |
| 方言行为 | e2e_real_db_dialect_behavior.rs | UPSERT/行锁/标识符引用/方言特有语法 | ≥8 |
| **合计** | — | — | **≥56** |

#### 2.2.2.2 探索能力成熟化接口（REQ-MAT-001/002/003）

**typed relation 接口**（既有，补齐测试与文档）：

```rust
// 既有接口签名不变（向后兼容）
pub trait TypedTable: 'static {
    const NAME: &'static str;
    type PrimaryKey: Clone + std::fmt::Debug;
    type ForeignKey: Clone + std::fmt::Debug;
}

pub trait Relation: 'static {
    type Child: TypedTable;
    type Parent: TypedTable;
    const KIND: RelationKind;
}

pub struct BelongsTo<Child: TypedTable, Parent: TypedTable>(PhantomData<(Child, Parent)>);
pub struct HasMany<Parent: TypedTable, Child: TypedTable>(PhantomData<(Parent, Child)>);
pub struct HasOne<Parent: TypedTable, Child: TypedTable>(PhantomData<(Parent, Child)>);
```

**成熟化变更**：
- Cargo.toml feature 注释更新：`typed-relation = []  # v3.7.0: stable（类型安全关联查询，编译期外键校验）`
- 补齐测试覆盖至 ≥10 用例
- 补齐文档：迁移指南 + 适用场景 + escape hatch 说明

**自定义诊断接口**（既有，新增独立 feature gate）：

```rust
// 既有 TypeMismatchDiagnostic 结构不变
pub struct TypeMismatchDiagnostic {
    pub location: String,
    pub expected: String,
    pub found: String,
    pub suggestion: String,
}

// sz-orm-macros/Cargo.toml 新增独立 feature gate
// custom-diagnostic = []  # v3.7.0: stable（自定义编译期诊断信息）
```

**成熟化变更**：
- 新增 `custom-diagnostic` 独立 feature gate（与 typed-dsl 解耦）
- 补齐测试覆盖至 ≥10 用例
-.补齐文档

**proc-macro SQL 验证接口**（既有，扩展连真 DB EXPLAIN 覆盖）：

```rust
// 既有 VerifyResult 结构不变
pub struct VerifyResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub sql: String,
}

// 扩展：连真 DB EXPLAIN 覆盖所有 QueryBuilder 路径
// SELECT/INSERT/UPDATE/DELETE/JOIN/子查询/CTE/窗口
```

**成熟化变更**：
- Cargo.toml feature 注释更新：`sql-verify-proc = ["dep:sqlparser", "dep:xxhash-rust"]  # v3.7.0: stable（proc-macro 编译期 SQL 验证）`
- 扩展连真 DB EXPLAIN 覆盖所有 QueryBuilder 路径
- 补齐测试覆盖至 ≥10 用例
- 补齐文档：启用方式 + DATABASE_URL 配置 + 降级模式说明

#### 2.2.2.3 方言扩展接口（REQ-DIALECT-001/002/003）

**InformixDialect 接口**（按需实现）：

```rust
// 概念示意（非最终代码，按需实现）
#[cfg(feature = "dialect-informix")]
pub struct InformixDialect;

#[cfg(feature = "dialect-informix")]
impl Dialect for InformixDialect {
    fn clone_box(&self) -> Box<dyn Dialect> { Box::new(InformixDialect) }
    fn db_type(&self) -> DbType { DbType::Informix }
    // ... Dialect trait 方法
    // Informix 特有：SERIAL/ROW 类型、PUT 语句
}

// db_type.rs 新增变体
#[cfg(feature = "dialect-informix")]
Informix,
```

**SAP HANA/Firebird 接口**：类似 Informix，按需实现。

**前置条件**：
- Rust 驱动成熟度评估通过（Informix/SAP HANA）
- 用户需求出现（Firebird）

**后置条件**：
- 新方言通过 `dialect-informix`/`dialect-saphana`/`dialect-firebird` feature gate 隔离
- 既有 20 种方言测试不回退
- DbType 枚举新增变体（#[non_exhaustive] 允许）

#### 2.2.2.4 对比分析文档更新接口（REQ-REEVAL-001/002/003/004）

**接口签名**（纯文档，非 Rust API）：

```markdown
# 文档头部更新
> 版本：v3.6.0 | 日期：2026-08-10 | 基于实际代码分析
> 代码基线：Cargo.toml workspace.package.version = "3.6.0"

# 13 维度重新评分（每条变更附 v3.6.0 file:line 证据）
## 2.1 异步支持
- v3.5.0 评分：8/10
- v3.6.0 评分：8/10（无变化）
- 证据：[pool.rs:45](file:///.../pool.rs#L45)（Connection trait 手动解糖）

## 2.2 类型安全
- v3.5.0 评分：7/10
- v3.6.0 评分：9/10（+2，新增 15 表达式 + typed relation + 自定义诊断）
- 证据：[typed_ast.rs:397](file:///.../typed_ast.rs#L397)（61 种表达式）+ [typed_relation.rs:35](file:///.../typed_relation.rs#L35)（typed relation）+ [diagnostic.rs:10](file:///.../diagnostic.rs#L10)（自定义诊断）

# §6 已知不足更新（标注 v3.6.0 改进状态）
## 6.1 生态成熟度
- v3.5.0：不足（社区规模小）
- v3.6.0：⚠️ 部分改进（已补齐 195 missing_docs，但社区规模未扩展）
- 证据：[lib.rs:404](file:///.../lib.rs#L404)（#![warn(missing_docs)]）
```

**前置条件**：v3.6.0 代码基线已验收。

**后置条件**：
- 文档头部更新到 v3.6.0 基线
- 13 维度重新评分，每条变更附 v3.6.0 file:line 证据
- §6 已知不足各子节标注 v3.6.0 改进状态
- §7 结论与建议更新
- §5 独特优势更新
- 审计验证脚本通过（所有 file:line 证据真实存在）
- 纯文档变更，无 .rs 代码变更

#### 2.2.2.5 云数仓验证接口（REQ-CLOUD-001/002/003）

**接口签名**（纯文档，验证报告）：

```markdown
# docs/snowflake-cloud-verification.md
> 版本：v3.7.0 | 日期：2026-08-10
> 验证目标：SnowflakeDialect 真实云数据库行为一致性
> 验证结果：✅ 已验证 / ❌ 缺口报告

## 1. 验证环境
- Snowflake 实例：[如可用，标注实例信息；如不可用，标注"无可用实例"]

## 2. 验证用例
| 用例 | SQL 生成测试 | 真实云验证 | 差异 |
|------|------------|-----------|------|
| UPSERT | ✅ | ✅/❌ | 无/有 |
| TIME TRAVEL | ✅ | ✅/❌ | 无/有 |
| VARIANT 类型 | ✅ | ✅/❌ | 无/有 |

## 3. 验证结论
- [如已验证] SnowflakeDialect 行为一致性已验证
- [如缺口] Snowflake 真实云验证待补，替代方案：[本地模拟/SQL 生成 + 人工审核]
```

**前置条件**：Snowflake/Redshift 云实例可用性评估完成。

**后置条件**：
- 验证报告附测试用例、结果、与 SQL 生成测试的差异（如有）
- 云实例不可用时输出缺口报告 + 替代方案

#### 2.2.2.6 工程规范化接口（REQ-ENG-001/002/003/004）

**接口签名**（git + 门禁，非 Rust API）：

```bash
# 1. v3.6.0 未提交工作入库（按里程碑分组提交）
git add packages/sz-orm-core/src/typed_relation.rs  # M1
git commit -m "feat(v3.6.0): M1 typed relation 探索实现"
git add packages/sz-orm-core/src/sql_verify.rs  # M1
git commit -m "feat(v3.6.0): M1 proc-macro SQL 验证探索"
# ... 按里程碑分组提交

# 2. Prisma 评估结论落地
# 审查 docs/prisma-dialect-evaluation.md 结论，标注正式落地

# 3. 14 道门禁全通过
.\scripts\gate.ps1  # 运行 14 道门禁
```

**前置条件**：v3.7.0 五个里程碑（M1-M5）已完成。

**后置条件**：
- v3.6.0 未提交工作（213 文件）入库
- Prisma 评估结论落地文档
- 14 道门禁全部通过
- 无 todo!/unimplemented!/unreachable! 占位实现

## 2.3 数据模型

### 2.3.1 设计目标

**真实 DB e2e 测试体系数据模型目标**：
1. 支持 8 类核心业务路径的真实 DB e2e 测试
2. 测试幂等且隔离（每次运行清理残留，测试间互不干扰）
3. 测试超时控制（单方言 60s，全方言 300s）
4. 复用既有 tests/common/ adapter，不重复造轮子

**对比分析文档数据模型目标**：
1. 文档头部与代码基线一致（v3.6.0）
2. 13 维度评分附 v3.6.0 file:line 证据
3. §6 已知不足标注 v3.6.0 改进状态
4. 纯文档不改变代码行为

**探索能力成熟化数据模型目标**：
1. 既有 typed_relation/sql_verify/diagnostic 数据模型不变（向后兼容）
2. 补齐测试覆盖至 ≥10 用例
3. 稳定性标注（Cargo.toml feature 注释 "stable"）

### 2.3.2 模型实现

**真实 DB e2e 测试数据模型**：

```plantuml
@startuml
!theme plain
title 真实 DB e2e 测试数据模型

class E2eRealDbTestSuite {
  +name: String
  +database_url: String
  +timeout_per_suite: Duration
  +cleanup_strategy: CleanupStrategy
  +run() -> TestResult
}

class CleanupStrategy <<enum>> {
  DROP_TABLE_IF_EXISTS
  TRANSACTION_ROLLBACK
  UNIQUE_TABLE_NAME
}

class TestResult {
  +passed: usize
  +failed: usize
  +skipped: usize
  +duration: Duration
  +failures: Vec<TestFailure>
}

class TestFailure {
  +test_name: String
  +error: String
  +duration: Duration
}

E2eRealDbTestSuite --> CleanupStrategy
E2eRealDbTestSuite --> TestResult
TestResult --> TestFailure

note right of E2eRealDbTestSuite
  8 类核心路径测试套件：
  1. e2e_real_db_crud (CRUD)
  2. e2e_real_db_transaction (事务)
  3. e2e_real_db_eager_load (预加载)
  4. e2e_real_db_pagination (分页)
  5. e2e_real_db_soft_delete (软删除)
  6. e2e_real_db_multi_tenant (多租户)
  7. e2e_real_db_cache (缓存)
  8. e2e_real_db_dialect_behavior (方言行为)
end note

@enduml
```

**对比分析文档数据模型**：

```plantuml
@startuml
!theme plain
title 对比分析文档数据模型

class ComparativeAnalysisDoc {
  +version: String  // "v3.6.0"
  +date: String  // "2026-08-10"
  +code_baseline: String  // "Cargo.toml:6 workspace.package.version = 3.6.0"
  +dimensions: Vec<Dimension>
  +unique_advantages: Vec<Advantage>
  +known_gaps: Vec<KnownGap>
  +conclusion: Conclusion
}

class Dimension {
  +name: String  // "异步/类型安全/..."
  +score_v3_5_0: u8
  +score_v3_6_0: u8
  +evidence: Vec<FileLineEvidence>
  +change_reason: String
}

class FileLineEvidence {
  +file_path: String
  +line_number: u32
  +description: String
}

class KnownGap {
  +section: String  // "§6.1 生态成熟度"
  +v3_5_0_status: String
  +v3_6_0_status: GapStatus  // ✅/⚠️/❌
  +evidence: Vec<FileLineEvidence>
}

class GapStatus <<enum>> {
  IMPROVED  // ✅ 已改进
  PARTIALLY_IMPROVED  // ⚠️ 部分改进
  NOT_IMPROVED  // ❌ 未改进
}

ComparativeAnalysisDoc --> Dimension
ComparativeAnalysisDoc --> KnownGap
Dimension --> FileLineEvidence
KnownGap --> FileLineEvidence
KnownGap --> GapStatus

@enduml
```

**探索能力成熟化数据模型**（既有，不变）：

```plantuml
@startuml
!theme plain
title 探索能力成熟化数据模型（既有，不变）

class TypedTable <<trait>> {
  +NAME: &'static str
  +PrimaryKey: Clone + Debug
  +ForeignKey: Clone + Debug
}

class Relation <<trait>> {
  +Child: TypedTable
  +Parent: TypedTable
  +KIND: RelationKind
}

class BelongsTo <<ZST>>
class HasMany <<ZST>>
class HasOne <<ZST>>

class TypeMismatchDiagnostic {
  +location: String
  +expected: String
  +found: String
  +suggestion: String
  +format_error() -> String
}

class VerifyResult {
  +is_valid: bool
  +errors: Vec<String>
  +sql: String
}

Relation --> TypedTable
BelongsTo ..|> Relation
HasMany ..|> Relation
HasOne ..|> Relation

note right of BelongsTo
  v3.7.0 成熟化：
  - 补齐测试 ≥10
  - 补齐文档
  - 标注 stable
  - 既有 API 不变
end note

@enduml
```

---

# 三、Feature Gate 设计

## 3.1 新增 Feature Gate 清单

| Feature | 所属包 | 默认 | 依赖 | 关联方向 | 说明 |
|---------|--------|------|------|---------|------|
| `e2e-real-db` | sz-orm-core | 关闭 | 无（复用既有 tests/common/ adapter + 本机 DB 实例） | 方向 1 | 真实 DB e2e 测试套件（连真实 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 验证 8 类核心路径） |
| `custom-diagnostic` | sz-orm-macros | 关闭 | 无（复用既有 diagnostic.rs） | 方向 3 | 自定义编译期诊断信息（独立 feature gate，与 typed-dsl 解耦） |
| `dialect-informix` | sz-orm-core | 关闭 | 无（独立实现 Dialect trait 或仅 SQL 生成） | 方向 4 | Informix 方言（按需实现，SERIAL/ROW 类型 + PUT 语句） |
| `dialect-saphana` | sz-orm-core | 关闭 | 无（独立实现 Dialect trait 或仅 SQL 生成） | 方向 4 | SAP HANA 方言（按需实现，计算列 + CE 函数） |
| `dialect-firebird` | sz-orm-core | 关闭 | 无（独立实现 Dialect trait 或仅 SQL 生成） | 方向 4 | Firebird 方言（按需实现，GENERATOR/SEQUENCE + EXECUTE BLOCK） |

**既有 Feature 复用**（不新增，v3.7.0 复用 + 成熟化标注）：

| Feature | 所属包 | 默认 | 关联方向 | 说明 |
|---------|--------|------|---------|------|
| `typed-relation` | sz-orm-core | 关闭 | 方向 3 | 类型安全关联查询（v3.6.0 探索 → v3.7.0 stable，[Cargo.toml:58](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L58) 既有） |
| `sql-verify-proc` | sz-orm-core | 关闭 | 方向 3 | proc-macro 编译期 SQL 验证（v3.6.0 探索 → v3.7.0 stable，[Cargo.toml:60](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L60) 既有） |
| `dialect-snowflake` | sz-orm-core | 关闭 | 方向 5 | Snowflake 方言（v3.6.0 既有，[Cargo.toml:70](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L70)） |
| `dialect-redshift` | sz-orm-core | 关闭 | 方向 5 | Redshift 方言（v3.6.0 既有，[Cargo.toml:72](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L72)） |

## 3.2 Feature 正交性矩阵

| Feature | e2e-real-db | typed-relation | sql-verify-proc | custom-diagnostic | dialect-informix | dialect-saphana | dialect-firebird | dialect-snowflake | dialect-redshift |
|---------|-------------|----------------|-----------------|-------------------|-------------------|------------------|-------------------|-------------------|------------------|
| e2e-real-db | — | ✅ 正交 | ✅ 正交 | ✅ 正交 | ✅ 正交 | ✅ 正交 | ✅ 正交 | ✅ 正交 | ✅ 正交 |
| typed-relation | ✅ | — | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| sql-verify-proc | ✅ | ✅ | — | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| custom-diagnostic | ✅ | ✅ | ✅ | — | ✅ | ✅ | ✅ | ✅ | ✅ |
| dialect-informix | ✅ | ✅ | ✅ | ✅ | — | ✅ | ✅ | ✅ | ✅ |
| dialect-saphana | ✅ | ✅ | ✅ | ✅ | ✅ | — | ✅ | ✅ | ✅ |
| dialect-firebird | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — | ✅ | ✅ |
| dialect-snowflake | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — | ✅ |
| dialect-redshift | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — |

**正交性结论**：所有新增 feature 互不依赖，可任意组合启用/关闭，无冲突。原因：
1. `e2e-real-db` 仅影响 tests/e2e_real_db_*.rs 测试套件，不触碰 src/ 代码。
2. `typed-relation`/`sql-verify-proc`/`custom-diagnostic` 仅影响各自模块，通过公开 API 协作，不依赖其他 feature。
3. `dialect-informix`/`dialect-saphana`/`dialect-firebird`/`dialect-snowflake`/`dialect-redshift` 仅新增 DbType 变体 + Dialect 实现，不依赖其他 feature。

## 3.3 默认 Feature 零行为变更保证

**默认 feature**（[Cargo.toml:15](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L15)）：`default = ["redis"]`。

**零行为变更保证**：
1. 所有新增 feature（e2e-real-db/custom-diagnostic/dialect-informix/dialect-saphana/dialect-firebird）默认关闭，不在 default = ["redis"] 中。
2. 既有 feature 复用（typed-relation/sql-verify-proc/dialect-snowflake/dialect-redshift）默认关闭。
3. 默认 feature 关闭时：
   - 编译产物大小与 v3.6.0 一致（无新增代码编译）。
   - 运行时开销与 v3.6.0 一致（无新增逻辑执行）。
   - 既有公开 API 行为与 v3.6.0 一致（无行为变更）。
4. 新增依赖（如 Informix/SAP HANA/Firebird 驱动，如需）为 optional，仅对应 feature 启用时引入，默认 feature 无新依赖（spec §4.1.5）。

---

# 四、兼容性设计

## 4.1 每个方向的 Breaking Change 风险评估

| 方向 | Breaking Change 风险 | 风险点 | 缓解措施 | sz-pay 影响评估 |
|------|---------------------|--------|---------|---------------|
| 1 真实 DB e2e 测试体系 | **无** | feature gate 隔离，既有 96 e2e + 63 integration 测试不变 | `e2e-real-db` feature gate（默认关闭） | 无（sz-pay 未用 e2e-real-db feature） |
| 2 对比分析重评估 | **无** | 纯文档，不改代码 | — | 无 |
| 3 探索能力成熟化 | **无** | 既有 API 向后兼容，仅补齐测试与文档 + 稳定性标注 | feature gate + 仅补齐测试文档 | 无（sz-pay 未用 typed-relation/sql-verify-proc/custom-diagnostic feature） |
| 4 方言扩展 | **无** | feature gate 隔离，既有 20 种方言不变 | `dialect-informix`/`dialect-saphana`/`dialect-firebird` feature gate + #[non_exhaustive] | 无（sz-pay 用既有 DbType 变体） |
| 5 云数仓验证 | **无** | 仅验证报告，不改变代码 | — | 无 |
| 6 工程规范化 | **无** | git 提交 + 门禁，不改 API | — | 无 |

## 4.2 向后兼容方案

### 4.2.1 Feature Gate 隔离

所有新能力通过 feature gate 隔离，默认关闭，既有公开 API 行为不变：
- `e2e-real-db`：真实 DB e2e 测试套件仅在 feature 启用时编译运行，既有 96 e2e + 63 integration 测试不变。
- `custom-diagnostic`：自定义诊断独立 feature gate，既有 typed-dsl feature 不受影响。
- `dialect-informix`/`dialect-saphana`/`dialect-firebird`：新方言仅在 feature 启用时编译，既有 20 种方言不变。
- `typed-relation`/`sql-verify-proc`：既有 feature 复用，仅补齐测试与文档 + 稳定性标注，API 不变。

### 4.2.2 既有测试保留不替换

- 既有 96 e2e 测试（InMemoryDb）保留不替换（spec §1.4.3）。
- 既有 63 integration 测试（`#[ignore]`）保留不替换（spec §1.4.4）。
- 真实 DB e2e 测试为新增补充，与既有测试共存。

### 4.2.3 SemVer 兼容

- v3.7.0 版本号遵循 SemVer，minor 版本升级（3.6.0 → 3.7.0）保证向后兼容（spec §4.5.2）。
- 既有公开 API 签名不变，新增 API 通过 feature gate 隔离。
- sz-pay 从 crates.io 3.6.0 升级到 3.7.0，dry-run + 零回归验证保证平滑升级。

## 4.3 v3.6.0 测试基线不回退保证

**基线**：v3.6.0 已验收测试基线（M1-M5 五个里程碑完成，96 e2e + 63 integration + 单元/集成测试）。

**不回退保证**：
1. v3.7.0 不得使 v3.6.0 已验收测试基线回退，仅增不减（spec §4.2.3）。
2. 所有新增能力附测试（单元测试 + 集成测试按需），不修改既有测试。
3. 既有公开 API 签名不变，既有测试不受影响。
4. CI 门禁前置：`cargo test --workspace -j 2 --no-fail-fast` 全通过才允许合入。
5. sz-pay 零回归验证：升级到 3.7.0 后 cargo test 零回归（与 sz-pay 既有测试基线对比）。

---

# 五、里程碑设计

## 5.1 6 个里程碑详细任务分解

### 5.1.1 M1：方向 1 真实 DB e2e 测试体系（最高优先级，中风险）

**周期**：3 周
**依赖**：本机 DB 实例（MySQL/PostgreSQL/SQLite/Oracle/MSSQL）+ 既有 tests/common/ adapter
**关联需求**：REQ-E2E-001~006

**任务分解**：

| 任务 ID | 任务描述 | 子任务 | 预估工时 | 关联需求 |
|---------|---------|--------|---------|---------|
| M1-T1 | 新增 e2e-real-db feature gate | Cargo.toml feature 定义 + #[cfg(feature)] 条件编译 | 1h | REQ-E2E-003 |
| M1-T2 | 实现 e2e_real_db_crud.rs | 连真实 DB + CRUD + 批量操作 + 清理 + ≥10 测试 | 6h | REQ-E2E-001/002 |
| M1-T3 | 实现 e2e_real_db_transaction.rs | 事务 commit/rollback/savepoint + 嵌套事务 + ≥8 测试 | 4h | REQ-E2E-002 |
| M1-T4 | 实现 e2e_real_db_eager_load.rs | BelongsTo/HasMany/HasOne + N+1 检测 + ≥8 测试 | 4h | REQ-E2E-002 |
| M1-T5 | 实现 e2e_real_db_pagination.rs | offset/limit + keyset 分页 + ≥6 测试 | 3h | REQ-E2E-002 |
| M1-T6 | 实现 e2e_real_db_soft_delete.rs | deleted_at 字段 + 过滤 + 恢复 + ≥5 测试 | 3h | REQ-E2E-002 |
| M1-T7 | 实现 e2e_real_db_multi_tenant.rs | tenant_id 隔离 + 行级安全 + ≥5 测试 | 3h | REQ-E2E-002 |
| M1-T8 | 实现 e2e_real_db_cache.rs | L1/L2 缓存命中/失效/一致性 + ≥6 测试 | 4h | REQ-E2E-002 |
| M1-T9 | 实现 e2e_real_db_dialect_behavior.rs | UPSERT/行锁/标识符引用/方言特有语法 + ≥8 测试 | 4h | REQ-E2E-002 |
| M1-T10 | 实现测试清理机制 | DROP TABLE IF EXISTS + 独立表名 + 事务回滚 | 3h | REQ-E2E-004 |
| M1-T11 | 实现测试超时机制 | 单方言 60s + 全方言 300s + tokio::time::timeout | 2h | REQ-E2E-001 |
| M1-T12 | DATABASE_URL 不硬编码验证 | 环境变量传入 + 测试库 sz_orm_test 验证 | 1h | REQ-E2E-006 |
| M1-T13 | 既有测试不回退验证 | 既有 96 e2e + 63 integration 测试全通过 | 2h | REQ-E2E-005 |

**验收标准**：
- [ ] 连真实 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 的 e2e 测试套件实现
- [ ] 覆盖 8 类核心业务路径（CRUD/事务/预加载/分页/软删除/多租户/缓存/方言行为）
- [ ] `e2e-real-db` feature gate 隔离，默认关闭，CI 启用
- [ ] 无 DB 环境默认 cargo test 编译运行通过
- [ ] `cargo test --features e2e-real-db` 连真实 DB 运行通过
- [ ] 测试幂等且隔离（连续两次运行结果一致）
- [ ] 复用 tests/common/ 既有 adapter
- [ ] 单方言执行 ≤60s，全方言 ≤300s
- [ ] DATABASE_URL 不硬编码，测试库为 sz_orm_test
- [ ] 既有 96 e2e + 63 integration 测试不回退

### 5.1.2 M2：方向 2 对比分析重评估与文档同步（高优先级，低风险）

**周期**：1 周
**依赖**：M1（真实 DB e2e 测试体系就绪后评分更准确）
**关联需求**：REQ-REEVAL-001~005

**任务分解**：

| 任务 ID | 任务描述 | 子任务 | 预估工时 | 关联需求 |
|---------|---------|--------|---------|---------|
| M2-T1 | 更新文档头部到 v3.6.0 基线 | 版本号 + 日期 + 代码基线 | 1h | REQ-REEVAL-001 |
| M2-T2 | 13 维度重新评分 | 逐维度基于 v3.6.0 代码证据重新评分 + 每条变更附 file:line 证据 | 8h | REQ-REEVAL-002 |
| M2-T3 | 更新 §6 已知不足 | 各子节标注 v3.6.0 改进状态（✅/⚠️/❌）+ 证据 | 4h | REQ-REEVAL-003 |
| M2-T4 | 更新 §7 结论与建议 | 综合结论 + 定位建议 + 改进建议反映 v3.6.0 后真实状态 | 2h | REQ-REEVAL-004 |
| M2-T5 | 更新 §5 独特优势 | 新增 v3.6.0 独特优势项（15 表达式 + typed relation + 自定义诊断 + proc-macro SQL 验证 + Snowflake/Redshift + QueryBuilder 迁移工具） | 2h | REQ-REEVAL-004 |
| M2-T6 | 审计验证脚本运行 | bash scripts/audit-verify.sh 验证所有 file:line 证据真实存在 | 2h | REQ-REEVAL-002 |
| M2-T7 | 纯文档变更验证 | git diff 确认仅文档变更，无 .rs 代码变更 | 1h | REQ-REEVAL-005 |

**验收标准**：
- [ ] 文档头部更新到 v3.6.0 基线（版本/日期/代码基线）
- [ ] 13 个维度重新评分，每条变更附 file:line 证据
- [ ] §6 已知不足各子节标注 v3.6.0 改进状态（✅/⚠️/❌）
- [ ] §7 结论与建议更新
- [ ] §5 独特优势更新（v3.6.0 新增项）
- [ ] 审计验证脚本通过（所有 file:line 证据真实存在）
- [ ] 纯文档变更，无 .rs 代码变更

### 5.1.3 M3：方向 3 v3.6.0 探索能力成熟化（高优先级，低风险）

**周期**：2 周
**依赖**：M1（feature gate 体系就绪）
**关联需求**：REQ-MAT-001~005

**任务分解**：

| 任务 ID | 任务描述 | 子任务 | 预估工时 | 关联需求 |
|---------|---------|--------|---------|---------|
| M3-T1 | typed relation 补齐测试 | 编译期校验 + 运行时查询 + 外键类型匹配 + 表归属 + 与 EagerLoader 协作 + escape hatch，≥10 测试 | 6h | REQ-MAT-001 |
| M3-T2 | typed relation 补齐文档 | 迁移指南 + 适用场景 + escape hatch 说明 + 稳定性标注 | 3h | REQ-MAT-001 |
| M3-T3 | typed relation Cargo.toml 标注 stable | feature 注释更新 "stable" | 1h | REQ-MAT-001 |
| M3-T4 | 自定义诊断独立 feature gate | sz-orm-macros/Cargo.toml 新增 custom-diagnostic feature + #[cfg(feature)] 条件编译 | 2h | REQ-MAT-002 |
| M3-T5 | 自定义诊断补齐测试 | 错误位置 + 期望类型 + 实际类型 + 修复建议 + 各诊断场景，≥10 测试 | 4h | REQ-MAT-002 |
| M3-T6 | 自定义诊断补齐文档 | 启用方式 + 诊断场景 + 迁移指南 | 2h | REQ-MAT-002 |
| M3-T7 | proc-macro SQL 验证扩展连真 DB EXPLAIN | 覆盖所有 QueryBuilder 路径（SELECT/INSERT/UPDATE/DELETE/JOIN/子查询/CTE/窗口） | 6h | REQ-MAT-003 |
| M3-T8 | proc-macro SQL 验证补齐测试 | SQL 解析 + 表/列存在性 + 类型匹配 + EXPLAIN only + 缓存 + 降级模式，≥10 测试 | 4h | REQ-MAT-003 |
| M3-T9 | proc-macro SQL 验证补齐文档 | 启用方式 + DATABASE_URL 配置 + 降级模式说明 + 稳定性标注 | 2h | REQ-MAT-003 |
| M3-T10 | v3.6.0 既有 API 向后兼容验证 | 既有 typed-relation/sql-verify 调用编译运行通过，无 breaking change | 2h | REQ-MAT-004 |
| M3-T11 | 无运行时开销验证 | 性能基准对比，启用/禁用 feature 运行时性能无差异 | 2h | REQ-MAT-005 |

**验收标准**：
- [ ] typed relation 转正式 feature（`typed-relation` stable），测试覆盖 ≥10 + 文档完整
- [ ] 自定义编译期诊断转正式 feature（`custom-diagnostic` stable），测试覆盖 ≥10 + 文档完整
- [ ] proc-macro SQL 验证转正式 feature（`sql-verify-proc` stable），连真 DB EXPLAIN 覆盖所有路径
- [ ] v3.6.0 既有 API 向后兼容（无 breaking change）
- [ ] 无运行时开销（编译期工作）
- [ ] 每个 feature 至少 10 个测试用例

### 5.1.4 M4：方向 4 方言扩展延续（中优先级，中风险）

**周期**：2 周
**依赖**：M1（feature gate 体系就绪）
**关联需求**：REQ-DIALECT-001~005

**任务分解**：

| 任务 ID | 任务描述 | 子任务 | 预估工时 | 关联需求 |
|---------|---------|--------|---------|---------|
| M4-T1 | 评估 Rust Informix 驱动成熟度 | 调研 Informix Rust 驱动 + 成熟度评估 | 2h | REQ-DIALECT-001 |
| M4-T2 | InformixDialect 实现（按需） | Dialect trait + SERIAL/ROW 类型 + PUT 语句 + 方言测试（如驱动成熟） | 8h | REQ-DIALECT-001 |
| M4-T3 | 评估 Rust SAP HANA 驱动成熟度 + 企业需求 | 调研 SAP HANA Rust 驱动 + 企业需求评估 | 2h | REQ-DIALECT-002 |
| M4-T4 | SapHanaDialect 实现（按需） | Dialect trait + 计算列 + CE 函数 + 方言测试（如驱动成熟 + 需求出现） | 8h | REQ-DIALECT-002 |
| M4-T5 | 评估 Firebird 用户需求 | 调研 Firebird 用户需求 | 1h | REQ-DIALECT-003 |
| M4-T6 | FirebirdDialect 实现（按需） | Dialect trait + GENERATOR/SEQUENCE + EXECUTE BLOCK + 方言测试（如需求出现） | 6h | REQ-DIALECT-003 |
| M4-T7 | DbType 新增变体 + 路线图更新 | db_type.rs 新增 Informix/SapHana/Firebird 变体 + 路线图标注 v3.7.0 状态 | 2h | REQ-DIALECT-005 |
| M4-T8 | 既有 20 种方言不回退验证 | 既有方言测试全通过 | 2h | REQ-DIALECT-004 |
| M4-T9 | Cargo.toml 新增 dialect-informix/saphana/firebird feature | feature 定义 + #[cfg(feature)] | 1h | REQ-DIALECT-001/002/003 |

**验收标准**：
- [ ] Informix 方言实现（或标注 Rust 驱动不成熟暂缓）
- [ ] SAP HANA 方言实现（或标注暂缓）
- [ ] Firebird 方言实现（或标注暂缓）
- [ ] 既有 20 种方言测试不回退
- [ ] DbType 枚举新增变体 + 路线图更新
- [ ] 对比分析文档方言数量更新

### 5.1.5 M5：方向 5 Snowflake/Redshift 真实云数据库验证（中优先级，中风险）

**周期**：1 周
**依赖**：M1（feature gate 体系就绪）
**关联需求**：REQ-CLOUD-001~003

**任务分解**：

| 任务 ID | 任务描述 | 子任务 | 预估工时 | 关联需求 |
|---------|---------|--------|---------|---------|
| M5-T1 | 评估 Snowflake 云实例可用性 | 调研 Snowflake 云账号/实例可达性 | 2h | REQ-CLOUD-001 |
| M5-T2 | Snowflake 真实云验证（如可用） | 连真实 Snowflake 验证 UPSERT/TIME TRAVEL/VARIANT 类型行为一致性 | 6h | REQ-CLOUD-001 |
| M5-T3 | Snowflake 验证缺口报告（如不可用） | 输出缺口报告 + 替代方案（本地模拟/SQL 生成 + 人工审核） | 2h | REQ-CLOUD-001 |
| M5-T4 | 评估 Redshift 云实例可用性 | 调研 AWS Redshift Serverless/Provisioned 可达性 | 2h | REQ-CLOUD-002 |
| M5-T5 | Redshift 真实云验证（如可用） | 连真实 Redshift 验证 COPY/UNLOAD/PG 兼容性行为一致性 | 6h | REQ-CLOUD-002 |
| M5-T6 | Redshift 验证缺口报告（如不可用） | 输出缺口报告 + 替代方案 | 2h | REQ-CLOUD-002 |
| M5-T7 | 验证报告文档化 | 测试用例 + 结果 + 与 SQL 生成测试的差异分析 | 2h | REQ-CLOUD-003 |

**验收标准**：
- [ ] Snowflake 真实云验证（或输出缺口报告 + 替代方案）
- [ ] Redshift 真实云验证（或输出缺口报告）
- [ ] 验证报告文档化（用例/结果/差异分析）

### 5.1.6 M6：方向 6 工程规范化（低但必须，低风险）

**周期**：1 周
**依赖**：M1-M5（五个里程碑完成后入库）
**关联需求**：REQ-ENG-001~004

**任务分解**：

| 任务 ID | 任务描述 | 子任务 | 预估工时 | 关联需求 |
|---------|---------|--------|---------|---------|
| M6-T1 | v3.6.0 未提交工作入库（按里程碑分组） | 213 文件按 M1-M5 分组提交 + 提交信息遵循既有风格 | 4h | REQ-ENG-001 |
| M6-T2 | Prisma 评估结论落地 | 审查 docs/prisma-dialect-evaluation.md 结论 + 标注正式落地 | 2h | REQ-ENG-002 |
| M6-T3 | 14 道门禁运行 | fmt/check/clippy/test/doc/audit/integration/占位检查/SQL注入/feature全组合/上游未改/文档一致性/审计证据/文档同步 | 4h | REQ-ENG-003 |
| M6-T4 | 占位实现检查 | grep todo!/unimplemented!/unreachable! 无匹配 | 1h | REQ-ENG-004 |
| M6-T5 | sz-pay 零回归验证 | sz-pay 升级到 3.7.0 + cargo check + cargo test 零回归 | 2h | REQ-ENG-003 |

**验收标准**：
- [ ] v3.6.0 未提交工作（213 文件）入库
- [ ] Prisma 方言评估结论落地文档
- [ ] 14 道门禁全部通过
- [ ] 无 todo!/unimplemented!/unreachable! 占位实现
- [ ] sz-pay 零回归验证通过

## 5.2 里程碑间依赖关系

```plantuml
@startuml
!theme plain
title v3.7.0 里程碑依赖关系

rectangle "M1 真实 DB e2e 测试体系\n(3周, 最高优先级, 中风险)" as M1
rectangle "M2 对比分析重评估\n(1周, 高优先级, 低风险)" as M2
rectangle "M3 探索能力成熟化\n(2周, 高优先级, 低风险)" as M3
rectangle "M4 方言扩展延续\n(2周, 中优先级, 中风险)" as M4
rectangle "M5 云数仓真实验证\n(1周, 中优先级, 中风险)" as M5
rectangle "M6 工程规范化\n(1周, 低但必须, 低风险)" as M6

M1 --> M2 : 真实 DB e2e 测试就绪后评分更准确
M1 --> M3 : feature gate 体系就绪
M1 --> M4 : feature gate 体系就绪
M1 --> M5 : feature gate 体系就绪
M1 --> M6 : M1 完成后入库
M2 --> M6 : M2 完成后入库
M3 --> M6 : M3 完成后入库
M4 --> M6 : M4 完成后入库
M5 --> M6 : M5 完成后入库

note bottom of M1
  **关键路径**：M1 → M2（串行 4 周）→ M6（串行 5 周）
  **并行机会**：
  - M1 完成后 M3/M4/M5 可并行
  - M1 内部：8 类核心路径测试可部分并行
  - M3 内部：typed relation + 自定义诊断 + proc-macro SQL 验证可并行
  - M4 内部：Informix + SAP HANA + Firebird 评估可并行
  **总周期**：关键路径 5 周；并行开发下可压缩至 4-5 周
end note

@enduml
```

## 5.3 每个里程碑的验收标准

（已在 §5.1.1~§5.1.6 各里程碑任务分解后列出验收标准，此处汇总）

| 里程碑 | 验收标准数 | 关键验收标准 |
|--------|-----------|------------|
| M1 | 10 | 真实 DB e2e 测试套件 + 8 类核心路径 + feature gate 隔离 + 幂等隔离 + 既有测试不回退 |
| M2 | 7 | 文档头部 v3.6.0 + 13 维度重新评分 + §6 改进状态 + §7 结论更新 + 审计验证通过 + 纯文档 |
| M3 | 6 | typed relation stable + custom-diagnostic stable + sql-verify-proc stable + 向后兼容 + 无运行时开销 + ≥10 测试 |
| M4 | 6 | Informix/SAP HANA/Firebird 实现（或暂缓）+ 既有 20 种不变 + DbType 变体 + 路线图更新 |
| M5 | 3 | Snowflake 真实云验证（或缺口报告）+ Redshift 真实云验证（或缺口报告）+ 验证报告文档化 |
| M6 | 5 | 213 文件入库 + Prisma 落地 + 14 门禁通过 + 无占位实现 + sz-pay 零回归 |

---

# 六、测试策略

## 6.1 每个方向的测试方案

### 6.1.1 方向 1：真实 DB e2e 测试体系

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| 真实 DB e2e 测试 | 8 类核心路径（CRUD/事务/预加载/分页/软删除/多租户/缓存/方言行为）连真实 DB 验证 | `cargo test --features e2e-real-db` | ≥56 测试用例 |
| 测试幂等性 | 连续两次运行同一 e2e 套件结果一致 | `cargo test --features e2e-real-db`（连续两次） | 无残留数据冲突 |
| 测试隔离性 | 每个测试用独立表名或独立事务回滚 | 代码审查 + 测试验证 | 测试间互不干扰 |
| 测试超时 | 单方言 ≤60s，全方言 ≤300s | `cargo test --features e2e-real-db` + tokio::time::timeout | 超时标记失败 |
| 无 DB 环境跳过 | 无 DB 环境默认 cargo test 编译运行通过 | `cargo test`（无 DATABASE_URL） | 编译通过 + 测试跳过 |
| 既有测试不回退 | 既有 96 e2e + 63 integration 测试全通过 | `cargo test`（默认 feature） | v3.6.0 基线不回退 |

### 6.1.2 方向 2：对比分析重评估与文档同步

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| 审计验证 | 所有 file:line 证据真实存在 | `bash scripts/audit-verify.sh docs/sz-orm与同类产品对比分析.md` | 证据 100% 真实 |
| 纯文档验证 | git diff 确认仅文档变更，无 .rs 代码变更 | `git diff --name-only` | 仅 .md 文件变更 |
| 文档头部验证 | 版本号/日期/代码基线与 v3.6.0 一致 | 代码审查 | 头部正确 |

### 6.1.3 方向 3：v3.6.0 探索能力成熟化

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| typed relation 测试 | 编译期外键校验 + 运行时查询 + 表归属 + 与 EagerLoader 协作 + escape hatch | `cargo test --features typed-relation` | ≥10 测试用例 |
| 自定义诊断测试 | 错误位置 + 期望类型 + 实际类型 + 修复建议 + 各诊断场景 | `cargo test --features custom-diagnostic` | ≥10 测试用例 |
| proc-macro SQL 验证测试 | SQL 解析 + 表/列存在性 + 类型匹配 + EXPLAIN only + 缓存 + 降级模式 | `cargo test --features sql-verify-proc` | ≥10 测试用例 |
| 向后兼容测试 | v3.6.0 既有 typed-relation/sql-verify 调用编译运行通过 | `cargo test --features typed-relation,sql-verify-proc` | 无 breaking change |
| 无运行时开销测试 | 启用/禁用 feature 运行时性能无差异 | `cargo bench --features typed-relation,sql-verify-proc` | 无开销增加 |
| 既有测试不回退 | 既有测试全通过 | `cargo test` | v3.6.0 基线不回退 |

### 6.1.4 方向 4：方言扩展延续

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| 新方言单元测试 | InformixDialect/SapHanaDialect/FirebirdDialect Dialect trait 实现 + 特有特性 | `cargo test --features dialect-informix,dialect-saphana,dialect-firebird` | Dialect trait 方法覆盖 |
| 既有方言不回退 | 既有 20 种方言测试全通过 | `cargo test` | v3.6.0 基线不回退 |
| DbType 变体测试 | 新增 Informix/SapHana/Firebird 变体 + as_str/from_str/default_port | `cargo test -p sz-orm-core --test db_type` | 变体正确 |

### 6.1.5 方向 5：Snowflake/Redshift 真实云数据库验证

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| Snowflake 真实云验证 | 连真实 Snowflake 验证 UPSERT/TIME TRAVEL/VARIANT 类型行为一致性 | 手动运行（需云实例） | 行为一致性 |
| Redshift 真实云验证 | 连真实 Redshift 验证 COPY/UNLOAD/PG 兼容性行为一致性 | 手动运行（需云实例） | 行为一致性 |
| 验证报告文档化 | 验证报告附测试用例/结果/差异分析 | 代码审查 | 报告完整 |

### 6.1.6 方向 6：工程规范化

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| 14 道门禁 | fmt/check/clippy/test/doc/audit/integration/占位检查/SQL注入/feature全组合/上游未改/文档一致性/审计证据/文档同步 | `.\scripts\gate.ps1` | 全部通过 |
| 占位实现检查 | grep todo!/unimplemented!/unreachable! 无匹配 | `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'` | 无匹配 |
| sz-pay 零回归 | sz-pay 升级到 3.7.0 + cargo check + cargo test 零回归 | `.\scripts\verify_sz_pay.ps1` | sz-pay 零回归 |

## 6.2 五方言集成测试覆盖

**五方言覆盖约束**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL，所有新能力必须保持五方言行为一致（spec §1.2.12）。

**五方言集成测试矩阵**：

| 新能力 | MySQL | PostgreSQL | SQLite | Oracle | MSSQL | 测试命令 |
|--------|-------|------------|--------|--------|-------|---------|
| 真实 DB e2e 测试（CRUD/事务/预加载/分页/软删除/多租户/缓存） | ✅ | ✅ | ✅ | ✅ | ✅ | `cargo test --features e2e-real-db` |
| 真实 DB e2e 测试（方言行为一致性） | ✅ | ✅ | ✅ | ✅ | ✅ | `cargo test --features e2e-real-db --test e2e_real_db_dialect_behavior` |
| typed relation 成熟化 | ✅ | ✅ | ✅ | ✅ | ✅ | `cargo test --features typed-relation` |
| 自定义诊断成熟化 | N/A（编译期） | N/A | N/A | N/A | N/A | `cargo test --features custom-diagnostic` |
| proc-macro SQL 验证成熟化 | ✅ | ✅ | ✅ | ✅ | ✅ | `cargo test --features sql-verify-proc` |
| Informix/SAP HANA/Firebird 方言 | N/A | N/A | N/A | N/A | N/A | `cargo test --features dialect-informix,dialect-saphana,dialect-firebird` |

**五方言测试环境**（本机数据库，AGENTS.md 记录）：
- MySQL 9.6：`mysql://root:test123@127.0.0.1:3306/sz_orm_test`
- PostgreSQL 18：`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`
- SQLite：内存数据库或文件型
- Oracle 23ai Free：`127.0.0.1:1521/freepdb1`（用户 sys，密码 test123，Sysdba 权限）
- MSSQL：待配置（或通过 Docker）

## 6.3 sz-pay 零回归验证

**验证流程**（ADR-0001 不修改 sz-pay 代码）：
1. 复制 sz-pay 项目到临时验证目录（`E:\vue\test\sz-pay-upgrade-verify\`）。
2. 修改临时目录的 `Cargo.toml`，将 sz-orm-* 版本号从 3.6.0 改为 3.7.0。
3. 设置编译环境：`$env:RUST_MIN_STACK="67108864"` + `$env:CARGO_INCREMENTAL="0"`。
4. `cargo check` 验证编译通过。
5. `cargo test -j 2 --no-fail-fast` 验证测试零回归（与 sz-pay 既有测试基线对比）。
6. 验证结果记录到 `logs/sz-pay-upgrade-<timestamp>.log`。
7. 删除临时验证目录。
8. 不提交任何 sz-pay 修改（ADR-0001 严禁修改下游仓库）。

**零回归标准**：
- `cargo check` 通过（无编译错误）。
- `cargo test` 全通过（与 sz-pay 既有测试基线对比，0 failed）。
- 无 Breaking Change（sz-orm-* API 兼容）。

---

# 七、设计决策汇总

## 7.1 关键设计决策

| 决策点 | 选项 | 选择 | 理由 | file:line 证据 |
|--------|------|------|------|--------------|
| 真实 DB e2e 测试隔离方式 | A. 既有 testing feature / B. 新增 e2e-real-db feature | B | testing feature 用于 tokio/full 运行时，e2e-real-db 专注真实 DB e2e 测试，语义不同 | [Cargo.toml:16](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L16)（testing feature 既有） |
| 真实 DB e2e 测试复用方式 | A. 修改既有 e2e_*.rs / B. 新增 e2e_real_db_*.rs | B | 既有 96 e2e 测试用 InMemoryDb 保留不替换（spec §1.4.3） | [tests/e2e_transaction.rs:27](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/e2e_transaction.rs#L27)（InMemoryDb 既有） |
| 真实 DB e2e 测试覆盖路径 | A. 复刻既有 96 e2e / B. 覆盖 8 类核心路径 | B | 8 类核心路径覆盖更广业务路径，避免简单复刻 | [tests/common/sqlx_pg_adapter.rs:15](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/common/sqlx_pg_adapter.rs#L15)（adapter 复用） |
| 自定义诊断隔离方式 | A. 既有 typed-dsl / B. 新增 custom-diagnostic | B | 自定义诊断是独立能力，与 typed-dsl 表达式不同，独立 feature 便于按需启用 | [sz-orm-macros/Cargo.toml:37](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/Cargo.toml#L37)（typed-dsl 既有） |
| 探索能力成熟化方式 | A. 重写实现 / B. 补齐测试文档 + 标注 stable | B | v3.6.0 探索实现已有，仅需补齐测试 + 文档 + 稳定性标注 | [typed_relation.rs:35](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_relation.rs#L35)（TypedTable 既有）+ [sql_verify.rs:22](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/sql_verify.rs#L22)（VerifyResult 既有）+ [diagnostic.rs:10](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/src/diagnostic.rs#L10)（TypeMismatchDiagnostic 既有） |
| Informix/SAP HANA/Firebird 实现策略 | A. 全部实现 / B. 按需实现（驱动成熟 + 用户需求） | B | spec §5.4.1 要求按需实现，Rust 驱动成熟度 + 用户需求是触发条件 | [dialect.rs:23](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L23)（Dialect trait） |
| Snowflake/Redshift 真实云验证策略 | A. 强制连真实云 / B. 评估可用性，无则输出缺口报告 | B | spec §5.5.3.1 要求云实例不可用时不阻断交付 | [dialect.rs:1567](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1567)（SnowflakeDialect 既有） |
| 对比分析文档更新方式 | A. 重写文档 / B. 更新到 v3.6.0 基线 + 重新评分 | B | 文档结构完整（963 行），仅需更新头部 + 重新评分 + 更新 §6/§7 | [docs/sz-orm与同类产品对比分析.md:3](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/sz-orm与同类产品对比分析.md#L3)（v3.5.0 基线） |
| v3.6.0 未提交工作入库策略 | A. 一次性提交 213 文件 / B. 按里程碑分组提交 | B | 按里程碑分组提交便于追溯 + 每次提交后运行门禁验证 | git status（213 文件未提交） |
| crates.io 版本号 | A. 3.7.0 minor / B. 4.0.0 major | A | feature gate 隔离，向后兼容，SemVer minor | [Cargo.toml:6](file:///E:/vue/test/鲜视达/rust/sz-orm/Cargo.toml#L6)（当前 3.6.0） |
| 既有 integration 测试处理 | A. 移除 #[ignore] / B. 保留不替换 | B | 既有 63 integration 测试保留，真实 DB e2e 测试为新增层次（spec §1.4.4） | [tests/integration_mysql.rs:113](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/integration_mysql.rs#L113)（#[ignore] 既有） |
| 真实 DB e2e 测试清理机制 | A. 全局 DROP SCHEMA / B. DROP TABLE IF EXISTS + 独立表名 | B | 独立表名避免测试间冲突，DROP TABLE IF EXISTS 保证幂等 | [tests/common/schema_builder.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/common/schema_builder.rs)（schema_builder 既有） |

## 7.2 风险矩阵

| 风险 | 概率 | 影响 | 风险等级 | 缓解措施 |
|------|------|------|---------|---------|
| 真实 DB e2e 测试环境依赖（本机 DB 不可用） | 中 | 中 | 中 | feature gate 默认关闭缓解，CI 预置 DB 实例，本地无 DB 可跳过 |
| 真实 DB e2e 测试超时（单方言 >60s） | 低 | 中 | 低 | tokio::time::timeout 超时标记失败，输出卡点 |
| 测试数据清理失败（DROP TABLE 权限不足） | 低 | 低 | 低 | 输出清理失败警告，不影响测试结果判定 |
| 对比分析文档评分与证据矛盾 | 低 | 中 | 低 | 审计验证脚本检测证据真实性，矛盾时修正评分或补充证据 |
| v3.6.0 代码证据不存在（文档引用 file:line 在 v3.6.0 代码库中不存在） | 低 | 中 | 低 | 审计验证脚本检测，标注证据验证失败，修正引用 |
| typed relation 编译期校验误报（合法关联被误判为类型不匹配） | 低 | 中 | 低 | 提供 escape hatch（运行时关联回退 EagerLoader），补充回归测试 |
| proc-macro SQL 验证连真 DB 失败（DATABASE_URL 未设置或 DB 不可达） | 中 | 低 | 低 | 回退到仅语法校验，输出降级警告 |
| proc-macro SQL 验证编译时间显著增加 | 中 | 中 | 中 | 缓存验证结果（按 SQL 哈希缓存），仅 SQL 变更时重新验证，默认关闭 |
| Informix/SAP HANA/Firebird Rust 驱动不成熟 | 高 | 中 | 中 | 仅实现 SQL 生成方言，标注"SQL generation only, no real DB driver" |
| Snowflake/Redshift 云实例不可用 | 高 | 中 | 中 | 输出验证缺口报告 + 替代方案，不阻断交付 |
| 新方言破坏既有 20 种方言 SQL 生成 | 低 | 中 | 低 | feature gate 隔离，既有 20 种方言测试不回退验证 |
| v3.6.0 未提交工作入库冲突（213 文件合并冲突） | 低 | 中 | 低 | 按里程碑分组提交，每次提交后运行门禁验证 |
| 14 道门禁失败 | 中 | 高 | 中 | 输出失败门禁列表与修复建议，阻断交付，修复后重新运行 |
| sz-pay 升级到 3.7.0 回归 | 低 | 高 | 中 | 回退 sz-pay 版本号，分析失败原因，修复后再升级或维持 3.6.0 |

## 7.3 实施依赖与顺序

**实施顺序**（按优先级 + 依赖关系）：

1. **M1 方向 1 真实 DB e2e 测试体系**（3 周，最高优先级，关键路径起点）
   - 先新增 e2e-real-db feature gate
   - 再实现 8 类核心路径测试套件（CRUD/事务/预加载/分页/软删除/多租户/缓存/方言行为）
   - 再实现测试清理机制 + 超时机制
   - 最后验证既有测试不回退

2. **M2 方向 2 对比分析重评估**（1 周，高优先级，依赖 M1 真实 DB e2e 测试就绪后评分更准确）
   - M1 完成后开始（真实 DB e2e 测试体系就绪后评分更准确）
   - 更新文档头部 → 13 维度重新评分 → §6 已知不足 → §7 结论 → §5 独特优势 → 审计验证

3. **M3 方向 3 探索能力成熟化**（2 周，高优先级，依赖 M1 feature gate）
   - M1 完成后可并行开始
   - typed relation + 自定义诊断 + proc-macro SQL 验证可并行成熟化

4. **M4 方向 4 方言扩展延续**（2 周，中优先级，依赖 M1 feature gate）
   - M1 完成后可并行开始
   - Informix + SAP HANA + Firebird 评估可并行

5. **M5 方向 5 云数仓真实验证**（1 周，中优先级，依赖 M1 feature gate）
   - M1 完成后可并行开始
   - Snowflake + Redshift 验证可并行

6. **M6 方向 6 工程规范化**（1 周，低但必须，依赖 M1-M5 完成后入库）
   - M1-M5 完成后开始
   - v3.6.0 未提交工作入库 → Prisma 评估落地 → 14 道门禁 → sz-pay 零回归

**关键路径**：M1 → M2 → M6（串行 5 周）
**并行机会**：M1 完成后 M3/M4/M5 可并行（2 周并行）
**总周期**：关键路径 5 周；并行开发下可压缩至 4-5 周

---

> 本文档为 sz-orm v3.7.0 技术设计文档，基于 v3.6.0 已验收基线（M1-M5 五个里程碑完成）+ v3.6.0 端到端测试缺口（96 e2e 用 InMemoryDb，63 真实 DB 全 ignore）+ 对比分析文档滞后（停留 v3.5.0）+ v3.6.0 探索能力成熟度不足 + v3.5.0 方言扩展路线图 v3.7.0 候选 + v3.6.0 Snowflake/Redshift 无真实云验证 + 213 文件未提交 git 生成。所有改进通过 feature gate 隔离，保证既有 API 完全向后兼容与测试基线不回退。
> 生成日期：2026-08-10
> 基线版本：v3.6.0（M1-M5 五个里程碑完成，workspace.package.version = "3.6.0"）
> 目标版本：v3.7.0
> 需求总数：28 条（REQ-E2E-001~006 + REQ-REEVAL-001~005 + REQ-MAT-001~005 + REQ-DIALECT-001~005 + REQ-CLOUD-001~003 + REQ-ENG-001~004）
> 设计方向：6 个（真实 DB e2e 测试体系 / 对比分析重评估 / 探索能力成熟化 / 方言扩展延续 / 云数仓真实验证 / 工程规范化）
> 里程碑：6 个（M1~M6，关键路径 5 周，并行开发可压缩至 4-5 周）
> Feature Gate：5 个新增（e2e-real-db/custom-diagnostic/dialect-informix/dialect-saphana/dialect-firebird）+ 4 个既有复用 + 成熟化标注（typed-relation/sql-verify-proc/dialect-snowflake/dialect-redshift）
> 真实 DB e2e 测试：8 类核心路径 ≥56 测试用例，复用既有 tests/common/ adapter
> 探索能力成熟化：3 个 feature 标注 stable（typed-relation/sql-verify-proc/custom-diagnostic），每个 ≥10 测试
> 方言扩展：Informix/SAP HANA/Firebird 按需实现，总计 20 + 新增数方言
