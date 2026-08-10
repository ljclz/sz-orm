# sz-orm v3.6.0 技术设计文档

> 版本：v3.6.0（编译期类型安全深入优化 + 313 pub API 文档补齐 + QueryBuilder 渐进合并 + 方言扩展 + async trait 风格统一重评估）
> 基线：v3.5.0（已完成：6 里程碑 / 28 主任务 / 115 子任务，6,751 passed / 0 failed / 253 ignored；44 包已发布 crates.io；补充任务 sz-pay 回归修复 + crates.io 发布 + 剩余不足评估均已完成）
> 日期：2026-08-10
> 文档定位：技术设计（How to build），对应需求规格 `docs/spec/v3.6.0/spec.md`（5 方向 / 37 条 EARS 需求 / 5 组 REQ-TS/REQ-DOC-API/REQ-QB-MIG/REQ-DIALECT/REQ-ASYNC）
> 设计约束：Rust 2021 Edition / rust-version 1.81 / API 向后兼容（无 Breaking Change）/ 禁止占位实现 / unsafe 零容忍 / 参数化查询铁律 / Feature 隔离 / 五方言行为一致 / ADR-0001 严禁修改下游/上游仓库 / 编译时 `$env:RUST_MIN_STACK="67108864"` + `$env:CARGO_INCREMENTAL=0` / 测试 `cargo test --workspace -j 2 --no-fail-fast`
> 优先级声明：五项能力按"编译期类型安全深入优化(1,最高,超越 Diesel 核心竞争力) → 313 pub API 文档补齐(2,补竞品短板解除采用门槛) → QueryBuilder 渐进合并(3,消歧义降迁移成本) → 方言扩展(4,补企业数据库按需) → async trait 风格统一重评估(5,可选,降维护成本)"的收益/风险序推进；方向 1 为最高收益中风险（新表达式+编译期诊断，需 feature gate 隔离+自定义诊断验证），方向 2 为高收益低风险（纯文档工作不引入新代码依赖），方向 3 为中收益中风险（自动迁移工具开发需 lint 注册+fix 机械性），方向 4 为中收益中风险（新方言实现需 Rust 驱动+行为一致验证），方向 5 为低收益低风险（仅评估不强制实施）
> 缺陷来源：用户 5 项深入优化请求 + 对比分析文档 §6 剩余 4 项不足（§6.1 生态成熟度 / §6.2 文档完整度 / §6.3 社区规模 / §6.4 生产案例）+ v3.5.0 方言扩展路线图（§10.4.2 建议 v3.6.0 实现 Snowflake/Redshift）

---

# 一、需求与存量功能关系分析

## 1.1 需求功能与存量功能对比

v3.6.0 的五项深入优化任务与 v3.5.0 已交付代码的关系如下。v3.5.0 已完成文档同步约束规则化、typed_ast DSL 46 种表达式补齐（对齐 Diesel）、无锁连接池架构文档、方言扩展规划（CockroachDB/YugabyteDB）、L1 缓存设计、crates.io v3.5.0 拓扑发布、async trait 风格统一评估、QueryBuilder 合并评估、MOCK-ONLY 包真实后端补齐、文档与迁移指南补齐十项能力，workspace 版本 3.5.0，6,751 测试全部通过，44 包已发布 crates.io。本版本在此基础上向"编译期类型安全成熟度超越 Diesel、文档完整度对齐竞品、API 歧义消除与迁移自动化、方言覆盖度扩展、代码风格统一"五个维度能力突破，所有新增能力以扩展模块 + feature gate 方式提供，不修改 sz-orm-core / sz-orm-macros / 扩展包既有公开 API 签名（满足 spec §4.5 兼容性约束：既有公开 API 完全向后兼容）。

### 1.1.1 已实现功能

