# sz-orm v3.5.0 技术设计文档

> 版本：v3.5.0（已知不足改进 + 文档同步约束规则化 + typed_ast DSL 补齐 + 无锁连接池架构文档 + 方言扩展规划 + L1 缓存设计 + crates.io 发布流程 + async trait 风格统一 + QueryBuilder 合并 + MOCK-ONLY 包补齐）
> 基线：v3.4.0（已完成：测试覆盖补齐 + 架构改进 + 性能优化落地 + 编译期类型安全增强 + 文档与生态建设 + sz-pay 生产案例深化，6,738 passed / 0 failed / 253 ignored）
> 日期：2026-08-09
> 文档定位：技术设计（How to build），对应需求规格 `docs/spec/v3.5.0/spec.md`（10 方向 / 60 条 EARS 需求 / 10 组 REQ-DOC-SYNC/REQ-DSL/REQ-POOL-DOC/REQ-DIALECT/REQ-L1CACHE/REQ-PUBLISH/REQ-ASYNC/REQ-QB-MERGE/REQ-MOCK/REQ-DOC-FILL）
> 设计约束：Rust 2021 Edition / rust-version 1.81 / API 向后兼容（无 Breaking Change）/ 禁止占位实现 / unsafe 零容忍 / 参数化查询铁律 / Feature 隔离 / 五方言行为一致 / ADR-0001 严禁修改下游/上游仓库 / 编译时 `$env:RUST_MIN_STACK="67108864"` + `$env:CARGO_INCREMENTAL=0`
> 优先级声明：十项能力按"文档同步约束(1,最高) → crates.io 发布(2) → typed_ast DSL 补齐(3) → L1 缓存(4) → MOCK-ONLY 补齐(5) → async trait 统一(6) → QueryBuilder 合并(7) → 方言扩展规划(8) → 无锁连接池文档(9) → 文档与迁移指南补齐(10)"的收益/风险序推进；前两项为最高收益低风险（约束规则 + 发布流程，不引入新代码依赖），DSL/L1 缓存/MOCK 为中收益中风险（新功能模块，需 feature gate 隔离），async trait/QB 合并为中收益中风险（重构，需兼容性评估），方言扩展为低收益高风险（企业数据库需求不明朗），文档类为低风险高收益
> 缺陷来源：`docs/sz-orm与同类产品对比分析.md` §6 已知不足（10 项）+ §7.3 改进建议（7 项）+ 用户 7 项请求

---

# 一、需求与存量功能关系分析

## 1.1 需求功能与存量功能对比

v3.5.0 的十项已知不足改进任务与 v3.4.0 已交付代码的关系如下。v3.4.0 已完成测试覆盖补齐、架构改进、性能优化落地、编译期类型安全增强、文档与生态建设、sz-pay 生产案例深化六项能力，workspace 版本 3.4.0，6,738 测试全部通过。本版本在此基础上向"文档一致性保证、编译期类型安全成熟度、缓存层级完整性、后端实现真实性、代码风格统一性、API 歧义消除、方言覆盖度、架构知识沉淀、采用门槛"九个维度能力突破，所有新增能力以扩展模块 + feature gate 方式提供，不修改 sz-orm-core / sz-orm-macros / 扩展包既有公开 API 签名（满足 spec §4.5 兼容性约束：既有公开 API 完全向后兼容）。

### 1.1.1 已实现功能