| 需求功能 | 存量功能 | 代码位置 | 匹配度 |
|---------|---------|---------|--------|
| typed_ast.rs 46 种表达式（方向 1 基础） | v3.5.0 已对齐 Diesel 46 种表达式：Eq/Ne/Lt/Gt/Le/Ge/And/Or/Like/In/Not + 聚合(Max/Min/Sum/Avg/Count/CountStar) + 算术(Add/Sub/Mul/Div/Modulo) + 字符串(Concat/ILike/Length/Lower/Upper/Trim/Substring) + 日期(Extract/Year/Month/Day/Hour/Minute/Second/Now) + 窗口(Over/PartitionBy/Lag/Lead/RowNumber/Rank/DenseRank) + NULL(IsNull/IsNotNull/Coalesce/NullIf) + BETWEEN/DISTINCT/子查询 + 类型转换(Cast/As) | [packages/sz-orm-core/src/typed_ast.rs:397](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L397)（Eq）至 [packages/sz-orm-core/src/typed_ast.rs:883](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L883)（Not）+ v3.5.0 新增 46 种 | 100%（46 种已有，需补 CTE/Window Frame/JSON 操作符等超越 Diesel） |
| SqlType 类型标记体系（方向 1 基础） | Bool/Integer/SmallInt/BigInt/Real/Double/Text/Date/DateTime/Json/Uuid/Binary/Nullable/Untyped 共 14 种 SqlType ZST | [packages/sz-orm-core/src/typed_ast.rs:69](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L69)（SqlType trait）至 [packages/sz-orm-core/src/typed_ast.rs:129](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L129)（Untyped） | 100%（类型基础已有） |
| TypedExpression trait + ExprTable trait（方向 1 基础） | TypedExpression trait（关联类型 SqlType + to_sql 方法）+ ExprTable trait（表达式所属表关联类型，跨表列引用检查）+ TypedSelectQuery<T>（类型安全 SELECT 构造器） | [packages/sz-orm-core/src/typed_ast.rs:249](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L249)（TypedExpression）+ [packages/sz-orm-core/src/typed_ast.rs:616](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L616)（ExprTable）+ [packages/sz-orm-core/src/typed_ast.rs:672](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L672)（TypedSelectQuery） | 100%（trait 体系完整） |
| 18 种方言实现（方向 4 基础） | 8 独立方言（MySqlDialect/PostgreSqlDialect/SqliteDialect/OracleDialect/SqlServerDialect/ClickHouseDialect/DuckDBDialect/Db2Dialect）+ 8 兼容方言（MariaDB/TiDB→MySQL、KingbaseES/PolarDB/GaussDB→PG、Dameng→Oracle、Sybase/GBase→SQL Server）+ CockroachDB + YugabyteDB（v3.5.0 新增，委派 PG） | [packages/sz-orm-core/src/dialect.rs:228](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L228)（MySqlDialect）至 [packages/sz-orm-core/src/dialect.rs:1991](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1991)（Db2Dialect impl）+ v3.5.0 新增 CockroachDB/YugabyteDB | 100%（18 方言完整，需补 Snowflake/Redshift 达 20 种） |
| DbType 枚举变体（方向 4 基础） | DbType 枚举含 21 变体（v3.5.0 在 v3.4.0 的 19 变体基础上新增 CockroachDB + YugabyteDB），#[non_exhaustive] 允许扩展 | [packages/sz-orm-core/src/db_type.rs:11](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/db_type.rs#L11) | 100%（#[non_exhaustive] 允许新增 Snowflake/Redshift 变体） |
| Dialect trait + delegate_dialect_to 宏（方向 4 基础） | Dialect trait（quote/escape_string/build_pagination/supports_returning/json_extract/full_text_search/build_create_table 等）+ delegate_dialect_to! 宏（委派基础方言） | [packages/sz-orm-core/src/dialect.rs:23](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L23)（Dialect trait）+ [packages/sz-orm-core/src/dialect.rs:1429](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1429)（delegate_dialect_to 宏） | 100%（trait + 宏完整，Redshift 可委派 PG） |
| docs.rs cfg 跳过 + 313 pub API 文档缺口（方向 2 基础） | `#![cfg_attr(docsrs, warn(missing_docs))]`，313 个 pub API 缺 `///` 文档，v3.5.0 已新增 doc-completion feature 但未补齐文档 | [packages/sz-orm-core/src/lib.rs:403](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L403)（docs.rs cfg 跳过）+ [packages/sz-orm-core/Cargo.toml:42](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L42)（doc-completion feature） | 25%（feature 占位已有，313 文档补齐缺失） |
| sz-orm-query-builder deprecated 标注（方向 3 基础） | v3.5.0 已为 sz-orm-query-builder 标注 `#[deprecated]` + 提供选择指南 | [packages/sz-orm-query-builder/src/lib.rs:214](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-query-builder/src/lib.rs#L214)（deprecated 标注）+ [docs/query-builder-guide.md:1](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/query-builder-guide.md#L1)（选择指南） | 100%（deprecated + 指南完整，缺自动迁移 lint/fix 工具） |
| core::QueryBuilder（方向 3 基础） | QueryBuilder<M: Model> 链式 SQL 构造器，绑定 Model，编译期表/列校验，4295 行 | [packages/sz-orm-core/src/query.rs:36](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L36) | 100%（完整，推荐使用） |
| sz-orm-query-builder::Query（方向 3 基础） | Query 独立 SQL 构造器（sea-query 风格，不绑定 Model），4185 行 | [packages/sz-orm-query-builder/src/lib.rs:53](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-query-builder/src/lib.rs#L53) | 100%（完整，v3.6.0 保持可用） |
| Connection trait 手动解糖（方向 5 基础） | Connection trait 手动解糖 async 方法（`fn execute<'a>(&'a mut self, sql: &'a str) -> Pin<Box<dyn Future + Send + 'a>>`），注释明确说明为避免 HRTB 与 sqlx::Executor 冲突 | [packages/sz-orm-core/src/pool.rs:45](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L45) | 100%（手动解糖完整，需基于 RPITIT 重评估） |
| `#[async_trait]` 使用（方向 5 基础） | ConnectionFactory trait（pool.rs:732）+ Model trait（model.rs:271）+ 多处 impl 使用 `#[async_trait]` | [packages/sz-orm-core/src/pool.rs:732](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L732)（ConnectionFactory）+ [packages/sz-orm-core/src/model.rs:271](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/model.rs#L271)（Model） | 100%（混用现状完整） |
| async trait 评估文档（方向 5 基础） | v3.5.0 已完成 async trait 风格评估文档（329 行），评估了方案 A（统一 `#[async_trait]`）与方案 B（统一手动解糖），v3.5.0 选择方案 C（保持现状 + 文档说明） | [docs/async-trait-evaluation.md:1](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/async-trait-evaluation.md#L1) | 100%（评估文档完整，需基于 RPITIT 重评估） |
| EagerLoader 运行时关联（方向 1 基础） | EagerLoader 运行时关联查询实现（BelongsTo/HasMany/HasOne 等），v3.5.0 既有 | [packages/sz-orm-core/src/eager_loader.rs:129](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/eager_loader.rs#L129) | 100%（运行时关联完整，缺编译期类型安全关联 typed relation） |
| query! 宏 db-verify feature（方向 1 基础） | v3.5.0 已有 `query!` 宏的 db-verify feature（编译期连真 DB 验证，执行 EXPLAIN） | [packages/sz-orm-macros/Cargo.toml](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/Cargo.toml)（db-verify feature）+ [packages/sz-orm-core/Cargo.toml:18](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L18)（db-verify 转发） | 100%（db-verify 既有，需探索更深入 proc-macro SQL 验证） |
| sz-orm-core feature 体系（基础） | default=["redis"]，含 25+ feature（testing/db-verify/redis/circuit-breaker/rate-limit/auto-prewarm/plan-cache/zero-copy/simd/multi-tenant-enhanced/dist-cache/test-coverage/arch-improvement/doc-completion/perf-smallstring/perf-enum-dispatch/perf-zero-copy-l2/perf-box-str/type-safe-columns/typed-column/typed-dsl/l1-cache/dialect-cockroachdb/dialect-yugabytedb/migration-guide） | [packages/sz-orm-core/Cargo.toml:13](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L13) | 100%（25+ feature 完整，typed-dsl/doc-completion 既有） |
| workspace 版本集中管理 | `workspace.package.version = "3.5.0"`，edition="2021"，rust-version="1.81" | [Cargo.toml:6](file:///E:/vue/test/鲜视达/rust/sz-orm/Cargo.toml#L6) | 100%（需升级到 3.6.0） |
| sz-pay 生产依赖证据 | sz-pay 从 crates.io 拉取 sz-orm-core/sqlx/config/auth/macros/queue/scheduler 共 7 个包 3.5.0 版本（v3.5.0 补充任务已发布） | `E:\vue\test\sz-pay\server\sz-rust\Cargo.toml` | 100%（需升级到 3.6.0） |
| crates.io token | token = [REDACTED] | `E:\vue\test\鲜视达\服务器信息.md` | 100%（凭证已有） |

### 1.1.2 需要扩展的功能

| 需求功能 | 存量功能 | 差异说明 | 扩展方向 |
|---------|---------|---------|---------|
| CTE 表达式补齐（REQ-TS-001） | typed_ast.rs 46 种表达式无 CTE（With/WithRecursive/CteRef） | 表达式类别差异：Diesel 2.2.x 支持 CTE，SZ-ORM v3.5.0 未实现；递归差异：缺 WITH RECURSIVE 支持 | typed_ast.rs 新增 With/WithRecursive/CteRef 三种 ZST 表达式 + TypedExpression trait 实现 + to_sql 生成 `WITH cte AS (...) SELECT ...` + 递归 CTE 生成 `WITH RECURSIVE`，通过 `typed-dsl` feature gate 隔离（[Cargo.toml:56](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L56) 既有），既有 46 种表达式不变 |
| Window Frame 表达式补齐（REQ-TS-002） | v3.5.0 已有窗口函数（Over/PartitionBy/Lag/Lead/RowNumber/Rank/DenseRank），缺 FRAME 子句（ROWS/RANGE/GROUPS BETWEEN） | FRAME 差异：v3.5.0 窗口函数仅支持 PARTITION BY + ORDER BY，缺 FRAME 子句精确控制窗口范围；边界差异：缺 UNBOUNDED PRECEDING/CURRENT ROW/UNBOUNDED FOLLOWING | typed_ast.rs 新增 RowsFrame/RangeFrame/GroupsFrame/FrameBetween/FrameUnboundedPreceding/FrameCurrentRow 六种 ZST 表达式 + to_sql 生成 `ROWS BETWEEN ... AND ...`/`RANGE BETWEEN ... AND ...`/`GROUPS BETWEEN ... AND ...`，与既有窗口函数协作，通过 `typed-dsl` feature gate 隔离 |
| JSON 操作符表达式补齐（REQ-TS-003） | typed_ast.rs 无 JSON 操作符（JsonGet/JsonGetText/JsonPathGet/JsonPathGetText/JsonContains/JsonExists） | 方言差异：PostgreSQL `->`/`->>`/`#>`/`#>>`、MySQL `JSON_EXTRACT`/`JSON_UNQUOTE`/`->`/`->>`、SQLite `json_extract`/`->`/`->>`，各方言语法差异大；Diesel 差异：Diesel 支持 JSON 操作符（diesel-json crate），SZ-ORM v3.5.0 未实现 | typed_ast.rs 新增六种 JSON 操作符 ZST 表达式 + to_sql 按方言分派生成对应 SQL（PG `col->'key'`/MySQL `JSON_EXTRACT(col, '$.key')`/SQLite `json_extract(col, '$.key')`），通过 `typed-dsl` feature gate 隔离 |
| 自定义编译期诊断信息（REQ-TS-004） | v3.5.0 依赖 Rust 默认类型不匹配错误信息 | 诊断差异：Rust 默认错误信息不清晰，缺错误位置/期望类型/实际类型/修复建议；Diesel 差异：Diesel 编译期错误信息较为友好 | 通过 proc-macro 的 `Diagnostic` API 或 `compile_error!` 生成自定义编译期诊断信息，诊断信息包含错误位置（列名/表达式）+ 期望类型（Expected SqlType）+ 实际类型（Found SqlType）+ 修复建议（如"请使用 Cast 显式转换"），通过 `typed-dsl` feature gate 隔离 |
| 类型安全关联查询 typed relation（REQ-TS-005） | v3.5.0 typed_ast 专注单表表达式，关联查询通过 EagerLoader（[eager_loader.rs:129](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/eager_loader.rs#L129)）运行时实现 | 类型安全差异：EagerLoader 运行时关联，缺编译期外键类型匹配校验 + 表归属校验；Diesel 差异：Diesel 支持 join_to 等编译期关联 | typed_ast.rs 新增 BelongsTo/HasMany/HasOne 关联类型 + 编译期校验外键类型匹配 + 表归属，与既有 EagerLoader 协作（严格类型安全关联 + 运行时 escape hatch），通过 `typed-relation` feature gate 隔离（新增） |
| proc-macro 编译期 SQL 验证探索（REQ-TS-006） | v3.5.0 已有 `query!` 宏的 db-verify feature（编译期连真 DB 执行 EXPLAIN 验证） | 验证范围差异：db-verify 仅验证 `query!` 宏，未扩展到 QueryBuilder 生态；验证深度差异：缺表/列存在性 + 类型匹配校验 | 探索通过 proc-macro 在编译期解析 SQL 字符串，校验 SQL 语法 + 表/列存在性 + 类型匹配，扩展 v3.5.0 既有 db-verify feature 到 QueryBuilder 生态，通过 `sql-verify-proc` feature gate 隔离（新增），仅执行 EXPLAIN 不执行修改 SQL |
| 313 pub API 文档补齐（REQ-DOC-API-001~005） | v3.5.0 已新增 doc-completion feature（[Cargo.toml:42](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L42)）但未补齐文档，313 个 pub API 缺 `///` 文档注释 | 文档差异：缺 313 个 `///` 注释（功能/参数/返回/示例/错误）；配置差异：需移除 [lib.rs:403](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L403) docs.rs cfg 跳过；约束差异：API 签名不变 | 逐批补齐 313 个 pub API 文档（功能/参数/返回/示例/错误），移除 lib.rs:403 docs.rs cfg 跳过，改为全局 `#![warn(missing_docs)]`，`cargo doc --workspace --no-deps --all-features` 零警告 + `cargo test --workspace --doc` doctest 通过，API 签名不变 |
| 代码迁移 lint 开发（REQ-QB-MIG-001） | v3.5.0 已标注 deprecated + 选择指南，缺自动迁移 lint | 自动化差异：v3.5.0 仅手动迁移，缺 lint 自动检测 sz-orm-query-builder::Query 使用 + 迁移建议输出 | 开发 QueryBuilder 代码迁移 lint，检测 sz-orm-query-builder 的 `Query` 类型使用（`use sz_orm_query_builder::Query` / `Query::select()` 等），输出告警信息（含迁移建议 + core::QueryBuilder 等价 API 指引 + 选择指南链接），可注册到 clippy（custom lint）或作为独立工具运行，通过 `qb-migration-tool` feature gate 隔离（新增） |
| 代码迁移 fix 开发（REQ-QB-MIG-002） | v3.5.0 无自动 fix 工具 | 自动化差异：缺自动将 Query 代码转换为 QueryBuilder 等价代码的 fix 工具 | 开发 QueryBuilder 代码迁移 fix 工具，自动将 sz-orm-query-builder 的 `Query` 代码转换为 core::QueryBuilder 的等价代码（`Query::select()` → `QueryBuilder::<Model>::new()` + `.select()` 等），fix 需用户确认后执行（不自动修改），复杂场景（UNION/CTE/窗口函数）标注需人工审查，通过 `qb-migration-tool` feature gate 隔离 |
| Snowflake 方言实现（REQ-DIALECT-001） | v3.5.0 18 种方言无 Snowflake | 方言差异：Snowflake 云数仓特性（VARIANT/OBJECT/ARRAY 半结构化类型、COPY INTO 数据加载、TIME TRAVEL 时间旅行查询），Hibernate/SQLAlchemy 支持 | 实现 SnowflakeDialect（Dialect trait + DbType 新增 Snowflake 变体 + 方言测试 + 五方言行为一致验证），支持 Snowflake 特性，通过 `dialect-snowflake` feature gate 隔离（新增），既有 18 种方言不变 |
| Redshift 方言实现（REQ-DIALECT-002） | v3.5.0 18 种方言无 Redshift | 方言差异：Redshift 基于 PostgreSQL 8.0.2 扩展，大部分 SQL 语法与 PG 兼容，但有 COPY/UNLOAD 等特有特性 | 实现 RedshiftDialect（委派 PostgreSqlDialect + Redshift 特性扩展 COPY/UNLOAD），DbType 新增 Redshift 变体 + 方言测试，通过 `dialect-redshift` feature gate 隔离（新增），既有 18 种方言不变 |
| async trait 风格统一重评估（REQ-ASYNC-001~004） | v3.5.0 已完成评估文档（329 行），选择方案 C（保持现状 + 文档说明），未基于 RPITIT 重评估 | 评估差异：v3.5.0 评估时 RPITIT 已稳定但 Send bound 限制未完全解决，v3.6.0 需基于 Rust 1.80+ 最新进展重新评估；方案差异：v3.5.0 评估方案 A/B，v3.6.0 新增方案 C（原生 async fn in trait，基于 RPITIT） | 调研 Rust async trait 最新进展（RPITIT Send bound / async fn in trait dyn trait 限制 / Rust 1.80+ 改进 / async-trait crate 最新版），复审 v3.5.0 评估结论，重新评估三方案（A 统一 `#[async_trait]` / B 统一手动解糖 / C 原生 async fn in trait），输出更新评估文档 + 推荐方案 + 渐进迁移方案（如推荐迁移），既有 Connection trait 签名在评估期内保持不变 |

### 1.1.3 需要新增的功能或接口

按业务模块分组，以下功能在存量代码中完全没有对应实现，需新增。

**模块 A：CTE 表达式（对应 REQ-TS-001）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| With 表达式 | CTE 名称 + 子查询 | ZST 表达式 + TypedExpression impl + to_sql | ZST + TypedExpression trait 实现 + to_sql 生成 `WITH cte_name AS (subquery) SELECT ...`，参数化占位符 | 无（复用既有 typed_ast 基础） |
| WithRecursive 表达式 | CTE 名称 + 初始查询 + 递归查询 | ZST 表达式 + to_sql | to_sql 生成 `WITH RECURSIVE cte_name AS (initial UNION ALL recursive) SELECT ...` | 无 |
| CteRef 表达式 | CTE 名称 | ZST 表达式 + to_sql | to_sql 生成 CTE 引用（在主查询中引用 CTE 名称） | 无 |

**模块 B：Window Frame 表达式（对应 REQ-TS-002）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| RowsFrame/RangeFrame/GroupsFrame | Frame 范围 | ZST 表达式 + to_sql | to_sql 生成 `ROWS BETWEEN ... AND ...`/`RANGE BETWEEN ... AND ...`/`GROUPS BETWEEN ... AND ...` | 无 |
| FrameBetween | 起始 + 结束 | ZST 表达式 + to_sql | to_sql 生成 `BETWEEN <start> AND <end>` | 无 |
| FrameUnboundedPreceding/FrameCurrentRow | 无 | ZST 表达式 + to_sql | to_sql 生成 `UNBOUNDED PRECEDING`/`CURRENT ROW` | 无 |

**模块 C：JSON 操作符表达式（对应 REQ-TS-003）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| JsonGet/JsonGetText | JSON 列 + 键 | ZST 表达式 + to_sql | to_sql 按方言分派：PG `col->'key'`/`col->>'key'`、MySQL `JSON_EXTRACT(col, '$.key')`、SQLite `json_extract(col, '$.key')` | 既有 Dialect trait |
| JsonPathGet/JsonPathGetText | JSON 列 + 路径 | ZST 表达式 + to_sql | to_sql 按方言分派：PG `col#>'{path}'`/`col#>>'{path}'`、MySQL `JSON_EXTRACT(col, '$.path')` | 既有 Dialect trait |
| JsonContains/JsonExists | JSON 列 + 值/路径 | ZST 表达式 + to_sql | to_sql 按方言分派：PG `col @> 'value'`/`col ? 'key'`、MySQL `JSON_CONTAINS(col, 'value')`/`JSON_EXISTS(col, '$.key')` | 既有 Dialect trait |

**模块 D：自定义编译期诊断（对应 REQ-TS-004）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| 自定义诊断信息生成 | 类型不匹配错误（期望 SqlType + 实际 SqlType + 错误位置） | 编译期诊断信息 | 通过 proc-macro 的 `Diagnostic` API 或 `compile_error!` 生成自定义诊断信息，含错误位置 + 期望类型 + 实际类型 + 修复建议 | proc-macro2 / syn / quote（sz-orm-macros 既有） |

**模块 E：类型安全关联查询 typed relation（对应 REQ-TS-005）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| BelongsTo/HasMany/HasOne 关联类型 | Model A + Model B + 外键 | ZST 关联类型 + 编译期校验 | 编译期校验外键类型匹配（A 的外键类型 == B 的主键类型）+ 表归属校验（外键属于 A 表），生成类型安全关联查询 | 既有 typed_ast + Model trait |
| typed relation 与 EagerLoader 协作 | typed relation 关联 | EagerLoader 运行时关联 | typed relation 提供编译期类型安全，EagerLoader 提供运行时执行，复杂关联回退 EagerLoader（escape hatch） | 既有 EagerLoader（[eager_loader.rs:129](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/eager_loader.rs#L129)） |

**模块 F：proc-macro 编译期 SQL 验证探索（对应 REQ-TS-006）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| proc-macro SQL 解析 | SQL 字符串 | 解析 AST + 校验结果 | 通过 proc-macro 在编译期解析 SQL 字符串，校验 SQL 语法 + 表/列存在性（连真 DB）+ 类型匹配，仅执行 EXPLAIN 不执行修改 SQL | 既有 db-verify feature + sqlparser |
| SQL 验证结果缓存 | SQL 哈希 | 缓存验证结果 | 按 SQL 哈希缓存验证结果，仅 SQL 变更时重新验证，避免每次编译都连 DB | 无 |

**模块 G：QueryBuilder 迁移 lint/fix 工具（对应 REQ-QB-MIG-001/002）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| 迁移 lint | Rust 源代码 | 告警 + 迁移建议 | 检测 sz-orm-query-builder::Query 使用（精确匹配 `sz_orm_query_builder::Query` 路径），输出告警 + 迁移建议 + core::QueryBuilder 等价 API 指引 + 选择指南链接 | syn / quote（解析 Rust AST） |
| 迁移 fix | Rust 源代码 + 用户确认 | 转换后的 Rust 代码 | 自动将 Query 代码转换为 QueryBuilder 等价代码（`Query::select()` → `QueryBuilder::<Model>::new().select()`），需用户 `--fix` 标志或交互式确认，复杂场景标注需人工审查 | syn / quote |
| 差分测试 | Query + QueryBuilder 构造的相同查询 | SQL 等价验证 | 对同一查询用 Query 和 QueryBuilder 构造，比较生成 SQL，验证语义等价 | 既有 Query + QueryBuilder |

**模块 H：Snowflake 方言（对应 REQ-DIALECT-001）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| SnowflakeDialect 实现 | SQL 构造请求 | Snowflake SQL | 实现 Dialect trait（quote/escape_string/build_pagination/json_extract 等），支持 VARIANT/OBJECT/ARRAY 半结构化类型、COPY INTO 数据加载、TIME TRAVEL 时间旅行查询（`AT(OBJECT => ...)`/`BEFORE(...)`） | 既有 Dialect trait |
| DbType::Snowflake 变体 | 无 | DbType 枚举变体 | db_type.rs 新增 Snowflake 变体（#[non_exhaustive] 允许） | 既有 DbType |
| Snowflake 方言测试 | 方言测试用例 | 测试通过 | 单元测试（Dialect trait 方法）+ 集成测试（标注 `#[ignore]`，需真实 Snowflake 云数据库） | 无 |

**模块 I：Redshift 方言（对应 REQ-DIALECT-002）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| RedshiftDialect 实现 | SQL 构造请求 | Redshift SQL | 委派 PostgreSqlDialect + Redshift 特性扩展（COPY 数据加载、UNLOAD 数据卸载、Redshift 特有函数），覆盖不兼容的 SQL 构造（返回 Err 或 Redshift 特有语法） | 既有 PostgreSqlDialect + delegate_dialect_to 宏 |
| DbType::Redshift 变体 | 无 | DbType 枚举变体 | db_type.rs 新增 Redshift 变体 | 既有 DbType |
| Redshift 方言测试 | 方言测试用例 | 测试通过 | 单元测试 + 集成测试（标注 `#[ignore]`，需真实 Redshift 云数据库） | 无 |

**模块 J：Prisma 方言兼容评估（对应 REQ-DIALECT-003）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| Prisma Schema DSL 映射评估 | Prisma Schema DSL | 映射评估文档 | 评估 Prisma Schema DSL 与 sz-orm Model trait 的映射关系（model/entity/field/relation 映射） | 无（纯评估） |
| Prisma 查询语法映射评估 | Prisma 查询语法 | 映射评估文档 | 评估 Prisma 查询语法与 sz-orm QueryBuilder 的映射（findMany/findUnique/create/update 映射） | 无（纯评估） |
| 跨生态兼容可行性评估 | 映射评估 | 可行性结论 + 推荐方案 | 评估跨生态兼容的技术可行性（TypeScript/Node.js vs Rust）+ 实现难度 + 收益，输出推荐方案 | 无（纯评估） |

**模块 K：async trait 重评估（对应 REQ-ASYNC-001~004）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| Rust async trait 最新进展调研 | 无 | 调研报告 | 调研 RPITIT（Rust 1.75+ 稳定）Send bound 限制 + async fn in trait dyn trait 限制 + Rust 1.80+ 改进 + async-trait crate 最新版 + tokio 影响 | 无（纯调研） |
| v3.5.0 评估结论复审 | v3.5.0 评估文档 | 复审结论 | 逐条复审 v3.5.0 评估结论，基于 Rust 最新进展判断是否仍然成立，标注结论是否变更 | 既有 [async-trait-evaluation.md](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/async-trait-evaluation.md#L1) |
| 三方案重新评估 | 方案 A/B/C | 更新评估文档 + 推荐方案 | 重新评估方案 A（统一 `#[async_trait]`）/ B（统一手动解糖）/ C（原生 async fn in trait，基于 RPITIT）优缺点 + Send bound 兼容性 + dyn trait 兼容性 + 性能基准 + 迁移影响 + 学习成本 | 既有 async-trait crate |
| 渐进迁移方案（如推荐迁移） | 推荐方案 + trait 清单 | 分阶段迁移计划 | 按推荐方案分阶段迁移 trait，每阶段迁移少量 trait + 全量测试 + sz-pay 零回归，不一次性迁移 | 无 |

## 1.2 存量功能详细分析

### 1.2.1 typed_ast.rs 46 种表达式 + SqlType 类型体系（typed_ast.rs:69-919+）

- **接口契约**：
  - `SqlType` trait（[typed_ast.rs:69](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L69)）：所有 SQL 类型标记的基 trait，要求 `'static`，实现者应为 ZST（unit struct）。
  - 14 种 SqlType ZST：Bool/Integer/SmallInt/BigInt/Real/Double/Text/Date/DateTime/Json/Uuid/Binary/Nullable<T>/Untyped（[typed_ast.rs:72-129](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L72)）。
  - `TypedExpression` trait（[typed_ast.rs:249](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L249)）：所有表达式基 trait，关联类型 `SqlType`，方法 `to_sql(&self, dialect: &dyn Dialect) -> String`。
  - `ExprTable` trait（[typed_ast.rs:616](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L616)）：表达式所属表的关联类型，用于跨表列引用检查。
  - 46 种表达式 ZST：v3.5.0 已对齐 Diesel 46 种表达式（11 种基础比较/逻辑 + 35 种聚合/算术/字符串/日期/窗口/NULL/BETWEEN/DISTINCT/子查询/类型转换）。
  - `TypedSelectQuery<T>`（[typed_ast.rs:672](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L672)）：类型安全的 SELECT 查询构造器，`filter<E: TypedExpression<SqlType = Bool> + ExprTable<Table = T>>` 编译期拒绝跨表列引用。
- **业务规则**：每个表达式为 ZST（零大小类型），仅在编译期携带类型信息，运行时通过 `to_sql` 生成 SQL 片段。类型安全保证：`Eq<C, T>` 要求 `C: TypedColumn<RustType = T>`，列类型必须与值类型匹配；`And<L, R>` 要求 `L: TypedExpression<SqlType = Bool>`；`TypedSelectQuery::filter<E>` 要求 `E: TypedExpression<SqlType = Bool> + ExprTable<Table = T>`，跨表列引用编译期拒绝。
- **扩展点**：v3.6.0 在既有 46 种表达式基础上新增 CTE（With/WithRecursive/CteRef）+ Window Frame（RowsFrame/RangeFrame/GroupsFrame/FrameBetween/FrameUnboundedPreceding/FrameCurrentRow）+ JSON 操作符（JsonGet/JsonGetText/JsonPathGet/JsonPathGetText/JsonContains/JsonExists）共约 15 种新表达式，每种为 ZST + TypedExpression trait 实现 + to_sql 参数化生成，通过 `typed-dsl` feature gate 隔离（[Cargo.toml:56](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L56) 既有）。既有 46 种表达式 + TypedExpression trait + ExprTable trait + TypedSelectQuery 保持完全向后兼容。
- **约束**：
  - 零成本抽象：所有新增表达式为 ZST，`static_assert!(size_of::<T>() == 0)` 编译期断言。
  - 参数化查询：新增表达式 to_sql 生成的 SQL 必须使用参数化占位符（`?`），禁止字符串拼接，沿用 v3.5.0 SQL 注入防护铁律。
  - 方言分派：JSON 操作符按方言分派（PG `->`/`->>`/`#>`/`#>>`、MySQL `JSON_EXTRACT`/`JSON_UNQUOTE`、SQLite `json_extract`），不支持的方言返回 Err(UnsupportedFeature)。
  - 类型检查严格性：算术表达式类型检查要求操作数 SqlType 兼容，编译期拒绝不兼容类型，提供 Cast 表达式让用户显式转换。

### 1.2.2 18 种方言 + DbType 21 变体（dialect.rs:228-1991 + db_type.rs:11）

- **接口契约**：
  - `Dialect` trait（[dialect.rs:23](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L23)）：所有方言的基 trait，方法 `clone_box`/`db_type`/`quote`/`escape_string`/`build_pagination`/`supports_returning`/`auto_increment_keyword`/`json_extract`/`full_text_search`/`build_create_table`/`build_drop_table`/`build_alter_table` 等。
  - 8 独立方言：MySqlDialect（[dialect.rs:228](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L228)）/PostgreSqlDialect（[dialect.rs:451](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L451)）/SqliteDialect/OracleDialect/SqlServerDialect/ClickHouseDialect/DuckDBDialect/Db2Dialect，各自独立实现 Dialect trait。
  - 8 兼容方言：通过 `delegate_dialect_to!` 宏（[dialect.rs:1429](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1429)）委派基础方言：MariaDbDialect/TiDbDialect→MySqlDialect、KingbaseDialect/PolarDbDialect/GaussDbDialect→PostgreSqlDialect、DamengDialect→OracleDialect、SybaseDialect/GBaseDialect→SqlServerDialect。
  - v3.5.0 新增 CockroachDB + YugabyteDB（委派 PostgreSqlDialect，通过 `dialect-cockroachdb`/`dialect-yugabytedb` feature gate 隔离）。
  - `DbType` 枚举（[db_type.rs:11](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/db_type.rs#L11)）：21 变体（v3.5.0 在 v3.4.0 的 19 变体基础上新增 CockroachDB + YugabyteDB），`#[non_exhaustive]` 允许未来扩展。
  - `get_dialect(db_type: DbType) -> Result<Box<dyn Dialect>, DbError>`（[dialect.rs:2256](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L2256)）：方言工厂函数。
- **业务规则**：独立方言各自实现完整 Dialect trait（quote/escape/pagination/DDL 等），兼容方言通过 `delegate_dialect_to!` 宏委派基础方言（仅 db_type() 不同）。OceanBase 委派 MySqlDialect（MySQL 兼容）。
- **扩展点**：v3.6.0 新增 Snowflake（独立实现，支持 VARIANT/OBJECT/ARRAY + COPY INTO + TIME TRAVEL）+ Redshift（委派 PostgreSqlDialect + COPY/UNLOAD 特性扩展），新增 DbType::Snowflake + DbType::Redshift 变体（#[non_exhaustive] 允许），通过 `dialect-snowflake`/`dialect-redshift` feature gate 隔离。既有 18 种方言不变。
- **约束**：
  - 无 Rust 驱动的方言不实现：如 SAP HANA 无官方 Rust 驱动，标注"暂不需要（无 Rust 驱动）"，禁止实现无驱动支撑的方言（spec §5.4.1.7）。
  - 五方言行为一致：所有新能力必须保持 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言行为一致（spec §1.2.12）。
  - 兼容方言优先：新方言若与既有方言 SQL 兼容，优先用 `delegate_dialect_to!` 宏委派，减少实现成本（Redshift 委派 PG）。

### 1.2.3 docs.rs cfg 跳过 + 313 pub API 文档缺口（lib.rs:403）

- **接口契约**：`#![cfg_attr(docsrs, warn(missing_docs))]`（[lib.rs:403](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L403)），仅在 docs.rs 构建时启用 missing_docs lint，本地和 CI clippy 不触发。注释说明"避免 313 个 pub API 缺文档阻塞开发"。v3.5.0 已新增 `doc-completion` feature（[Cargo.toml:42](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L42)）但未补齐文档。
- **业务规则**：docs.rs 构建文档时启用 missing_docs lint，但因 313 个 pub API 缺文档，实际 docs.rs 文档不完整。本地和 CI 不触发 missing_docs，避免阻塞开发。
- **扩展点**：v3.6.0 逐批补齐 313 个 pub API 文档（功能/参数/返回/示例/错误），移除 [lib.rs:403](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L403) docs.rs cfg 跳过，改为全局 `#![warn(missing_docs)]`，使 docs.rs 文档完整。API 签名不变（仅新增 `///` 注释）。
- **约束**：
  - 文档与实际不符：补齐的文档与代码实际行为必须一致，通过 `cargo doc --workspace --no-deps --all-features` 无警告 + `cargo test --workspace --doc` doctest 通过 + 代码审查杜绝（spec §5.2.1.7）。
  - pub API 数量变化：v3.6.0 新增/删除 pub API 导致 313 数量变化时，重新统计缺文档 pub API 数量，补齐所有缺文档 API，文档标注实际数量（spec §5.2.3.1）。

### 1.2.4 sz-orm-query-builder deprecated + core::QueryBuilder 重叠（query-builder/lib.rs:214 + query.rs:36）

- **接口契约**：
  - `sz-orm-core::QueryBuilder<M: Model>`（[query.rs:36](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L36)）：ORM 集成 QueryBuilder，绑定 Model `<M: Model>`，编译期表/列校验，4295 行，含 select/where/where_eq/order_by/limit/offset/join/group_by/having/lock_for_update/keyset_pagination/cache_ttl 等链式 API。
  - `sz-orm-query-builder::Query`（[query-builder/lib.rs:53](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-query-builder/src/lib.rs#L53)）：独立 SQL 构造器（sea-query 风格，不绑定 Model），4185 行，含 select/insert/update/delete 链式 API，可独立编译、独立发布到 crates.io。
  - v3.5.0 已为 sz-orm-query-builder 标注 `#[deprecated]`（[lib.rs:214](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-query-builder/src/lib.rs#L214)）+ 提供选择指南（[docs/query-builder-guide.md:1](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/query-builder-guide.md#L1)）。
- **业务规则**：两个 QueryBuilder 长期共存，core::QueryBuilder 适用于 ORM 完整流程（绑定 Model 编译期校验），sz-orm-query-builder 适用于纯 SQL 构造/动态查询（不依赖 Model）。v3.5.0 标注 deprecated 但保持可用，给用户迁移周期。
- **扩展点**：v3.6.0 提供自动迁移工具（代码迁移 lint 检测 sz-orm-query-builder::Query 使用 + 自动 fix 建议生成 core::QueryBuilder 等价代码），制定 v3.7.0 正式移除 sz-orm-query-builder 的路线图，v3.6.0 保持 sz-orm-query-builder 可用（标注 deprecated）。通过 `qb-migration-tool` feature gate 隔离（新增）。
- **约束**：
  - 不引入 Breaking Change：v3.6.0 保持 sz-orm-query-builder API 完全兼容，sz-pay cargo check 通过，SemVer 兼容（spec §5.3.1.7）。
  - 渐进 deprecation：v3.6.0 标注 deprecated 但保持可用，v3.7.0 才正式移除，给用户迁移周期（spec §4.5.3）。
  - 迁移工具不自动修改用户代码：fix 需用户显式确认（`--fix` 标志或交互式确认），不自动修改（spec §5.3.1.6）。

### 1.2.5 Connection trait 手动解糖 + `#[async_trait]` 混用 + v3.5.0 评估文档（pool.rs:45 + async-trait-evaluation.md）

- **接口契约**：
  - `Connection` trait（[pool.rs:45](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L45)）手动解糖 async 方法：`fn execute<'a>(&'a mut self, sql: &'a str) -> Pin<Box<dyn Future<Output = Result<u64, crate::DbError>> + Send + 'a>>`，所有 async 方法使用单一生命周期 `'a`（绑定 `&'a mut self` 和 `&'a str`），而非 HRTB。注释明确说明"以避免 `&str` 参数触发 HRTB 与 sqlx::Executor 冲突"（[pool.rs:41-44](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L41)）。
  - `ConnectionFactory` trait（[pool.rs:732](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L732)）使用 `#[async_trait]` 宏。
  - `Model` trait（[model.rs:271](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/model.rs#L271)）使用 `#[async_trait]` 宏。
  - v3.5.0 评估文档（[async-trait-evaluation.md:1](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/async-trait-evaluation.md#L1)，329 行）已完成方案 A/B/C 评估，v3.5.0 选择方案 C（保持现状 + 文档说明 HRTB 冲突原因）。
- **业务规则**：Connection trait 手动解糖有技术原因（避免 HRTB 与 sqlx::Executor 冲突），其他 trait 使用 `#[async_trait]` 宏。混用增加学习成本与维护负担。v3.5.0 评估结论：方案 B（全 `#[async_trait]`）技术不可行（HRTB 冲突），方案 A（全手动解糖）可行但工作量大 + Breaking Change，方案 C（保持现状 + 文档）推荐（零风险零成本）。
- **扩展点**：v3.6.0 基于 Rust RPITIT（Return Position Impl Trait In Trait，Rust 1.75+ 稳定）、async fn in trait、Send bound 等最新进展，重新评估 async trait 风格统一，新增方案 C（原生 async fn in trait，基于 RPITIT），输出更新评估文档 + 推荐方案 + 渐进迁移方案（如推荐迁移）。既有 Connection trait 签名在评估期内保持不变（spec §5.5.1.5）。
- **约束**：
  - 不引入 Breaking Change：迁移后公开 API 签名语义不变，feature gate 隔离 + SemVer 兼容，sz-pay cargo check 通过（spec §5.5.1.6）。
  - HRTB 冲突约束：Connection trait 手动解糖有技术原因，统一为 `#[async_trait]` 可能重新引入 HRTB 冲突。评估必须考虑此约束，推荐方案可能是"保持手动解糖 + 文档说明原因"而非强制统一。
  - 评估期内签名不变：v3.6.0 重评估仅输出评估文档与推荐方案，不强制实施迁移（除非用户明确要求且 sz-pay 零回归验证通过）。

---

# 二、增量设计方案

## 2.0 架构总览

### 2.0.1 v3.6.0 整体架构图

v3.6.0 在 v3.5.0 现有 workspace 基础上，不新增独立包，而是在 sz-orm-core 内新增 CTE/Window Frame/JSON 操作符等表达式 + 自定义编译期诊断 + typed relation + proc-macro SQL 验证探索 + Snowflake/Redshift 方言 + 313 pub API 文档补齐，在 sz-orm-query-builder 内保持 deprecated 可用，在 scripts/ 内新增 QueryBuilder 迁移 lint/fix 工具，在 docs/ 内新增 async trait 重评估更新文档 + Prisma 兼容评估文档，通过 6 个新增 feature gate（typed-relation/sql-verify-proc/dialect-snowflake/dialect-redshift/dialect-prisma/qb-migration-tool）+ 既有 feature 体系（typed-dsl/doc-completion）隔离，复用既有 typed_ast/Dialect trait/delegate_dialect_to 宏/EagerLoader/db-verify feature 基础设施。整体架构如下：

```plantuml
@startuml
!theme plain
title sz-orm v3.6.0 整体架构（增量视图）

rectangle "sz-orm v3.6.0 workspace" as WS {

  rectangle "核心层 sz-orm-core (扩展)" as Core {
    rectangle "既有: typed_ast.rs (46 种表达式, v3.5.0)" as TaOld
    rectangle "既有: dialect.rs (18 种方言, v3.5.0)" as DialectOld
    rectangle "既有: query.rs (QueryBuilder<M>)" as QueryOld
    rectangle "既有: lib.rs (docs.rs cfg 跳过)" as LibOld
    rectangle "既有: eager_loader.rs (运行时关联)" as EagerOld
    rectangle "既有: pool.rs (Connection trait 手动解糖)" as PoolOld

    rectangle "CTE/Window Frame/JSON 操作符表达式 <<new>>\n[typed-dsl]" as TaNew
    rectangle "自定义编译期诊断 <<new>>\n[typed-dsl]" as DiagNew
    rectangle "typed relation (类型安全关联查询) <<new>>\n[typed-relation]" as RelNew
    rectangle "proc-macro SQL 验证探索 <<new>>\n[sql-verify-proc]" as VerifyNew
    rectangle "SnowflakeDialect <<new>>\n[dialect-snowflake]" as SnowNew
    rectangle "RedshiftDialect <<new>>\n[dialect-redshift]" as RedNew
    rectangle "313 pub API 文档补齐 + 移除 docs.rs cfg 跳过 <<new>>\n[doc-completion]" as DocApi
  }

  rectangle "sz-orm-query-builder (扩展)" as ExtQB {
    rectangle "既有: Query (deprecated, 不变)" as ExtQbOld
    rectangle "迁移 lint/fix 工具目标 <<new>>\n[qb-migration-tool]" as QbMig
  }

  rectangle "sz-orm-macros (扩展)" as Macros {
    rectangle "既有: query! 宏 + db-verify" as MacroOld
    rectangle "自定义诊断 proc-macro <<new>>\n[typed-dsl]" as DiagMacro
    rectangle "SQL 验证 proc-macro 探索 <<new>>\n[sql-verify-proc]" as VerifyMacro
  }

  rectangle "scripts/ (扩展)" as Scripts {
    rectangle "既有: publish_all.py / check-doc-sync.py" as ScriptOld
    rectangle "qb_migration_lint.rs <<new>>\n[qb-migration-tool]" as LintScript
    rectangle "qb_migration_fix.rs <<new>>\n[qb-migration-tool]" as FixScript
  }

  rectangle "docs/ (扩展)" as Docs {
    rectangle "既有: async-trait-evaluation.md (v3.5.0)" as AsyncOld
    rectangle "既有: query-builder-guide.md (v3.5.0)" as QbGuideOld
    rectangle "async-trait-evaluation.md 更新 (RPITIT 重评估) <<new>>" as AsyncNew
    rectangle "prisma-dialect-evaluation.md <<new>>\n[dialect-prisma]" as PrismaDoc
    rectangle "qb-migration-roadmap.md (v3.7.0 移除路线图) <<new>>" as QbRoadmap
  }
}

cloud "crates.io" as CratesIo
database "MySQL/PG/SQLite/Oracle/MSSQL" as DB
database "Snowflake" as Snowflake
database "AWS Redshift" as Redshift
rectangle "sz-pay 生产项目" as SzPay
rectangle "Diesel (对标基准)" as Diesel

TaNew ..> TaOld : 扩展 CTE/Window Frame/JSON (不破坏 46 种既有)
RelNew ..> EagerOld : typed relation + EagerLoader 协作 (escape hatch)
VerifyNew ..> MacroOld : 扩展 db-verify 到 QueryBuilder 生态
SnowNew --> Snowflake : Snowflake 方言 SQL 生成
RedNew --> Redshift : Redshift 方言 SQL 生成 (委派 PG)
QbMig ..> QueryOld : 迁移目标 core::QueryBuilder
QbMig ..> ExtQbOld : 检测 sz-orm-query-builder::Query 使用
TaNew ..> Diesel : 表达式覆盖度超越 Diesel
CratesIo --> SzPay : sz-pay 升级到 3.6.0 (零回归验证)

@enduml
```

### 2.0.2 5 大方向在 workspace 中的定位

| 方向 | 需求组 | 包名 | 形态 | feature gate | 在 workspace 中的位置 | 依赖关系 |
|------|--------|------|------|-------------|---------------------|---------|
| 1 编译期类型安全深入优化 | REQ-TS-001~009 | sz-orm-core + sz-orm-macros | **扩展 typed_ast.rs 新表达式 + 自定义诊断 + typed relation + proc-macro SQL 验证** | `typed-dsl`（既有，[Cargo.toml:56](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L56)）+ `typed-relation`（新增）+ `sql-verify-proc`（新增） | `packages/sz-orm-core/src/typed_ast.rs` 内 `#[cfg(feature = "typed-dsl")]` 条件编译 + `packages/sz-orm-core/src/typed_relation.rs` 新增 + `packages/sz-orm-macros/src/` 自定义诊断 proc-macro | 无新增（复用既有 typed_ast + proc-macro2/syn/quote + EagerLoader + db-verify） |
| 2 313 pub API 文档补齐 | REQ-DOC-API-001~007 | sz-orm-core | **纯文档补齐 + 移除 docs.rs cfg 跳过** | `doc-completion`（既有，[Cargo.toml:42](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L42)） | `packages/sz-orm-core/src/*.rs` 文档注释 + `packages/sz-orm-core/src/lib.rs:403` 移除 cfg 跳过 | 无新增（纯文档） |
| 3 QueryBuilder 渐进合并 | REQ-QB-MIG-001~007 | sz-orm-query-builder + scripts/ + docs/ | **迁移 lint/fix 工具 + v3.7.0 移除路线图** | `qb-migration-tool`（新增） | `scripts/qb_migration_lint.rs` + `scripts/qb_migration_fix.rs` + `docs/qb-migration-roadmap.md` | syn / quote（解析 Rust AST，sz-orm-macros 既有） |
| 4 方言扩展 | REQ-DIALECT-001~008 | sz-orm-core + docs/ | **Snowflake/Redshift 方言实现 + Prisma 兼容评估** | `dialect-snowflake`（新增）+ `dialect-redshift`（新增）+ `dialect-prisma`（新增，仅评估） | `packages/sz-orm-core/src/dialect.rs` + `packages/sz-orm-core/src/db_type.rs` + `docs/prisma-dialect-evaluation.md` | 既有 Dialect trait + delegate_dialect_to 宏（Redshift 委派 PG） |
| 5 async trait 重评估 | REQ-ASYNC-001~006 | sz-orm-core + docs/ | **评估文档更新 + 渐进迁移（如推荐）** | 无（纯评估文档 + 可选 `async-trait-unify` feature gate） | `docs/async-trait-evaluation.md` 更新 + `packages/sz-orm-core/src/pool.rs` 渐进迁移（如推荐） | 无新增（复用既有 async-trait crate） |

### 2.0.3 与 v3.5.0 现有架构的演进关系

```plantuml
@startuml
!theme plain
title v3.5.0 → v3.6.0 架构演进

rectangle "v3.5.0 (文档同步 + typed_ast 46 表达式 + L1 缓存 + 真实后端 + 方言扩展 + crates.io 发布 + async trait 评估 + QB deprecated + 文档补齐)" as V35 {
  rectangle "sz-orm-core\n(query/pool/dialect/l2_cache/l1_cache/typed_ast 46 种/eager_loader\n+typed-dsl/l1-cache/dialect-cockroachdb/dialect-yugabytedb/doc-completion)" as Core35
  rectangle "sz-orm-query-builder (deprecated + 选择指南)" as ExtQb35
  rectangle "sz-orm-macros (query! + db-verify)" as Macros35
  rectangle "scripts/ (check-doc-sync + publish-workspace)" as Scripts35
  rectangle "docs/ (async-trait-eval + query-builder-guide + 迁移指南)" as Docs35
}

rectangle "v3.6.0 增量 (6 feature gate 隔离 + 纯文档/脚本)" as V36 {
  rectangle "CTE/Window Frame/JSON 操作符 + 自定义诊断 + typed relation + proc-macro SQL 验证\n[typed-dsl/typed-relation/sql-verify-proc]" as Ta36
  rectangle "313 pub API 文档补齐 + 移除 docs.rs cfg 跳过\n[doc-completion]" as Doc36
  rectangle "QueryBuilder 迁移 lint/fix 工具 + v3.7.0 路线图\n[qb-migration-tool]" as QbMig36
  rectangle "Snowflake/Redshift 方言 + Prisma 评估\n[dialect-snowflake/dialect-redshift/dialect-prisma]" as Dialect36
  rectangle "async trait 重评估 (RPITIT) + 渐进迁移\n(纯评估文档 + 可选 async-trait-unify)" as Async36
}

Core35 ..> Ta36 : 扩展 CTE/Window Frame/JSON (不破坏 46 种既有)
Core35 ..> Dialect36 : 新增 Snowflake/Redshift (不破坏 18 种既有)
Core35 ..> Doc36 : 补齐 313 pub API 文档
ExtQb35 ..> QbMig36 : 迁移工具检测 Query 使用
Macros35 ..> Ta36 : 自定义诊断 proc-macro + SQL 验证探索
Docs35 ..> Async36 : 更新评估文档 (RPITIT 重评估)

note bottom of V36
  **演进原则**：
  1. 无 Breaking Change（现有公开 API 签名不变）
  2. 新增能力通过 feature gate 隔离（默认关闭）
  3. 不新增独立包，全部在既有包内扩展
  4. 复用现有基础设施（typed_ast/Dialect trait/delegate_dialect_to/EagerLoader/db-verify）
  5. 五方言行为一致（增强在 core 层统一，不触碰方言驱动）
  6. ADR-0001 严禁修改下游/上游仓库（sz-pay 仅验证不改）
  7. 测试覆盖不回退（v3.5.0 基线 6,751 测试仅增不减）
end note

@enduml
```

**演进关键决策**：

| 决策点 | 选项 | 选择 | 理由 |
|--------|------|------|------|
| CTE/Window Frame/JSON 表达式隔离方式 | A. 既有 typed-dsl feature / B. 新增 typed-dsl-v2 feature | A | typed-dsl feature 既有（[Cargo.toml:56](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L56)），v3.5.0 已用于 46 种表达式，v3.6.0 复用此 feature 扩展 CTE/Window Frame/JSON，避免 feature 爆炸 |
| typed relation 隔离方式 | A. 既有 typed-dsl / B. 新增 typed-relation | B | typed relation 是独立能力（关联查询），与单表表达式不同，独立 feature 便于按需启用 |
| proc-macro SQL 验证隔离方式 | A. 既有 db-verify / B. 新增 sql-verify-proc | B | db-verify 仅验证 `query!` 宏，sql-verify-proc 扩展到 QueryBuilder 生态，验证范围不同，独立 feature |
| 自定义编译期诊断实现方式 | A. proc-macro Diagnostic API / B. compile_error! 宏 | A | proc-macro Diagnostic API 提供更丰富的诊断信息（错误位置/期望类型/实际类型/修复建议），compile_error! 仅支持简单字符串 |
| Snowflake 方言实现方式 | A. 独立实现 / B. 委派基础方言 | A | Snowflake 基于 ANSI SQL 扩展，有独特特性（VARIANT/OBJECT/ARRAY + COPY INTO + TIME TRAVEL），无合适基础方言可委派 |
| Redshift 方言实现方式 | A. 独立实现 / B. 委派 PostgreSqlDialect + 特性扩展 | B | Redshift 基于 PostgreSQL 8.0.2 扩展，大部分 SQL 语法与 PG 兼容，委派 PG + 覆盖不兼容构造减少实现成本 |
| QueryBuilder 迁移 lint 注册方式 | A. clippy custom lint / B. 独立工具 | B | clippy custom lint 需 fork clippy 编译器，维护成本高；独立工具基于 syn/quote 解析 Rust AST，可控且轻量 |
| QueryBuilder 迁移 fix 执行方式 | A. 自动修改 / B. 需用户确认 | B | 安全约束（spec §5.3.1.6），fix 需用户显式确认（`--fix` 标志或交互式确认），不自动修改用户代码 |
| async trait 重评估方案 C 可行性 | A. 立即迁移 / B. 评估后定 | B | RPITIT Send bound 限制需评估，dyn trait + async fn 仍可能需 `#[async_trait]`，评估后定推荐方案 |
| crates.io 发布版本号 | A. 3.6.0 minor / B. 4.0.0 major | A | v3.6.0 通过 feature gate 隔离新能力，默认 feature 行为不变，向后兼容，SemVer minor 升级（spec §4.5.4） |

## 2.1 方向 1：编译期类型安全深入优化

### 2.1.1 上下文视图

```plantuml
@startuml
!theme plain
title 方向 1 编译期类型安全深入优化 上下文视图

actor "类型安全优化开发者" as Dev
rectangle "sz-orm-core typed_ast.rs" as Ta {
  port "既有 46 种表达式 (v3.5.0)" as Old46
  port "CTE/Window Frame/JSON 操作符 <<new>>\n[typed-dsl]" as NewExpr
  port "TypedExpression trait" as Trait
  port "ExprTable trait" as ExprTable
}
rectangle "sz-orm-core typed_relation.rs <<new>>\n[typed-relation]" as Rel {
  port "BelongsTo/HasMany/HasOne" as Relations
  port "编译期外键类型校验" as FkCheck
}
rectangle "sz-orm-core eager_loader.rs (既有)" as Eager
rectangle "sz-orm-macros <<new>>" as Macros {
  port "自定义诊断 proc-macro [typed-dsl]" as DiagMacro
  port "SQL 验证 proc-macro [sql-verify-proc]" as VerifyMacro
}
participant "Dialect" as Dialect
participant "cargo test" as Test
rectangle "Diesel (对标基准)" as Diesel

Dev --> Ta : 新增 CTE/Window Frame/JSON 表达式 (ZST + to_sql 参数化)
Dev --> Rel : 新增 typed relation (编译期外键校验)
Dev --> Macros : 自定义诊断 + SQL 验证探索
Ta --> Dialect : to_sql 按方言分派 (JSON 操作符)
Rel --> Eager : typed relation + EagerLoader 协作 (escape hatch)
Dev --> Test : cargo test --features typed-dsl,typed-relation,sql-verify-proc
Test --> Dev : 全通过 + 既有 46 种不回退
Ta ..> Diesel : 表达式覆盖度超越 Diesel

@enduml
```

### 2.1.2 CTE/Window Frame/JSON 操作符表达式类型系统设计

**设计原则**：
1. **零成本抽象（ZST）**：每个新增表达式为零大小类型（unit struct 或仅含 PhantomData），`static_assert!(size_of::<T>() == 0)` 编译期断言。
2. **TypedExpression trait 实现**：每个表达式实现 `TypedExpression` trait（[typed_ast.rs:249](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L249)），关联类型 `SqlType`，方法 `to_sql(&self, dialect: &dyn Dialect) -> String`。
3. **参数化 SQL 生成**：to_sql 生成的 SQL 必须使用参数化占位符（`?`），禁止字符串拼接，沿用 v3.5.0 SQL 注入防护铁律。
4. **方言分派**：to_sql 按方言分派，不支持的方言返回 Err 或回退到通用语法。
5. **类型检查严格性**：表达式类型检查要求操作数 SqlType 兼容，编译期拒绝不兼容类型，提供 Cast 表达式让用户显式转换。

**新增表达式分类**：

| 类别 | 表达式 | 数量 | SqlType 约束 | to_sql 示例 | 方言支持 |
|------|--------|------|------------|-----------|---------|
| CTE | With/WithRecursive/CteRef | 3 | 子查询 SqlType → 输出同子查询 | `WITH cte AS (...) SELECT ...`/`WITH RECURSIVE cte AS (...) SELECT ...`/`cte` | MySQL 8.0+/PG/SQLite 3.8.3+/Oracle/MSSQL |
| Window Frame | RowsFrame/RangeFrame/GroupsFrame/FrameBetween/FrameUnboundedPreceding/FrameCurrentRow | 6 | 窗口表达式 → 输出窗口表达式 | `ROWS BETWEEN ... AND ...`/`RANGE BETWEEN ... AND ...`/`GROUPS BETWEEN ... AND ...`/`UNBOUNDED PRECEDING`/`CURRENT ROW` | MySQL 8.0+/PG/Oracle/MSSQL/SQLite 3.25+ |
| JSON 操作符 | JsonGet/JsonGetText/JsonPathGet/JsonPathGetText/JsonContains/JsonExists | 6 | Json → 输出 Json/Text/Bool | PG: `col->'key'`/`col->>'key'`/`col#>'{path}'`/`col#>>'{path}'`/`col @> 'value'`/`col ? 'key'`；MySQL: `JSON_EXTRACT(col, '$.key')`/`JSON_UNQUOTE(...)`/`JSON_CONTAINS(...)`/`JSON_EXISTS(...)`；SQLite: `json_extract(col, '$.key')` | PG/MySQL 5.7+/SQLite 3.9+ |
| **合计** | — | **15** | — | — | — |

**ZST 实现方案**（以 With 为例）：

```rust
// 概念示意（非最终代码）
pub struct With<Name: CteName, Subquery: TypedExpression>(pub std::marker::PhantomData<(Name, Subquery)>);

impl<Name, Subquery> TypedExpression for With<Name, Subquery>
where
    Name: CteName,
    Subquery: TypedExpression,
{
    type SqlType = Subquery::SqlType;
    fn to_sql(&self, dialect: &dyn Dialect) -> String {
        format!("WITH {} AS ({})", Name::NAME, Subquery::to_sql_default(dialect))
    }
}

// 编译期 ZST 断言
const _: () = assert!(std::mem::size_of::<With<SomeCte, SomeSubquery>>() == 0);
```

**JSON 操作符方言分派方案**（以 JsonGet 为例）：

```rust
// 概念示意（非最终代码）
impl<C, K> TypedExpression for JsonGet<C, K>
where
    C: TypedColumn<RustType = serde_json::Value>,
    K: JsonKey,
{
    type SqlType = Json;
    fn to_sql(&self, dialect: &dyn Dialect) -> String {
        match dialect.db_type() {
            DbType::PostgreSQL => format!("{}->'{}'", C::NAME, K::NAME),
            DbType::MySQL => format!("JSON_EXTRACT({}, '$.{}')", C::NAME, K::NAME),
            DbType::Sqlite => format!("json_extract({}, '$.{}')", C::NAME, K::NAME),
            _ => panic!("JsonGet not supported for {:?}", dialect.db_type()),
        }
    }
}
```

### 2.1.3 自定义编译期诊断信息设计

**诊断信息结构**：

```rust
// 概念示意（非最终代码）
// 通过 proc-macro Diagnostic API 生成
struct TypeMismatchDiagnostic {
    error_location: String,      // 错误位置（列名/表达式）
    expected_type: String,       // 期望 SqlType（如 "Integer"）
    found_type: String,          // 实际 SqlType（如 "Text"）
    fix_suggestion: String,      // 修复建议（如 "请使用 Cast 显式转换"）
}
```

**诊断信息生成流程**：
1. typed_ast DSL 类型不匹配触发编译期错误。
2. proc-macro 捕获类型不匹配信息（期望 SqlType + 实际 SqlType + 错误位置）。
3. 通过 `Diagnostic` API 生成自定义诊断信息，含错误位置 + 期望类型 + 实际类型 + 修复建议。
4. 自定义诊断信息优先输出，抑制 Rust 默认错误（通过 proc-macro 展开控制）。

**诊断信息示例**：

```text
error: typed_ast 类型不匹配
  --> src/query.rs:42:18
   |
42 |     .filter(users::id.eq("Alice"))
   |                  ^^ 期望 Integer，实际 Text
   |
   = 修复建议：请使用 Cast 显式转换，如 users::id.eq(Cast::<Integer>::new("Alice"))
```

### 2.1.4 typed relation 类型安全关联查询设计

**关联类型设计**：

```rust
// 概念示意（非最终代码）
pub struct BelongsTo<Child: Model, Parent: Model, Fk: TypedColumn> {
    _child: PhantomData<Child>,
    _parent: PhantomData<Parent>,
    _fk: PhantomData<Fk>,
}

// 编译期校验外键类型匹配 + 表归属
impl<Child, Parent, Fk> BelongsTo<Child, Parent, Fk>
where
    Child: Model,
    Parent: Model,
    Fk: TypedColumn<Table = Child>,
    Fk: TypedColumn<RustType = <Parent as Model>::PkType>,  // 外键类型 == 父表主键类型
{
    pub fn load(&self, parent: &Parent) -> TypedSelectQuery<Child> {
        TypedSelectQuery::<Child>::new().filter(Fk::eq(parent.pk()))
    }
}
```

**与 EagerLoader 协作**：
- typed relation 提供编译期类型安全关联（BelongsTo/HasMany/HasOne），编译期校验外键类型匹配 + 表归属。
- EagerLoader 提供运行时关联执行（[eager_loader.rs:129](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/eager_loader.rs#L129)）。
- 简单关联用 typed relation（编译期安全），复杂关联（多态关联 MorphMany/MorphTo、自引用关联）回退 EagerLoader（escape hatch）。
- 文档标注适用场景：typed relation 适用于简单 BelongsTo/HasMany/HasOne，EagerLoader 适用于复杂关联。

### 2.1.5 proc-macro 编译期 SQL 验证探索设计

**验证流程**：
1. proc-macro 解析 SQL 字符串（通过 sqlparser crate，既有 plan-cache feature 依赖）。
2. 校验 SQL 语法（解析成功）。
3. 校验表/列存在性（连真 DB 执行 EXPLAIN/EXPLAIN QUERY PLAN，复用既有 db-verify feature 的连真 DB 逻辑）。
4. 校验类型匹配（列类型与值类型兼容）。
5. 仅执行 EXPLAIN 不执行 INSERT/UPDATE/DELETE 等修改 SQL（安全约束 spec §4.3.2）。
6. 缓存验证结果（按 SQL 哈希缓存，仅 SQL 变更时重新验证，避免每次编译都连 DB）。

**与既有 db-verify feature 的关系**：
- db-verify（[Cargo.toml:18](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L18)）仅验证 `query!` 宏。
- sql-verify-proc 扩展到 QueryBuilder 生态（通过 `QueryBuilder::to_sql()` 生成 SQL 字符串后验证）。
- 两者复用连真 DB 逻辑（EXPLAIN 验证），但验证范围不同。

### 2.1.6 `typed-dsl`/`typed-relation`/`sql-verify-proc` feature gate 隔离方式

**Cargo.toml feature 定义**：

```toml
# v3.4.0：类型化 DSL（typed_ast.rs Diesel 风格表达式 DSL，既有）
typed-dsl = []
# v3.6.0：类型安全关联查询（typed relation，新增）
typed-relation = []
# v3.6.0：proc-macro 编译期 SQL 验证探索（新增）
sql-verify-proc = ["dep:sqlparser"]  # 复用 plan-cache 的 sqlparser
```

**条件编译隔离**：
- CTE/Window Frame/JSON 操作符表达式用 `#[cfg(feature = "typed-dsl")]` 条件编译隔离，默认 feature 关闭时不编译，既有 46 种表达式不受影响。
- typed relation 用 `#[cfg(feature = "typed-relation")]` 条件编译隔离，新增 `packages/sz-orm-core/src/typed_relation.rs` 模块。
- proc-macro SQL 验证用 `#[cfg(feature = "sql-verify-proc")]` 条件编译隔离。

**默认 feature 零行为变更**：三个 feature 默认关闭（不在 default = ["redis"] 中），关闭时编译产物大小与运行时开销与 v3.5.0 一致（spec §4.1.5）。

### 2.1.7 实现设计文档

```plantuml
@startuml
!theme plain
title 方向 1 编译期类型安全深入优化实现流程

start
:读取既有 46 种表达式 + SqlType 体系;
:设计 CTE/Window Frame/JSON 操作符 15 种新表达式 ZST 结构;
:为每种表达式实现 TypedExpression trait + to_sql (参数化 + 方言分派);
:添加编译期 ZST 断言 (static_assert size_of == 0);
:编写单元测试 (每种表达式 to_sql + 类型检查 + 方言分派);

:设计自定义编译期诊断 proc-macro;
:实现 Diagnostic API 生成诊断信息 (位置/期望/实际/建议);
:编写诊断信息测试;

:设计 typed relation (BelongsTo/HasMany/HasOne);
:实现编译期外键类型匹配校验 + 表归属校验;
:实现与 EagerLoader 协作 (escape hatch);
:编写 typed relation 测试;

:评估 proc-macro 编译期 SQL 验证可行性;
:实现 SQL 解析 + 表/列存在性校验 + 类型匹配校验 (如可行);
:实现验证结果缓存 (SQL 哈希缓存);
:编写 SQL 验证测试;

:运行 cargo test --features typed-dsl,typed-relation,sql-verify-proc;
if (全通过 + 既有 46 种不回退?) then (是)
  :更新对比分析文档 §6.1 为"已超越 Diesel 表达式覆盖度";
  :附表达式覆盖度对比表 (file:line 证据);
else (否)
  :修复 + 重新测试;
endif
stop

@enduml
```

## 2.2 方向 2：313 pub API 文档补齐

### 2.2.1 上下文视图

```plantuml
@startuml
!theme plain
title 方向 2 313 pub API 文档补齐 上下文视图

actor "文档补齐工程师" as DocEng
rectangle "sz-orm-core src/*.rs" as Src {
  port "313 个缺文档 pub API" as Api313
  port "lib.rs:403 docs.rs cfg 跳过" as CfgSkip
}
participant "cargo doc --all-features" as CargoDoc
participant "cargo test --doc" as Doctest
participant "docs.rs" as DocsRs
rectangle "对比分析文档" as CmpDoc

DocEng --> Src : 扫描定位 313 缺文档 pub API
DocEng --> Src : 逐批补齐 /// 文档 (功能/参数/返回/示例/错误)
DocEng --> Src : 移除 lib.rs:403 docs.rs cfg 跳过
Src --> CargoDoc : 零警告验证
Src --> Doctest : doctest 通过验证
Src --> DocsRs : docs.rs 页面完整
DocEng --> CmpDoc : §6.2 更新为"文档完整度已对齐竞品"

@enduml
```

### 2.2.2 313 pub API 文档补齐策略

**补齐策略**：
1. 定位 313 个缺文档的 pub API（`cargo doc --workspace --no-deps --all-features 2>&1 | grep "missing_docs"`）。
2. 按优先级分批补齐：
   - 第一批：核心 API（QueryBuilder/Pool/Connection/L1Cache/L2Cache/Model/Dialect 等 pub trait/struct/fn）。
   - 第二批：扩展 API（dialect/value/result_map/typed_ast 等）。
   - 第三批：测试/辅助 API。
3. 每个 API 补齐 `///` 文档注释（功能描述 + 参数 + 返回值 + 示例 + 错误），遵循 rustdoc 规范。
4. 移除 [lib.rs:403](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L403) docs.rs cfg 跳过，改为全局 `#![warn(missing_docs)]`。
5. `cargo doc --workspace --no-deps --all-features` 无警告验证。
6. `cargo test --workspace --doc` doctest 验证。
7. API 签名不变（仅新增 `///` 注释）。

**文档注释规范**（每个 API 必须包含）：
- **功能描述**：一句话说明 API 作用。
- **参数说明**：每个参数的含义与类型约束（`# Arguments`）。
- **返回值说明**：返回值含义与类型（`# Returns`）。
- **示例代码**：可运行 doctest（`# Examples` + ```` ```rust ```` 代码块）。
- **错误说明**：可能返回的 Err 及触发条件（`# Errors`）。
- **Panic 说明**：可能 panic 的条件（`# Panics`，按需）。

**doctest 处理策略**：
- 不需要真实 DB 连接的 API：doctest 可运行（`# Examples` + ```` ```rust ````）。
- 需要真实 DB 连接的 API（如 Pool::connect）：doctest 标注 `# ```ignore` 或 `# ```no_run`，或用 Mock 连接，真实 DB 示例标注 `#[ignore]`。
- intra-doc link：文档注释引用的类型/模块路径变更时，`cargo doc` 检测 broken_intra_doc_links 警告，门禁 14 阻断，修复 link。

### 2.2.3 `doc-completion` feature gate 隔离方式

**Cargo.toml feature 定义**（既有，[Cargo.toml:42](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L42)）：

```toml
# v3.4.0：文档补全门禁矩阵标识（不引入依赖，仅用于 feature 组合编译门禁）
doc-completion = []
```

**门禁矩阵标识**：`doc-completion` feature 仅用于门禁矩阵标识（不引入依赖），启用时 `cargo doc --workspace --no-deps --all-features` 零警告验证。

### 2.2.4 实现设计文档

```plantuml
@startuml
!theme plain
title 方向 2 313 pub API 文档补齐实现流程

start
:扫描 packages/sz-orm-core/src/*.rs pub API;
:定位 313 个缺文档 pub API (cargo doc missing_docs);
:第一批补齐: 核心 API (QueryBuilder/Pool/Connection/L1Cache/L2Cache/Model/Dialect);
:第二批补齐: 扩展 API (dialect/value/result_map/typed_ast);
:第三批补齐: 测试/辅助 API;
:移除 lib.rs:403 docs.rs cfg 跳过;
:改为全局 #![warn(missing_docs)];
:运行 cargo doc --workspace --no-deps --all-features;
if (零警告?) then (是)
  :运行 cargo test --workspace --doc;
  if (doctest 全通过?) then (是)
    :验证 docs.rs 页面完整;
    :更新对比分析文档 §6.2 为"文档完整度已对齐竞品";
  else (否)
    :修复 doctest + 重新验证;
  endif
else (否)
  :修复 missing_docs + 重新验证;
endif
stop

@enduml
```

## 2.3 方向 3：QueryBuilder 渐进合并

### 2.3.1 上下文视图

```plantuml
@startuml
!theme plain
title 方向 3 QueryBuilder 渐进合并 上下文视图

actor "迁移工具开发者" as Dev
actor "sz-pay 生产用户" as PayUser
rectangle "sz-orm-query-builder" as ExtQB {
  port "Query (deprecated, lib.rs:214)" as ExtQb
}
rectangle "sz-orm-core query.rs" as CoreQB {
  port "QueryBuilder<M: Model> (query.rs:36)" as CQB
}
rectangle "scripts/ <<new>>\n[qb-migration-tool]" as Scripts {
  port "qb_migration_lint.rs" as Lint
  port "qb_migration_fix.rs" as Fix
}
rectangle "docs/ <<new>>" as Docs {
  port "qb-migration-roadmap.md" as Roadmap
  port "query-builder-guide.md (既有)" as Guide
}
participant "差分测试" as DiffTest

Dev --> Lint : 开发迁移 lint (检测 Query 使用)
Dev --> Fix : 开发迁移 fix (Query → QueryBuilder 转换)
Dev --> DiffTest : 差分测试验证语义等价
Dev --> Roadmap : 制定 v3.7.0 移除路线图
PayUser --> Lint : 运行 lint 检测
Lint --> PayUser : 告警 + 迁移建议
PayUser --> Fix : 运行 fix --dry-run
Fix --> PayUser : 显示转换 diff
PayUser --> Fix : 确认执行 fix (--fix)
Fix --> CQB : 生成 QueryBuilder 等价代码

@enduml
```

### 2.3.2 代码迁移 lint 设计

**lint 职责**：检测 sz-orm-query-builder 的 `Query` 类型使用，输出告警信息（含迁移建议 + core::QueryBuilder 等价 API 指引 + 选择指南链接）。

**lint 检测规则**：
- 精确匹配 `sz_orm_query_builder::Query` 路径（不匹配其他库的 Query）。
- 检测 `use sz_orm_query_builder::Query` / `use sz_orm_query_builder::*` 后使用 `Query::select()` / `Query::insert()` 等。
- 输出告警信息：告警位置 + 迁移建议（`Query::select()` → `QueryBuilder::<Model>::new().select()`）+ 选择指南链接（[docs/query-builder-guide.md](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/query-builder-guide.md#L1)）。

**lint 实现方式**：独立工具（基于 syn/quote 解析 Rust AST），不 fork clippy 编译器，可控且轻量。可通过 `cargo run --bin qb_migration_lint -- <path>` 运行，或集成到 CI。

### 2.3.3 代码迁移 fix 设计

**fix 职责**：自动将 sz-orm-query-builder 的 `Query` 代码转换为 core::QueryBuilder 的等价代码。

**fix 转换规则**：
- `Query::select("table")` → `QueryBuilder::<TableModel>::new().select()`
- `Query::insert("table")` → `QueryBuilder::<TableModel>::new().insert()`
- `Query::update("table")` → `QueryBuilder::<TableModel>::new().update()`
- `Query::delete("table")` → `QueryBuilder::<TableModel>::new().delete()`
- `.where_clause(...)` → `.where_eq(...)`（参数化）
- 复杂场景（UNION/CTE/窗口函数）标注"需人工审查"，不自动转换。

**fix 执行方式**：
- `--dry-run`：仅显示转换 diff，不修改文件。
- `--fix`：执行转换，需用户显式确认。
- 交互式确认：显示 diff 后询问"确认执行？(y/n)"。
- 修改前显示 diff，用户确认后执行。
- 不自动修改用户代码（安全约束 spec §5.3.1.6）。

### 2.3.4 差分测试验证语义等价

**差分测试设计**：
- 对同一查询用 Query 和 QueryBuilder 构造，比较生成 SQL。
- 覆盖所有可转换的查询类型（SELECT/INSERT/UPDATE/DELETE + WHERE/JOIN/GROUP BY/HAVING/ORDER BY/LIMIT/OFFSET）。
- 复杂场景（UNION/CTE/窗口函数）标注"需人工审查"，差分测试不覆盖。
- 验证语义等价（生成的 SQL 相同，或语义等价但语法差异在可接受范围内）。

### 2.3.5 v3.7.0 移除路线图

| 阶段 | 版本 | 内容 | 用户通知 |
|------|------|------|---------|
| 阶段 1 | v3.6.0 | 提供迁移 lint/fix 工具 + deprecated 告警 | CHANGELOG + README + 迁移指南更新 |
| 阶段 2 | v3.6.x（x≥1） | 收集用户反馈优化迁移工具 | CHANGELOG + 迁移工具版本更新 |
| 阶段 3 | v3.7.0 | 正式移除 sz-orm-query-builder 包（从 workspace 移除 + crates.io yank 或保留但标注 EOL） | CHANGELOG + README + 迁移指南 + 提前 1 版本通知 |

**用户通知计划**：
- v3.6.0 CHANGELOG：标注 sz-orm-query-builder deprecated + 迁移工具可用 + v3.7.0 移除计划。
- v3.6.0 README：新增迁移指南链接。
- v3.6.0 迁移指南更新：新增自动迁移工具使用说明。
- v3.6.x CHANGELOG：收集用户反馈，优化迁移工具。
- v3.7.0 CHANGELOG：标注 sz-orm-query-builder 正式移除 + 迁移完成通知。

### 2.3.6 `qb-migration-tool` feature gate 隔离方式

**Cargo.toml feature 定义**（新增）：

```toml
# v3.6.0：QueryBuilder 迁移工具（lint + fix，新增）
qb-migration-tool = ["dep: syn", "dep: quote"]  # 复用 sz-orm-macros 的 syn/quote
```

**条件编译隔离**：迁移 lint/fix 工具用 `#[cfg(feature = "qb-migration-tool")]` 条件编译隔离，默认 feature 关闭时不编译。

### 2.3.7 实现设计文档

```plantuml
@startuml
!theme plain
title 方向 3 QueryBuilder 渐进合并实现流程

start
:开发 qb_migration_lint (检测 Query 使用 + 告警 + 迁移建议);
:开发 qb_migration_fix (Query → QueryBuilder 转换 + --dry-run + --fix);
:编写差分测试 (Query vs QueryBuilder SQL 等价);
:制定 v3.7.0 移除路线图 (v3.6.0/v3.6.x/v3.7.0 各阶段);
:编写 docs/qb-migration-roadmap.md;
:保持 sz-orm-query-builder v3.6.0 可用 (deprecated 但不删除);
:运行 cargo test --features qb-migration-tool;
if (全通过 + sz-orm-query-builder API 兼容?) then (是)
  :sz-pay cargo check 验证 (如使用 sz-orm-query-builder);
  :更新 CHANGELOG + README + 迁移指南;
else (否)
  :修复 + 重新测试;
endif
stop

@enduml
```

## 2.4 方向 4：方言扩展

### 2.4.1 上下文视图

```plantuml
@startuml
!theme plain
title 方向 4 方言扩展 上下文视图

actor "方言扩展开发者" as Dev
rectangle "sz-orm-core dialect.rs" as Dialect {
  port "既有 18 种方言 (v3.5.0)" as Old18
  port "SnowflakeDialect <<new>> [dialect-snowflake]" as SnowNew
  port "RedshiftDialect <<new>> [dialect-redshift]" as RedNew
  port "Dialect trait" as Trait
  port "delegate_dialect_to 宏" as Delegate
}
rectangle "sz-orm-core db_type.rs" as DbType {
  port "既有 21 变体 (v3.5.0)" as Old21
  port "Snowflake + Redshift 变体 <<new>>" as NewVar
}
rectangle "docs/ <<new>>" as Docs {
  port "prisma-dialect-evaluation.md [dialect-prisma]" as PrismaDoc
}
participant "cargo test" as Test
database "Snowflake" as Snowflake
database "AWS Redshift" as Redshift

Dev --> Dialect : 实现 SnowflakeDialect (独立 + VARIANT/OBJECT/ARRAY + TIME TRAVEL)
Dev --> Dialect : 实现 RedshiftDialect (委派 PG + COPY/UNLOAD)
Dev --> DbType : 新增 Snowflake + Redshift 变体
Dev --> Docs : Prisma 兼容评估文档
SnowNew --> Snowflake : Snowflake SQL 生成
RedNew --> Redshift : Redshift SQL 生成 (委派 PG)
Dev --> Test : cargo test --features dialect-snowflake,dialect-redshift
Test --> Dev : 新方言测试通过 + 既有 18 种不变

@enduml
```

### 2.4.2 SnowflakeDialect 实现方案

**SnowflakeDialect 设计**（独立实现，支持 Snowflake 特有特性）：

```rust
// 概念示意（非最终代码）
pub struct SnowflakeDialect;

impl Dialect for SnowflakeDialect {
    fn clone_box(&self) -> Box<dyn Dialect> { Box::new(SnowflakeDialect) }
    fn db_type(&self) -> DbType { DbType::Snowflake }
    fn quote(&self, identifier: &str) -> String { format!("\"{}\"", identifier) }
    // ... 其他 Dialect trait 方法

    // Snowflake 特有方法
    fn build_time_travel(&self, sql: &str, clause: &TimeTravelClause) -> String {
        match clause {
            TimeTravelClause::At(timestamp) => format!("{} AT(TIMESTAMP => '{}')", sql, timestamp),
            TimeTravelClause::Before(timestamp) => format!("{} BEFORE(TIMESTAMP => '{}')", sql, timestamp),
            TimeTravelClause::AtOffset(offset) => format!("{} AT(OFFSET => {})", sql, offset),
        }
    }

    fn build_copy_into(&self, table: &str, source: &str) -> String {
        format!("COPY INTO {} FROM {}", self.quote(table), source)
    }
}
```

**Snowflake 特性支持**：
- VARIANT/OBJECT/ARRAY 半结构化类型：`build_create_table` 支持 VARIANT/OBJECT/ARRAY 列类型。
- COPY INTO 数据加载：`build_copy_into(table, source)` 生成 `COPY INTO table FROM source`。
- TIME TRAVEL 时间旅行查询：`build_time_travel(sql, clause)` 生成 `AT(TIMESTAMP => ...)`/`BEFORE(...)`/`AT(OFFSET => ...)`。
- Snowflake 特有函数：通过 `to_sql` 分派生成。

### 2.4.3 RedshiftDialect 实现方案

**RedshiftDialect 设计**（委派 PostgreSqlDialect + Redshift 特性扩展）：

```rust
// 概念示意（非最终代码）
// 1. db_type.rs 新增 DbType 变体
pub enum DbType {
    // ... 既有 21 变体
    Snowflake,  // 新增
    Redshift,   // 新增
}

// 2. dialect.rs 委派 PostgreSqlDialect + Redshift 特性扩展
pub struct RedshiftDialect(PostgreSqlDialect);

impl Dialect for RedshiftDialect {
    fn clone_box(&self) -> Box<dyn Dialect> { Box::new(RedshiftDialect(PostgreSqlDialect)) }
    fn db_type(&self) -> DbType { DbType::Redshift }
    // 委派 PG 的方法
    fn quote(&self, identifier: &str) -> String { self.0.quote(identifier) }
    fn escape_string(&self, s: &str) -> String { self.0.escape_string(s) }
    fn build_pagination(&self, sql: &str, page: u64, limit: u64) -> String { self.0.build_pagination(sql, page, limit) }
    // ... 其他委派 PG 的方法

    // Redshift 特有方法
    fn build_copy(&self, table: &str, source: &str) -> String {
        format!("COPY {} FROM '{}'", self.quote(table), source)
    }
    fn build_unload(&self, query: &str, target: &str) -> String {
        format!("UNLOAD('{}') TO '{}'", query, target)
    }
}
```

**Redshift 特性支持**：
- COPY 数据加载：`build_copy(table, source)` 生成 `COPY table FROM 'source'`。
- UNLOAD 数据卸载：`build_unload(query, target)` 生成 `UNLOAD('query') TO 'target'`。
- Redshift 特有函数：通过 `to_sql` 分派生成。
- 不兼容的 PG 构造：覆盖委派方法，返回 Err 或 Redshift 特有语法。

### 2.4.4 Prisma 方言兼容评估方案

**评估内容**：
1. **Prisma Schema DSL 映射评估**：Prisma model/entity/field/relation 与 sz-orm Model trait 的映射关系。
2. **Prisma 查询语法映射评估**：Prisma findMany/findUnique/create/update 与 sz-orm QueryBuilder 的映射。
3. **跨生态兼容技术可行性评估**：TypeScript/Node.js（Prisma 生态）vs Rust（sz-orm 生态）跨生态兼容的技术可行性。
4. **实现难度与收益评估**：实现 Prisma 方言兼容的难度（高，跨生态）与收益（低，Prisma 用户少）。
5. **推荐方案**：评估后输出推荐方案（可能为"不实施，跨生态兼容难度高收益低"）。

### 2.4.5 `dialect-snowflake`/`dialect-redshift`/`dialect-prisma` feature gate 隔离方式

**Cargo.toml feature 定义**（新增）：

```toml
# v3.6.0：Snowflake 方言（云数仓，独立实现）
dialect-snowflake = []
# v3.6.0：Redshift 方言（AWS 云数仓，委派 PG + 特性扩展）
dialect-redshift = []
# v3.6.0：Prisma 方言兼容评估（仅评估，不实现）
dialect-prisma = []
```

**条件编译隔离**：
- SnowflakeDialect 用 `#[cfg(feature = "dialect-snowflake")]` 条件编译隔离。
- RedshiftDialect 用 `#[cfg(feature = "dialect-redshift")]` 条件编译隔离。
- Prisma 评估文档通过 `dialect-prisma` feature gate 标识（纯文档，不引入依赖）。

### 2.4.6 实现设计文档

```plantuml
@startuml
!theme plain
title 方向 4 方言扩展实现流程

start
:实现 SnowflakeDialect (Dialect trait + VARIANT/OBJECT/ARRAY + COPY INTO + TIME TRAVEL);
:新增 DbType::Snowflake 变体;
:编写 Snowflake 方言单元测试;
:编写 Snowflake 集成测试 (#[ignore], 需真实 Snowflake);

:实现 RedshiftDialect (委派 PG + COPY/UNLOAD 特性扩展);
:新增 DbType::Redshift 变体;
:编写 Redshift 方言单元测试;
:编写 Redshift 集成测试 (#[ignore], 需真实 Redshift);

:评估 Prisma 方言兼容 (Schema DSL 映射 + 查询语法映射 + 跨生态可行性);
:编写 docs/prisma-dialect-evaluation.md;

:运行 cargo test --features dialect-snowflake,dialect-redshift;
if (新方言测试通过 + 既有 18 种不变?) then (是)
  :更新对比分析文档 §6.7 为"20 种方言";
  :更新方言扩展路线图;
else (否)
  :修复 + 重新测试;
endif
stop

@enduml
```

## 2.5 方向 5：async trait 风格统一重评估

### 2.5.1 上下文视图

```plantuml
@startuml
!theme plain
title 方向 5 async trait 风格统一重评估 上下文视图

actor "重评估者" as Eval
rectangle "Rust 最新进展" as RustNews {
  port "RPITIT (Rust 1.75+ 稳定)" as RPITIT
  port "async fn in trait" as AsyncFn
  port "Send bound 限制" as SendBound
  port "Rust 1.80+ 改进" as Rust180
  port "async-trait crate 最新版" as AsyncCrate
}
rectangle "sz-orm-core pool.rs" as Pool {
  port "Connection trait (手动解糖, pool.rs:45)" as Conn
  port "ConnectionFactory trait (#[async_trait], pool.rs:732)" as Factory
}
rectangle "docs/async-trait-evaluation.md" as EvalDoc {
  port "v3.5.0 评估 (既有)" as OldEval
  port "v3.6.0 更新 (RPITIT 重评估) <<new>>" as NewEval
}
participant "sz-pay" as Pay

Eval --> RustNews : 调研 RPITIT / async fn in trait / Send bound
Eval --> OldEval : 复审 v3.5.0 评估结论
Eval --> Pool : 重新评估三方案 (A/B/C)
Eval --> NewEval : 输出更新评估文档 + 推荐方案
alt 推荐方案为迁移
  Eval --> Pool : 渐进迁移 (分阶段)
  Eval --> Pay : sz-pay cargo check + cargo test 零回归
else 推荐方案为不改
  Eval --> NewEval : 标注"v3.6.0 维持现状"及原因
end

@enduml
```

### 2.5.2 Rust async trait 最新进展调研内容

**调研内容**：
1. **RPITIT（Return Position Impl Trait In Trait）**：Rust 1.75+ 稳定，允许在 trait 方法返回类型使用 `impl Trait`，使 async fn in trait 无需 `#[async_trait]` 宏即可使用（但有 Send bound 限制）。
2. **async fn in trait 的 dyn trait 限制**：原生 async fn in trait 不支持 `dyn trait`（需 `#[async_trait]` 宏才能使用 dyn trait），限制 Connection trait 等 trait object 使用。
3. **Rust 1.80+ 的 async fn in trait + Send bound 改进**：Rust 1.80+ 是否改进 Send bound 限制（如 `async fn in trait + Send bound` 是否更易表达）。
4. **async-trait crate 最新版本与特性**：async-trait crate 最新版是否支持原生 async fn in trait 兼容、是否减少运行时开销。
5. **tokio 异步运行时对 async trait 的影响**：tokio 对原生 async fn in trait 的支持情况、Send bound 要求。

### 2.5.3 三方案重新评估

**方案 A：统一为 `#[async_trait]` 宏**
- 优点：代码简洁，错误信息可读，新人学习成本低，生态主流。
- 缺点：宏展开开销（编译时间增加），**HRTB 与 sqlx::Executor 冲突（致命缺陷，v3.5.0 评估已确认）**，Breaking Change。
- 最新进展影响：RPITIT 不解决 HRTB 冲突（HRTB 是 sqlx::Executor 的约束，非 async trait 风格问题）。
- 结论：**技术不可行**（HRTB 冲突无法绕过）。

**方案 B：统一为手动解糖**
- 优点：无宏依赖，无宏展开开销，避免 HRTB 冲突，风格统一。
- 缺点：签名复杂（`fn execute<'a>(&'a mut self, ...) -> Pin<Box<dyn Future + 'a>>`），错误信息难读，新人学习成本高，迁移工作量大，Breaking Change。
- 最新进展影响：RPITIT 不影响手动解糖方案。
- 结论：可行但工作量大 + Breaking Change，不推荐。

**方案 C：原生 async fn in trait（基于 RPITIT）**
- 优点：代码简洁（`async fn execute(&mut self, sql: &str) -> Result<...>`），无宏依赖，编译期类型检查，Rust 原生支持。
- 缺点：**dyn trait 限制**（原生 async fn in trait 不支持 `dyn trait`，Connection trait 需 trait object 则不可用），**Send bound 限制**（async fn in trait 的 Send bound 表达复杂），可能 Breaking Change。
- 最新进展影响：Rust 1.80+ 可能改进 Send bound 表达，但 dyn trait 限制仍存在。
- 结论：需评估 dyn trait 限制是否影响 Connection trait 使用（Connection trait 是否需要 trait object）。

**三方案对比表**：

| 维度 | 方案 A（`#[async_trait]`） | 方案 B（手动解糖） | 方案 C（原生 async fn in trait） |
|------|---------------------------|-------------------|-------------------------------|
| 运行时性能 | 无差异 | 无差异 | 无差异 |
| 编译时间 | 略慢（宏展开） | 略快（无宏） | 基线（原生） |
| 代码简洁性 | 好（简洁） | 差（冗长） | 好（简洁） |
| 学习成本 | 低 | 高 | 低 |
| 迁移工作量 | 中 | 大 | 中 |
| Breaking Change | **是** | **是** | **可能** |
| HRTB 冲突 | **有（致命）** | 无 | 无 |
| dyn trait 支持 | ✅ | ✅ | **❌（限制）** |
| Send bound | ✅ | ✅ | **限制** |
| sz-pay 影响 | 无法编译 | 需验证 | 需验证 |
| 技术可行性 | **不可行** | 可行 | **需评估** |

### 2.5.4 推荐方案与渐进迁移

**推荐方案**：基于 v3.5.0 评估结论（方案 A 不可行 HRTB 冲突，方案 B 工作量大 Breaking Change）+ v3.6.0 重评估（方案 C dyn trait 限制需评估），推荐：

- **若 Connection trait 不需要 trait object**：方案 C（原生 async fn in trait），代码简洁 + 无宏依赖 + 无 HRTB 冲突。
- **若 Connection trait 需要 trait object**：方案 C 不可用（dyn trait 限制），维持方案 C-v3.5.0（保持现状 + 文档说明 HRTB 冲突原因）。
- **评估期内**：既有 Connection trait 签名不变，重评估仅输出评估文档与推荐方案，不强制实施迁移（spec §5.5.1.5）。

**渐进迁移方案（若推荐方案 C 且 Connection trait 不需要 trait object）**：
- 阶段 1：迁移非 Connection trait（ConnectionFactory/Model 等），全量测试 + sz-pay 零回归验证。
- 阶段 2：评估迁移 Connection trait（若 dyn trait 限制不影响），全量测试 + sz-pay 零回归验证。
- 不一次性迁移所有 trait。

### 2.5.5 实现设计文档

```plantuml
@startuml
!theme plain
title 方向 5 async trait 风格统一重评估实现流程

start
:调研 Rust async trait 最新进展 (RPITIT / async fn in trait / Send bound / Rust 1.80+ / async-trait crate);
:复审 v3.5.0 评估结论 (逐条判断是否仍然成立);
:重新评估方案 A (统一 #[async_trait]) 优缺点 + 最新进展;
:重新评估方案 B (统一手动解糖) 优缺点;
:重新评估方案 C (原生 async fn in trait) 优缺点 + dyn trait 限制 + Send bound;
:输出更新评估文档 + 推荐方案;
if (推荐方案为迁移?) then (是)
  :制定渐进迁移方案 (分阶段 + 每阶段测试 + sz-pay 零回归);
  :阶段 1: 迁移非 Connection trait;
  :sz-pay cargo check + cargo test 零回归验证;
  :阶段 2: 评估迁移 Connection trait (若 dyn trait 限制不影响);
else (否)
  :标注"v3.6.0 维持现状"及原因;
endif
:保持既有 Connection trait 评估期内签名不变;
stop

@enduml
```

---

# 三、Feature Gate 设计

## 3.1 新增 Feature Gate 清单

| Feature | 所属包 | 默认 | 依赖 | 关联方向 | 说明 |
|---------|--------|------|------|---------|------|
| `typed-relation` | sz-orm-core | 关闭 | 无（复用既有 typed_ast + Model trait + EagerLoader） | 方向 1 | 类型安全关联查询（typed relation，BelongsTo/HasMany/HasOne 编译期校验） |
| `sql-verify-proc` | sz-orm-core | 关闭 | sqlparser（复用 plan-cache 的 sqlparser） | 方向 1 | proc-macro 编译期 SQL 验证探索（扩展 db-verify 到 QueryBuilder 生态） |
| `dialect-snowflake` | sz-orm-core | 关闭 | 无（独立实现 Dialect trait） | 方向 4 | Snowflake 方言（云数仓，VARIANT/OBJECT/ARRAY + COPY INTO + TIME TRAVEL） |
| `dialect-redshift` | sz-orm-core | 关闭 | 无（委派 PostgreSqlDialect） | 方向 4 | Redshift 方言（AWS 云数仓，委派 PG + COPY/UNLOAD 特性扩展） |
| `dialect-prisma` | sz-orm-core | 关闭 | 无（纯评估文档） | 方向 4 | Prisma 方言兼容评估（仅评估，不实现） |
| `qb-migration-tool` | sz-orm-core | 关闭 | syn / quote（复用 sz-orm-macros 的 syn/quote） | 方向 3 | QueryBuilder 迁移 lint/fix 工具 |

**既有 Feature 复用**（不新增，v3.6.0 复用）：

| Feature | 所属包 | 默认 | 关联方向 | 说明 |
|---------|--------|------|---------|------|
| `typed-dsl` | sz-orm-core | 关闭 | 方向 1 | CTE/Window Frame/JSON 操作符新表达式 + 自定义诊断（[Cargo.toml:56](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L56) 既有） |
| `doc-completion` | sz-orm-core | 关闭 | 方向 2 | 313 pub API 文档补齐门禁矩阵标识（[Cargo.toml:42](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L42) 既有） |

## 3.2 Feature 正交性矩阵

| Feature | typed-dsl | typed-relation | sql-verify-proc | doc-completion | qb-migration-tool | dialect-snowflake | dialect-redshift | dialect-prisma |
|---------|-----------|----------------|-----------------|----------------|-------------------|-------------------|------------------|----------------|
| typed-dsl | — | ✅ 正交 | ✅ 正交 | ✅ 正交 | ✅ 正交 | ✅ 正交 | ✅ 正交 | ✅ 正交 |
| typed-relation | ✅ | — | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| sql-verify-proc | ✅ | ✅ | — | ✅ | ✅ | ✅ | ✅ | ✅ |
| doc-completion | ✅ | ✅ | ✅ | — | ✅ | ✅ | ✅ | ✅ |
| qb-migration-tool | ✅ | ✅ | ✅ | ✅ | — | ✅ | ✅ | ✅ |
| dialect-snowflake | ✅ | ✅ | ✅ | ✅ | ✅ | — | ✅ | ✅ |
| dialect-redshift | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — | ✅ |
| dialect-prisma | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — |

**正交性结论**：所有新增 feature 互不依赖，可任意组合启用/关闭，无冲突。原因：
1. `typed-dsl` 仅影响 typed_ast.rs 表达式，不触碰其他模块。
2. `typed-relation` 仅新增 typed_relation.rs 模块，与 EagerLoader 协作但通过公开 API，不依赖其他 feature。
3. `sql-verify-proc` 仅扩展 proc-macro SQL 验证，复用 db-verify 连真 DB 逻辑，不依赖其他 feature。
4. `doc-completion` 仅用于门禁矩阵标识，不引入依赖。
5. `qb-migration-tool` 仅影响迁移 lint/fix 工具，不依赖其他 feature。
6. `dialect-snowflake`/`dialect-redshift`/`dialect-prisma` 仅新增 DbType 变体 + Dialect 实现/评估文档，不依赖其他 feature。

## 3.3 默认 Feature 零行为变更保证

**默认 feature**（[Cargo.toml:15](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L15)）：`default = ["redis"]`。

**零行为变更保证**：
1. 所有新增 feature（typed-relation/sql-verify-proc/dialect-snowflake/dialect-redshift/dialect-prisma/qb-migration-tool）默认关闭，不在 default = ["redis"] 中。
2. 既有 feature 复用（typed-dsl/doc-completion）默认关闭。
3. 默认 feature 关闭时：
   - 编译产物大小与 v3.5.0 一致（无新增代码编译）。
   - 运行时开销与 v3.5.0 一致（无新增逻辑执行）。
   - 既有公开 API 行为与 v3.5.0 一致（无行为变更）。
4. 新增依赖（syn/quote for qb-migration-tool，sqlparser for sql-verify-proc）为 optional，仅对应 feature 启用时引入，默认 feature 无新依赖（spec §4.1.5）。

---

# 四、兼容性设计

## 4.1 每个方向的 Breaking Change 风险评估

| 方向 | Breaking Change 风险 | 风险点 | 缓解措施 | sz-pay 影响评估 |
|------|---------------------|--------|---------|---------------|
| 1 编译期类型安全深入优化 | **无** | feature gate 隔离，既有 46 种表达式不变 | `typed-dsl`/`typed-relation`/`sql-verify-proc` feature gate（默认关闭） | 无（sz-pay 未用 typed-dsl/typed-relation/sql-verify-proc feature） |
| 2 313 pub API 文档补齐 | **无** | 纯文档，不改 API 签名 | — | 无 |
| 3 QueryBuilder 渐进合并 | **无** | v3.6.0 保持 sz-orm-query-builder 可用，v3.7.0 才移除 | deprecated + 不立即删除 + 迁移工具 | 无（sz-pay 用 sz-orm-query-builder API 兼容，deprecated 告警不影响编译） |
| 4 方言扩展 | **无** | feature gate 隔离，既有 18 种方言不变 | `dialect-snowflake`/`dialect-redshift` feature gate + #[non_exhaustive] | 无（sz-pay 用既有 DbType 变体） |
| 5 async trait 重评估 | **低** | 评估期内签名不变，迁移仅评估不强制 | 评估期 + feature gate 隔离 | 无（评估期内无影响） |

## 4.2 向后兼容方案

### 4.2.1 Feature Gate 隔离

所有新能力通过 feature gate 隔离，默认关闭，既有公开 API 行为不变：
- `typed-dsl`：CTE/Window Frame/JSON 操作符新表达式 + 自定义诊断仅在 feature 启用时编译，既有 46 种表达式不受影响。
- `typed-relation`：typed relation 模块仅在 feature 启用时编译，既有 EagerLoader API 不变。
- `sql-verify-proc`：proc-macro SQL 验证仅在 feature 启用时编译，既有 db-verify feature 不受影响。
- `dialect-snowflake`/`dialect-redshift`：新方言仅在 feature 启用时编译，既有 18 种方言不变。
- `qb-migration-tool`：迁移 lint/fix 工具仅在 feature 启用时编译，既有 sz-orm-query-builder API 不变。

### 4.2.2 渐进 Deprecation

- sz-orm-query-builder：v3.6.0 保持 deprecated 标注但不删除，v3.7.0 才正式移除，给用户迁移周期（spec §4.5.3）。
- deprecated 标注指向选择指南（[docs/query-builder-guide.md](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/query-builder-guide.md#L1)）+ 迁移路线图（docs/qb-migration-roadmap.md）。

### 4.2.3 SemVer 兼容

- v3.6.0 版本号遵循 SemVer，minor 版本升级（3.5.0 → 3.6.0）保证向后兼容（spec §4.5.4）。
- 既有公开 API 签名不变，新增 API 通过 feature gate 隔离。
- sz-pay 从 crates.io 3.5.0 升级到 3.6.0，dry-run + 零回归验证保证平滑升级。

## 4.3 v3.5.0 测试基线不回退保证

**基线**：v3.5.0 已验收 6,751 passed / 0 failed / 253 ignored（spec §1.2.1）。

**不回退保证**：
1. v3.6.0 不得使 v3.5.0 已验收的 6,751 测试基线回退，仅增不减（spec §4.2.1）。
2. 所有新增能力附测试（单元测试 + 集成测试按需），不修改既有测试。
3. 既有公开 API 签名不变，既有测试不受影响。
4. CI 门禁前置：`cargo test --workspace -j 2 --no-fail-fast` 全通过才允许合入。
5. sz-pay 零回归验证：升级到 3.6.0 后 cargo test 零回归（与 sz-pay 既有测试基线对比）。

---

# 五、里程碑设计

## 5.1 5 个里程碑详细任务分解

### 5.1.1 M1：方向 1 编译期类型安全深入优化（最高优先级，中风险）

**周期**：3 周
**依赖**：无
**关联需求**：REQ-TS-001~009

**任务分解**：

| 任务 ID | 任务描述 | 子任务 | 预估工时 | 关联需求 |
|---------|---------|--------|---------|---------|
| M1-T1 | CTE 表达式 With/WithRecursive/CteRef | 3 种 ZST + TypedExpression + to_sql + 递归 CTE + 测试 | 6h | REQ-TS-001 |
| M1-T2 | Window Frame 表达式 RowsFrame/RangeFrame/GroupsFrame/FrameBetween/FrameUnboundedPreceding/FrameCurrentRow | 6 种 ZST + to_sql + 与既有窗口函数协作 + 测试 | 8h | REQ-TS-002 |
| M1-T3 | JSON 操作符表达式 JsonGet/JsonGetText/JsonPathGet/JsonPathGetText/JsonContains/JsonExists | 6 种 ZST + 方言分派（PG/MySQL/SQLite）+ to_sql + 测试 | 8h | REQ-TS-003 |
| M1-T4 | 自定义编译期诊断信息 | proc-macro Diagnostic API + 错误位置/期望类型/实际类型/修复建议 + 测试 | 8h | REQ-TS-004 |
| M1-T5 | typed relation 类型安全关联查询 | BelongsTo/HasMany/HasOne + 编译期外键类型校验 + 表归属校验 + 与 EagerLoader 协作 + 测试 | 10h | REQ-TS-005 |
| M1-T6 | proc-macro 编译期 SQL 验证探索 | SQL 解析 + 表/列存在性校验 + 类型匹配 + EXPLAIN only + 缓存 + 测试 | 8h | REQ-TS-006 |
| M1-T7 | 编译期 ZST 断言 + 差分测试 | static_assert size_of == 0 + typed_ast vs QueryBuilder SQL 一致 | 4h | REQ-TS-008 |
| M1-T8 | 表达式覆盖度超越 Diesel 验证 | 对比表 + file:line 证据 + §6.1 更新 | 2h | REQ-TS-007 |
| M1-T9 | SQL 注入扫描验证 | scripts/check-sql-injection.ps1 通过 + 参数化占位符 | 2h | REQ-TS-009 |
| M1-T10 | Cargo.toml 新增 typed-relation/sql-verify-proc feature + lib.rs 模块声明 | feature 定义 + #[cfg(feature)] | 2h | REQ-TS-001/005/006 |

**验收标准**：
- [ ] CTE 表达式（With/WithRecursive/CteRef）实现，支持递归 CTE
- [ ] Window Frame 表达式（RowsFrame/RangeFrame/GroupsFrame/FrameBetween/FrameUnboundedPreceding/FrameCurrentRow）实现
- [ ] JSON 操作符表达式（JsonGet/JsonGetText/JsonPathGet/JsonPathGetText/JsonContains/JsonExists）实现，PostgreSQL/MySQL/SQLite 三方言分派
- [ ] 自定义编译期诊断信息实现（含错误位置/期望类型/实际类型/修复建议）
- [ ] 类型安全关联查询（typed relation）实现（BelongsTo/HasMany/HasOne 编译期校验）
- [ ] proc-macro 编译期 SQL 验证探索完成（如可行）
- [ ] 所有新增表达式为 ZST（零成本抽象）
- [ ] `cargo test -p sz-orm-core --features typed-dsl,typed-relation,sql-verify-proc` 全通过
- [ ] 既有 46 种表达式测试不回退
- [ ] 对比分析文档 §6.1 更新为"已超越 Diesel 表达式覆盖度"
- [ ] 表达式覆盖度 > Diesel 2.2.x
- [ ] SQL 注入扫描通过（参数化占位符）

### 5.1.2 M2：方向 2 313 pub API 文档补齐（高优先级，低风险）

**周期**：2 周
**依赖**：M1（新表达式 API 需同步补齐文档）
**关联需求**：REQ-DOC-API-001~007

**任务分解**：

| 任务 ID | 任务描述 | 子任务 | 预估工时 | 关联需求 |
|---------|---------|--------|---------|---------|
| M2-T1 | 定位 313 缺文档 pub API | cargo doc missing_docs 扫描 + 分类 | 2h | REQ-DOC-API-001 |
| M2-T2 | 第一批补齐：核心 API | QueryBuilder/Pool/Connection/L1Cache/L2Cache/Model/Dialect 文档 | 8h | REQ-DOC-API-001 |
| M2-T3 | 第二批补齐：扩展 API | dialect/value/result_map/typed_ast 文档 | 6h | REQ-DOC-API-001 |
| M2-T4 | 第三批补齐：测试/辅助 API | 测试/辅助 API 文档 | 4h | REQ-DOC-API-001 |
| M2-T5 | 移除 docs.rs cfg 跳过 | lib.rs:403 改为全局 #![warn(missing_docs)] | 1h | REQ-DOC-API-002 |
| M2-T6 | cargo doc 零警告验证 | cargo doc --workspace --no-deps --all-features | 2h | REQ-DOC-API-003 |
| M2-T7 | doctest 验证 | cargo test --workspace --doc + 修复 doctest | 4h | REQ-DOC-API-004 |
| M2-T8 | 文档与代码一致性验证 | doctest + 代码审查 | 2h | REQ-DOC-API-005 |
| M2-T9 | 对比分析文档 §6.2 更新 | "文档完整度已对齐竞品" + file:line 证据 | 1h | REQ-DOC-API-006 |

**验收标准**：
- [ ] 313 个 pub API 文档补齐（每个 API 有 `///` 文档注释，含功能/参数/返回/示例/错误说明）
- [ ] 移除 `lib.rs:403` docs.rs cfg 跳过配置
- [ ] `cargo doc --workspace --no-deps --all-features` 零警告
- [ ] `cargo test --workspace --doc` doctest 全通过
- [ ] docs.rs 页面完整可浏览
- [ ] 对比分析文档 §6.2 更新为"文档完整度已对齐竞品"

### 5.1.3 M3：方向 3 QueryBuilder 渐进合并（中优先级，中风险）

**周期**：2 周
**依赖**：M1（feature gate 体系就绪）
**关联需求**：REQ-QB-MIG-001~007

**任务分解**：

| 任务 ID | 任务描述 | 子任务 | 预估工时 | 关联需求 |
|---------|---------|--------|---------|---------|
| M3-T1 | 开发 qb_migration_lint | syn/quote 解析 AST + 检测 Query 使用 + 告警 + 迁移建议 | 8h | REQ-QB-MIG-001 |
| M3-T2 | 开发 qb_migration_fix | Query → QueryBuilder 转换 + --dry-run + --fix + 交互式确认 | 8h | REQ-QB-MIG-002 |
| M3-T3 | 差分测试验证语义等价 | Query vs QueryBuilder SQL 等价 + 覆盖可转换查询类型 | 6h | REQ-QB-MIG-003 |
| M3-T4 | 制定 v3.7.0 移除路线图 | v3.6.0/v3.6.x/v3.7.0 各阶段 + 用户通知计划 | 2h | REQ-QB-MIG-004 |
| M3-T5 | 保持 sz-orm-query-builder v3.6.0 可用 | deprecated 标注 + API 兼容 + 不删除 | 2h | REQ-QB-MIG-005 |
| M3-T6 | sz-pay cargo check 验证 | 验证 sz-pay 使用 sz-orm-query-builder 兼容 | 2h | REQ-QB-MIG-007 |
| M3-T7 | Cargo.toml 新增 qb-migration-tool feature | feature 定义 + #[cfg(feature)] | 1h | REQ-QB-MIG-001 |

**验收标准**：
- [ ] 代码迁移 lint 开发完成（检测 sz-orm-query-builder::Query 使用，输出告警 + 迁移建议）
- [ ] 代码迁移 fix 开发完成（Query → QueryBuilder 等价转换，需用户确认）
- [ ] 差分测试验证迁移工具语义保持（Query 与 QueryBuilder 生成 SQL 等价）
- [ ] v3.7.0 移除路线图制定（含 v3.6.0/v3.6.x/v3.7.0 各阶段计划 + 用户通知计划）
- [ ] sz-orm-query-builder v3.6.0 保持可用（标注 deprecated 但不删除，API 完全兼容）
- [ ] sz-pay cargo check 通过（如使用 sz-orm-query-builder）

### 5.1.4 M4：方向 4 方言扩展（中优先级，中风险）

**周期**：2 周
**依赖**：M1（feature gate 体系就绪）
**关联需求**：REQ-DIALECT-001~008

**任务分解**：

| 任务 ID | 任务描述 | 子任务 | 预估工时 | 关联需求 |
|---------|---------|--------|---------|---------|
| M4-T1 | SnowflakeDialect 实现 | Dialect trait + VARIANT/OBJECT/ARRAY + COPY INTO + TIME TRAVEL + 测试 | 10h | REQ-DIALECT-001 |
| M4-T2 | RedshiftDialect 实现 | 委派 PG + COPY/UNLOAD 特性扩展 + 测试 | 6h | REQ-DIALECT-002 |
| M4-T3 | DbType 新增 Snowflake + Redshift 变体 | db_type.rs 新增变体 + get_dialect 新增分支 | 2h | REQ-DIALECT-001/002 |
| M4-T4 | 新方言五方言行为一致验证 | 公共 SQL 构造行为一致 + 特有构造仅该方言支持 | 4h | REQ-DIALECT-004 |
| M4-T5 | 新方言 Rust 驱动评估 | Snowflake 驱动评估 + Redshift PG 兼容评估 | 2h | REQ-DIALECT-005 |
| M4-T6 | Prisma 方言兼容评估 | Schema DSL 映射 + 查询语法映射 + 跨生态可行性 + 推荐方案 | 4h | REQ-DIALECT-003 |
| M4-T7 | 方言扩展路线图更新 | v3.6.0 已实现 + v3.7.0+ 候选 + §6.7 更新 | 2h | REQ-DIALECT-006 |
| M4-T8 | Cargo.toml 新增 dialect-snowflake/redshift/prisma feature | feature 定义 + #[cfg(feature)] | 1h | REQ-DIALECT-001/002/003 |

**验收标准**：
- [ ] SnowflakeDialect 实现完成（Dialect trait + VARIANT/OBJECT/ARRAY + TIME TRAVEL）
- [ ] RedshiftDialect 实现完成（委派 PG + COPY/UNLOAD 特性扩展）
- [ ] Prisma 方言兼容评估完成（含 Schema DSL 映射 + 查询语法映射 + 可行性 + 推荐方案）
- [ ] 新方言五方言行为一致验证通过
- [ ] 新方言 Rust 驱动评估完成
- [ ] `cargo test --features dialect-snowflake,dialect-redshift` 通过
- [ ] 既有 18 种方言测试不回退
- [ ] 方言扩展路线图更新（v3.6.0 已实现 + v3.7.0+ 候选）
- [ ] 对比分析文档 §6.7 更新为"20 种方言"

### 5.1.5 M5：方向 5 async trait 风格统一重评估（低优先级，低风险）

**周期**：1 周
**依赖**：M1（feature gate 体系就绪）
**关联需求**：REQ-ASYNC-001~006

**任务分解**：

| 任务 ID | 任务描述 | 子任务 | 预估工时 | 关联需求 |
|---------|---------|--------|---------|---------|
| M5-T1 | Rust async trait 最新进展调研 | RPITIT + async fn in trait + Send bound + Rust 1.80+ + async-trait crate | 4h | REQ-ASYNC-001 |
| M5-T2 | v3.5.0 评估结论复审 | 逐条复审 + 标注结论是否变更 | 2h | REQ-ASYNC-002 |
| M5-T3 | 三方案重新评估 | 方案 A/B/C 优缺点 + Send bound + dyn trait + 性能 + 迁移影响 | 6h | REQ-ASYNC-003 |
| M5-T4 | 输出更新评估文档 + 推荐方案 | docs/async-trait-evaluation.md 更新 | 2h | REQ-ASYNC-003 |
| M5-T5 | 渐进迁移方案制定（如推荐迁移） | 分阶段计划 + 每阶段测试 + sz-pay 零回归 | 4h | REQ-ASYNC-004 |
| M5-T6 | 保持既有 Connection trait 评估期内不变 | 签名不变 + sz-pay cargo check | 1h | REQ-ASYNC-005 |

**验收标准**：
- [ ] Rust async trait 最新进展调研完成（RPITIT / async fn in trait / Send bound）
- [ ] v3.5.0 评估结论复审完成（逐条复审，标注结论是否变更）
- [ ] 三方案重新评估完成（方案 A/B/C 优缺点 + Send bound + dyn trait + 性能 + 迁移影响 + 推荐方案）
- [ ] 渐进迁移方案制定（如推荐迁移，含分阶段计划 + sz-pay 零回归验证）
- [ ] 既有 Connection trait 评估期内签名不变
- [ ] 更新评估文档输出

## 5.2 里程碑间依赖关系

```plantuml
@startuml
!theme plain
title v3.6.0 里程碑依赖关系

rectangle "M1 编译期类型安全深入优化\n(3周, 最高优先级, 中风险)" as M1
rectangle "M2 313 pub API 文档补齐\n(2周, 高优先级, 低风险)" as M2
rectangle "M3 QueryBuilder 渐进合并\n(2周, 中优先级, 中风险)" as M3
rectangle "M4 方言扩展\n(2周, 中优先级, 中风险)" as M4
rectangle "M5 async trait 重评估\n(1周, 低优先级, 低风险)" as M5

M1 --> M2 : 新表达式 API 需同步补齐文档
M1 --> M3 : feature gate 体系就绪
M1 --> M4 : feature gate 体系就绪
M1 --> M5 : feature gate 体系就绪

note bottom of M1
  **关键路径**：M1 → M2（串行 5 周）
  **并行机会**：
  - M1 完成后 M3/M4/M5 可并行
  - M1 内部：CTE/Window Frame/JSON + 自定义诊断 + typed relation + proc-macro 验证可部分并行
  - M2 内部：3 批 API 文档可并行
  - M4 内部：Snowflake + Redshift + Prisma 评估可并行
  **总周期**：关键路径 5 周；并行开发下可压缩至 4-5 周
end note

@enduml
```

## 5.3 每个里程碑的验收标准

（已在 §5.1.1~§5.1.5 各里程碑任务分解后列出验收标准，此处汇总）

| 里程碑 | 验收标准数 | 关键验收标准 |
|--------|-----------|------------|
| M1 | 12 | CTE/Window Frame/JSON 表达式 ZST + 自定义诊断 + typed relation + proc-macro 验证 + 既有 46 种不变 + 超越 Diesel |
| M2 | 6 | 313 API 文档补齐 + 移除 docs.rs cfg 跳过 + cargo doc 零警告 + doctest 通过 |
| M3 | 6 | 迁移 lint/fix 工具 + 差分测试 + v3.7.0 路线图 + sz-orm-query-builder v3.6.0 可用 |
| M4 | 9 | Snowflake/Redshift 方言 + Prisma 评估 + 五方言行为一致 + 既有 18 种不变 + 20 种方言 |
| M5 | 6 | 最新进展调研 + v3.5.0 复审 + 三方案重评估 + 渐进迁移（如推荐） + 评估期内签名不变 |

---

# 六、测试策略

## 6.1 每个方向的测试方案

### 6.1.1 方向 1：编译期类型安全深入优化

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| 单元测试 | 15 种新表达式 to_sql 生成正确 SQL + 类型检查 + ZST 断言 + 自定义诊断信息 | `cargo test -p sz-orm-core --features typed-dsl,typed-relation,sql-verify-proc` | 每表达式至少 3 测试（正常/边界/错误） |
| 差分测试 | typed_ast vs QueryBuilder SQL 一致（相同查询生成相同 SQL） | `cargo test -p sz-orm-core --features typed-dsl --test dsl_diff` | 关键查询类型覆盖 |
| 五方言测试 | to_sql 按方言分派正确（MySQL/PG/SQLite/Oracle/MSSQL），JSON 操作符三方言分派 | `cargo test -p sz-orm-core --features typed-dsl --test dialect_dispatch` | 五方言行为一致 |
| typed relation 测试 | BelongsTo/HasMany/HasOne 编译期外键校验 + 与 EagerLoader 协作 | `cargo test -p sz-orm-core --features typed-relation` | 关联类型安全覆盖 |
| proc-macro SQL 验证测试 | SQL 解析 + 表/列存在性 + 类型匹配 + EXPLAIN only + 缓存 | `cargo test -p sz-orm-core --features sql-verify-proc` | 验证逻辑覆盖 |
| 基准测试 | ZST 零运行时开销（表达式构造 vs 既有 46 种表达式） | `cargo bench -p sz-orm-core --features typed-dsl` | 无开销增加 |
| 既有测试不回退 | 既有 46 种表达式测试全通过 | `cargo test -p sz-orm-core` | 6,751 基线不回退 |

### 6.1.2 方向 2：313 pub API 文档补齐

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| 文档构建 | `cargo doc --workspace --no-deps --all-features` 无警告 + docs.rs 文档完整 | `cargo doc --workspace --no-deps --all-features` | 无 missing_docs 警告 |
| Doctest | 313 pub API 文档注释中的 doctest 可运行且通过 | `cargo test --workspace --doc` | doctest 通过 |
| 文档与代码一致性 | 文档描述的参数约束/返回值/错误条件与代码实现一致 | 代码审查 + doctest | API 一致 |

### 6.1.3 方向 3：QueryBuilder 渐进合并

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| lint 检测测试 | lint 精确匹配 sz_orm_query_builder::Query 路径 + 输出告警 + 迁移建议 | `cargo test --features qb-migration-tool --test lint` | 检测准确 |
| fix 转换测试 | fix 自动转换 Query → QueryBuilder 等价代码 + --dry-run + --fix | `cargo test --features qb-migration-tool --test fix` | 转换正确 |
| 差分测试 | Query 与 QueryBuilder 生成 SQL 等价 | `cargo test --features qb-migration-tool --test diff` | 语义等价 |
| API 兼容测试 | sz-orm-query-builder deprecated 标注后 API 仍可用 | `cargo test -p sz-orm-query-builder` | API 兼容 |
| sz-pay 零回归 | sz-pay cargo check 通过（sz-orm-query-builder API 兼容） | `bash scripts/verify-sz-pay.sh` | sz-pay 零回归 |

### 6.1.4 方向 4：方言扩展

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| 单元测试 | SnowflakeDialect/RedshiftDialect Dialect trait 实现 + 特有特性（VARIANT/COPY/TIME TRAVEL/UNLOAD） | `cargo test -p sz-orm-core --features dialect-snowflake,dialect-redshift` | Dialect trait 方法覆盖 |
| 五方言行为一致 | 新方言与基础方言（PG for Redshift）行为一致 + 公共 SQL 构造一致 | `cargo test -p sz-orm-core --test dialect_consistency` | 行为一致 |
| 真实集成测试 | 真实 Snowflake/Redshift 云数据库集成（标注 `#[ignore]`） | `cargo test --features dialect-snowflake,dialect-redshift -- --ignored` | 关键场景覆盖 |
| 既有测试不回退 | 既有 18 种方言测试全通过 | `cargo test -p sz-orm-core` | 6,751 基线不回退 |

### 6.1.5 方向 5：async trait 风格统一重评估

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| 性能基准 | 方案 A/B/C 编译时间 + 运行时开销对比 | `cargo bench -p sz-orm-core` | 基准数据可复现 |
| 渐进迁移测试 | 每阶段迁移后全量测试 + sz-pay 零回归（如推荐迁移） | `cargo test --workspace + sz-pay cargo test` | 每阶段零回归 |
| 既有测试不回退 | 既有 trait 签名不变（评估期内） | `cargo test --workspace` | 6,751 基线不回退 |

## 6.2 五方言集成测试覆盖

**五方言覆盖约束**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL，所有新能力必须保持五方言行为一致（spec §1.2.12）。

**五方言集成测试矩阵**：

| 新能力 | MySQL | PostgreSQL | SQLite | Oracle | MSSQL | 测试命令 |
|--------|-------|------------|--------|--------|-------|---------|
| CTE 表达式 | ✅（8.0+） | ✅ | ✅（3.8.3+） | ✅ | ✅ | `cargo test -p sz-orm-core --features typed-dsl --test integration_*` |
| Window Frame 表达式 | ✅（8.0+） | ✅ | ✅（3.25+） | ✅ | ✅ | `cargo test -p sz-orm-core --features typed-dsl --test integration_*` |
| JSON 操作符 | ✅（5.7+） | ✅ | ✅（3.9+） | ❌（返回 Err） | ❌（返回 Err） | `cargo test -p sz-orm-core --features typed-dsl --test integration_*` |
| typed relation | ✅ | ✅ | ✅ | ✅ | ✅ | `cargo test -p sz-orm-core --features typed-relation --test integration_*` |
| Snowflake/Redshift 方言 | N/A | ✅（Redshift 委派 PG） | N/A | N/A | N/A | `cargo test --features dialect-snowflake,dialect-redshift` |

**五方言测试环境**（本机数据库，AGENTS.md 记录）：
- MySQL 9.6：`mysql://root:test123@127.0.0.1:3306/sz_orm_test`
- PostgreSQL 18：`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`
- SQLite：内存数据库
- Oracle 23ai Free：`127.0.0.1:1521/freepdb1`（用户 sys，密码 test123，Sysdba 权限）
- MSSQL：待配置（或通过 Docker）

## 6.3 sz-pay 零回归验证

**验证流程**（ADR-0001 不修改 sz-pay 代码）：
1. 复制 sz-pay 项目到临时验证目录（`E:\vue\test\sz-pay-upgrade-verify\`）。
2. 修改临时目录的 `Cargo.toml`，将 sz-orm-* 版本号从 3.5.0 改为 3.6.0。
3. 设置编译环境：`$env:RUST_MIN_STACK="67108864"` + `$env:CARGO_INCREMENTAL=0`。
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
| CTE/Window Frame/JSON 表达式隔离 | A. 既有 typed-dsl / B. 新增 typed-dsl-v2 | A | typed-dsl 既有，复用避免 feature 爆炸 | [Cargo.toml:56](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L56) |
| typed relation 隔离 | A. 既有 typed-dsl / B. 新增 typed-relation | B | typed relation 是独立能力（关联查询），独立 feature 便于按需启用 | [typed_ast.rs:672](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L672)（TypedSelectQuery 单表） |
| proc-macro SQL 验证隔离 | A. 既有 db-verify / B. 新增 sql-verify-proc | B | db-verify 仅验证 query! 宏，sql-verify-proc 扩展到 QueryBuilder 生态，验证范围不同 | [Cargo.toml:18](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L18)（db-verify 既有） |
| 自定义诊断实现方式 | A. proc-macro Diagnostic API / B. compile_error! 宏 | A | Diagnostic API 提供更丰富诊断信息（位置/期望/实际/建议） | [typed_ast.rs:249](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L249)（TypedExpression trait） |
| Snowflake 方言实现 | A. 独立实现 / B. 委派基础方言 | A | Snowflake 有独特特性（VARIANT/OBJECT/ARRAY + COPY INTO + TIME TRAVEL），无合适基础方言 | [dialect.rs:23](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L23)（Dialect trait） |
| Redshift 方言实现 | A. 独立实现 / B. 委派 PG + 特性扩展 | B | Redshift 基于 PG 8.0.2 扩展，委派 PG + 覆盖不兼容构造减少实现成本 | [dialect.rs:1429](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1429)（delegate_dialect_to 宏） |
| QueryBuilder 迁移 lint 注册 | A. clippy custom lint / B. 独立工具 | B | clippy custom lint 需 fork clippy，维护成本高；独立工具基于 syn/quote 可控轻量 | [query-builder/lib.rs:214](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-query-builder/src/lib.rs#L214)（deprecated 标注） |
| QueryBuilder 迁移 fix 执行 | A. 自动修改 / B. 需用户确认 | B | 安全约束（spec §5.3.1.6），fix 需用户显式确认 | [query-builder-guide.md:1](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/query-builder-guide.md#L1)（选择指南） |
| async trait 重评估方案 C | A. 立即迁移 / B. 评估后定 | B | RPITIT Send bound + dyn trait 限制需评估，评估后定推荐方案 | [pool.rs:45](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L45)（Connection trait 手动解糖） |
| crates.io 版本号 | A. 3.6.0 minor / B. 4.0.0 major | A | feature gate 隔离，向后兼容，SemVer minor | [Cargo.toml:6](file:///E:/vue/test/鲜视达/rust/sz-orm/Cargo.toml#L6)（当前 3.5.0） |
| 313 pub API 文档补齐策略 | A. 一次性补齐 / B. 分批补齐 | B | 313 API 数量大，分批补齐（核心/扩展/测试）便于管理与验证 | [lib.rs:403](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L403)（docs.rs cfg 跳过） |
| typed relation 与 EagerLoader 关系 | A. 替换 EagerLoader / B. 协作 + escape hatch | B | typed relation 适用于简单关联，EagerLoader 适用于复杂关联，协作 + escape hatch 兼顾类型安全与灵活性 | [eager_loader.rs:129](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/eager_loader.rs#L129)（EagerLoader） |

## 7.2 风险矩阵

| 风险 | 概率 | 影响 | 风险等级 | 缓解措施 |
|------|------|------|---------|---------|
| 新表达式在部分方言不支持（CTE 旧版 MySQL/Window Frame SQLite < 3.25/JSON 操作符方言差异） | 中 | 中 | 中 | to_sql 按方言+版本分派，不支持的方言返回 Err(UnsupportedFeature)，文档标注各方言版本支持矩阵 |
| 自定义诊断信息与 Rust 默认错误冲突 | 低 | 低 | 低 | 自定义诊断信息优先输出，抑制 Rust 默认错误（通过 proc-macro 展开控制） |
| typed relation 过于严格拒绝合法关联 | 中 | 中 | 中 | 提供 escape hatch（运行时关联回退 EagerLoader），文档标注适用场景 |
| proc-macro SQL 验证编译时间显著增加 | 中 | 中 | 中 | 缓存验证结果（按 SQL 哈希缓存），仅 SQL 变更时重新验证，默认关闭 |
| Snowflake Rust 驱动不成熟 | 高 | 中 | 中 | 方言实现完成（SQL 生成正确），标注"需用户自备驱动（ODBC/HTTP API）"，集成测试标注 `#[ignore]` |
| Redshift 委派 PG 不完全兼容 | 中 | 中 | 中 | RedshiftDialect 委派 PG 但覆盖不兼容的 SQL 构造（返回 Err 或 Redshift 特有语法） |
| Prisma 兼容评估结论为不可行 | 中 | 低 | 低 | 评估文档标注"不可行"及原因，不实施 Prisma 方言兼容 |
| 迁移 lint 误报 | 低 | 低 | 低 | lint 精确匹配 `sz_orm_query_builder::Query` 路径，不匹配其他库的 Query |
| 迁移 fix 转换语义不等价 | 中 | 中 | 中 | 差分测试发现不等价时标注"需人工审查"，不自动转换复杂场景 |
| sz-pay 使用 sz-orm-query-builder 导致 deprecated 告警 | 中 | 低 | 低 | 告警为 `#[deprecated]` 标准告警，不影响编译通过，sz-pay 可选择迁移或忽略 |
| async trait 迁移后 sz-pay 回归 | 低 | 高 | 中 | 回退该 trait 迁移，分析失败原因，修复后再迁移或维持现状 |
| Rust 最新进展仍不解决 Send bound 限制 | 中 | 低 | 低 | 评估文档标注"Send bound 限制未完全解决，维持 v3.5.0 现状"，推荐方案为不改 |

## 7.3 实施依赖与顺序

**实施顺序**（按优先级 + 依赖关系）：

1. **M1 方向 1 编译期类型安全深入优化**（3 周，最高优先级，关键路径起点）
   - 先实施 CTE/Window Frame/JSON 操作符表达式（基础）
   - 再实施自定义编译期诊断（依赖表达式类型不匹配触发）
   - 再实施 typed relation（依赖 typed_ast 基础）
   - 最后探索 proc-macro SQL 验证（依赖 db-verify 基础）

2. **M2 方向 2 313 pub API 文档补齐**（2 周，高优先级，依赖 M1 新表达式 API）
   - M1 完成后开始（新表达式 API 需同步补齐文档）
   - 分批补齐：核心 API → 扩展 API → 测试/辅助 API

3. **M3 方向 3 QueryBuilder 渐进合并**（2 周，中优先级，依赖 M1 feature gate）
   - M1 完成后可并行开始
   - 先开发 lint，再开发 fix，最后差分测试 + 路线图

4. **M4 方向 4 方言扩展**（2 周，中优先级，依赖 M1 feature gate）
   - M1 完成后可并行开始
   - Snowflake + Redshift 可并行，Prisma 评估独立

5. **M5 方向 5 async trait 重评估**（1 周，低优先级，依赖 M1 feature gate）
   - M1 完成后可并行开始
   - 纯评估工作，不依赖其他方向

**关键路径**：M1 → M2（串行 5 周）
**并行机会**：M1 完成后 M3/M4/M5 可并行（2 周并行）
**总周期**：关键路径 5 周；并行开发下可压缩至 4-5 周

---

> 本文档为 sz-orm v3.6.0 技术设计文档，基于 v3.5.0 已验收基线（6,751 passed / 0 failed / 253 ignored）+ 用户 5 项深入优化请求 + 对比分析文档剩余不足 + v3.5.0 方言扩展路线图生成。所有改进通过 feature gate 隔离，保证既有 API 完全向后兼容与测试基线不回退。
> 生成日期：2026-08-10
> 基线版本：v3.5.0（6,751 passed / 0 failed / 253 ignored，44 包已发布 crates.io）
> 目标版本：v3.6.0
> 需求总数：37 条（REQ-TS-001~009 + REQ-DOC-API-001~007 + REQ-QB-MIG-001~007 + REQ-DIALECT-001~008 + REQ-ASYNC-001~006）
> 设计方向：5 个（编译期类型安全深入优化 / 313 pub API 文档补齐 / QueryBuilder 渐进合并 / 方言扩展 / async trait 重评估）
> 里程碑：5 个（M1~M5，关键路径 5 周，并行开发可压缩至 4-5 周）
> Feature Gate：6 个新增（typed-relation/sql-verify-proc/dialect-snowflake/dialect-redshift/dialect-prisma/qb-migration-tool）+ 2 个既有复用（typed-dsl/doc-completion）
> 新增表达式：15 种（CTE 3 + Window Frame 6 + JSON 操作符 6），目标超越 Diesel 表达式覆盖度
> 新增方言：2 种（Snowflake + Redshift），总计 20 种方言，对齐竞品（Hibernate 20+/SQLAlchemy 20+）