| 需求功能 | 存量功能 | 代码位置 | 匹配度 |
|---------|---------|---------|--------|
| 门禁 12 文档与代码一致性检查（方向 1 基础） | 已有 `scripts/check-doc-consistency.py` 检查版本号/包数量数据一致性 | [docs/sz-orm-engineering-practices.md:269](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/sz-orm-engineering-practices.md#L269) | 50%（数据一致性已有，代码变更触发文档更新检查缺失） |
| typed_ast.rs 11 种表达式（方向 2 基础） | Eq/Ne/Lt/Gt/Le/Ge/And/Or/Like/In/Not 共 11 种 ZST 表达式 + TypedExpression trait + ExprTable trait | [packages/sz-orm-core/src/typed_ast.rs:397](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L397)（Eq）至 [packages/sz-orm-core/src/typed_ast.rs:883](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L883)（Not） | 100%（11 种已有，需补 46 种） |
| SqlType 类型标记体系（方向 2 基础） | Bool/Integer/SmallInt/BigInt/Real/Double/Text/Date/DateTime/Json/Uuid/Binary/Nullable/Untyped 共 14 种 SqlType ZST | [packages/sz-orm-core/src/typed_ast.rs:69](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L69)（SqlType trait）至 [packages/sz-orm-core/src/typed_ast.rs:129](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L129)（Untyped） | 100%（类型基础已有） |
| 无锁连接池实现（方向 3 基础） | ArrayQueue（crossbeam-queue 无锁 MPMC）+ AtomicU32（无锁原子计数）+ Notify（异步通知）+ AtomicBool（closed 标志） | [packages/sz-orm-core/src/pool.rs:751](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L751)（idle: ArrayQueue）+ [packages/sz-orm-core/src/pool.rs:761](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L761)（total_count: AtomicU32）+ [packages/sz-orm-core/src/pool.rs:764](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L764)（notify: Notify） | 100%（实现完整，缺架构文档） |
| 8 独立方言实现（方向 4 基础） | MySqlDialect/PostgreSqlDialect/SqliteDialect/OracleDialect/SqlServerDialect/ClickHouseDialect/DuckDBDialect/Db2Dialect 各自实现 Dialect trait | [packages/sz-orm-core/src/dialect.rs:228](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L228)（MySqlDialect）至 [packages/sz-orm-core/src/dialect.rs:1991](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1991)（Db2Dialect impl） | 100%（8 独立方言完整） |
| 8 兼容方言委派（方向 4 基础） | delegate_dialect_to 宏委派：MariaDB/TiDB→MySQL、KingbaseES/PolarDB/GaussDB→PG、Dameng→Oracle、Sybase/GBase→SQL Server | [packages/sz-orm-core/src/dialect.rs:1496](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1496)（MariaDB）至 [packages/sz-orm-core/src/dialect.rs:1517](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1517)（GBase） | 100%（8 兼容方言完整） |
| DbType 枚举 19 变体（方向 4 基础） | MySQL/PostgreSQL/Sqlite/Redis/MongoDB/ClickHouse/Oracle/OceanBase/SqlServer/VectorDb/PureJsDb/Dameng/Kingbase/Db2/MariaDB/TiDB/PolarDB/GaussDB/GBase/Sybase/DuckDB | [packages/sz-orm-core/src/db_type.rs:11](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/db_type.rs#L11) | 100%（19 变体完整，#[non_exhaustive] 允许扩展） |
| L2Cache 跨 Session 缓存（方向 5 基础） | L2Cache + InMemoryBackend + RedisBackend + InvalidationBus + LocalInvalidationBus + WriteBehindWriter + LRU 淘汰 + 表级失效 + 跨实例失效 | [packages/sz-orm-core/src/l2_cache.rs:517](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/l2_cache.rs#L517)（L2Cache struct） | 100%（L2 完整，缺 L1） |
| crates.io 发布脚本（方向 6 基础） | 已有 `scripts/publish_all.py`、`scripts/publish_crates_io.ps1`、`scripts/compute_topology.ps1`、`scripts/publish_v120.ps1`/`publish_v121.ps1` | [scripts/publish_all.py:1](file:///E:/vue/test/鲜视达/rust/sz-orm/scripts/publish_all.py#L1) + [scripts/compute_topology.ps1:1](file:///E:/vue/test/鲜视达/rust/sz-orm/scripts/compute_topology.ps1#L1) | 75%（脚本已有，缺 v3.5.0 拓扑发布 + dry-run + sz-pay 验证流程） |
| Connection trait 手动解糖（方向 7 基础） | Connection trait 手动解糖 async 方法（`fn execute<'a>(&'a mut self, sql: &'a str) -> Pin<Box<dyn Future + Send + 'a>>`），注释明确说明为避免 HRTB 与 sqlx::Executor 冲突 | [packages/sz-orm-core/src/pool.rs:45](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L45) | 100%（手动解糖完整，需评估统一） |
| `#[async_trait]` 使用（方向 7 基础） | ConnectionFactory trait（pool.rs:732）+ Model trait（model.rs:271）+ 多处 impl 使用 `#[async_trait]` | [packages/sz-orm-core/src/pool.rs:732](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L732)（ConnectionFactory）+ [packages/sz-orm-core/src/model.rs:271](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/model.rs#L271)（Model） | 100%（混用现状完整） |
| core::QueryBuilder（方向 8 基础） | QueryBuilder<M: Model> 链式 SQL 构造器，绑定 Model，编译期表/列校验，4295 行 | [packages/sz-orm-core/src/query.rs:36](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L36) | 100%（完整） |
| sz-orm-query-builder（方向 8 基础） | Query 独立 SQL 构造器（sea-query 风格，不绑定 Model），4185 行，已有区别说明表格 | [packages/sz-orm-query-builder/src/lib.rs:53](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-query-builder/src/lib.rs#L53) | 100%（完整，缺选择指南） |
| sz-orm-es Mock 实现（方向 9 基础） | InMemoryEsSync（HashMap 存储）+ EsSync trait + EsSyncManager，lib.rs 顶部明确标注 MOCK-ONLY | [packages/sz-orm-es/src/lib.rs:1](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-es/src/lib.rs#L1) | 100%（Mock 完整，缺真实 ES） |
| sz-orm-es `real` feature 占位（方向 9 基础） | Cargo.toml 已声明 `real = []` feature（占位，未实现真实后端） | [packages/sz-orm-es/Cargo.toml:15](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-es/Cargo.toml#L15) | 50%（feature 占位已有，真实实现缺失） |
| sz-orm-config 内存实现（方向 9 基础） | ConsulConfigCenter（HashMap + subscribers + events）+ ConfigCenter trait | [packages/sz-orm-config/src/lib.rs:47](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-config/src/lib.rs#L47) | 100%（内存完整，缺真实 Consul/Nacos） |
| sz-orm-config `real-consul`/`real-nacos` feature（方向 9 基础） | Cargo.toml 已声明 feature + reqwest optional 依赖 | [packages/sz-orm-config/Cargo.toml:14](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-config/Cargo.toml#L14) | 50%（feature + 依赖已有，真实实现缺失） |
| docs.rs cfg 跳过（方向 10 基础） | `#![cfg_attr(docsrs, warn(missing_docs))]`，313 个 pub API 缺 `///` 文档 | [packages/sz-orm-core/src/lib.rs:406](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L406) | 0%（需补齐 313 文档 + 移除跳过） |
| sz-orm-core feature 体系（基础） | default=["redis"]，含 22 个 feature（testing/db-verify/redis/circuit-breaker/rate-limit/auto-prewarm/plan-cache/zero-copy/simd/multi-tenant-enhanced/dist-cache/test-coverage/arch-improvement/doc-completion/perf-smallstring/perf-enum-dispatch/perf-zero-copy-l2/perf-box-str/type-safe-columns/typed-column/typed-dsl/migration-guide） | [packages/sz-orm-core/Cargo.toml:13](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L13) | 100%（22 feature 完整，typed-dsl 既有） |
| workspace 版本集中管理 | `workspace.package.version = "3.4.0"`，edition="2021"，rust-version="1.81" | [Cargo.toml:6](file:///E:/vue/test/鲜视达/rust/sz-orm/Cargo.toml#L6) | 100%（需升级到 3.5.0） |
| sz-pay 生产依赖证据 | sz-pay 从 crates.io 拉取 sz-orm-core/sqlx/config/auth/macros/queue/scheduler 共 7 个包 2.3.0 版本 | `E:\vue\test\sz-pay\server\sz-rust\Cargo.toml` 第 27-33 行 | 100%（需升级到 3.5.0） |
| crates.io token | token = [REDACTED] | `E:\vue\test\鲜视达\服务器信息.md` | 100%（凭证已有） |

### 1.1.2 需要扩展的功能

| 需求功能 | 存量功能 | 差异说明 | 扩展方向 |
|---------|---------|---------|---------|
| 门禁 14 文档同步更新检查（REQ-DOC-SYNC-001~004） | 门禁 12 仅检查数据一致性，不检查代码变更触发文档更新 | 检查维度差异：门禁 12 检查版本号/包数量，门禁 14 检查代码变更是否触发文档更新；触发时机差异：门禁 12 任何时候，门禁 14 仅代码变更时 | 新增 `scripts/check-doc-sync.py` 脚本（分析 git diff → 受影响文档清单 → 检查文档是否修改），AGENTS.md/engineering-practices.md 新增门禁 14 章节，CI 配置新增 check-doc-sync job |
| typed_ast.rs 46 种新增表达式（REQ-DSL-001~010） | 11 种基础表达式（Eq/Ne/Lt/Gt/Le/Ge/And/Or/Like/In/Not），缺聚合/算术/字符串/日期/窗口/NULL 处理/BETWEEN/DISTINCT/子查询/类型转换 | 表达式类别差异：Diesel 支持 50+ 表达式，SZ-ORM 仅 11 种；类型检查差异：算术表达式需 SqlType 兼容检查；方言差异：ILike/DistinctOn 仅 PG 支持 | typed_ast.rs 新增 46 种 ZST 表达式 + TypedExpression trait 实现 + to_sql 参数化生成，通过 `typed-dsl` feature gate 隔离（[Cargo.toml:56](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L56) 既有），既有 11 种表达式不变 |
| 无锁连接池架构文档（REQ-POOL-DOC-001~004） | pool.rs 实现完整，缺详细架构文档解释工作原理/数据结构/并发安全证明/竞品对比 | 文档差异：实现有注释但无独立架构文档；证明差异：缺线性化保证/内存序选择论证；对比差异：缺与 Diesel r2d2/SeaORM/SQLx 的对比 | 新增 `docs/lock-free-pool-design.md` 架构文档，含工作原理/数据结构图解/acquire-release 时序图/并发安全证明/竞品对比，每条结论附 pool.rs file:line 证据 |
| 方言扩展规划（REQ-DIALECT-001~006） | 16 种方言（8 独立 + 8 兼容）已实现，缺未实现方言清单/必要性评估/路线图 | 覆盖度差异：Hibernate 20+/SQLAlchemy 20+，SZ-ORM 16；规划差异：缺"必须/建议/暂不需要"分类；路线图差异：缺版本里程碑 | 列出未实现方言（Informix/SAP HANA/Firebird/CockroachDB/YugabyteDB/Snowflake/Redshift/Vertica/Teradata 等），按使用场景/Rust 驱动/实现难度/市场趋势分类评估，对"必须实现"方言生成 EARS 需求 + Dialect trait 实现 + DbType 变体 + 测试，路线图写入 spec.md |
| L1 缓存 Identity Map（REQ-L1CACHE-001~007） | L2Cache 跨 Session 共享完整，缺 L1Cache Session 级别 Identity Map | 层级差异：Hibernate/EF Core/SQLAlchemy 均有 L1+L2 双层，SZ-ORM 仅 L2；生命周期差异：L1 与 Session 绑定，L2 跨 Session；查询顺序差异：L1→L2→DB | sz-orm-core 新增 `l1_cache.rs` 模块（HashMap<TableKey, HashMap<PkValue, Arc<Mutex<Entity>>>> + LRU + AtomicU64 统计），通过 `l1-cache` feature gate 隔离（新增），与既有 L2Cache 协作（L1 未命中查 L2，L2 未命中查 DB，命中回填），既有 L2Cache API 不变 |
| crates.io v3.5.0 拓扑发布（REQ-PUBLISH-001~007） | 已有 publish_all.py/compute_topology.ps1 发布脚本，sz-orm-core 1.0.0 曾发布，当前 3.4.0 未发布 | 版本差异：当前 3.4.0 未发布，目标 3.5.0；流程差异：缺 dry-run 验证 + sz-pay 升级零回归验证；拓扑差异：需重新计算 43 包拓扑 | 升级 workspace.package.version = 3.5.0，内部依赖版本同步 3.5.0，按拓扑顺序（macros/sql-validator → core → sqlx/oracle/mssql/扩展包）执行 `cargo publish --dry-run` 验证后实际发布，sz-pay 本地修改版本号验证零回归（不提交，ADR-0001） |
| async trait 风格统一评估（REQ-ASYNC-001~004） | Connection trait 手动解糖（pool.rs:45），其他 trait 用 `#[async_trait]`，混用 | 评估差异：缺方案 A/B 优缺点对比 + 性能基准 + 迁移影响分析；推荐差异：缺推荐方案；迁移差异：缺渐进迁移方案 | 输出评估文档（方案 A 统一 `#[async_trait]` / 方案 B 统一手动解糖 优缺点 + 性能基准 + 迁移影响 + 推荐方案），按推荐方案渐进迁移（每阶段少量 trait + 全量测试 + sz-pay 零回归），既有 Connection trait 签名在评估期内不变 |
| QueryBuilder 合并评估（REQ-QB-MERGE-001~004） | core::QueryBuilder（query.rs:36）+ sz-orm-query-builder（lib.rs:53）两个 QueryBuilder 重叠 | 评估差异：缺方案 A/B 优缺点对比 + API 兼容 + 用户迁移成本；指南差异：缺选择指南；合并差异：缺渐进合并/deprecation 方案 | 输出评估文档 + 选择指南（能力对比表/适用场景/性能基准/迁移建议），按推荐方案渐进合并或 deprecation（v3.5.0 不立即删除 sz-orm-query-builder，标注 deprecated + 委派实现保持 API 兼容） |
| sz-orm-es 真实 ES 后端（REQ-MOCK-001~005） | Mock 完整 + `real` feature 占位，缺真实 ES 实现 | 实现差异：缺真实 Elasticsearch 客户端（reqwest/elasticsearch crate）；测试差异：缺真实 ES 集成测试 + Mock/真实差分测试 | sz-orm-es Cargo.toml 新增 elasticsearch/reqwest optional 依赖（`real` feature 启用），实现真实 ES 后端（索引/搜索/聚合/过滤），编写真实 ES 集成测试（`#[cfg(feature="real")]` + `#[ignore]`）+ Mock/真实差分测试，默认 Mock 行为不变 |
| sz-orm-config 真实 Consul/Nacos 后端（REQ-MOCK-001~005） | 内存实现完整 + `real-consul`/`real-nacos` feature + reqwest optional 依赖已有，缺真实实现 | 实现差异：缺真实 Consul/Nacos HTTP API 客户端；测试差异：缺真实集成测试 + 差分测试 | sz-orm-config 实现真实 Consul/Nacos 后端（基于 reqwest HTTP API，配置读写/监听/服务发现），编写真实集成测试 + 差分测试，默认内存行为不变 |
| 313 个 pub API 文档补齐（REQ-DOC-FILL-001~005） | docs.rs cfg 跳过 missing_docs，313 个 pub API 缺 `///` 文档 | 文档差异：缺 313 个 `///` 注释；配置差异：需移除 docs.rs cfg 跳过；约束差异：API 签名不变 | 逐批补齐 313 个 pub API 文档（功能/参数/返回/示例/错误），移除 [lib.rs:406](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L406) docs.rs cfg 跳过，改为全局 `#![warn(missing_docs)]`，API 签名不变 |
| Diesel/SeaORM/SQLx 迁移指南（REQ-DOC-FILL-001~005） | 迁移指南尚未完成 | 内容差异：缺概念映射表/API 对照表/示例代码/迁移注意事项 | 新增 `docs/migration/diesel-to-sz-orm.md`、`docs/migration/seaorm-to-sz-orm.md`、`docs/migration/sqlx-to-sz-orm.md` 三份迁移指南，通过 `migration-guide` feature gate 隔离（[Cargo.toml:58](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L58) 既有） |

### 1.1.3 需要新增的功能或接口

按业务模块分组，以下功能在存量代码中完全没有对应实现，需新增。

**模块 A：文档同步约束规则化（对应 REQ-DOC-SYNC-001~004）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| `scripts/check-doc-sync.py` 脚本 | git diff 输出（变更文件清单） | 受影响文档清单 + 退出码（0 通过/非 0 阻断） | 解析 git diff → 按受影响文档映射规则确定应更新文档清单 → 检查文档是否在本次变更中修改 → 未修改则输出告警 + 阻断 | 无（纯 Python 脚本） |
| 受影响文档映射规则 | 代码变更文件类型 | 应更新文档清单 | 修改 `packages/sz-orm-core/src/*.rs` 公开 API → spec.md/design.md/对比分析文档；修改 Cargo.toml workspace 包列表 → AGENTS.md/engineering-practices.md；修改 feature 列表 → engineering-practices.md feature 矩阵；修改 pool.rs/dialect.rs/l2_cache.rs → 对比分析文档对应章节；版本号变更 → 所有含版本号文档 | 无（纯规则文档） |
| CI check-doc-sync job | PR 代码变更 | 门禁通过/阻断 | CI 配置新增 check-doc-sync job，调用 `python scripts/check-doc-sync.py`，退出码非 0 阻断 PR | 无（CI 配置） |

**模块 B：typed_ast DSL 46 种新增表达式（对应 REQ-DSL-001~010）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| 聚合表达式 Max/Min/Sum/Avg/Count/CountStar | TypedColumn | ZST 表达式 + TypedExpression impl + to_sql | 每个表达式为 ZST + TypedExpression trait 实现 + to_sql 生成 `MAX(col)`/`MIN(col)`/`SUM(col)`/`AVG(col)`/`COUNT(col)`/`COUNT(*)` | 无（复用既有 typed_ast 基础） |
| 算术表达式 Add/Sub/Mul/Div/Modulo | 两个 TypedExpression | ZST 表达式 + 类型检查 + to_sql | 类型检查要求操作数 SqlType 兼容（Integer+Integer=Integer），to_sql 生成 `col1 + col2` 等，编译期拒绝不兼容类型 | 无 |
| 字符串表达式 Concat/ILike/Length/Lower/Upper/Trim/Substring | TypedColumn + 参数 | ZST 表达式 + to_sql | to_sql 生成对应 SQL 函数，ILike 在 PG 生成 `ILIKE`、其他方言回退 `LIKE` | 无 |
| 日期表达式 Extract/Year/Month/Day/Hour/Minute/Second/Now | TypedColumn + 字段 | ZST 表达式 + to_sql | to_sql 生成 `EXTRACT(field FROM col)`/`YEAR(col)` 等 | 无 |
| 窗口表达式 Over/PartitionBy/OrderByInWindow/Lag/Lead/RowNumber/Rank/DenseRank | TypedExpression + Partition/Order | ZST 表达式 + to_sql | to_sql 生成 `ROW_NUMBER() OVER (PARTITION BY ... ORDER BY ...)` 等，按方言版本分派 | 无 |
| NULL 处理表达式 IsNull/IsNotNull/Coalesce/NullIf | TypedExpression | ZST 表达式 + to_sql | to_sql 生成 `col IS NULL`/`col IS NOT NULL`/`COALESCE(col, default)`/`NULLIF(col1, col2)` | 无 |
| BETWEEN/DISTINCT/子查询表达式 Between/NotBetween/Distinct/DistinctOn/Exists/InSubquery | TypedExpression + 范围/子查询 | ZST 表达式 + to_sql | to_sql 生成对应 SQL，DistinctOn 在 PG 生成 `DISTINCT ON (col)`、其他方言返回 Err | 无 |
| 类型转换表达式 Cast/As | TypedExpression + 目标 SqlType | ZST 表达式 + to_sql | to_sql 生成 `CAST(col AS type)`，PG 支持 `col::type` 语法 | 无 |
| 编译期 ZST 断言 | 表达式类型 | 编译期断言通过/失败 | `static_assert!(size_of::<T>() == 0)` 确保新增表达式为 ZST | 无 |

**模块 C：无锁连接池架构文档（对应 REQ-POOL-DOC-001~004）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| `docs/lock-free-pool-design.md` 架构文档 | pool.rs 实现 | 架构文档 | 工作原理（ArrayQueue+AtomicU32+Notify）+ 数据结构图解 + acquire/release 时序图 + 并发安全证明（线性化+内存序）+ 竞品对比（Diesel r2d2/SeaORM/SQLx） | 无（纯文档） |

**模块 D：方言扩展规划（对应 REQ-DIALECT-001~006）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| 未实现方言清单 + 竞品对比 | Hibernate/EF Core/SQLAlchemy 方言清单 | 未实现方言清单 + 竞品支持矩阵 | 差集计算：竞品支持但 SZ-ORM 未实现的方言 | 无（纯分析） |
| 必要性评估分类表 | 未实现方言清单 | 分类表（必须/建议/暂不需要） | 按使用场景/Rust 驱动/实现难度/市场趋势评估 | 无（纯评估） |
| 必须实现方言 Dialect trait 实现 | 方言名 + SQL 语法规则 | Dialect trait impl + DbType 变体 + 测试 | 实现新方言的 Dialect trait（quote/escape_string/build_pagination 等），新增 DbType 枚举变体，编写方言测试 | Rust 驱动（按方言） |
| 方言扩展路线图 | 分类表 + 评估理由 | 路线图（版本里程碑） | 必须实现方言按版本里程碑排列，建议实现方言标注触发条件，暂不需要方言标注理由 | 无 |

**模块 E：L1 缓存设计（对应 REQ-L1CACHE-001~007）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| `packages/sz-orm-core/src/l1_cache.rs` 模块 | Session + 实体 | L1Cache 类型 + Identity Map | `HashMap<TableKey, HashMap<PkValue, Arc<Mutex<Entity>>>>` 按表分桶按主键缓存，同主键同引用（Identity Map），LRU 淘汰 + max_size 配置 | 无（复用既有 HashMap + parking_lot） |
| L1 缓存生命周期管理 | Session 创建/销毁 | L1 自动清空 | Session 创建时 L1 为空，Session 结束（close/drop）时 L1 自动清空，L1 不跨 Session | 无 |
| L1→L2→DB 查询协作 | 查询请求 | 实体对象 | L1 命中直接返回；L1 未命中查 L2，L2 命中回填 L1 返回；L2 未命中查 DB，DB 命中回填 L1+L2 返回 | 既有 L2Cache（[l2_cache.rs:517](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/l2_cache.rs#L517)） |
| L1 缓存失效策略 | 写操作/手动 evict | L1 条目失效 | 写操作（insert/update/delete）自动 evict 对应 L1 条目，手动 evict(entity)/evict_table(table)/clear()，Session 结束全清 | 无 |
| L1 缓存统计 | 缓存操作 | hit/miss/entry/evict 计数 | AtomicU64 无锁原子计数，支持命中率监控 | 无 |

**模块 F：crates.io 发布流程（对应 REQ-PUBLISH-001~007）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| `scripts/publish-workspace.sh` 发布脚本 | workspace 各包 | 拓扑发布到 crates.io | 拓扑排序（macros/sql-validator → core → sqlx/oracle/mssql/扩展包）→ 每包 `cargo publish --dry-run` 验证 → 实际发布 → 验证 crates.io 页面可访问 | 既有 publish_all.py/compute_topology.ps1 |
| dry-run 验证流程 | 每包元数据 | dry-run 通过/失败 | 检查包元数据（name/version/description/license/repository）+ 依赖可解析 + 无 secrets + 包大小合理 | 无 |
| sz-pay 升级零回归验证 | sz-pay Cargo.toml | 零回归验证结果 | 本地修改 sz-orm-* 版本号为 3.5.0（不提交，ADR-0001）→ cargo check + cargo test → 验证零回归 | sz-pay 项目（仅验证不改） |
| secrets 预检脚本 | 发布产物 | 预检通过/阻断 | 扫描 .env/credentials/token/私钥模式，发现则阻断发布 | 无 |

**模块 G：async trait 风格统一（对应 REQ-ASYNC-001~004）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| async trait 评估文档 | Connection trait + 其他 trait | 评估文档 + 推荐方案 | 方案 A（统一 `#[async_trait]`）/ 方案 B（统一手动解糖）优缺点 + 性能基准 + 迁移影响 + 学习成本 + 推荐 | 无（纯评估文档） |
| 渐进迁移方案 | 推荐方案 + trait 清单 | 分阶段迁移计划 | 每阶段迁移少量 trait + 全量测试 + sz-pay 零回归，不一次性迁移 | 无 |

**模块 H：QueryBuilder 合并（对应 REQ-QB-MERGE-001~004）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| QueryBuilder 合并评估文档 | 两个 QueryBuilder | 评估文档 + 推荐方案 | 方案 A（合并，委派）/ 方案 B（保留+指南+deprecation）优缺点 + API 兼容 + 用户迁移成本 + 性能基准 + 推荐 | 无（纯评估文档） |
| 选择指南 | 两个 QueryBuilder 能力 | 选择指南文档 | 能力对比表（查询类型/方言/特性）+ 适用场景 + 性能基准 + 迁移建议 | 无 |
| 渐进合并/deprecation | 推荐方案 | 委派实现或 deprecated 标注 | 若推荐合并：sz-orm-query-builder 委派 core::QueryBuilder；若推荐保留：标注 deprecated + 指向选择指南，v3.5.0 不立即删除 | 无 |

**模块 I：MOCK-ONLY 包真实后端（对应 REQ-MOCK-001~005）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| sz-orm-es 真实 ES 后端 | ES API | 真实 ES 实现 | 基于 elasticsearch crate 或 reqwest 调用 ES API，实现索引/搜索/聚合/过滤，通过 `real` feature gate 隔离 | elasticsearch crate 或 reqwest（optional，`real` feature） |
| sz-orm-config 真实 Consul 后端 | Consul HTTP API | 真实 Consul 实现 | 基于 reqwest 调用 Consul HTTP API（配置读写/监听/服务发现），通过 `real-consul` feature gate 隔离 | reqwest（optional，已有） |
| sz-orm-config 真实 Nacos 后端 | Nacos HTTP API | 真实 Nacos 实现 | 基于 reqwest 调用 Nacos HTTP API，通过 `real-nacos` feature gate 隔离 | reqwest（optional，已有） |
| Mock 与真实差分测试 | 相同输入 | Mock 与真实输出语义一致 | 对相同输入验证 Mock 与真实后端输出语义一致，确保切换 feature 不改变业务逻辑 | 无 |

**模块 J：文档与迁移指南补齐（对应 REQ-DOC-FILL-001~005）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| 313 个 pub API 文档补齐 | 缺文档的 pub API | `///` 文档注释 | 逐批补齐功能/参数/返回/示例/错误文档，移除 docs.rs cfg 跳过 | 无（纯文档） |
| Diesel 迁移指南 | Diesel API 对照 | 迁移指南文档 | 概念映射（Diesel schema → sz-orm Model）+ API 对照（Diesel query → sz-orm QueryBuilder）+ 示例代码 + 迁移注意事项 | 无（纯文档） |
| SeaORM 迁移指南 | SeaORM API 对照 | 迁移指南文档 | 概念映射（SeaORM Entity → sz-orm Model）+ API 对照 + 示例 + 注意事项 | 无 |
| SQLx 迁移指南 | SQLx API 对照 | 迁移指南文档 | 概念映射（SQLx query! → sz-orm query! 宏）+ API 对照 + 示例 + 注意事项 | 无 |

## 1.2 存量功能详细分析

### 1.2.1 门禁 12 文档与代码一致性检查（engineering-practices.md:269）

- **接口契约**：`scripts/check-doc-consistency.py` 脚本，输入无（读取 AGENTS.md/engineering-practices.md/Cargo.toml），输出退出码 0（一致）/非 0（不一致）+ 不一致清单。CI Job 名 `check-doc-consistency`。
- **业务规则**：检查 AGENTS.md 中的版本号、包数量，engineering-practices.md 中的项目版本、workspace 包数量，所有数据必须与实际 Cargo.toml 一致。支持 `--fix` 自动修复文档中的不一致数据。
- **扩展点**：门禁 12 检查"数据一致性"（静态数据匹配），门禁 14（v3.5.0 新增）检查"代码变更触发文档更新"（动态变更触发），两者互补。门禁 14 复用门禁 12 的 CI Job 模式，但检查逻辑不同。
- **约束**：门禁 12 仅检查静态数据（版本号/包数量），不检查代码变更是否触发文档更新；门禁 14 需分析 git diff，确定代码变更类型，按映射规则检查文档是否同步更新。

### 1.2.2 typed_ast.rs 11 种表达式 + SqlType 类型体系（typed_ast.rs:69-919）

- **接口契约**：
  - `SqlType` trait（typed_ast.rs:69）：所有 SQL 类型标记的基 trait，要求 `'static`，实现者应为 ZST（unit struct）。
  - 14 种 SqlType ZST：Bool/Integer/SmallInt/BigInt/Real/Double/Text/Date/DateTime/Json/Uuid/Binary/Nullable<T>/Untyped（typed_ast.rs:72-129）。
  - `TypedExpression` trait（typed_ast.rs:249）：所有表达式基 trait，关联类型 `SqlType`，方法 `to_sql(&self, dialect: &dyn Dialect) -> String`。
  - `ExprTable` trait（typed_ast.rs:616）：表达式所属表的关联类型，用于跨表列引用检查。
  - 11 种表达式 ZST：Eq/Ne/Lt/Gt/Le/Ge（比较，typed_ast.rs:397-522）/And/Or（逻辑组合，typed_ast.rs:549-575）/Like/In/Not（typed_ast.rs:825-883）。
  - `TypedSelectQuery<T>`（typed_ast.rs:672）：类型安全的 SELECT 查询构造器，`filter<E: TypedExpression<SqlType = Bool> + ExprTable<Table = T>>` 编译期拒绝跨表列引用。
  - `TypedColumnExt` trait（typed_ast.rs:780）+ `BoolExpressionExt` trait（typed_ast.rs:904）：表达式构造扩展方法（`eq/ne/lt/gt/le/ge/like/in/and/or/not`）。
- **业务规则**：每个表达式为 ZST（零大小类型），仅在编译期携带类型信息，运行时通过 `to_sql` 生成 SQL 片段。类型安全保证：`Eq<C, T>` 要求 `C: TypedColumn<RustType = T>`，列类型必须与值类型匹配；`And<L, R>` 要求 `L: TypedExpression<SqlType = Bool>`；`TypedSelectQuery::filter<E>` 要求 `E: TypedExpression<SqlType = Bool> + ExprTable<Table = T>`，跨表列引用编译期拒绝。
- **扩展点**：v3.5.0 在既有 11 种表达式基础上新增 46 种表达式（聚合/算术/字符串/日期/窗口/NULL 处理/BETWEEN/DISTINCT/子查询/类型转换），每种为 ZST + TypedExpression trait 实现 + to_sql 参数化生成，通过 `typed-dsl` feature gate 隔离（[Cargo.toml:56](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L56) 既有）。既有 11 种表达式 + TypedExpression trait + ExprTable trait + TypedSelectQuery 保持完全向后兼容。
- **约束**：
  - 零成本抽象：所有新增表达式为 ZST，`static_assert!(size_of::<T>() == 0)` 编译期断言。
  - 参数化查询：新增表达式 to_sql 生成的 SQL 必须使用参数化占位符（`?`），禁止字符串拼接，沿用 v3.4.0 SQL 注入防护铁律。
  - 方言分派：ILike 仅 PostgreSQL 支持（其他方言回退 `LIKE`），DistinctOn 仅 PostgreSQL 支持（其他方言返回 Err），窗口函数按方言版本分派（MySQL 8.0+/SQLite 3.25+ 支持）。
  - 类型检查严格性：算术表达式类型检查要求操作数 SqlType 兼容（Integer+Integer=Integer），编译期拒绝不兼容类型（Text+Integer），提供 Cast 表达式让用户显式转换。

### 1.2.3 无锁连接池实现（pool.rs:743-801）

- **接口契约**：`Pool` 结构体（pool.rs:743）含以下无锁数据结构：
  - `idle: Arc<ArrayQueue<PooledConnection>>`（pool.rs:751）：crossbeam-queue 无锁 MPMC 有界队列，容量固定为 `config.max_size`，push/pop 无锁。
  - `total_count: Arc<AtomicU32>`（pool.rs:761）：无锁原子计数（idle + borrowed），fetch_add/fetch_sub 单条 CPU 指令。
  - `closed: Arc<AtomicBool>`（pool.rs:763）：池是否已关闭标志。
  - `notify: Arc<Notify>`（pool.rs:764）：tokio 异步通知，唤醒等待 acquire 的任务。
  - `waiters_count: Arc<AtomicU32>`（pool.rs:766）：等待 acquire 的任务数（监控用）。
  - `dynamic_max_size: Arc<AtomicU32>`（pool.rs:768）：动态 max_size（可通过 resize/set_max_size 修改）。
  - 6 个 AtomicU64 统计计数（pool.rs:790-800）：acquire_count/acquire_failed_count/acquire_wait_time_ns/release_count/connection_created_count/connection_closed_count。
- **业务规则**：
  - acquire 流程：检查 closed → 检查 circuit_breaker/rate_limit（feature gate）→ `idle.pop()` 无锁弹出 → 命中返回；未命中 → `total_count.fetch_add(1)` 原子增 → 若 < max_size 则创建新连接 → 否则 `notify.notified().await` 等待。
  - release 流程：`total_count.fetch_sub(1)` 原子减 → `idle.push(conn)` 无锁推入 → `notify.notify_one()` 唤醒一个等待者。
  - v0.2.1 修复 Critical P-1：从 `Mutex<u32>` 改为 `AtomicU32`，吞吐量提升 ~3x（pool.rs:754-760 注释记录）。
  - v1.1.0 优化 2：从 `Arc<Mutex<VecDeque<PooledConnection>>>` 改为 `Arc<ArrayQueue<PooledConnection>>`，消除锁竞争（pool.rs:746-750 注释记录）。
- **扩展点**：v3.5.0 不修改 pool.rs 实现（无锁连接池已完整），仅编写架构文档 `docs/lock-free-pool-design.md` 沉淀工作原理/数据结构/并发安全证明/竞品对比。文档引用 pool.rs file:line 证据，通过 doctest + 代码审查保证文档与代码一致。
- **约束**：
  - 性能不回退：v3.5.0 改进不得使无锁连接池吞吐量回退，既有 ~3x 加速比保持（spec §4.1.3）。
  - 文档与代码一致：架构文档描述的工作原理与 pool.rs 实际代码必须一致，通过 doctest + 代码审查杜绝脱节（spec §5.3.1.4）。
  - 基准数据可复现：竞品对比的吞吐量基准数据附完整复现命令（硬件/软件环境/参数），文档标注"在指定环境下复现"（spec §5.3.3.2）。

### 1.2.4 16 种方言 + DbType 19 变体（dialect.rs:228-1991 + db_type.rs:11）

- **接口契约**：
  - `Dialect` trait（dialect.rs:23）：所有方言的基 trait，方法 `clone_box`/`db_type`/`quote`/`escape_string`/`build_pagination`/`supports_returning`/`auto_increment_keyword`/`json_extract`/`full_text_search`/`build_create_table`/`build_drop_table`/`build_alter_table` 等。
  - 8 独立方言：MySqlDialect（dialect.rs:228）/PostgreSqlDialect（dialect.rs:451）/SqliteDialect（dialect.rs:677）/OracleDialect（dialect.rs:952）/SqlServerDialect（dialect.rs:1198）/ClickHouseDialect（dialect.rs:1533）/DuckDBDialect（dialect.rs:1786）/Db2Dialect（dialect.rs:1989），各自独立实现 Dialect trait。
  - 8 兼容方言：通过 `delegate_dialect_to!` 宏（dialect.rs:1429）委派基础方言：MariaDbDialect/TiDbDialect→MySqlDialect（dialect.rs:1496/1499）、KingbaseDialect/PolarDbDialect/GaussDbDialect→PostgreSqlDialect（dialect.rs:1502/1505/1508）、DamengDialect→OracleDialect（dialect.rs:1511）、SybaseDialect/GBaseDialect→SqlServerDialect（dialect.rs:1514/1517）。
  - `DbType` 枚举（db_type.rs:11）：19 变体（MySQL/PostgreSQL/Sqlite/Redis/MongoDB/ClickHouse/Oracle/OceanBase/SqlServer/VectorDb/PureJsDb/Dameng/Kingbase/Db2/MariaDB/TiDB/PolarDB/GaussDB/GBase/Sybase/DuckDB），`#[non_exhaustive]` 允许未来扩展。
  - `get_dialect(db_type: DbType) -> Result<Box<dyn Dialect>, DbError>`（dialect.rs:2256）：方言工厂函数，Redis/MongoDB/VectorDb/PureJsDb 返回 Err（不支持标准 SQL）。
- **业务规则**：独立方言各自实现完整 Dialect trait（quote/escape/pagination/DDL 等），兼容方言通过 `delegate_dialect_to!` 宏委派基础方言（仅 db_type() 不同）。OceanBase 委派 MySqlDialect（dialect.rs:2269，MySQL 兼容）。
- **扩展点**：v3.5.0 列出未实现方言（Informix/SAP HANA/Firebird/CockroachDB/YugabyteDB/Snowflake/Redshift/Vertica/Teradata 等），按"必须实现/建议实现/暂不需要"分类评估。对"必须实现"方言：新增 DbType 枚举变体（db_type.rs:11 `#[non_exhaustive]` 允许）+ Dialect trait 实现（独立或委派）+ 方言测试 + 五方言行为一致验证，通过 feature gate 隔离。既有 16 种方言不变。
- **约束**：
  - 无 Rust 驱动的方言不实现：如 SAP HANA 无官方 Rust 驱动，标注"暂不需要（无 Rust 驱动）"，禁止实现无驱动支撑的方言（spec §5.4.1.6）。
  - 五方言行为一致：所有新能力必须保持 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言行为一致（spec §1.2.18）。
  - 兼容方言优先：新方言若与既有方言 SQL 兼容，优先用 `delegate_dialect_to!` 宏委派，减少实现成本。

### 1.2.5 L2Cache 跨 Session 缓存（l2_cache.rs:517）

- **接口契约**：`L2Cache` 结构体（l2_cache.rs:517）含：
  - `data: RwLock<HashMap<String, CacheEntry>>`：缓存数据（按 CacheKey 字符串索引）。
  - `table_index: RwLock<HashMap<String, Vec<String>>>`：表名索引（用于表级失效）。
  - `access_order: RwLock<LruOrder>`：LRU 访问顺序跟踪器（O(1) touch/remove/lru_key，arena 双向链表 + HashMap）。
  - `stats: RwLock<L2CacheStats>`：全局统计信息。
  - `table_stats: RwLock<HashMap<String, PerTableStats>>`：按表分桶统计。
  - `default_ttl: Option<Duration>`：默认 TTL。
  - `max_size: usize`：最大容量（LRU 淘汰）。
  - `invalidation_bus: Option<Arc<dyn InvalidationBus>>`：缓存失效总线（跨实例失效通知）。
- **业务规则**：L2Cache 跨 Session 共享，按 CacheKey（表+主键/表+查询）缓存，LRU 淘汰，TTL 过期，表级失效，跨实例失效通过 InvalidationBus 通知。锁顺序约定：`data` → `access_order` → `table_index` → `stats`，避免死锁。
- **扩展点**：v3.5.0 新增 L1Cache（Session 级别 Identity Map），与 L2Cache 协作：L1 未命中查 L2，L2 命中回填 L1 返回；L2 未命中查 DB，DB 命中回填 L1+L2 返回。L1 缓存通过 `l1-cache` feature gate 隔离（新增），既有 L2Cache 公开 API（put/get/invalidate/invalidate_table/with_default_ttl/with_max_size/with_invalidation_bus）保持完全向后兼容。
- **约束**：
  - L1 缓存不跨 Session 泄露：L1 与 Session 绑定，Session 结束自动清空，L1 不跨 Session 共享（跨 Session 共享是 L2 职责，spec §5.5.1.7）。
  - L1 缓存数据一致性：写操作自动失效对应 L1 条目，避免脏读（spec §4.2.3）。
  - L1 缓存查询 O(1)：按主键 HashMap 查找，不引入遍历开销（spec §4.1.2）。
  - L1 缓存不泄露敏感数据：遵循既有脱敏规则（multi-tenant-enhanced feature 的列级脱敏），不绕过脱敏（spec §4.3.2）。

### 1.2.6 Connection trait 手动解糖 + `#[async_trait]` 混用（pool.rs:45 + pool.rs:732）

- **接口契约**：
  - `Connection` trait（pool.rs:45）手动解糖 async 方法：`fn execute<'a>(&'a mut self, sql: &'a str) -> Pin<Box<dyn Future<Output = Result<u64, crate::DbError>> + Send + 'a>>`，所有 async 方法使用单一生命周期 `'a`（绑定 `&'a mut self` 和 `&'a str`），而非 HRTB。注释明确说明"以避免 `&str` 参数触发 HRTB 与 sqlx::Executor 冲突"（pool.rs:41-44）。
  - `ConnectionFactory` trait（pool.rs:732）使用 `#[async_trait]` 宏：`async fn create(&self) -> Result<Box<dyn Connection>, PoolError>`。
  - `Model` trait（model.rs:271）使用 `#[async_trait]` 宏。
  - 多处 impl 使用 `#[async_trait]`（pool.rs:1870/2190 等，共 16 处）。
- **业务规则**：Connection trait 手动解糖有技术原因（避免 HRTB 与 sqlx::Executor 冲突），其他 trait 使用 `#[async_trait]` 宏。混用增加学习成本与维护负担（对比分析文档 §6.9）。
- **扩展点**：v3.5.0 评估统一方案（方案 A 统一 `#[async_trait]` / 方案 B 统一手动解糖），输出评估文档 + 推荐方案 + 渐进迁移方案。既有 Connection trait 签名在评估期内保持不变（spec §1.4.3）。
- **约束**：
  - 不引入 Breaking Change：迁移后公开 API 签名语义不变，feature gate 隔离 + SemVer 兼容，sz-pay cargo check 通过（spec §5.7.1.4）。
  - HRTB 冲突约束：Connection trait 手动解糖有技术原因，统一为 `#[async_trait]` 可能重新引入 HRTB 冲突。评估必须考虑此约束，推荐方案可能是"保持手动解糖 + 文档说明原因"而非强制统一。

### 1.2.7 core::QueryBuilder + sz-orm-query-builder 重叠（query.rs:36 + query-builder/lib.rs:53）

- **接口契约**：
  - `sz-orm-core::QueryBuilder<M: Model>`（query.rs:36）：ORM 集成 QueryBuilder，绑定 Model `<M: Model>`，编译期表/列校验，4295 行，含 select/where/where_eq/order_by/limit/offset/join/group_by/having/lock_for_update/keyset_pagination/cache_ttl 等链式 API。
  - `sz-orm-query-builder::Query`（query-builder/lib.rs:53）：独立 SQL 构造器（sea-query 风格，不绑定 Model），4185 行，含 select/insert/update/delete 链式 API，可独立编译、独立发布到 crates.io。
  - lib.rs:53 已有区别说明表格（绑定 Model/类型安全/适用场景/依赖/独立发布 5 维对比）。
- **业务规则**：两个 QueryBuilder 长期共存，core::QueryBuilder 适用于 ORM 完整流程（绑定 Model 编译期校验），sz-orm-query-builder 适用于纯 SQL 构造/动态查询（不依赖 Model）。对比分析文档 §6.10 指出用户困惑。
- **扩展点**：v3.5.0 评估合并方案（方案 A 合并，sz-orm-query-builder 委派 core::QueryBuilder / 方案 B 保留，明确分工 + 选择指南 + deprecation 标注），输出评估文档 + 选择指南 + 推荐方案。按推荐方案渐进合并或 deprecation，v3.5.0 不立即删除 sz-orm-query-builder（spec §1.4.4）。
- **约束**：
  - 不引入 Breaking Change：合并后 sz-orm-query-builder API 兼容（委派实现），sz-pay cargo check 通过，SemVer 兼容（spec §5.8.1.4）。
  - 渐进 deprecation：v3.5.0 标注 deprecated 但保持可用，给用户迁移周期（spec §4.5.3）。

### 1.2.8 sz-orm-es Mock + `real` feature 占位（es/lib.rs:1 + es/Cargo.toml:15）

- **接口契约**：
  - `EsSync` trait：索引/搜索/聚合/过滤抽象。
  - `InMemoryEsSync`：Mock 实现（HashMap 存储），lib.rs:1 顶部明确标注"MOCK-ONLY，非生产可用"。
  - `EsSyncManager`：同步管理器（默认使用 Mock，可通过 `with_backend` 替换）。
  - Cargo.toml:15 已声明 `real = []` feature（占位，未实现真实后端）。
- **业务规则**：Mock 实现完整覆盖 EsSync trait 接口，查询语义为简化版，不保证与真实 ES 行为一致。不支持分布式、副本、分片、持久化、TLS、认证等任何 ES 生产特性。
- **扩展点**：v3.5.0 实现 sz-orm-es 真实 ES 后端（索引/搜索/聚合/过滤），通过 `real` feature gate 隔离。真实实现使用 elasticsearch crate 或 reqwest 调用 ES API，Mock 实现保持默认（`real` feature 关闭时）。真实与 Mock 行为语义一致（差分测试验证）。
- **约束**：
  - 真实后端依赖不进默认 feature：reqwest/elasticsearch 为 optional 依赖，仅 `real` feature 启用时引入，默认 feature 无新依赖（spec §5.9.1.5）。
  - 真实后端集成测试标注 `#[ignore]`：默认 cargo test 跳过，需真实环境 `cargo test --features real -- --ignored`（spec §5.9.1.3）。

### 1.2.9 sz-orm-config 内存 + `real-consul`/`real-nacos` feature（config/lib.rs:47 + config/Cargo.toml:14）

- **接口契约**：
  - `ConfigCenter` trait（config/lib.rs:30）：配置中心抽象（get/set/delete/list/watch/subscribe）。
  - `ConsulConfigCenter`（config/lib.rs:47）：内存实现（HashMap + subscribers + events），未集成真实 Consul/Nacos。
  - Cargo.toml:14 已声明 `real-consul = ["dep:reqwest", "dep:tokio"]` + `real-nacos = ["dep:reqwest", "dep:tokio"]` feature，reqwest optional 依赖已有。
- **业务规则**：配置变更触发订阅者回调（ConfigChangeCallback），事件记录在 Mutex<Vec> 中。未集成真实 Consul/Nacos HTTP API，无网络通信。
- **扩展点**：v3.5.0 实现真实 Consul/Nacos 后端（基于 reqwest HTTP API，配置读写/监听/服务发现），通过 `real-consul`/`real-nacos` feature gate 隔离。内存实现保持默认。
- **约束**：
  - 真实后端依赖不进默认 feature：reqwest 已为 optional 依赖（Cargo.toml:14），仅 `real-consul`/`real-nacos` feature 启用时引入。
  - 真实实现需认证：Consul ACL Token / Nacos Username+Password，禁止未认证读取配置。

### 1.2.10 docs.rs cfg 跳过 + 313 pub API 文档缺口（lib.rs:406）

- **接口契约**：`#![cfg_attr(docsrs, warn(missing_docs))]`（lib.rs:406），仅在 docs.rs 构建时启用 missing_docs lint，本地和 CI clippy 不触发。注释说明"避免 313 个 pub API 缺文档阻塞开发"（lib.rs:404-405）。
- **业务规则**：docs.rs 构建文档时启用 missing_docs lint，但因 313 个 pub API 缺文档，实际 docs.rs 文档不完整。本地和 CI 不触发 missing_docs，避免阻塞开发。
- **扩展点**：v3.5.0 逐批补齐 313 个 pub API 文档（功能/参数/返回/示例/错误），移除 lib.rs:406 docs.rs cfg 跳过，改为全局 `#![warn(missing_docs)]`，使 docs.rs 文档完整。API 签名不变（仅新增 `///` 注释）。
- **约束**：
  - 文档与实际不符：补齐的文档与代码实际行为必须一致，通过 `cargo doc --workspace --no-deps` 无警告 + `cargo test --workspace --doc` doctest 通过 + 代码审查杜绝（spec §5.10.1.5）。
  - pub API 数量变化：v3.5.0 新增/删除 pub API 导致 313 数量变化时，重新统计缺文档 pub API 数量，补齐所有缺文档 API，文档标注实际数量（spec §5.10.3.1）。

---

# 二、增量设计方案

## 2.0 架构总览

### 2.0.1 v3.5.0 整体架构图

v3.5.0 在 v3.4.0 现有 workspace 基础上，不新增独立包，而是在 sz-orm-core 内新增 L1 缓存模块 + typed_ast 46 种表达式 + 真实后端实现，在 sz-orm-es/config 内实现真实后端，在 scripts/ 内新增文档同步检查脚本 + 发布脚本，在 docs/ 内新增无锁连接池架构文档 + 迁移指南，通过 6 个新增 feature gate（l1-cache/real/real-consul/real-nacos 已有占位 + typed-dsl 既有 + migration-guide 既有）+ 既有 feature 体系隔离，复用既有 typed_ast/L2Cache/dialect/delegate_dialect_to 宏/publish 脚本基础设施。整体架构如下：

```plantuml
@startuml
!theme plain
title sz-orm v3.5.0 整体架构（增量视图）

rectangle "sz-orm v3.5.0 workspace" as WS {

  rectangle "核心层 sz-orm-core (扩展)" as Core {
    rectangle "既有: typed_ast.rs (11 种表达式)" as TaOld
    rectangle "既有: pool.rs (无锁连接池)" as PoolOld
    rectangle "既有: dialect.rs (16 种方言)" as DialectOld
    rectangle "既有: l2_cache.rs (L2Cache)" as L2Old
    rectangle "既有: query.rs (QueryBuilder<M>)" as QueryOld
    rectangle "既有: lib.rs (docs.rs cfg 跳过)" as LibOld

    rectangle "typed_ast 46 种新增表达式 <<new>>\n[typed-dsl]" as TaNew
    rectangle "l1_cache.rs (L1Cache Identity Map) <<new>>\n[l1-cache]" as L1New
    rectangle "313 pub API 文档补齐 + 移除 docs.rs cfg 跳过 <<new>>\n[doc-completion]" as DocApi
  }

  rectangle "sz-orm-es (扩展)" as Es {
    rectangle "既有: InMemoryEsSync (不变)" as EsMock
    rectangle "真实 ES 后端 <<new>>\n[real]" as EsReal
  }

  rectangle "sz-orm-config (扩展)" as Cfg {
    rectangle "既有: ConsulConfigCenter 内存 (不变)" as CfgMem
    rectangle "真实 Consul/Nacos 后端 <<new>>\n[real-consul/real-nacos]" as CfgReal
  }

  rectangle "sz-orm-query-builder (扩展)" as ExtQB {
    rectangle "既有: Query (不变)" as ExtQbOld
    rectangle "deprecated 标注 或 委派 core::QueryBuilder <<new>>" as ExtQbDep
  }

  rectangle "scripts/ (扩展)" as Scripts {
    rectangle "既有: check-doc-consistency.py (门禁 12)" as Script12
    rectangle "既有: publish_all.py / compute_topology.ps1" as ScriptPub
    rectangle "check-doc-sync.py (门禁 14) <<new>>" as Script14
    rectangle "publish-workspace.sh (v3.5.0 拓扑发布) <<new>>" as ScriptPub35
  }

  rectangle "docs/ (扩展)" as Docs {
    rectangle "lock-free-pool-design.md <<new>>" as DocPool
    rectangle "migration/diesel-to-sz-orm.md <<new>>\n[migration-guide]" as DocDiesel
    rectangle "migration/seaorm-to-sz-orm.md <<new>>\n[migration-guide]" as DocSeaOrm
    rectangle "migration/sqlx-to-sz-orm.md <<new>>\n[migration-guide]" as DocSqlx
    rectangle "async-trait-evaluation.md <<new>>" as DocAsync
    rectangle "query-builder-merge-evaluation.md <<new>>" as DocQbMerge
    rectangle "dialect-extension-roadmap.md <<new>>" as DocDialect
  }

  rectangle "AGENTS.md / engineering-practices.md (扩展)" as Eng {
    rectangle "既有: 门禁 1-13" as Gates13
    rectangle "门禁 14 (文档同步更新检查) <<new>>" as Gate14
  }
}

cloud "crates.io" as CratesIo
database "MySQL/PG/SQLite/Oracle/MSSQL" as Rdb
cloud "真实 Elasticsearch" as RealEs
cloud "真实 Consul / Nacos" as RealCfg
rectangle "sz-pay 生产项目" as SzPay

TaNew ..> TaOld : 扩展 46 种表达式 (不破坏 11 种既有)
L1New ..> L2Old : L1→L2→DB 协作 (L2 API 不变)
EsReal --> RealEs : 真实 ES 集成 (#[cfg(feature="real")] + #[ignore])
CfgReal --> RealCfg : 真实配置中心集成
Script14 --> Gate14 : 门禁 14 脚本
ScriptPub35 --> CratesIo : 拓扑发布 v3.5.0
CratesIo --> SzPay : sz-pay 升级到 3.5.0 (零回归验证)
DocPool ..> PoolOld : 沉淀无锁连接池架构
ExtQbDep ..> QueryOld : 委派或 deprecated (不立即删除)

@enduml
```

### 2.0.2 10 大方向在 workspace 中的定位

| 方向 | 需求组 | 包名 | 形态 | feature gate | 在 workspace 中的位置 | 依赖关系 |
|------|--------|------|------|-------------|---------------------|---------|
| 1 文档同步约束 | REQ-DOC-SYNC-001~004 | scripts/ + AGENTS.md + engineering-practices.md + CI | **新增脚本 + 门禁 14 + CI job** | 无（纯脚本 + 文档） | `scripts/check-doc-sync.py` + AGENTS.md/engineering-practices.md 门禁 14 章节 + .github/workflows/ci.yml | 无新增依赖（纯 Python 脚本） |
| 2 typed_ast DSL 补齐 | REQ-DSL-001~010 | sz-orm-core | **扩展 typed_ast.rs 46 种表达式** | `typed-dsl`（既有，[Cargo.toml:56](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L56)） | `packages/sz-orm-core/src/typed_ast.rs` 内 `#[cfg(feature = "typed-dsl")]` 条件编译 | 无新增（复用既有 typed_ast 基础） |
| 3 无锁连接池文档 | REQ-POOL-DOC-001~004 | docs/ | **纯文档** | 无（纯文档） | `docs/lock-free-pool-design.md` | 无（纯文档） |
| 4 方言扩展规划 | REQ-DIALECT-001~006 | sz-orm-core + docs/ | **规划文档 + 必须实现方言 Dialect trait** | 按方言（如 `dialect-firebird`） | `packages/sz-orm-core/src/dialect.rs` + `packages/sz-orm-core/src/db_type.rs` + `docs/dialect-extension-roadmap.md` | Rust 驱动（按方言，optional） |
| 5 L1 缓存设计 | REQ-L1CACHE-001~007 | sz-orm-core | **新增 l1_cache.rs 模块** | `l1-cache`（新增） | `packages/sz-orm-core/src/l1_cache.rs` + `packages/sz-orm-core/src/lib.rs` 模块声明 + `packages/sz-orm-core/Cargo.toml` feature 定义 | 无新增（复用既有 HashMap + parking_lot + L2Cache） |
| 6 crates.io 发布 | REQ-PUBLISH-001~007 | scripts/ + workspace | **发布脚本 + 版本号升级** | 无（纯脚本 + 版本号） | `scripts/publish-workspace.sh` + `Cargo.toml` workspace.package.version = 3.5.0 + workspace.dependencies 版本号 | 既有 publish_all.py/compute_topology.ps1 |
| 7 async trait 统一 | REQ-ASYNC-001~004 | sz-orm-core + docs/ | **评估文档 + 渐进迁移** | `async-trait-unify`（新增，可选） | `docs/async-trait-evaluation.md` + `packages/sz-orm-core/src/pool.rs` 渐进迁移 | 无新增（复用既有 async-trait crate） |
| 8 QueryBuilder 合并 | REQ-QB-MERGE-001~004 | sz-orm-query-builder + docs/ | **评估文档 + 选择指南 + 渐进 deprecation** | 无（纯文档 + deprecated 标注） | `docs/query-builder-merge-evaluation.md` + `packages/sz-orm-query-builder/src/lib.rs` deprecated 标注或委派实现 | 无新增 |
| 9 MOCK-ONLY 补齐 | REQ-MOCK-001~005 | sz-orm-es + sz-orm-config | **真实后端实现 + 集成测试 + 差分测试** | `real`（既有占位，[es/Cargo.toml:15](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-es/Cargo.toml#L15)）+ `real-consul`/`real-nacos`（既有，[config/Cargo.toml:14](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-config/Cargo.toml#L14)） | `packages/sz-orm-es/src/real_es.rs` + `packages/sz-orm-config/src/real_consul.rs`/`real_nacos.rs` + 集成测试 | elasticsearch crate 或 reqwest（optional，`real` feature）；reqwest（optional，已有） |
| 10 文档与迁移指南 | REQ-DOC-FILL-001~005 | sz-orm-core + docs/ | **313 pub API 文档 + 迁移指南** | `doc-completion`（既有，[Cargo.toml:42](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L42)）+ `migration-guide`（既有，[Cargo.toml:58](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L58)） | `packages/sz-orm-core/src/*.rs` 文档注释 + `docs/migration/*.md` | 无新增（纯文档） |

### 2.0.3 与 v3.4.0 现有架构的演进关系

```plantuml
@startuml
!theme plain
title v3.4.0 → v3.5.0 架构演进

rectangle "v3.4.0 (测试覆盖 + 架构改进 + 性能优化 + 类型安全 + 文档生态 + sz-pay 案例)" as V34 {
  rectangle "sz-orm-core\n(query/pool/dialect/l2_cache/typed_ast/value/result_map\n+perf-*/type-safe-columns/typed-dsl)" as Core34
  rectangle "sz-orm-es (Mock + real 占位)" as Es34
  rectangle "sz-orm-config (内存 + real-consul/nacos feature)" as Cfg34
  rectangle "sz-orm-query-builder (独立 SQL 构造器)" as ExtQb34
  rectangle "scripts/ (check-doc-consistency + publish_all)" as Scripts34
  rectangle "docs/ (engineering-practices + 对比分析)" as Docs34
}

rectangle "v3.5.0 增量 (6 feature gate 隔离 + 纯文档/脚本)" as V35 {
  rectangle "门禁 14 + check-doc-sync.py\n(无 feature, 纯脚本)" as Gate35
  rectangle "typed_ast 46 种新增表达式\n[typed-dsl]" as Ta35
  rectangle "l1_cache.rs L1Cache Identity Map\n[l1-cache]" as L135
  rectangle "sz-orm-es 真实 ES + sz-orm-config 真实 Consul/Nacos\n[real/real-consul/real-nacos]" as Real35
  rectangle "async trait 评估 + QB 合并评估\n(纯文档 + 渐进迁移)" as Eval35
  rectangle "方言扩展规划\n(规划文档 + 必须实现方言)" as Dialect35
  rectangle "313 pub API 文档 + 迁移指南 + 无锁连接池架构文档\n[doc-completion/migration-guide]" as Doc35
  rectangle "crates.io v3.5.0 拓扑发布\n(纯脚本 + 版本号)" as Pub35
}

Core34 ..> Ta35 : 扩展 46 种表达式 (不破坏 11 种既有)
Core34 ..> L135 : 新增 L1 缓存 (L2 API 不变)
Es34 ..> Real35 : 真实 ES 实现
Cfg34 ..> Real35 : 真实 Consul/Nacos 实现
ExtQb34 ..> Eval35 : deprecated 或委派 (不立即删除)
Scripts34 ..> Gate35 : 新增门禁 14 脚本
Scripts34 ..> Pub35 : 新增 v3.5.0 发布脚本
Core34 ..> Doc35 : 补齐 313 pub API 文档
Docs34 ..> Doc35 : 新增迁移指南 + 架构文档

note bottom of V35
  **演进原则**：
  1. 无 Breaking Change（现有公开 API 签名不变）
  2. 新增能力通过 feature gate 隔离（默认关闭）
  3. 不新增独立包，全部在既有包内扩展
  4. 复用现有基础设施（typed_ast/L2Cache/dialect/delegate_dialect_to/publish 脚本）
  5. 五方言行为一致（增强在 core 层统一，不触碰方言驱动）
  6. ADR-0001 严禁修改下游/上游仓库（sz-pay 仅验证不改）
  7. 测试覆盖不回退（v3.4.0 基线 6,738 测试仅增不减）
end note

@enduml
```

**演进关键决策**：

| 决策点 | 选项 | 选择 | 理由 |
|--------|------|------|------|
| 门禁 14 实现方式 | A. 复用门禁 12 脚本扩展 / B. 新增独立 check-doc-sync.py | B | 门禁 12 检查静态数据一致性，门禁 14 检查动态变更触发文档更新，检查逻辑不同；独立脚本职责单一，便于维护 |
| typed_ast 46 种表达式隔离方式 | A. 既有 typed-dsl feature / B. 新增 typed-dsl-full feature | A | typed-dsl feature 既有（[Cargo.toml:56](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L56)），v3.4.0 已用于 Diesel 风格 DSL 完善，v3.5.0 复用此 feature 扩展 46 种表达式，避免 feature 爆炸 |
| L1 缓存数据结构 | A. HashMap<TableKey, HashMap<PkValue, Arc<Mutex<Entity>>>> / B. HashMap<CompositeKey, Arc<Mutex<Entity>>> | A | 按表分桶便于表级失效（evict_table），按主键缓存实现 Identity Map，Arc<Mutex<Entity>> 保证同主键同引用 + 修改安全 |
| sz-orm-es 真实 ES 实现依赖 | A. elasticsearch 官方 crate / B. reqwest 自研 HTTP 客户端 | A | elasticsearch crate 是官方维护，覆盖 ES API 完整，比 reqwest 自研更可靠；作为 optional 依赖不进默认 feature |
| sz-orm-config 真实实现方式 | A. 引入 consul/nacos 官方 crate / B. 基于 reqwest 自研 HTTP 客户端 | B | consul/nacos 无成熟 Rust 官方 crate；reqwest 已是 sz-orm-config optional 依赖（[config/Cargo.toml:14](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-config/Cargo.toml#L14)），自研 HTTP 客户端可控且轻量 |
| async trait 统一推荐方案 | A. 统一 `#[async_trait]` / B. 统一手动解糖 / C. 保持现状 + 文档说明 | 评估后定（倾向 C） | Connection trait 手动解糖有技术原因（HRTB 冲突），强制统一可能重新引入冲突；评估文档附性能基准 + 迁移影响，推荐方案有依据 |
| QueryBuilder 合并推荐方案 | A. 合并（委派）/ B. 保留 + 指南 + deprecation | 评估后定（倾向 B） | 两个 QueryBuilder 适用场景不同（ORM 集成 vs 独立 SQL），合并可能损失独立发布能力；选择指南消除困惑，deprecation 给迁移周期 |
| 方言扩展"必须实现"判定 | A. 按 Rust 驱动可用性 / B. 按用户需求证据 / C. 综合 A+B+市场趋势 | C | 单一维度评估易偏差，综合使用场景/Rust 驱动/实现难度/市场趋势四维评估，评估表附理由与依据 |
| crates.io 发布版本号 | A. 3.5.0 / B. 4.0.0（major） | A | v3.5.0 通过 feature gate 隔离新能力，默认 feature 行为不变，向后兼容，SemVer minor 升级（spec §4.5.4） |

## 2.1 方向 1：文档同步更新约束规则化

### 2.1.1 上下文视图

```plantuml
@startuml
!theme plain
title 方向 1 文档同步更新约束规则化 上下文视图

actor "ORM 库维护者" as Dev
rectangle "sz-orm v3.5.0 方向 1" as Dir1 {
  port "scripts/check-doc-sync.py" as Script
  port "门禁 14 规则" as Gate14
  port "受影响文档映射规则" as MapRule
}
participant "git diff" as Git
participant "CI check-doc-sync job" as CIJob
rectangle "受影响文档" as Docs {
  port "AGENTS.md" as Agents
  port "engineering-practices.md" as Eng
  port "README.md" as Readme
  port "spec.md / design.md" as Spec
  port "对比分析文档" as CmpDoc
}

Dev --> Git : 提交代码变更
Git --> Script : 变更文件清单
Script --> MapRule : 查询映射规则
MapRule --> Docs : 确定应更新文档清单
Script --> CIJob : 退出码 0/非 0
CIJob --> Dev : 门禁通过/阻断

@enduml
```

### 2.1.2 `scripts/check-doc-sync.py` 脚本架构

**脚本职责**：分析 git diff 的代码变更文件，根据受影响文档映射规则确定应更新的文档清单，检查这些文档是否在本次变更中被修改，未修改则输出告警与阻断信号。

**脚本输入**：
- `--diff <git-diff-output>`：git diff 输出（变更文件清单），或默认从 `git diff --name-only HEAD` 获取
- `--skip-marker <marker>`：跳过标记（默认 `# doc-sync-skip:`），标注此注释的变更放行
- `--fix`：自动修复模式（输出建议更新的文档清单，不阻断）

**脚本输出**：
- 退出码 0：所有应更新文档已同步更新
- 退出码非 0：存在未同步更新的文档，输出告警 + 未更新文档清单
- `--fix` 模式：输出建议更新的文档清单（不阻断）

**脚本核心逻辑**：
1. 解析 git diff 获取变更文件清单（区分代码文件 vs 文档文件）。
2. 对每个代码文件，按受影响文档映射规则确定应更新文档清单。
3. 检查应更新文档是否在本次变更中被修改。
4. 未修改则记录告警，支持 `# doc-sync-skip: <reason>` 注释跳过。
5. 输出告警 + 退出码。

**受影响文档映射规则**（写入 engineering-practices.md 门禁 14 章节）：

| 代码变更类型 | 应更新文档清单 |
|-------------|--------------|
| 修改 `packages/sz-orm-core/src/*.rs` 公开 API（pub fn/struct/enum/trait） | spec.md + design.md + 对比分析文档 |
| 修改 `Cargo.toml` workspace 包列表（members 增减） | AGENTS.md + engineering-practices.md |
| 修改 `packages/*/Cargo.toml` feature 列表 | engineering-practices.md feature 组合矩阵 |
| 修改 `packages/sz-orm-core/src/pool.rs` 核心逻辑 | 对比分析文档 §5.2 + docs/lock-free-pool-design.md |
| 修改 `packages/sz-orm-core/src/dialect.rs` 方言实现 | 对比分析文档 §6.7 + docs/dialect-extension-roadmap.md |
| 修改 `packages/sz-orm-core/src/l2_cache.rs` 或新增 `l1_cache.rs` | 对比分析文档 §6.6 + spec.md + design.md |
| 修改 `packages/sz-orm-core/src/typed_ast.rs` 表达式 | 对比分析文档 §6.1 + spec.md + design.md |
| 版本号变更（workspace.package.version） | 所有含版本号的文档（AGENTS.md/README.md/engineering-practices.md/spec.md/design.md） |
| 修改 `packages/sz-orm-es/src/*.rs` 或 `packages/sz-orm-config/src/*.rs` | 对比分析文档 §6.8 + spec.md + design.md |
| 修改 `packages/sz-orm-query-builder/src/lib.rs` | 对比分析文档 §6.10 + docs/query-builder-merge-evaluation.md |

### 2.1.3 门禁 14 CI 集成方式

**AGENTS.md 10 道门禁表新增门禁 14**：

| # | 门禁 | 命令 |
|---|------|------|
| 14 | 文档同步更新检查 | `python scripts/check-doc-sync.py` |

**engineering-practices.md 新增门禁 14 章节**（紧接门禁 13）：

| 属性 | 值 |
|------|-----|
| **教训来源** | 用户指出"很多文档没有及时更新，导致前后不一致，反复出现多次" |
| **命令** | `python scripts/check-doc-sync.py` |
| **CI Job 名** | `check-doc-sync`（新增） |
| **状态** | ✅ 已通过 |

**校验内容**：
- 代码变更是否同步更新了所有受影响文档
- 按受影响文档映射规则确定应更新文档清单
- 未同步更新则 CI 阻断提交

**CI 配置新增 check-doc-sync job**（.github/workflows/ci.yml）：
- 在既有 ci.yml 中新增 `check-doc-sync` job
- 依赖 `check-doc-consistency`（门禁 12）job 完成
- 调用 `python scripts/check-doc-sync.py`
- 退出码非 0 阻断 PR

### 2.1.4 实现设计文档

```plantuml
@startuml
!theme plain
title 门禁 14 文档同步检查流程

start
:获取 git diff 变更文件清单;
:区分代码文件 vs 文档文件;
if (代码文件非空?) then (是)
  :按映射规则确定应更新文档清单;
  :检查应更新文档是否在变更中;
  if (所有应更新文档已修改?) then (是)
    :退出码 0 (通过);
  else (否)
    if (有 # doc-sync-skip 注释?) then (是)
      :放行 + 记录跳过原因;
    else (否)
      :输出告警 + 未更新文档清单;
      :退出码非 0 (阻断);
    endif
  endif
else (否)
  :无代码变更, 退出码 0;
endif
stop

@enduml
```

## 2.2 方向 2：typed_ast DSL 补齐 Diesel 覆盖度

### 2.2.1 上下文视图

```plantuml
@startuml
!theme plain
title 方向 2 typed_ast DSL 补齐 上下文视图

actor "DSL 增强开发者" as Dev
rectangle "sz-orm-core typed_ast.rs" as Ta {
  port "既有 11 种表达式" as Old11
  port "46 种新增表达式 <<new>>\n[typed-dsl]" as New46
  port "TypedExpression trait" as Trait
  port "ExprTable trait" as ExprTable
}
participant "Dialect" as Dialect
participant "cargo test --features typed-dsl" as Test

Dev --> Ta : 新增 46 种表达式 (ZST + TypedExpression + to_sql)
Ta --> Dialect : to_sql 按方言分派
Dev --> Test : 验证
Test --> Dev : 全通过 + 既有 11 种不回退

@enduml
```

### 2.2.2 46 种新增表达式类型系统设计

**设计原则**：
1. **零成本抽象（ZST）**：每个新增表达式为零大小类型（unit struct 或仅含 PhantomData），`static_assert!(size_of::<T>() == 0)` 编译期断言。
2. **TypedExpression trait 实现**：每个表达式实现 `TypedExpression` trait（[typed_ast.rs:249](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L249)），关联类型 `SqlType`，方法 `to_sql(&self, dialect: &dyn Dialect) -> String`。
3. **参数化 SQL 生成**：to_sql 生成的 SQL 必须使用参数化占位符（`?`），禁止字符串拼接，沿用 v3.4.0 SQL 注入防护铁律。
4. **方言分派**：to_sql 按方言分派，不支持的方言返回 Err 或回退到通用语法（如 ILike 在非 PG 方言回退 LIKE）。
5. **类型检查严格性**：算术表达式类型检查要求操作数 SqlType 兼容，编译期拒绝不兼容类型，提供 Cast 表达式让用户显式转换。

**46 种新增表达式分类**：

| 类别 | 表达式 | 数量 | SqlType 约束 | to_sql 示例 | 方言支持 |
|------|--------|------|------------|-----------|---------|
| 聚合 | Max/Min/Sum/Avg/Count/CountStar | 6 | 输入 SqlType → 输出 SqlType（Max/Min 同输入，Sum/Avg/Count → Integer/BigInt） | `MAX(col)`/`MIN(col)`/`SUM(col)`/`AVG(col)`/`COUNT(col)`/`COUNT(*)` | 全方言 |
| 算术 | Add/Sub/Mul/Div/Modulo | 5 | 操作数 SqlType 兼容（Integer+Integer=Integer），编译期拒绝不兼容 | `col1 + col2`/`col1 - col2`/`col1 * col2`/`col1 / col2`/`col1 % col2` | 全方言 |
| 字符串 | Concat/ILike/Length/Lower/Upper/Trim/Substring | 7 | 输入 Text → 输出 Text/Bool/Integer | `CONCAT(col1, col2)`/`col ILIKE 'pattern'`/`LENGTH(col)`/`LOWER(col)`/`UPPER(col)`/`TRIM(col)`/`SUBSTRING(col FROM pos FOR len)` | ILike 仅 PG，其他回退 LIKE |
| 日期 | Extract/Year/Month/Day/Hour/Minute/Second/Now | 8 | 输入 Date/DateTime → 输出 Integer/Date/DateTime | `EXTRACT(field FROM col)`/`YEAR(col)`/`MONTH(col)`/`DAY(col)`/`HOUR(col)`/`MINUTE(col)`/`SECOND(col)`/`NOW()` | 全方言（语法略有差异） |
| 窗口 | Over/PartitionBy/OrderByInWindow/Lag/Lead/RowNumber/Rank/DenseRank | 8 | 输入表达式 → 输出窗口表达式 | `ROW_NUMBER() OVER (PARTITION BY ... ORDER BY ...)`/`RANK()`/`DENSE_RANK()`/`LAG(col, n)`/`LEAD(col, n)` | MySQL 8.0+/PG/Oracle/MSSQL/SQLite 3.25+ |
| NULL 处理 | IsNull/IsNotNull/Coalesce/NullIf | 4 | 输入 Nullable<T> → 输出 Bool/T | `col IS NULL`/`col IS NOT NULL`/`COALESCE(col, default)`/`NULLIF(col1, col2)` | 全方言 |
| BETWEEN/DISTINCT/子查询 | Between/NotBetween/Distinct/DistinctOn/Exists/InSubquery | 6 | 输入表达式 + 范围/子查询 → 输出 Bool | `col BETWEEN low AND high`/`col NOT BETWEEN ...`/`DISTINCT col`/`DISTINCT ON (col)`/`EXISTS (subquery)`/`col IN (subquery)` | DistinctOn 仅 PG，其他返回 Err |
| 类型转换 | Cast/As | 2 | 输入 SqlType → 输出目标 SqlType | `CAST(col AS type)`/PG: `col::type` | 全方言（PG 支持 `::type` 语法） |
| **合计** | — | **46** | — | — | — |

**ZST 实现方案**（以 Max 为例）：

```rust
// 概念示意（非最终代码）
pub struct Max<C: TypedColumn>(pub std::marker::PhantomData<C>);

impl<C: TypedColumn> TypedExpression for Max<C> {
    type SqlType = C::RustType as SqlType;  // Max 返回同类型
    fn to_sql(&self, dialect: &dyn Dialect) -> String {
        format!("MAX({})", C::NAME)  // C::NAME 为编译期常量
    }
}

// 编译期 ZST 断言
const _: () = assert!(std::mem::size_of::<Max<SomeColumn>>() == 0);
```

**算术表达式类型检查方案**（以 Add 为例）：

```rust
// 概念示意（非最终代码）
pub struct Add<L: TypedExpression, R: TypedExpression>(pub L, pub R)
where
    L::SqlType: Addable<R::SqlType>,  // 编译期类型兼容检查
    <L::SqlType as Addable<R::SqlType>>::Output: SqlType;

impl<L, R> TypedExpression for Add<L, R>
where
    L: TypedExpression,
    R: TypedExpression,
    L::SqlType: Addable<R::SqlType>,
    <L::SqlType as Addable<R::SqlType>>::Output: SqlType,
{
    type SqlType = <L::SqlType as Addable<R::SqlType>>::Output;
    fn to_sql(&self, dialect: &dyn Dialect) -> String {
        format!("{} + {}", self.0.to_sql(dialect), self.1.to_sql(dialect))
    }
}

// Addable trait 定义类型兼容规则
trait Addable<Rhs: SqlType>: SqlType {
    type Output: SqlType;
}
impl Addable<Integer> for Integer { type Output = Integer; }
impl Addable<BigInt> for BigInt { type Output = BigInt; }
impl Addable<Real> for Real { type Output = Real; }
impl Addable<Double> for Double { type Output = Double; }
// Text + Integer 不实现 Addable，编译期拒绝
```

### 2.2.3 `typed-dsl` feature gate 隔离方式

**Cargo.toml feature 定义**（既有，[Cargo.toml:56](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L56)）：

```toml
# v3.4.0：类型化 DSL（typed_ast.rs Diesel 风格表达式 DSL）
typed-dsl = []
```

**条件编译隔离**：在 typed_ast.rs 中，46 种新增表达式用 `#[cfg(feature = "typed-dsl")]` 条件编译隔离，默认 feature 关闭时不编译，既有 11 种表达式不受影响。

**默认 feature 零行为变更**：`typed-dsl` feature 默认关闭（不在 default = ["redis"] 中），关闭时编译产物大小与运行时开销与 v3.4.0 一致（spec §4.1.4）。

### 2.2.4 实现设计文档

```plantuml
@startuml
!theme plain
title typed_ast 46 种表达式实现流程

start
:读取既有 11 种表达式 + SqlType 体系;
:设计 46 种新增表达式 ZST 结构;
:为每种表达式实现 TypedExpression trait;
:为每种表达式实现 to_sql (参数化 + 方言分派);
:为算术表达式实现 Addable/Subable 等类型兼容 trait;
:添加编译期 ZST 断言 (static_assert size_of == 0);
:编写单元测试 (每种表达式 to_sql + 类型检查);
:编写差分测试 (typed_ast vs QueryBuilder SQL 一致);
:运行 cargo test -p sz-orm-core --features typed-dsl;
if (全通过 + 既有 11 种不回退?) then (是)
  :更新对比分析文档 §6.1 为"已对齐 Diesel";
  :附表达式覆盖度对比表 (file:line 证据);
else (否)
  :修复 + 重新测试;
endif
stop

@enduml
```

## 2.3 方向 3：无锁连接池架构文档沉淀

### 2.3.1 上下文视图

```plantuml
@startuml
!theme plain
title 方向 3 无锁连接池架构文档 上下文视图

actor "技术文档工程师" as DocEng
rectangle "sz-orm-core pool.rs" as Pool {
  port "ArrayQueue (pool.rs:751)" as AQ
  port "AtomicU32 (pool.rs:761)" as AU32
  port "Notify (pool.rs:764)" as Ntf
  port "AtomicBool (pool.rs:763)" as AB
}
rectangle "docs/lock-free-pool-design.md <<new>>" as ArchDoc {
  port "工作原理" as Principle
  port "数据结构图解" as DataStruct
  port "acquire/release 时序图" as SeqDiagram
  port "并发安全证明" as Proof
  port "竞品对比" as Compare
}

DocEng --> Pool : 读取实现代码 (file:line 证据)
Pool --> ArchDoc : 沉淀架构知识
ArchDoc --> DocEng : 文档完成

@enduml
```

### 2.3.2 `docs/lock-free-pool-design.md` 文档结构

**文档章节结构**：

1. **概述**：无锁连接池的定义、优势、SZ-ORM 实现位置（pool.rs:743-801）。
2. **工作原理**：
   - ArrayQueue（crossbeam-queue 无锁 MPMC 队列）：基于数组实现，push/pop 无锁，容量固定为 max_size。
   - AtomicU32（无锁原子计数）：fetch_add/fetch_sub 单条 CPU 指令，替代 Mutex<u32> 消除锁瓶颈。
   - Notify（tokio 异步通知）：唤醒等待 acquire 的任务。
   - AtomicBool（closed 标志）：无锁关闭检查。
3. **数据结构图解**：Pool 结构体字段关系图（idle/total_count/closed/notify/waiters_count/dynamic_max_size/统计计数）。
4. **acquire/release 流程时序图**：
   - acquire 流程：检查 closed → 检查 circuit_breaker/rate_limit → idle.pop() → 命中返回；未命中 → total_count.fetch_add(1) → 若 < max_size 创建新连接 → 否则 notify.notified().await 等待。
   - release 流程：total_count.fetch_sub(1) → idle.push(conn) → notify.notify_one() 唤醒等待者。
5. **并发安全证明**：
   - ArrayQueue 线性化（linearizability）保证：引用 crossbeam-queue 官方正确性证明。
   - AtomicU32 内存序选择：fetch_add/fetch_sub 使用 AcqRel（acquire + release），保证计数可见性；closed.load 使用 Acquire，保证关闭可见。
   - 不丢连接证明：total_count 限制池中总连接数不超过 max_size，idle.push 不会因容量不足失败（除非 close_all 后归还，此时连接直接关闭）。
   - 不重复归还证明：PooledConnection Drop 自动归还，Arc 引用计数保证唯一归还。
6. **竞品对比**：
   - Diesel r2d2：Mutex<VecDeque> 方案，锁竞争瓶颈。
   - SeaORM：基于 sqlx Pool，Mutex 方案。
   - SQLx Pool：Mutex 方案。
   - SZ-ORM：ArrayQueue + AtomicU32 无锁方案，吞吐量提升 ~3x（v0.2.1 修复 Critical P-1 后实测）。
   - 锁机制对比表 + 吞吐量基准数据（附测试复现命令）+ 内存开销对比 + 功能特性对比。
7. **演进历史**：v0.2.1 修复 Critical P-1（Mutex<u32> → AtomicU32，吞吐量提升 ~3x）+ v1.1.0 优化 2（Mutex<VecDeque> → ArrayQueue，消除锁竞争）。
8. **引用**：crossbeam-queue 官方文档 + 正确性证明链接 + 版本与引用日期。

### 2.3.3 实现设计文档

```plantuml
@startuml
!startuml
!theme plain
title 无锁连接池架构文档编写流程

start
:读取 pool.rs:743-801 Pool 结构体;
:读取 pool.rs:1268+ acquire/release 流程;
:读取 ArrayQueue/AtomicU32/Notify 用法;
:编写工作原理 (ArrayQueue + AtomicU32 + Notify);
:编写数据结构图解 (PlantUML);
:编写 acquire/release 时序图 (PlantUML);
:编写并发安全证明 (线性化 + 内存序 + 不丢连接/不重复归还);
:编写竞品对比 (Diesel r2d2 / SeaORM / SQLx);
:每条结论附 pool.rs file:line 证据;
:运行 cargo doc 验证 doctest;
if (doctest 通过 + 文档与代码一致?) then (是)
  :更新对比分析文档 §5.2 引用;
else (否)
  :修复 doctest + 重新验证;
endif
stop

@enduml
```

## 2.4 方向 4：方言扩展规划

### 2.4.1 上下文视图

```plantuml
@startuml
!theme plain
title 方向 4 方言扩展规划 上下文视图

actor "方言扩展规划者" as Planner
rectangle "sz-orm-core dialect.rs" as Dialect {
  port "8 独立方言" as Ind8
  port "8 兼容方言 (delegate_dialect_to)" as Comp8
  port "DbType 19 变体" as Dbt19
}
rectangle "docs/dialect-extension-roadmap.md <<new>>" as Roadmap {
  port "未实现方言清单" as Unimpl
  port "竞品对比矩阵" as CmpMatrix
  port "必要性评估分类表" as EvalTable
  port "路线图" as Roadmap
}
participant "spec.md" as Spec

Planner --> Dialect : 读取当前 16 种方言
Planner --> Roadmap : 列出未实现方言 + 竞品对比
Planner --> Roadmap : 必要性评估分类 (必须/建议/暂不需要)
Planner --> Spec : 必须实现方言生成 EARS 需求
Planner --> Roadmap : 路线图 (版本里程碑)

@enduml
```

### 2.4.2 未实现方言清单与竞品对比

**未实现方言清单**（Hibernate/EF Core/SQLAlchemy 支持但 SZ-ORM 未实现）：

| 方言 | Hibernate | EF Core | SQLAlchemy | Rust 驱动 | 使用场景 | 实现难度 |
|------|-----------|---------|------------|----------|---------|---------|
| Informix | ✅ | ❌ | ✅ | ❌（无成熟驱动） | 企业遗留系统 | 高（无驱动） |
| SAP HANA | ✅ | ❌ | ✅ | ❌（无官方驱动） | SAP 生态 | 高（无驱动） |
| Firebird | ✅ | ❌ | ✅ | ✅（firebird-rs） | 中小企业 | 中 |
| CockroachDB | ❌ | ❌ | ✅ | ✅（crdb-rust） | 分布式 NewSQL | 低（PG 兼容） |
| YugabyteDB | ❌ | ❌ | ✅ | ✅（yb-rust） | 分布式 NewSQL | 低（PG 兼容） |
| Snowflake | ✅ | ❌ | ✅ | ✅（snowflake-api） | 云数仓 | 中 |
| Redshift | ✅ | ❌ | ✅ | ❌（无成熟驱动） | AWS 云数仓 | 高（无驱动） |
| Vertica | ✅ | ❌ | ❌ | ❌（无驱动） | 列式分析 | 高（无驱动） |
| Teradata | ✅ | ❌ | ❌ | ❌（无驱动） | 企业数仓 | 高（无驱动） |
| Interbase | ✅ | ❌ | ✅ | ❌（无驱动） | 遗留系统 | 高（无驱动） |
| Ingres | ✅ | ❌ | ❌ | ❌（无驱动） | 遗留系统 | 高（无驱动） |
| Cache (InterSystems) | ✅ | ❌ | ❌ | ❌（无驱动） | 医疗 | 高（无驱动） |
| FrontBase | ✅ | ❌ | ❌ | ❌（无驱动） | 遗留 | 高（无驱动） |
| MaxDB | ✅ | ❌ | ❌ | ❌（无驱动） | SAP 遗留 | 高（无驱动） |
| PointBase | ✅ | ❌ | ❌ | ❌（无驱动） | 嵌入式 | 高（无驱动） |
| Phoenix (HBase SQL) | ❌ | ❌ | ✅ | ❌（无驱动） | HBase 生态 | 高（无驱动） |
| Cassandra CQL | ❌ | ❌ | ✅ | ✅（cassandra-rs） | NoSQL | 高（非标准 SQL） |
| Spanner | ❌ | ❌ | ✅ | ❌（无驱动） | GCP 分布式 | 高（无驱动） |

### 2.4.3 必要性评估分类

**评估维度**：实际使用场景（是否有真实用户需求）+ Rust 生态需求（是否有 Rust 驱动）+ 实现难度（独立实现 vs 兼容委派）+ 市场趋势（数据库是否活跃）。

**分类评估表**：

| 分类 | 方言 | 评估理由 |
|------|------|---------|
| **必须实现** | CockroachDB | PG 兼容，可委派 PostgreSqlDialect；Rust 有 crdb-rust 驱动；分布式 NewSQL 趋势 |
| **必须实现** | YugabyteDB | PG 兼容，可委派 PostgreSqlDialect；Rust 有 yb-rust 驱动；分布式 NewSQL 趋势 |
| **建议实现** | Firebird | Rust 有 firebird-rs 驱动；中小企业场景；但用户需求不明朗 |
| **建议实现** | Snowflake | Rust 有 snowflake-api 驱动；云数仓趋势；但实现难度中 |
| **暂不需要（无 Rust 驱动）** | Informix/SAP HANA/Redshift/Vertica/Teradata/Interbase/Ingres/Cache/FrontBase/MaxDB/PointBase/Phoenix/Spanner | 无成熟 Rust 驱动，禁止实现无驱动支撑的方言（spec §5.4.1.6） |
| **暂不需要（非标准 SQL）** | Cassandra CQL | CQL 非标准 SQL，与 SZ-ORM SQL 抽象不匹配 |

### 2.4.4 必须实现方言 Dialect trait 实现方案

**CockroachDB 方言实现**（PG 兼容，委派 PostgreSqlDialect）：

```rust
// 概念示意（非最终代码）
// 1. db_type.rs 新增 DbType 变体
pub enum DbType {
    // ... 既有 19 变体
    CockroachDB,  // 新增
}

// 2. dialect.rs 用 delegate_dialect_to 宏委派 PostgreSqlDialect
delegate_dialect_to!(CockroachDbDialect, PostgreSqlDialect, DbType::CockroachDB);

// 3. get_dialect 新增分支
DbType::CockroachDB => Ok(Box::new(CockroachDbDialect)),
```

**YugabyteDB 方言实现**（PG 兼容，委派 PostgreSqlDialect）：同 CockroachDB 模式。

**feature gate 隔离**：新增方言通过 `dialect-cockroachdb`/`dialect-yugabytedb` feature gate 隔离（可选），或直接纳入默认 feature（因委派实现零成本）。

### 2.4.5 方言扩展路线图

| 版本里程碑 | 方言 | 实现方式 | 触发条件 |
|-----------|------|---------|---------|
| v3.5.0 | CockroachDB | 委派 PostgreSqlDialect | 必须实现（PG 兼容 + Rust 驱动 + 分布式趋势） |
| v3.5.0 | YugabyteDB | 委派 PostgreSqlDialect | 必须实现（PG 兼容 + Rust 驱动 + 分布式趋势） |
| v3.6.0+ | Firebird | 独立实现 | 建议实现，用户需求出现时实施 |
| v3.6.0+ | Snowflake | 独立实现 | 建议实现，用户需求出现时实施 |
| 不规划 | Informix/SAP HANA/Redshift 等 | — | 暂不需要（无 Rust 驱动），驱动出现时重新评估 |

## 2.5 方向 5：L1 缓存设计

### 2.5.1 上下文视图

```plantuml
@startuml
!theme plain
title 方向 5 L1 缓存设计 上下文视图

actor "ORM 库维护者" as Dev
rectangle "sz-orm-core" as Core {
  port "Session" as Session
  rectangle "l1_cache.rs <<new>>\n[l1-cache]" as L1 {
    port "L1Cache (Identity Map)" as L1Cache
    port "HashMap<TableKey, HashMap<PkValue, Arc<Mutex<Entity>>>>" as DataStruct
    port "LRU 淘汰 + max_size" as Lru
    port "AtomicU64 统计" as Stats
  }
  port "l2_cache.rs (L2Cache, 既有)" as L2
}
database "DB" as DB

Dev --> Session : query_by_pk(table, pk)
Session --> L1 : get(table, pk)
L1 --> L2 : L1 未命中查 L2
L2 --> DB : L2 未命中查 DB
DB --> L2 : 回填 L2
L2 --> L1 : 回填 L1
L1 --> Session : 返回对象 (Identity Map)

@enduml
```

### 2.5.2 L1Cache 数据结构设计

**L1Cache 结构体**（`packages/sz-orm-core/src/l1_cache.rs` 新增）：

```rust
// 概念示意（非最终代码）
pub struct L1Cache {
    /// 按表分桶，按主键缓存实体（Identity Map）
    data: RwLock<HashMap<String, HashMap<Value, Arc<Mutex<serde_json::Value>>>>>,
    /// LRU 访问顺序跟踪器（O(1) touch/remove/lru_key）
    access_order: RwLock<LruOrder>,
    /// 最大容量（LRU 淘汰）
    max_size: usize,
    /// 统计：hit_count（无锁原子计数）
    hit_count: AtomicU64,
    /// 统计：miss_count
    miss_count: AtomicU64,
    /// 统计：entry_count
    entry_count: AtomicU64,
    /// 统计：evict_count
    evict_count: AtomicU64,
}
```

**Identity Map 保证**：同一 Session 内同一主键查询返回同一 `Arc<Mutex<Entity>>` 引用（Arc ptr eq），修改该对象后后续查询看到修改（同一引用）。

**LRU 淘汰**：L1 缓存支持 max_size 配置 + LRU 淘汰，超限时淘汰最久未访问条目（复用既有 LruOrder，[l2_cache.rs:528](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/l2_cache.rs#L528)）。

### 2.5.3 L1→L2→DB 查询顺序流程

```plantuml
@startuml
!theme plain
title L1→L2→DB 查询协作流程

actor "Dev" as Dev
participant "Session" as Session
participant "L1Cache" as L1
participant "L2Cache" as L2
database "DB" as DB

== 查询流程 ==
Dev -> Session : query_by_pk(table, pk)
Session -> L1 : get(table, pk)
alt L1 命中
    L1 --> Session : 返回缓存对象 (Identity Map, Arc ptr eq)
    L1 -> L1 : hit_count.fetch_add(1)
else L1 未命中
    L1 -> L1 : miss_count.fetch_add(1)
    Session -> L2 : get(table, pk)
    alt L2 命中
        L2 --> Session : 返回 L2 缓存
        Session -> L1 : put(table, pk, entity) 回填 L1
    else L2 未命中
        Session -> DB : SELECT ... WHERE pk = ?
        DB --> Session : entity
        Session -> L1 : put 回填 L1
        Session -> L2 : put 回填 L2
    end
end
Session --> Dev : 返回对象

== 写操作失效 ==
Dev -> Session : update(entity)
Session -> DB : UPDATE ...
Session -> L1 : evict(table, pk) 自动失效
Session -> L2 : invalidate(table, pk) 通知 L2 失效

== Session 结束 ==
Dev -> Session : close/drop
Session -> L1 : clear() 自动清空

@enduml
```

### 2.5.4 L1 缓存生命周期管理

**生命周期与 Session 绑定**：
- Session 创建时 L1 缓存为空（new()）。
- Session 结束（close/drop）时 L1 缓存自动清空（Drop impl 调用 clear()）。
- L1 缓存不跨 Session 共享（跨 Session 共享是 L2 缓存职责）。

**L1 缓存失效策略**：
- 写操作（insert/update/delete）自动失效对应 L1 条目（按主键 evict）。
- 手动 evict(entity) / evict_table(table) / clear() 失效。
- Session 结束自动全清。
- LRU 淘汰：超 max_size 时淘汰最久未访问条目。

### 2.5.5 `l1-cache` feature gate 隔离方式

**Cargo.toml feature 定义**（新增）：

```toml
# v3.5.0：L1 缓存（Session 级别 Identity Map）
l1-cache = []
```

**条件编译隔离**：在 lib.rs 中，`pub mod l1_cache;` 用 `#[cfg(feature = "l1-cache")]` 条件编译隔离，默认 feature 关闭时不编译，既有 L2Cache 不受影响。

**与既有 L2Cache 的协作接口**：L1Cache 通过 `L2Cache::get`/`L2Cache::put`/`L2Cache::invalidate` 与 L2Cache 协作，既有 L2Cache 公开 API（[l2_cache.rs:517](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/l2_cache.rs#L517)）保持完全向后兼容。

### 2.5.6 实现设计文档

```plantuml
@startuml
!theme plain
title L1 缓存实现流程

start
:新增 packages/sz-orm-core/src/l1_cache.rs;
:实现 L1Cache 结构体 (HashMap 分桶 + LRU + AtomicU64 统计);
:实现 Identity Map (同主键同 Arc 引用);
:实现 L1→L2→DB 查询协作;
:实现写操作自动 evict + 手动 evict/clear;
:实现 Session 生命周期绑定 (Drop 自动清空);
:实现 LRU 淘汰 (复用既有 LruOrder);
:Cargo.toml 新增 l1-cache feature;
:lib.rs 新增 #[cfg(feature="l1-cache")] pub mod l1_cache;
:编写单元测试 (Identity Map + 查询顺序 + 失效 + 统计);
:编写集成测试 (L1+L2+DB 协作);
:运行 cargo test -p sz-orm-core --features l1-cache;
if (全通过 + 既有 L2Cache API 不变?) then (是)
  :完成;
else (否)
  :修复 + 重新测试;
endif
stop

@enduml
```

## 2.6 方向 6：crates.io 发布流程

### 2.6.1 上下文视图

```plantuml
@startuml
!theme plain
title 方向 6 crates.io 发布流程 上下文视图

actor "发布执行者" as Pub
rectangle "scripts/" as Scripts {
  port "publish-workspace.sh <<new>>" as Script35
  port "compute_topology.ps1 (既有)" as Topology
  port "publish_all.py (既有)" as PubAll
}
rectangle "Cargo.toml" as Cargo {
  port "workspace.package.version = 3.5.0" as Ver
  port "workspace.dependencies 版本号" as DepVer
}
cloud "crates.io" as Crates
rectangle "sz-pay" as Pay

Pub --> Cargo : 升级版本号 3.5.0
Pub --> Script35 : 执行拓扑发布
Script35 --> Topology : 计算依赖拓扑
Script35 --> PubAll : 调用既有发布逻辑
Script35 --> Crates : 按拓扑顺序发布
Pub --> Pay : 本地验证零回归 (不改 sz-pay)

@enduml
```

### 2.6.2 内部依赖包拓扑排序发布方案

**拓扑层级**（基于 workspace.dependencies 分析）：

| 层级 | 包名 | 内部依赖 | 发布顺序 |
|------|------|---------|---------|
| 第一层（无内部依赖） | sz-orm-macros | 无 | 1 |
| 第一层 | sz-orm-sql-validator | 无 | 2 |
| 第二层（依赖第一层） | sz-orm-core | sz-orm-macros + sz-orm-sql-validator | 3 |
| 第三层（依赖 core） | sz-orm-sqlx | sz-orm-core | 4 |
| 第三层 | sz-orm-oracle | sz-orm-core | 5 |
| 第三层 | sz-orm-mssql | sz-orm-core | 6 |
| 第三层 | sz-orm-query-builder | sz-orm-core | 7 |
| 第三层 | 各扩展包（auth/config/queue/scheduler/...） | sz-orm-core（按需） | 8-41 |
| 第四层（不发布） | cli | sz-orm-core + 各扩展包 | 不发布 |
| 第四层 | examples | sz-orm-core + 各扩展包 | 不发布 |

**拓扑排序算法**：复用既有 `scripts/compute_topology.ps1`（[scripts/compute_topology.ps1:1](file:///E:/vue/test/鲜视达/rust/sz-orm/scripts/compute_topology.ps1#L1)），该脚本已实现 workspace 依赖拓扑排序。

### 2.6.3 dry-run 验证流程

**dry-run 验证步骤**（每包发布前）：
1. `cargo publish -p <package> --dry-run` 验证包元数据（name/version/description/license/repository）。
2. 检查依赖可解析（内部依赖指向已发布版本）。
3. secrets 预检：扫描 .env/credentials/token/私钥模式，发现则阻断发布。
4. 包大小检查：`cargo package -p <package>` 检查包大小合理（< 10MB）。
5. dry-run 日志保存到 `logs/publish-dry-run-<package>-<timestamp>.log`。

### 2.6.4 sz-pay 升级零回归验证流程

**验证步骤**（发布后，ADR-0001 不修改 sz-pay 代码）：
1. 复制 sz-pay 项目到临时验证目录（不修改原项目）。
2. 修改临时目录的 Cargo.toml，将 sz-orm-* 版本号从 2.3.0 改为 3.5.0。
3. `cargo check` 验证编译通过。
4. `cargo test` 验证测试零回归（与 sz-pay 既有 5139 测试基线对比）。
5. 验证结果记录到 `logs/sz-pay-upgrade-<timestamp>.log`。
6. 删除临时验证目录。
7. 不提交任何 sz-pay 修改（ADR-0001 严禁修改下游仓库）。

### 2.6.5 `scripts/publish-workspace.sh` 脚本设计

**脚本职责**：按拓扑顺序将 v3.5.0 各包发布到 crates.io，dry-run 验证后实际发布，sz-pay 升级零回归验证。

**脚本流程**：
1. 前置检查：运行 10+ 道门禁（含门禁 14 文档同步）+ secrets 预检。
2. 版本号设置：workspace.package.version = 3.5.0，workspace.dependencies sz-orm-* version = 3.5.0。
3. 拓扑排序：调用 `scripts/compute_topology.ps1` 计算发布顺序。
4. 按拓扑顺序发布：
   - 第一层：sz-orm-macros → sz-orm-sql-validator
   - 第二层：sz-orm-core
   - 第三层：sz-orm-sqlx/oracle/mssql/扩展包
   - 第四层：cli/examples 不发布
5. 每包发布前 dry-run 验证，dry-run 全通过后实际发布。
6. 每包发布后验证 crates.io 页面可访问。
7. sz-pay 升级零回归验证（本地修改版本号，不提交）。

**脚本参数**：
- `--dry-run`：仅 dry-run 验证，不实际发布。
- `--skip-sz-pay-verify`：跳过 sz-pay 验证（用于快速发布）。
- `--token <token>`：crates.io token（默认从环境变量 `CARGO_REGISTRY_TOKEN` 读取）。

## 2.7 方向 7：async trait 风格统一

### 2.7.1 上下文视图

```plantuml
@startuml
!theme plain
title 方向 7 async trait 风格统一 上下文视图

actor "评估者" as Eval
rectangle "sz-orm-core pool.rs" as Pool {
  port "Connection trait (手动解糖, pool.rs:45)" as Conn
  port "ConnectionFactory trait (#[async_trait], pool.rs:732)" as Factory
}
rectangle "sz-orm-core model.rs" as Model {
  port "Model trait (#[async_trait], model.rs:271)" as ModelTrait
}
rectangle "docs/async-trait-evaluation.md <<new>>" as EvalDoc {
  port "方案 A/B 优缺点" as Pros
  port "性能基准对比" as Bench
  port "迁移影响分析" as Impact
  port "推荐方案" as Recommend
}
participant "sz-pay" as Pay

Eval --> Pool : 读取手动解糖 + #[async_trait] 混用
Eval --> EvalDoc : 输出评估文档 + 推荐方案
Eval --> Pay : 渐进迁移 + 零回归验证

@enduml
```

### 2.7.2 评估方案设计

**方案 A：统一为 `#[async_trait]` 宏**
- 优点：代码简洁，错误信息可读，新人学习成本低。
- 缺点：宏展开开销（编译时间增加），可能重新引入 HRTB 与 sqlx::Executor 冲突（Connection trait 手动解糖的技术原因）。
- 迁移影响：Connection trait 签名变更（手动解糖 → `#[async_trait]`），涉及所有 Connection impl + 调用方 + sz-pay。

**方案 B：统一为手动解糖**
- 优点：无宏依赖，无宏展开开销，避免 HRTB 冲突。
- 缺点：签名复杂（`fn execute<'a>(&'a mut self, ...) -> Pin<Box<dyn Future + 'a>>`），错误信息难读，新人学习成本高。
- 迁移影响：ConnectionFactory/Model 等 trait 签名变更（`#[async_trait]` → 手动解糖），涉及所有 impl + 调用方 + sz-pay。

**方案 C：保持现状 + 文档说明原因**（倾向推荐）
- 优点：无迁移成本，无 Breaking Change，Connection trait 手动解糖有技术原因（HRTB 冲突）。
- 缺点：混用风格保持，学习成本不降低。
- 迁移影响：无迁移，仅文档说明手动解糖原因 + `#[async_trait]` 使用指南。

**性能基准对比**：
- 编译时间：方案 A（`#[async_trait]` 宏展开）vs 方案 B（手动解糖无宏）vs 方案 C（现状）。
- 运行时开销：三者等价（`#[async_trait]` 宏展开后等价于手动解糖）。
- 基准数据附完整复现命令（criterion 基准测试）。

**迁移影响分析**：
- 手动解糖 trait 清单：Connection trait（[pool.rs:45](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L45)）。
- `#[async_trait]` trait 清单：ConnectionFactory（pool.rs:732）+ Model（model.rs:271）+ 多处 impl（pool.rs:1870/2190 等 16 处）。
- 每个 trait 迁移影响：涉及调用方 + 测试 + 下游 sz-pay。

### 2.7.3 渐进迁移方案（若推荐方案为 A 或 B）

**分阶段迁移**：
- 阶段 1：迁移 1-2 个非 Connection trait（如 Model trait），全量测试 + sz-pay 零回归验证。
- 阶段 2：迁移更多 trait，每阶段少量 trait + 全量测试 + sz-pay 零回归。
- 阶段 3：评估是否迁移 Connection trait（考虑 HRTB 冲突风险）。
- 不一次性迁移所有 trait。

**feature gate 隔离**：新增 `async-trait-unify` feature gate（可选），启用时按推荐方案迁移，关闭时保持现状。

## 2.8 方向 8：QueryBuilder 合并

### 2.8.1 上下文视图

```plantuml
@startuml
!theme plain
title 方向 8 QueryBuilder 合并 上下文视图

actor "合并评估者" as Eval
rectangle "sz-orm-core query.rs" as CoreQB {
  port "QueryBuilder<M: Model> (query.rs:36)" as CQB
}
rectangle "sz-orm-query-builder lib.rs" as ExtQB {
  port "Query (lib.rs:53)" as EQB
}
rectangle "docs/query-builder-merge-evaluation.md <<new>>" as EvalDoc {
  port "方案 A/B 优缺点" as Pros
  port "选择指南" as Guide
  port "推荐方案" as Recommend
}

Eval --> CoreQB : 读取 core::QueryBuilder 能力
Eval --> ExtQB : 读取 sz-orm-query-builder 能力
Eval --> EvalDoc : 输出评估文档 + 选择指南 + 推荐方案

@enduml
```

### 2.8.2 评估方案设计

**方案 A：合并为一个（sz-orm-query-builder 委派 core::QueryBuilder）**
- 优点：消除重叠，统一 API，用户无困惑。
- 缺点：sz-orm-query-builder 失去独立发布能力（依赖 sz-orm-core Model），委派实现可能引入额外开销。
- API 兼容性：sz-orm-query-builder API 保持兼容（委派实现），sz-pay cargo check 通过。
- 用户迁移成本：低（API 兼容，委派实现透明）。

**方案 B：保留两个 + 明确分工 + 选择指南 + deprecation 标注**（倾向推荐）
- 优点：保持 sz-orm-query-builder 独立发布能力，两个 QueryBuilder 适用场景不同（ORM 集成 vs 独立 SQL），选择指南消除困惑。
- 缺点：两个 QueryBuilder 长期共存，维护成本。
- API 兼容性：sz-orm-query-builder API 保持兼容，标注 deprecated + 指向选择指南。
- 用户迁移成本：低（不强制迁移，deprecated 标注引导）。

**性能基准对比**：
- core::QueryBuilder vs sz-orm-query-builder::Query：SQL 构造性能对比（相同查询）。
- 委派实现开销：方案 A 委派引入的间接调用开销。
- 基准数据附完整复现命令（criterion 基准测试）。

### 2.8.3 选择指南内容

**能力对比表**：

| 特性 | core::QueryBuilder<M> | sz-orm-query-builder::Query |
|------|----------------------|----------------------------|
| 绑定 Model | 是（`<M: Model>`） | 否 |
| 类型安全 | 编译期表/列校验 | 运行时字符串 |
| 适用场景 | ORM 完整流程 | 纯 SQL 构造、动态查询 |
| 依赖 | sz-orm-core 全部 | 仅 dialect 模块 |
| 独立发布 | 否 | 是 |
| 支持方言 | 16 种（8 独立 + 8 兼容） | 16 种 |
| 关联查询 | ✅（join/with） | ✅ |
| 事务支持 | ✅ | ❌ |
| 软删除 | ✅ | ❌ |
| 多租户 | ✅ | ❌ |

**适用场景说明**：
- core::QueryBuilder：ORM 完整流程（绑定 Model + 编译期校验 + 事务 + 软删除 + 多租户）。
- sz-orm-query-builder：纯 SQL 构造/动态查询（不依赖 Model + 独立发布）。

**迁移建议**：
- 若需 ORM 完整流程 → core::QueryBuilder。
- 若需纯 SQL 构造/独立发布 → sz-orm-query-builder。
- v3.5.0 不强制迁移，deprecated 标注引导。

### 2.8.4 渐进 deprecation 方案

**v3.5.0 执行**（推荐方案 B）：
- sz-orm-query-builder 保持可用（不立即删除）。
- 在 sz-orm-query-builder/src/lib.rs 顶部标注 `#![deprecated(note = "v3.5.0: 见 docs/query-builder-merge-evaluation.md 选择指南")]`。
- 在 README.md/文档中指向选择指南。
- v3.6.0+ 评估是否合并或删除。

## 2.9 方向 9：MOCK-ONLY 包真实后端补齐

### 2.9.1 上下文视图

```plantuml
@startuml
!theme plain
title 方向 9 MOCK-ONLY 包真实后端补齐 上下文视图

actor "真实后端开发者" as Dev
rectangle "sz-orm-es" as ES {
  port "InMemoryEsSync (Mock, 不变)" as Mock
  port "real_es.rs <<new>> [real]" as RealES
  port "EsSync trait" as EsSync
}
rectangle "sz-orm-config" as Cfg {
  port "ConsulConfigCenter (内存, 不变)" as Mem
  port "real_consul.rs <<new>> [real-consul]" as RealConsul
  port "real_nacos.rs <<new>> [real-nacos]" as RealNacos
  port "ConfigCenter trait" as CcTrait
}
cloud "真实 Elasticsearch" as RealEsCloud
cloud "真实 Consul" as RealConsulCloud
cloud "真实 Nacos" as RealNacosCloud

Dev --> ES : 实现真实 ES 后端
RealES --> RealEsCloud : elasticsearch crate / reqwest
Dev --> Cfg : 实现真实 Consul/Nacos 后端
RealConsul --> RealConsulCloud : reqwest HTTP API
RealNacos --> RealNacosCloud : reqwest HTTP API

@enduml
```

### 2.9.2 sz-orm-es 真实 ES 后端实现方案

**Cargo.toml 新增依赖**（optional，`real` feature 启用）：

```toml
[features]
default = []
real = ["dep:elasticsearch"]  # 或 dep:reqwest

[dependencies]
elasticsearch = { version = "8.5", optional = true }  # 或 reqwest
```

**真实 ES 后端实现**（`packages/sz-orm-es/src/real_es.rs` 新增）：
- 实现 `EsSync` trait（[es/lib.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-es/src/lib.rs) 既有 trait）。
- 索引：`index_doc(index, id, source)` → ES PUT `/{index}/_doc/{id}`。
- 搜索：`search(index, query)` → ES POST `/{index}/_search`。
- 聚合：`aggregate(index, aggs)` → ES POST `/{index}/_search` with `aggs`。
- 过滤：`filter(index, filter)` → ES POST `/{index}/_search` with `query`。
- 通过 `#[cfg(feature = "real")]` 条件编译隔离。

**真实 ES 集成测试**（`packages/sz-orm-es/tests/real_es_integration.rs` 新增）：
- 标注 `#[cfg(feature = "real")]` + `#[ignore]`（默认不运行）。
- 覆盖索引/搜索/聚合/过滤。
- 需真实 ES 环境：`cargo test --features real -- --ignored`。

### 2.9.3 sz-orm-config 真实 Consul/Nacos 实现方案

**真实 Consul 后端**（`packages/sz-orm-config/src/real_consul.rs` 新增）：
- 实现 `ConfigCenter` trait（[config/lib.rs:30](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-config/src/lib.rs#L30) 既有 trait）。
- 基于 reqwest 调用 Consul HTTP API：
  - `get(key)` → GET `v1/kv/{key}`。
  - `set(key, value)` → PUT `v1/kv/{key}`。
  - `delete(key)` → DELETE `v1/kv/{key}`。
  - `watch(key)` → GET `v1/kv/{key}?index=N`（长轮询）。
  - `subscribe(key, callback)` → 启动 watch 任务，变更触发回调。
- 认证：Consul ACL Token（Header `X-Consul-Token`）。
- 通过 `#[cfg(feature = "real-consul")]` 条件编译隔离。

**真实 Nacos 后端**（`packages/sz-orm-config/src/real_nacos.rs` 新增）：
- 实现 `ConfigCenter` trait。
- 基于 reqwest 调用 Nacos HTTP API：
  - `get(key)` → GET `v1/cs/configs?dataId={key}`。
  - `set(key, value)` → POST `v1/cs/configs`。
  - `delete(key)` → DELETE `v1/cs/configs?dataId={key}`。
  - `watch(key)` → POST `v1/cs/configs/listener`（长轮询）。
- 认证：Nacos Username+Password（Basic Auth）。
- 通过 `#[cfg(feature = "real-nacos")]` 条件编译隔离。

### 2.9.4 Mock 与真实差分测试方案

**差分测试设计**：
- 对相同输入验证 Mock 与真实后端输出语义一致。
- 索引：Mock `index_doc` vs 真实 ES `index_doc`，验证返回的 doc_id 一致。
- 搜索：Mock `search` vs 真实 ES `search`，验证返回的结果集语义一致（排序可能不同，按主键排序后对比）。
- 配置读写：Mock `get/set/delete` vs 真实 Consul/Nacos `get/set/delete`，验证返回值一致。
- 差分测试标注 `#[cfg(feature = "real")]` + `#[ignore]`（需真实环境）。

## 2.10 方向 10：文档与迁移指南补齐

### 2.10.1 上下文视图

```plantuml
@startuml
!theme plain
title 方向 10 文档与迁移指南补齐 上下文视图

actor "技术文档工程师" as DocEng
rectangle "sz-orm-core src/*.rs" as Src {
  port "313 个缺文档 pub API" as Api313
  port "lib.rs:406 docs.rs cfg 跳过" as CfgSkip
}
rectangle "docs/" as Docs {
  port "migration/diesel-to-sz-orm.md <<new>>" as Diesel
  port "migration/seaorm-to-sz-orm.md <<new>>" as SeaOrm
  port "migration/sqlx-to-sz-orm.md <<new>>" as Sqlx
}
participant "cargo doc" as CargoDoc
participant "cargo test --doc" as Doctest

DocEng --> Src : 补齐 313 pub API 文档
DocEng --> Src : 移除 docs.rs cfg 跳过
DocEng --> Docs : 编写三份迁移指南
Src --> CargoDoc : 无警告 + 文档完整
Docs --> Doctest : 示例代码 doctest 验证

@enduml
```

### 2.10.2 313 个 pub API 文档补齐方案

**补齐策略**：
1. 定位 313 个缺文档的 pub API（`cargo doc --workspace --no-deps` 2>&1 | grep "missing_docs"）。
2. 按优先级分批补齐：
   - 第一批：核心 API（QueryBuilder/Pool/Connection/L2Cache/Model 等 pub trait/struct/fn）。
   - 第二批：扩展 API（dialect/value/result_map 等）。
   - 第三批：测试/辅助 API。
3. 每个 API 补齐 `///` 文档注释（功能描述 + 参数 + 返回值 + 示例 + 错误）。
4. 移除 [lib.rs:406](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L406) docs.rs cfg 跳过，改为全局 `#![warn(missing_docs)]`。
5. `cargo doc --workspace --no-deps` 无警告验证。
6. `cargo test --workspace --doc` doctest 验证。
7. API 签名不变（仅新增 `///` 注释）。

### 2.10.3 Diesel/SeaORM/SQLx 迁移指南内容结构

**通用结构**（三份指南一致）：
1. **概述**：迁移目的、适用场景、预期收益。
2. **概念映射表**：源 ORM 概念 → sz-orm 概念。
3. **API 对照表**：源 ORM API → sz-orm API。
4. **示例代码**：CRUD/关联/事务/迁移。
5. **迁移注意事项**：类型安全差异、feature 对应、常见陷阱。
6. **附录**：完整迁移示例项目链接。

**Diesel 迁移指南特有**：
- 概念映射：Diesel schema → sz-orm Model、Diesel table! 宏 → sz-orm `#[derive(Model)]`。
- API 对照：Diesel `users.filter(id.eq(1))` → sz-orm `QueryBuilder::<User>::new().where_eq("id", 1)`。
- 类型安全差异：Diesel 编译期类型安全 vs sz-orm typed_ast DSL（`typed-dsl` feature）。

**SeaORM 迁移指南特有**：
- 概念映射：SeaORM Entity → sz-orm Model、SeaORM ActiveModel → sz-orm ActiveModel。
- API 对照：SeaORM `Entity::find().filter(Column::Id.eq(1))` → sz-orm `QueryBuilder::<User>::new().where_eq("id", 1)`。

**SQLx 迁移指南特有**：
- 概念映射：SQLx `query!` 宏 → sz-orm `query!` 宏、SQLx `query_as` → sz-orm QueryBuilder。
- API 对照：SQLx `query_as!(User, "SELECT * FROM users WHERE id = $1", id)` → sz-orm `QueryBuilder::<User>::new().where_eq("id", id).first()`。
- 编译期验证差异：SQLx 连真 DB 验证 vs sz-orm `db-verify` feature。

---

# 三、Feature Gate 设计

## 3.1 新增 Feature Gate 清单

| Feature | 所属包 | 默认 | 依赖 | 关联方向 | 说明 |
|---------|--------|------|------|---------|------|
| `l1-cache` | sz-orm-core | 关闭 | 无（复用既有 HashMap + parking_lot + L2Cache） | 方向 5 | L1 缓存（Session 级别 Identity Map） |
| `dialect-cockroachdb` | sz-orm-core | 关闭（或纳入默认，因委派零成本） | 无（委派 PostgreSqlDialect） | 方向 4 | CockroachDB 方言（PG 兼容委派） |
| `dialect-yugabytedb` | sz-orm-core | 关闭（或纳入默认，因委派零成本） | 无（委派 PostgreSqlDialect） | 方向 4 | YugabyteDB 方言（PG 兼容委派） |
| `async-trait-unify` | sz-orm-core | 关闭 | 无（复用既有 async-trait crate） | 方向 7 | async trait 风格统一渐进迁移（可选） |

**既有 Feature 复用**（不新增，v3.5.0 复用）：

| Feature | 所属包 | 默认 | 关联方向 | 说明 |
|---------|--------|------|---------|------|
| `typed-dsl` | sz-orm-core | 关闭 | 方向 2 | typed_ast 46 种新增表达式（[Cargo.toml:56](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L56) 既有） |
| `real` | sz-orm-es | 关闭 | 方向 9 | 真实 ES 后端（[es/Cargo.toml:15](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-es/Cargo.toml#L15) 既有占位，v3.5.0 实现真实后端） |
| `real-consul` | sz-orm-config | 关闭 | 方向 9 | 真实 Consul 后端（[config/Cargo.toml:14](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-config/Cargo.toml#L14) 既有） |
| `real-nacos` | sz-orm-config | 关闭 | 方向 9 | 真实 Nacos 后端（[config/Cargo.toml:14](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-config/Cargo.toml#L14) 既有） |
| `doc-completion` | sz-orm-core | 关闭 | 方向 10 | 313 pub API 文档补齐门禁矩阵标识（[Cargo.toml:42](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L42) 既有） |
| `migration-guide` | sz-orm-core | 关闭 | 方向 10 | 迁移指南门禁矩阵标识（[Cargo.toml:58](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L58) 既有） |

## 3.2 Feature 正交性矩阵

**正交性定义**：两个 feature 互不依赖，可任意组合启用/关闭，无冲突。

| Feature | typed-dsl | l1-cache | real | real-consul | real-nacos | dialect-cockroachdb | dialect-yugabytedb | async-trait-unify |
|---------|-----------|----------|------|-------------|------------|---------------------|-------------------|-------------------|
| typed-dsl | — | ✅ 正交 | ✅ 正交 | ✅ 正交 | ✅ 正交 | ✅ 正交 | ✅ 正交 | ✅ 正交 |
| l1-cache | ✅ | — | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| real | ✅ | ✅ | — | ✅ | ✅ | ✅ | ✅ | ✅ |
| real-consul | ✅ | ✅ | ✅ | — | ✅ | ✅ | ✅ | ✅ |
| real-nacos | ✅ | ✅ | ✅ | ✅ | — | ✅ | ✅ | ✅ |
| dialect-cockroachdb | ✅ | ✅ | ✅ | ✅ | ✅ | — | ✅ | ✅ |
| dialect-yugabytedb | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — | ✅ |
| async-trait-unify | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — |

**正交性结论**：所有新增 feature 互不依赖，可任意组合启用/关闭，无冲突。原因：
1. `typed-dsl` 仅影响 typed_ast.rs 表达式，不触碰其他模块。
2. `l1-cache` 仅新增 l1_cache.rs 模块，与 L2Cache 协作但通过公开 API，不依赖其他 feature。
3. `real`/`real-consul`/`real-nacos` 分别影响 sz-orm-es/sz-orm-config 不同包，互不依赖。
4. `dialect-cockroachdb`/`dialect-yugabytedb` 仅新增 DbType 变体 + Dialect 委派，不依赖其他 feature。
5. `async-trait-unify` 仅影响 trait 签名风格，不依赖其他 feature。

## 3.3 默认 Feature 零行为变更保证

**默认 feature**（[Cargo.toml:15](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L15)）：`default = ["redis"]`。

**零行为变更保证**：
1. 所有新增 feature（l1-cache/dialect-cockroachdb/dialect-yugabytedb/async-trait-unify）默认关闭，不在 default = ["redis"] 中。
2. 既有 feature 复用（typed-dsl/real/real-consul/real-nacos/doc-completion/migration-guide）默认关闭。
3. 默认 feature 关闭时：
   - 编译产物大小与 v3.4.0 一致（无新增代码编译）。
   - 运行时开销与 v3.4.0 一致（无新增逻辑执行）。
   - 既有公开 API 行为与 v3.4.0 一致（无行为变更）。
4. 真实后端依赖（elasticsearch/reqwest）为 optional，仅对应 feature 启用时引入，默认 feature 无新依赖（spec §4.1.4）。

---

# 四、兼容性设计

## 4.1 每个方向的 Breaking Change 风险评估

| 方向 | Breaking Change 风险 | 风险点 | 缓解措施 | sz-pay 影响评估 |
|------|---------------------|--------|---------|---------------|
| 1 文档同步约束 | **无** | 新增门禁 14，不改 API | — | 无（纯脚本 + 文档） |
| 2 typed_ast DSL | **无** | feature gate 隔离，既有 11 种表达式不变 | `typed-dsl` feature gate（默认关闭） | 无（sz-pay 未用 typed-dsl feature） |
| 3 无锁连接池文档 | **无** | 纯文档，不改代码 | — | 无 |
| 4 方言扩展 | **低** | 新增 DbType 变体（#[non_exhaustive] 允许），不改既有方言 | feature gate 隔离 + #[non_exhaustive] | 无（sz-pay 用既有 DbType 变体） |
| 5 L1 缓存 | **无** | feature gate 隔离，L2Cache API 不变 | `l1-cache` feature gate（默认关闭） | 无（sz-pay 未用 l1-cache feature） |
| 6 crates.io 发布 | **低** | SemVer minor 升级（3.4.0 → 3.5.0），向后兼容 | dry-run 验证 + sz-pay 零回归验证 | 低（sz-pay 从 2.3.0 升级到 3.5.0，dry-run + 零回归验证） |
| 7 async trait 统一 | **中** | Connection trait 签名变更风险 | 渐进迁移 + feature gate 隔离 + 评估期内签名不变 | 中（评估期内无影响，迁移后 sz-pay cargo check 验证） |
| 8 QueryBuilder 合并 | **中** | sz-orm-query-builder API 删除风险 | deprecation 标注 + 委派实现 + 不立即删除 | 中（sz-pay 用 sz-orm-query-builder？需验证，委派实现保持 API 兼容） |
| 9 MOCK-ONLY 补齐 | **无** | feature gate 隔离，Mock 行为不变 | `real`/`real-consul`/`real-nacos` feature gate（默认关闭） | 无（sz-pay 未用 real feature） |
| 10 文档补齐 | **无** | 纯文档，不改 API 签名 | — | 无 |

## 4.2 向后兼容方案

### 4.2.1 Feature Gate 隔离

所有新能力通过 feature gate 隔离，默认关闭，既有公开 API 行为不变：
- `typed-dsl`：46 种新增表达式仅在 feature 启用时编译，既有 11 种表达式不受影响。
- `l1-cache`：L1Cache 模块仅在 feature 启用时编译，既有 L2Cache API 不变。
- `real`/`real-consul`/`real-nacos`：真实后端仅在 feature 启用时编译，Mock/内存行为不变。
- `dialect-cockroachdb`/`dialect-yugabytedb`：新方言仅在 feature 启用时编译，既有 16 种方言不变。
- `async-trait-unify`：async trait 迁移仅在 feature 启用时生效，既有 trait 签名不变。

### 4.2.2 渐进 Deprecation

- sz-orm-query-builder：v3.5.0 标注 deprecated 但保持可用，不立即删除，给用户迁移周期（spec §4.5.3）。
- deprecated 标注指向选择指南（`docs/query-builder-merge-evaluation.md`）。
- v3.6.0+ 评估是否合并或删除。

### 4.2.3 SemVer 兼容

- v3.5.0 版本号遵循 SemVer，minor 版本升级（3.4.0 → 3.5.0）保证向后兼容（spec §4.5.4）。
- 既有公开 API 签名不变，新增 API 通过 feature gate 隔离。
- sz-pay 从 crates.io 2.3.0 升级到 3.5.0，dry-run + 零回归验证保证平滑升级。

## 4.3 v3.4.0 测试基线不回退保证

**基线**：v3.4.0 已验收 6,738 passed / 0 failed / 253 ignored（spec §1.2.1）。

**不回退保证**：
1. v3.5.0 不得使 v3.4.0 已验收的 6,738 测试基线回退，仅增不减（spec §4.2.1）。
2. 所有新增能力附测试（单元测试 + 集成测试按需），不修改既有测试。
3. 既有公开 API 签名不变，既有测试不受影响。
4. CI 门禁前置：`cargo test --workspace` 全通过才允许合入。
5. sz-pay 零回归验证：升级到 3.5.0 后 cargo test 零回归（与 sz-pay 既有 5139 测试基线对比）。

---

# 五、里程碑设计

## 5.1 6 个里程碑详细任务分解

### 5.1.1 M1：方向 1 文档同步约束 + 方向 6 crates.io 发布（最高优先级，低风险）

**周期**：2 周
**依赖**：无
**关联需求**：REQ-DOC-SYNC-001~004 + REQ-PUBLISH-001~007

**任务分解**：

| 任务 ID | 任务描述 | 子任务 | 预估工时 | 关联需求 |
|---------|---------|--------|---------|---------|
| M1-T1 | 编写 `scripts/check-doc-sync.py` 脚本 | 解析 git diff + 映射规则 + 检查文档更新 + 跳过标记 + 退出码 | 4h | REQ-DOC-SYNC-003 |
| M1-T2 | 定义受影响文档映射规则 | 10 类代码变更 → 应更新文档清单 | 2h | REQ-DOC-SYNC-002 |
| M1-T3 | AGENTS.md + engineering-practices.md 新增门禁 14 | 门禁表新增行 + 门禁 14 章节 + 映射规则 | 2h | REQ-DOC-SYNC-001 |
| M1-T4 | CI 配置新增 check-doc-sync job | .github/workflows/ci.yml 新增 job | 1h | REQ-DOC-SYNC-001 |
| M1-T5 | 编写 `scripts/publish-workspace.sh` 发布脚本 | 拓扑排序 + dry-run + 实际发布 + sz-pay 验证 | 6h | REQ-PUBLISH-002~005 |
| M1-T6 | 升级 workspace.package.version = 3.5.0 | Cargo.toml 版本号 + workspace.dependencies 版本号 | 1h | REQ-PUBLISH-004 |
| M1-T7 | secrets 预检脚本 | 扫描 .env/credentials/token/私钥模式 | 2h | REQ-PUBLISH-006 |
| M1-T8 | sz-pay 升级零回归验证 | 本地修改版本号 + cargo check + cargo test | 4h | REQ-PUBLISH-005 |
| M1-T9 | 实际发布到 crates.io | 按拓扑顺序执行 cargo publish | 2h | REQ-PUBLISH-004 |

**验收标准**：
- [ ] `scripts/check-doc-sync.py` 存在且可执行，未同步更新文档时退出码非 0
- [ ] AGENTS.md 和 engineering-practices.md 新增门禁 14
- [ ] CI 配置包含 check-doc-sync job
- [ ] workspace.package.version = 3.5.0，内部依赖版本 = 3.5.0
- [ ] 每包 `cargo publish --dry-run` 验证通过
- [ ] 按拓扑顺序实际发布到 crates.io，每包 crates.io 页面可访问
- [ ] sz-pay 升级 3.5.0 验证（cargo check + cargo test 零回归，不修改 sz-pay 代码）
- [ ] secrets 预检通过

### 5.1.2 M2：方向 2 typed_ast DSL 补齐 + 方向 5 L1 缓存（高优先级，中风险）

**周期**：3 周
**依赖**：M1（feature gate 体系就绪）
**关联需求**：REQ-DSL-001~010 + REQ-L1CACHE-001~007

**任务分解**：

| 任务 ID | 任务描述 | 子任务 | 预估工时 | 关联需求 |
|---------|---------|--------|---------|---------|
| M2-T1 | 聚合表达式 Max/Min/Sum/Avg/Count/CountStar | 6 种 ZST + TypedExpression + to_sql + 测试 | 6h | REQ-DSL-001 |
| M2-T2 | 算术表达式 Add/Sub/Mul/Div/Modulo | 5 种 ZST + Addable 类型检查 + to_sql + 测试 | 6h | REQ-DSL-002 |
| M2-T3 | 字符串表达式 Concat/ILike/Length/Lower/Upper/Trim/Substring | 7 种 ZST + 方言分派 + to_sql + 测试 | 6h | REQ-DSL-003 |
| M2-T4 | 日期表达式 Extract/Year/Month/Day/Hour/Minute/Second/Now | 8 种 ZST + to_sql + 测试 | 6h | REQ-DSL-004 |
| M2-T5 | 窗口表达式 Over/PartitionBy/OrderByInWindow/Lag/Lead/RowNumber/Rank/DenseRank | 8 种 ZST + 方言版本分派 + to_sql + 测试 | 8h | REQ-DSL-005 |
| M2-T6 | NULL 处理表达式 IsNull/IsNotNull/Coalesce/NullIf | 4 种 ZST + to_sql + 测试 | 4h | REQ-DSL-006 |
| M2-T7 | BETWEEN/DISTINCT/子查询表达式 | 6 种 ZST + DistinctOn PG-only + to_sql + 测试 | 6h | REQ-DSL-007 |
| M2-T8 | 类型转换表达式 Cast/As | 2 种 ZST + PG `::type` 语法 + to_sql + 测试 | 3h | REQ-DSL-008 |
| M2-T9 | 编译期 ZST 断言 + 差分测试 | static_assert size_of == 0 + typed_ast vs QueryBuilder SQL 一致 | 4h | REQ-DSL-010 |
| M2-T10 | 更新对比分析文档 §6.1 | "已对齐 Diesel 表达式覆盖度" + 对比表 | 2h | REQ-DSL-009 |
| M2-T11 | 新增 l1_cache.rs 模块 | L1Cache 结构体 + Identity Map + LRU + 统计 | 8h | REQ-L1CACHE-001 |
| M2-T12 | L1 缓存生命周期 + Session 绑定 | Drop 自动清空 + 不跨 Session | 4h | REQ-L1CACHE-002/007 |
| M2-T13 | L1 缓存失效策略 | 写操作自动 evict + 手动 evict/clear | 4h | REQ-L1CACHE-003 |
| M2-T14 | L1→L2→DB 查询协作 | 查询顺序 + 命中回填 + L2Cache API 不变 | 6h | REQ-L1CACHE-004 |
| M2-T15 | L1 缓存对象一致性保证 | 同主键同 Arc 引用 + 修改后查询看到修改 | 4h | REQ-L1CACHE-005 |
| M2-T16 | L1 缓存统计 API | hit/miss/entry/evict AtomicU64 无锁计数 | 3h | REQ-L1CACHE-006 |
| M2-T17 | Cargo.toml 新增 l1-cache feature + lib.rs 模块声明 | feature 定义 + #[cfg(feature="l1-cache")] | 2h | REQ-L1CACHE-001 |

**验收标准**：
- [ ] 46 种新增表达式实现（聚合/算术/字符串/日期/窗口/NULL/BETWEEN/DISTINCT/子查询/类型转换）
- [ ] 所有新增表达式为 ZST（编译期断言通过）
- [ ] `cargo test -p sz-orm-core --features typed-dsl` 全通过
- [ ] 既有 11 种表达式测试不回退
- [ ] 对比分析文档 §6.1 更新为"已对齐 Diesel 表达式覆盖度"
- [ ] L1Cache 类型实现（Identity Map）
- [ ] L1 缓存生命周期与 Session 绑定
- [ ] L1 缓存失效策略实现
- [ ] L1 与 L2 缓存协作（L1 → L2 → DB 查询顺序）
- [ ] L1 缓存对象一致性保证（同主键同引用）
- [ ] L1 缓存统计 API（无锁原子计数）
- [ ] `cargo test -p sz-orm-core --features l1-cache` 全通过
- [ ] 既有 L2Cache API 不变

### 5.1.3 M3：方向 9 MOCK-ONLY 包真实后端补齐（高优先级，中风险）

**周期**：2 周
**依赖**：M1（feature gate 体系就绪）
**关联需求**：REQ-MOCK-001~005

**任务分解**：

| 任务 ID | 任务描述 | 子任务 | 预估工时 | 关联需求 |
|---------|---------|--------|---------|---------|
| M3-T1 | sz-orm-es 真实 ES 后端实现 | elasticsearch crate optional 依赖 + EsSync trait 实现 + 索引/搜索/聚合/过滤 | 8h | REQ-MOCK-001 |
| M3-T2 | sz-orm-es 真实 ES 集成测试 | #[cfg(feature="real")] + #[ignore] + 索引/搜索/聚合/过滤覆盖 | 4h | REQ-MOCK-003 |
| M3-T3 | sz-orm-config 真实 Consul 后端实现 | reqwest HTTP API + ConfigCenter trait + 配置读写/监听/服务发现 + ACL Token 认证 | 6h | REQ-MOCK-002 |
| M3-T4 | sz-orm-config 真实 Nacos 后端实现 | reqwest HTTP API + ConfigCenter trait + 配置读写/监听 + Username+Password 认证 | 6h | REQ-MOCK-002 |
| M3-T5 | sz-orm-config 真实集成测试 | #[cfg(feature="real-consul/real-nacos")] + #[ignore] + 配置读写/监听/服务发现覆盖 | 4h | REQ-MOCK-003 |
| M3-T6 | Mock 与真实差分测试 | 相同输入 Mock vs 真实输出语义一致 | 6h | REQ-MOCK-004 |

**验收标准**：
- [ ] sz-orm-es 真实 ES 后端实现（`real` feature gate）
- [ ] sz-orm-config 真实 Consul/Nacos 后端实现（`real-consul`/`real-nacos` feature gate）
- [ ] 真实后端集成测试（`#[cfg(feature="real")]` + `#[ignore]`）
- [ ] Mock 与真实差分测试通过
- [ ] 真实后端依赖为 optional，不进默认 feature

### 5.1.4 M4：方向 7 async trait 统一 + 方向 8 QueryBuilder 合并（中优先级，中风险）

**周期**：2 周
**依赖**：M1（feature gate 体系就绪）
**关联需求**：REQ-ASYNC-001~004 + REQ-QB-MERGE-001~004

**任务分解**：

| 任务 ID | 任务描述 | 子任务 | 预估工时 | 关联需求 |
|---------|---------|--------|---------|---------|
| M4-T1 | async trait 风格统一评估 | 方案 A/B/C 优缺点 + 性能基准 + 迁移影响 + 学习成本 | 6h | REQ-ASYNC-001 |
| M4-T2 | 涉及 trait 清单与迁移影响分析 | 手动解糖 trait 列表 + #[async_trait] trait 列表 + 每个 trait 迁移影响 | 4h | REQ-ASYNC-002 |
| M4-T3 | 输出评估文档 + 推荐方案 | docs/async-trait-evaluation.md | 2h | REQ-ASYNC-001 |
| M4-T4 | 渐进迁移方案制定 | 分阶段迁移计划 + 每阶段测试 + sz-pay 零回归 | 4h | REQ-ASYNC-003 |
| M4-T5 | QueryBuilder 合并评估 | 方案 A/B 优缺点 + API 兼容 + 用户迁移成本 + 性能基准 | 6h | REQ-QB-MERGE-001 |
| M4-T6 | 选择指南编写 | 能力对比表 + 适用场景 + 性能基准 + 迁移建议 | 4h | REQ-QB-MERGE-002 |
| M4-T7 | 渐进合并/deprecation 执行 | deprecated 标注或委派实现 + 保持 API 兼容 + 不立即删除 | 4h | REQ-QB-MERGE-003 |

**验收标准**：
- [ ] async trait 评估文档输出（方案 A/B/C 优缺点 + 性能基准 + 迁移影响 + 推荐方案）
- [ ] 涉及 trait 清单与迁移影响分析
- [ ] 渐进迁移方案制定
- [ ] 迁移不引入 Breaking Change（sz-pay cargo check 通过）
- [ ] QueryBuilder 合并评估文档输出
- [ ] 选择指南编写（能力对比/适用场景/性能基准/迁移建议）
- [ ] 渐进合并/deprecation 执行
- [ ] sz-orm-query-builder v3.5.0 可用，API 兼容，不立即删除

### 5.1.5 M5：方向 4 方言扩展规划（低优先级，高风险）

**周期**：1 周
**依赖**：M1（feature gate 体系就绪）
**关联需求**：REQ-DIALECT-001~006

**任务分解**2：

| 任务 ID | 任务描述 | 子任务 | 预估工时 | 关联需求 |
|---------|---------|--------|---------|---------|
| M5-T1 | 列出当前 16 种方言清单 | 8 独立 + 8 兼容 + DbType 19 变体 + file:line 证据 | 2h | REQ-DIALECT-001 |
| M5-T2 | 列出未实现方言 + 竞品对比 | Hibernate/EF Core/SQLAlchemy 方言清单 + 差集 | 4h | REQ-DIALECT-002 |
| M5-T3 | 必要性评估分类 | 按使用场景/Rust 驱动/实现难度/市场趋势四维评估 | 4h | REQ-DIALECT-003 |
| M5-T4 | 必须实现方言 EARS 需求生成 | CockroachDB/YugabyteDB EARS 需求 | 2h | REQ-DIALECT-004 |
| M5-T5 | CockroachDB 方言实现 | delegate_dialect_to PostgreSqlDialect + DbType 变体 + 测试 | 3h | REQ-DIALECT-004 |
| M5-T6 | YugabyteDB 方言实现 | delegate_dialect_to PostgreSqlDialect + DbType 变体 + 测试 | 3h | REQ-DIALECT-004 |
| M5-T7 | 方言扩展路线图写入 spec.md | 版本里程碑 + 触发条件 + 暂不需要理由 | 2h | REQ-DIALECT-005 |

**验收标准**：
- [ ] 当前 16 种方言清单列出（附 file:line 证据）
- [ ] 对比分析文档 §6.7 更新"8 种"为"16 种"
- [ ] 未实现方言清单与竞品对比列出
- [ ] 必要性评估分类表输出（必须/建议/暂不需要）
- [ ] 必须实现方言 EARS 需求生成
- [ ] CockroachDB/YugabyteDB 方言实现（委派 PostgreSqlDialect）
- [ ] 方言扩展路线图写入 spec.md

### 5.1.6 M6：方向 3 连接池文档 + 方向 10 文档与迁移指南（低优先级，低风险）

**周期**：2 周
**依赖**：M2（typed_ast DSL 补齐完成，迁移指南引用新表达式）
**关联需求**：REQ-POOL-DOC-001~004 + REQ-DOC-FILL-001~005

**任务分解**：

| 任务 ID | 任务描述 | 子任务 | 预估工时 | 关联需求 |
|---------|---------|--------|---------|---------|
| M6-T1 | 编写无锁连接池架构文档 | 工作原理 + 数据结构图解 + acquire/release 时序图 + 并发安全证明 + 竞品对比 | 8h | REQ-POOL-DOC-001~003 |
| M6-T2 | 313 pub API 文档补齐（第一批） | 核心 API（QueryBuilder/Pool/Connection/L2Cache/Model） | 8h | REQ-DOC-FILL-001 |
| M6-T3 | 313 pub API 文档补齐（第二批） | 扩展 API（dialect/value/result_map） | 6h | REQ-DOC-FILL-001 |
| M6-T4 | 313 pub API 文档补齐（第三批） | 测试/辅助 API | 4h | REQ-DOC-FILL-001 |
| M6-T5 | 移除 docs.rs cfg 跳过 | lib.rs:406 改为全局 #![warn(missing_docs)] | 1h | REQ-DOC-FILL-001 |
| M6-T6 | Diesel 迁移指南 | 概念映射 + API 对照 + 示例 + 注意事项 | 6h | REQ-DOC-FILL-002 |
| M6-T7 | SeaORM 迁移指南 | 概念映射 + API 对照 + 示例 + 注意事项 | 4h | REQ-DOC-FILL-003 |
| M6-T8 | SQLx 迁移指南 | 概念映射 + API 对照 + 示例 + 注意事项 | 4h | REQ-DOC-FILL-004 |
| M6-T9 | doctest 验证 | cargo test --workspace --doc + cargo doc --workspace --no-deps 无警告 | 2h | REQ-DOC-FILL-005 |

**验收标准**：
- [ ] 架构文档编写完成（工作原理/数据结构/acquire/release 流程/并发安全证明）
- [ ] 竞品对比完成（Diesel r2d2 / SeaORM / SQLx）
- [ ] 无锁正确性证明完成
- [ ] 每条结论附 file:line 证据
- [ ] 313 个 pub API 文档补齐
- [ ] 移除 docs.rs cfg 跳过
- [ ] `cargo doc --workspace --no-deps` 无警告
- [ ] Diesel/SeaORM/SQLx 迁移指南编写
- [ ] `cargo test --workspace --doc` doctest 通过

## 5.2 里程碑间依赖关系

```plantuml
@startuml
!theme plain
title v3.5.0 里程碑依赖关系

rectangle "M1 文档同步约束 + crates.io 发布\n(2周, 最高优先级, 低风险)" as M1
rectangle "M2 typed_ast DSL + L1 缓存\n(3周, 高优先级, 中风险)" as M2
rectangle "M3 MOCK-ONLY 补齐\n(2周, 高优先级, 中风险)" as M3
rectangle "M4 async trait + QB 合并\n(2周, 中优先级, 中风险)" as M4
rectangle "M5 方言扩展规划\n(1周, 低优先级, 高风险)" as M5
rectangle "M6 连接池文档 + 文档与迁移指南\n(2周, 低优先级, 低风险)" as M6

M1 --> M2 : feature gate 体系就绪
M1 --> M3 : feature gate 体系就绪
M1 --> M4 : feature gate 体系就绪
M1 --> M5 : feature gate 体系就绪
M2 --> M6 : typed_ast DSL 补齐完成 (迁移指南引用新表达式)

note bottom of M1
  **关键路径**：M1 → M2 → M6（串行 7 周）
  **并行机会**：
  - M1 完成后 M2/M3/M4/M5 可并行
  - M2 内部：46 种表达式 + L1 缓存可并行
  - M3 内部：ES + Consul/Nacos 可并行
  - M6 内部：架构文档 + 313 API 文档 + 三份迁移指南可并行
  **总周期**：关键路径 7 周；并行开发下可压缩至 5-6 周
end note

@enduml
```

## 5.3 每个里程碑的验收标准

（已在 §5.1.1~§5.1.6 各里程碑任务分解后列出验收标准，此处汇总）

| 里程碑 | 验收标准数 | 关键验收标准 |
|--------|-----------|------------|
| M1 | 9 | check-doc-sync.py 可执行 + 门禁 14 + crates.io v3.5.0 发布 + sz-pay 零回归 |
| M2 | 14 | 46 种表达式 ZST + L1Cache Identity Map + L1→L2→DB 协作 + 既有 API 不变 |
| M3 | 5 | 真实 ES/Consul/Nacos + 集成测试 + 差分测试 + optional 依赖 |
| M4 | 8 | 评估文档 + 选择指南 + 渐进迁移/deprecation + 不引入 Breaking Change |
| M5 | 7 | 16 方言清单 + 分类评估 + CockroachDB/YugabyteDB 实现 + 路线图 |
| M6 | 9 | 架构文档 + 313 API 文档 + 迁移指南 + doctest 通过 |

---

# 六、测试策略

## 6.1 每个方向的测试方案

### 6.1.1 方向 1：文档同步更新约束规则化

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| 单元测试 | check-doc-sync.py 脚本逻辑（解析 git diff + 映射规则 + 检查文档更新 + 跳过标记） | `python -m pytest tests/test_check_doc_sync.py` | 90% 行覆盖 |
| 集成测试 | 端到端门禁 14 流程（代码变更未同步文档 → 阻断；同步文档 → 通过） | `python scripts/check-doc-sync.py --diff <test-diff>` | 关键场景覆盖 |
| CI 集成测试 | CI check-doc-sync job 触发 + 退出码传递 | CI PR 触发 | 阻断场景验证 |

### 6.1.2 方向 2：typed_ast DSL 补齐

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| 单元测试 | 46 种表达式 to_sql 生成正确 SQL + 类型检查 + ZST 断言 | `cargo test -p sz-orm-core --features typed-dsl` | 每表达式至少 3 测试（正常/边界/错误） |
| 差分测试 | typed_ast vs QueryBuilder SQL 一致（相同查询生成相同 SQL） | `cargo test -p sz-orm-core --features typed-dsl --test dsl_diff` | 关键查询类型覆盖 |
| 五方言测试 | to_sql 按方言分派正确（MySQL/PG/SQLite/Oracle/MSSQL） | `cargo test -p sz-orm-core --features typed-dsl --test dialect_dispatch` | 五方言行为一致 |
| 基准测试 | ZST 零运行时开销（表达式构造 vs 既有 11 种表达式） | `cargo bench -p sz-orm-core --features typed-dsl` | 无开销增加 |
| 既有测试不回退 | 既有 11 种表达式测试全通过 | `cargo test -p sz-orm-core` | 6,738 基线不回退 |

### 6.1.3 方向 3：无锁连接池架构文档

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| Doctest | 架构文档 doctest（引用 pool.rs 代码示例） | `cargo test --workspace --doc` | doctest 通过 |
| 文档与代码一致性 | 文档引用的 file:line 真实存在 + 描述行为与代码一致 | `bash scripts/audit-verify.sh docs/lock-free-pool-design.md` | 所有 file:line 验证通过 |
| 基准复现 | 竞品对比基准数据可复现 | 按文档复现命令执行 | 基准数据可复现 |

### 6.1.4 方向 4：方言扩展规划

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| 单元测试 | CockroachDB/YugabyteDB 方言 Dialect trait 实现 + delegate_dialect_to 委派正确 | `cargo test -p sz-orm-core --features dialect-cockroachdb,dialect-yugabytedb` | 委派行为正确 |
| 五方言行为一致 | 新方言与基础方言（PG）行为一致 | `cargo test -p sz-orm-core --test dialect_consistency` | 行为一致 |
| 既有测试不回退 | 既有 16 种方言测试全通过 | `cargo test -p sz-orm-core` | 6,738 基线不回退 |

### 6.1.5 方向 5：L1 缓存设计

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| 单元测试 | L1Cache Identity Map（同主键同 Arc 引用）+ LRU 淘汰 + 统计计数 | `cargo test -p sz-orm-core --features l1-cache` | 90% 行覆盖 |
| 集成测试 | L1→L2→DB 查询协作 + 写操作自动 evict + Session 结束清空 | `cargo test -p sz-orm-core --features l1-cache --test l1_l2_db` | 关键场景覆盖 |
| 并发测试 | L1 缓存线程安全（多线程并发查询/修改） | `cargo test -p sz-orm-core --features l1-cache --test l1_concurrent` | 无数据竞争 |
| 既有测试不回退 | 既有 L2Cache 测试全通过 | `cargo test -p sz-orm-core` | L2Cache API 不变 |

### 6.1.6 方向 6：crates.io 发布流程

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| dry-run 验证 | 每包 `cargo publish --dry-run` 通过 | `bash scripts/publish-workspace.sh --dry-run` | 全包 dry-run 通过 |
| secrets 预检 | 发布产物无 secrets | `python scripts/check-secrets.py` | 无 secrets |
| sz-pay 零回归 | sz-pay 升级 3.5.0 后 cargo check + cargo test 零回归 | `bash scripts/verify-sz-pay.sh --version 3.5.0` | 5139 测试基线不回退 |
| crates.io 页面验证 | 每包 crates.io 页面可访问 | 手动验证 | 全包可访问 |

### 6.1.7 方向 7：async trait 风格统一

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| 性能基准 | 方案 A/B/C 编译时间 + 运行时开销对比 | `cargo bench -p sz-orm-core --features async-trait-unify` | 基准数据可复现 |
| 渐进迁移测试 | 每阶段迁移后全量测试 + sz-pay 零回归 | `cargo test --workspace + sz-pay cargo test` | 每阶段零回归 |
| 既有测试不回退 | 既有 trait 签名不变（评估期内） | `cargo test --workspace` | 6,738 基线不回退 |

### 6.1.8 方向 8：QueryBuilder 合并

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| 性能基准 | core::QueryBuilder vs sz-orm-query-builder::Query SQL 构造性能对比 | `cargo bench -p sz-orm-core -p sz-orm-query-builder` | 基准数据可复现 |
| API 兼容测试 | sz-orm-query-builder deprecated 标注后 API 仍可用 | `cargo test -p sz-orm-query-builder` | API 兼容 |
| sz-pay 零回归 | sz-pay cargo check 通过（sz-orm-query-builder API 兼容） | `bash scripts/verify-sz-pay.sh` | sz-pay 零回归 |

### 6.1.9 方向 9：MOCK-ONLY 包真实后端补齐

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| 单元测试 | 真实 ES/Consul/Nacos 后端实现逻辑 | `cargo test -p sz-orm-es --features real + cargo test -p sz-orm-config --features real-consul,real-nacos` | 80% 行覆盖 |
| 真实集成测试 | 真实 ES/Consul/Nacos 环境集成（索引/搜索/聚合/过滤/配置读写/监听/服务发现） | `cargo test -p sz-orm-es --features real -- --ignored + cargo test -p sz-orm-config --features real-consul,real-nacos -- --ignored` | 关键场景覆盖 |
| 差分测试 | Mock 与真实后端行为语义一致 | `cargo test -p sz-orm-es --features real --test diff + cargo test -p sz-orm-config --features real-consul,real-nacos --test diff` | 语义一致 |
| 既有测试不回退 | Mock/内存行为不变（默认 feature） | `cargo test -p sz-orm-es + cargo test -p sz-orm-config` | Mock/内存行为不变 |

### 6.1.10 方向 10：文档与迁移指南补齐

| 测试类型 | 测试内容 | 测试命令 | 覆盖率目标 |
|---------|---------|---------|-----------|
| 文档构建 | `cargo doc --workspace --no-deps` 无警告 + docs.rs 文档完整 | `cargo doc --workspace --no-deps` | 无 missing_docs 警告 |
| Doctest | 迁移指南示例代码 doctest 通过 | `cargo test --workspace --doc` | doctest 通过 |
| 文档与代码一致性 | 迁移指南 API 对照表与实际 API 一致 | 代码审查 | API 一致 |

## 6.2 五方言集成测试覆盖

**五方言覆盖约束**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL，所有新能力必须保持五方言行为一致（spec §1.2.18）。

**五方言集成测试矩阵**：

| 新能力 | MySQL | PostgreSQL | SQLite | Oracle | MSSQL | 测试命令 |
|--------|-------|------------|--------|--------|-------|---------|
| typed_ast 46 种表达式 | ✅ | ✅（含 ILike/DistinctOn/`::type`） | ✅ | ✅ | ✅ | `cargo test -p sz-orm-core --features typed-dsl --test integration_*` |
| L1 缓存 | ✅ | ✅ | ✅ | ✅ | ✅ | `cargo test -p sz-orm-core --features l1-cache --test integration_*` |
| CockroachDB/YugabyteDB 方言 | N/A | ✅（委派 PG） | N/A | N/A | N/A | `cargo test -p sz-orm-core --features dialect-cockroachdb,dialect-yugabytedb` |
| 真实 ES/Consul/Nacos 后端 | N/A（非关系型） | N/A | N/A | N/A | N/A | `cargo test --features real/real-consul/real-nacos -- --ignored` |

**五方言测试环境**（本机数据库，AGENTS.md 记录）：
- MySQL 9.6：`mysql://root:test123@127.0.0.1:3306/sz_orm_test`
- PostgreSQL 18：`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`
- SQLite：内存数据库
- Oracle 23ai Free：`127.0.0.1:1521/freepdb1`（用户 sys，密码 test123，Sysdba 权限）
- MSSQL：待配置（或通过 Docker）

## 6.3 sz-pay 零回归验证

**验证流程**（ADR-0001 不修改 sz-pay 代码）：
1. 复制 sz-pay 项目到临时验证目录（`E:\vue\test\sz-pay-upgrade-verify\`）。
2. 修改临时目录的 `Cargo.toml`，将 sz-orm-* 版本号从 2.3.0 改为 3.5.0。
3. 设置编译环境：`$env:RUST_MIN_STACK="67108864"` + `$env:CARGO_INCREMENTAL=0`。
4. `cargo check` 验证编译通过。
5. `cargo test` 验证测试零回归（与 sz-pay 既有 5139 测试基线对比）。
6. 验证结果记录到 `logs/sz-pay-upgrade-<timestamp>.log`。
7. 删除临时验证目录。
8. 不提交任何 sz-pay 修改（ADR-0001 严禁修改下游仓库）。

**验证脚本**：`scripts/verify-sz-pay.sh --version 3.5.0`（复用既有 [scripts/verify_sz_pay.ps1](file:///E:/vue/test/鲜视达/rust/sz-orm/scripts/verify_sz_pay.ps1#L1)）。

**零回归标准**：
- `cargo check` 通过（无编译错误）。
- `cargo test` 全通过（与 sz-pay 既有 5139 测试基线对比，0 failed）。
- 无 Breaking Change（sz-orm-* API 兼容）。

---

# 七、设计决策汇总

## 7.1 关键设计决策

| 决策点 | 选项 | 选择 | 理由 | file:line 证据 |
|--------|------|------|------|--------------|
| 门禁 14 实现方式 | A. 复用门禁 12 / B. 新增独立 check-doc-sync.py | B | 检查逻辑不同（静态数据 vs 动态变更触发），独立脚本职责单一 | [engineering-practices.md:269](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/sz-orm-engineering-practices.md#L269)（门禁 12） |
| typed_ast 46 种表达式隔离 | A. 既有 typed-dsl / B. 新增 typed-dsl-full | A | typed-dsl 既有，复用避免 feature 爆炸 | [Cargo.toml:56](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L56) |
| L1 缓存数据结构 | A. 按表分桶 HashMap / B. 复合键 HashMap | A | 按表分桶便于表级失效，按主键缓存实现 Identity Map | [l2_cache.rs:517](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/l2_cache.rs#L517)（L2Cache 参考） |
| sz-orm-es 真实 ES 依赖 | A. elasticsearch crate / B. reqwest 自研 | A | 官方维护，API 完整，optional 不进默认 | [es/Cargo.toml:15](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-es/Cargo.toml#L15)（real feature 既有） |
| sz-orm-config 真实实现 | A. 官方 crate / B. reqwest 自研 | B | 无成熟官方 crate，reqwest 已是 optional 依赖 | [config/Cargo.toml:14](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-config/Cargo.toml#L14) |
| async trait 统一推荐 | A. #[async_trait] / B. 手动解糖 / C. 保持现状+文档 | C（倾向） | Connection trait 手动解糖有技术原因（HRTB 冲突） | [pool.rs:41](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L41)（HRTB 冲突注释） |
| QueryBuilder 合并推荐 | A. 合并委派 / B. 保留+指南+deprecation | B（倾向） | 适用场景不同，保持独立发布能力 | [query-builder/lib.rs:53](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-query-builder/src/lib.rs#L53)（区别说明已有） |
| 方言扩展"必须实现"判定 | A. Rust 驱动 / B. 用户需求 / C. 综合 A+B+市场 | C | 单一维度易偏差，四维综合评估 | [dialect.rs:1429](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1429)（delegate_dialect_to 宏） |
| crates.io 版本号 | A. 3.5.0 minor / B. 4.0.0 major | A | feature gate 隔离，向后兼容，SemVer minor | [Cargo.toml:6](file:///E:/vue/test/鲜视达/rust/sz-orm/Cargo.toml#L6)（当前 3.4.0） |
| CockroachDB/YugabyteDB 方言实现 | A. 独立实现 / B. 委派 PostgreSqlDialect | B | PG 兼容，委派零成本，减少实现成本 | [dialect.rs:1502](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1502)（KingbaseDialect 委派 PG） |
| 无锁连接池文档形式 | A. design.md 章节 / B. 独立 docs/lock-free-pool-design.md | B | 独立文档便于引用 + 沉淀架构知识 | [pool.rs:751](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L751)（ArrayQueue） |
| 313 pub API 文档补齐策略 | A. 一次性补齐 / B. 分批补齐（核心→扩展→测试） | B | 分批降低风险，优先核心 API | [lib.rs:406](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L406)（docs.rs cfg 跳过） |

## 7.2 与 spec.md 需求追溯

| spec 需求组 | design 章节 | 验收条件覆盖 |
|------------|-----------|------------|
| REQ-DOC-SYNC-001~004（方向 1） | §2.1 + §5.1.1（M1） + §6.1.1 | spec §9.1 全覆盖 |
| REQ-DSL-001~010（方向 2） | §2.2 + §5.1.2（M2） + §6.1.2 | spec §9.2 全覆盖 |
| REQ-POOL-DOC-001~004（方向 3） | §2.3 + §5.1.6（M6） + §6.1.3 | spec §9.3 全覆盖 |
| REQ-DIALECT-001~006（方向 4） | §2.4 + §5.1.5（M5） + §6.1.4 | spec §9.4 全覆盖 |
| REQ-L1CACHE-001~007（方向 5） | §2.5 + §5.1.2（M2） + §6.1.5 | spec §9.5 全覆盖 |
| REQ-PUBLISH-001~007（方向 6） | §2.6 + §5.1.1（M1） + §6.1.6 | spec §9.6 全覆盖 |
| REQ-ASYNC-001~004（方向 7） | §2.7 + §5.1.4（M4） + §6.1.7 | spec §9.7 全覆盖 |
| REQ-QB-MERGE-001~004（方向 8） | §2.8 + §5.1.4（M4） + §6.1.8 | spec §9.8 全覆盖 |
| REQ-MOCK-001~005（方向 9） | §2.9 + §5.1.3（M3） + §6.1.9 | spec §9.9 全覆盖 |
| REQ-DOC-FILL-001~005（方向 10） | §2.10 + §5.1.6（M6） + §6.1.10 | spec §9.10 全覆盖 |

---

> 本文档基于 SZ-ORM v3.4.0 已验收基线（6,738 passed / 0 failed / 253 ignored）+ v3.5.0 spec.md 10 方向 60 条 EARS 需求生成，所有代码结论附 `file:line` 证据，文档定位为技术设计（How to build），对应需求规格 `docs/spec/v3.5.0/spec.md`（What to build）。
> 生成日期：2026-08-09
> 代码基线：[Cargo.toml:6](file:///E:/vue/test/鲜视达/rust/sz-orm/Cargo.toml#L6) workspace.package.version = "3.4.0"
> 目标版本：v3.5.0
> 设计约束：Rust 2021 Edition / rust-version 1.81 / API 向后兼容（无 Breaking Change）/ 禁止占位实现 / unsafe 零容忍 / 参数化查询铁律 / Feature 隔离 / 五方言行为一致 / ADR-0001 严禁修改下游/上游仓库 / 编译时 `$env:RUST_MIN_STACK="67108864"` + `$env:CARGO_INCREMENTAL=0`