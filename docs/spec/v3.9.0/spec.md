# sz-orm v3.9.0 需求规格说明书

> 版本：v3.9.0（criterion benchmark 套件 + semver/API 稳定性 + 数据验证框架 + 迁移 dry-run/影响分析 + CI/CD 模板 + 流式导出）
> 基线：v3.8.0（已完成生产部署就绪检查清单 15 项 + ORM 库特性适配 + 配置化与验证体系 + 五方言连接安全，6760 tests passed 0 failed）
> 日期：2026-08-10
> 需求格式：EARS（Ubiquitous / Event-driven / State-driven / Optional / Unwanted）
> 文档定位：需求规格（What to build），不含技术设计（How to build）
> 优先级声明：六项任务按"P1（benchmark/semver/validator，生产用户信心与质量量化）→ P2（迁移 dry-run/CI 模板/流式导出，风险降低与生态便利）"序推进
> 需求编号约定：REQ-V39-xxx（v3.9.0 需求项）
> 缺陷来源：`docs/sz-orm与同类产品对比分析.md` 第 5.2 节技术弱点 + 第 6.1 节短期优化方向：无正式 benchmark 套件（`:229`）、无 API 稳定性保证（`:235`）、无数据验证框架（`:232`）、无迁移 dry-run+影响分析（`:243`）、无 CI/CD 模板（`:236`）、无查询结果流式导出（`:242`）
> 兼容性铁律：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容，v3.8.0 已验收测试基线（6760 passed）不回退；sz-pay 生产依赖（从 crates.io 拉取 sz-orm-* 6 个包）不得被破坏；五方言覆盖：MySQL/PostgreSQL/SQLite/Oracle/MSSQL
> 范围声明：本版本聚焦短期 P1-P2 共 6 项任务；中期（v4.0.0 AI 调优闭环/多 LLM/混合搜索/data lineage/分片 rebalance/failover/服务网格/GraphQL/CDC）与长期（v4.x+ Go/Java/C++ 绑定/社区扩展/可视化 Schema/缓存一致性/跨语言事务/Informix 真实驱动）在后续版本规划；crates.io 全 46 包发布与英文文档翻译放最后执行，本版本不涉及

---

# 1. 组件定位

## 1.1 核心职责

本组件负责交付 sz-orm v3.9.0 的六项短期优化能力：(1) criterion benchmark 套件以量化与 Diesel/SeaORM/SQLx 的性能对比；(2) semver 兼容性策略与 API 稳定性自动化保证以建立生产用户信心；(3) 数据验证框架（validator 集成）以减少用户样板代码；(4) 迁移 dry-run 与影响分析以降低迁移风险；(5) CI/CD GitHub Actions 可复用模板以降低用户部署门槛；(6) 查询结果流式导出（CSV/Parquet）以支持大数据集低内存导出。所有能力通过 feature gate 隔离，不破坏现有 API 兼容性与 v3.8.0 已验收基线。

## 1.2 核心输入

1. **v3.8.0 已验收基线**：生产部署就绪检查清单 15 项、五方言连接安全、6760 tests passed 0 failed，作为本版本基准。
2. **对比分析文档第 6.1 节**：`docs/sz-orm与同类产品对比分析.md:286-297` 短期（v3.9.0）优化方向 8 项（本版本取 P1-P2 共 6 项，P0 的 crates.io 发布与英文文档翻译放最后）。
3. **现有能力清单与缺口证据**：
   - **benchmark**：`packages/sz-orm-core/benches/` 已有 9 个 bench 文件（core_bench/l1_cache_bench/typed_overhead_bench 等）、`bench-comparison/benches/` 已有 13 个文件（orm_comparison/cross_db_comparison/full_comparison 等），criterion 0.5 已引入（`packages/sz-orm-core/Cargo.toml:177`、`bench-comparison/Cargo.toml:32`），bench-comparison 已对比 Diesel/SeaORM/SQLx（`bench-comparison/Cargo.toml:6`）。缺口：无系统化回归基准、无自动化量化对比报告、bench-comparison 独立于 workspace（`bench-comparison/Cargo.toml:110` `[workspace]` 空块）。
   - **semver/API 稳定性**：`CHANGELOG.md:6` 已声明遵循 Semantic Versioning、`docs/API-STABILITY.md` 已有 API 稳定性承诺（三层分级 Stable/Experimental/Internal + 废弃流程 2 个 MINOR 保留期，见 `CHANGELOG.md:748`）、`docs/api-contracts.md` 已有 API 契约。workspace.package.version = "3.8.0"（`Cargo.toml:6`）。缺口：无 semver 兼容性自动化检查（cargo-semver-checks 未集成 CI）、无破坏性变更自动检测。
   - **数据验证**：`packages/sz-orm-core/src/model.rs:37` `pub trait Model`（无 validate 方法）、现有 validate 均为配置校验（`pool.rs:530` PoolConfig::validate、`circuit_breaker.rs:175`、`pool.rs:1892` PoolProdConfig::validate）、`packages/sz-orm-macros/src/lib.rs` 11 个 derive 宏无 Validate 派生。缺口：无字段级数据验证框架（email/length/range/regex/required/custom）。
   - **迁移**：`packages/sz-orm-core/src/migration.rs:489` `migrate`（async）、`:587` `rollback`、`:626` `up`、`:677` `down`、`:308` `get_pending_migrations`；`packages/sz-orm-core/src/schema_sync.rs:660` `sync_dry_run`（schema 同步 dry-run，返回 DDL 列表）；`packages/sz-orm-core/src/qb_migration_fix.rs:38` `dry_run`（代码修复 dry-run）。缺口：MigrationManager 无 migrate_dry_run（预览将执行 SQL 不实际执行）、无影响分析（受影响表/行数预估/锁类型/破坏性 DDL 标记）。
   - **CI/CD**：`.github/workflows/` 已有 10 个 yml（ci.yml/integration.yml/security.yml/codeql.yml/semgrep.yml/docs.yml/publish.yml/bindings.yml/soak.yml/soak-self-hosted.yml），`ci.yml:17` 已有 lint/clippy/check。缺口：无用户可复用的 GitHub Actions 模板（下游项目如 sz-pay 无法直接复用 sz-orm 的 CI 配置）。
   - **流式导出**：`packages/sz-orm-core/src/stream_api.rs:50` `StreamApiExt` trait（`stream` 真游标/`stream_buffered` 兼容版/`stream_with_backpressure` 背压），无 csv/parquet/arrow/polars 依赖。缺口：无基于流式查询的 CSV/Parquet 导出。
4. **本机数据库连接信息**：MySQL 9.6（`mysql://root:test123@127.0.0.1:3306/sz_orm_test`）、PostgreSQL 18（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）、Oracle 23ai Free（`127.0.0.1:1521/freepdb1`）。
5. **sz-pay 生产依赖证据**：sz-pay 从 crates.io 拉取 sz-orm-* 6 个包，作为 API 兼容性验证的下游基准。
6. **五方言覆盖约束**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL，benchmark/dry-run/流式导出须覆盖全部方言（按方言能力适配）。
7. **既有 feature gate 体系**：`packages/sz-orm-core/Cargo.toml` 已有 25+ feature（含 v3.8.0 prod-ready 14 子 feature），作为新能力 feature gate 隔离的基础。

## 1.3 核心输出

1. **criterion benchmark 套件**：系统化的 criterion benchmark 套件 + 自动化量化对比报告 + 回归基准线。
2. **semver 兼容性策略**：正式 semver 兼容性策略文档 + cargo-semver-checks CI 集成 + 破坏性变更自动检测。
3. **数据验证框架**：`#[derive(Validate)]` 派生宏 + 字段级校验规则（email/length/range/regex/required/custom）+ Model::validate() 集成。
4. **迁移 dry-run + 影响分析**：MigrationManager::migrate_dry_run + 影响分析报告（受影响表/行数预估/锁类型/破坏性 DDL 标记/回滚预案）。
5. **CI/CD GitHub Actions 模板**：用户可复用的 GitHub Actions 模板集（lint/test/security/release/probe），附使用文档。
6. **查询结果流式导出**：基于 StreamApiExt 的 CSV/Parquet 导出，逐行写出，峰值内存可控。
7. **需求追溯矩阵**：本文档第 7 章，建立需求 ↔ 验收条件映射。
8. **验收标准总览**：本文档第 8 章，按需求项汇总验收条件。

## 1.4 职责边界

本组件**不负责**以下事项：

1. **不破坏既有公开 API**：所有新能力通过 feature gate 隔离，既有公开 API 签名保持完全向后兼容。
2. **不改变既有安全铁律**：任何 WHERE 条件必须参数化，默认禁止 `SELECT *`，N+1 检测自动拦截，沿用既有铁律。
3. **不替换既有迁移实现**：既有 `MigrationManager::migrate`（`packages/sz-orm-core/src/migration.rs:489`）保留，新增 `migrate_dry_run` 与 `impact_analysis` 方法，两者共存。
4. **不替换既有流式查询**：既有 `StreamApiExt`（`packages/sz-orm-core/src/stream_api.rs:50`）保留，新增导出方法基于既有流式查询实现。
5. **不负责 sz-pay / sz-rust 下游代码修改**：ADR-0001 严禁修改下游/上游仓库，仅保证 API 兼容性。
6. **不降低既有测试覆盖**：v3.9.0 不得使 v3.8.0 已验收测试基线（6760 passed）回退，仅增不减。
7. **不负责 crates.io 全 46 包发布**：crates.io 发布放最后执行，本版本不涉及（对比分析第 6.1 节 P0 项延后）。
8. **不负责英文文档翻译**：英文文档翻译放最后执行，本版本不涉及（对比分析第 6.1 节 P0 项延后）。
9. **不负责中期/长期任务**：AI 调优闭环/多 LLM/混合搜索/data lineage/分片 rebalance/failover/服务网格/GraphQL/CDC/Go/Java/C++ 绑定等在 v4.0.0+ 规划。
10. **不强制启用新能力**：所有新能力默认关闭或可选启用，避免无配置环境行为变化。

---

# 2. 领域术语

**criterion benchmark 套件（Criterion Benchmark Suite）**
: 基于 criterion crate（0.5）的系统化性能基准测试套件，覆盖查询构造/连接池/缓存/事务/序列化等核心路径，提供统计显著的耗时测量、HTML 报告、回归基准线对比，并与 Diesel/SeaORM/SQLx 量化对比。
: 备注：已有零散 bench 文件（`packages/sz-orm-core/benches/` 9 个 + `bench-comparison/benches/` 13 个），本版本补系统化套件与回归基准。

**semver 兼容性策略（Semantic Versioning Compatibility Strategy）**
: 遵循 Semantic Versioning 2.0.0（MAJOR.MINOR.PATCH），通过 cargo-semver-checks 在 CI 中自动检测破坏性变更，配合 API 稳定性三层分级（Stable/Experimental/Internal）与废弃流程（2 个 MINOR 版本保留期），为生产用户提供版本升级信心。
: 备注：`CHANGELOG.md:6` 已声明 SemVer、`docs/API-STABILITY.md` 已有三层分级，本版本补自动化检查与 CI 集成。

**数据验证框架（Data Validation Framework）**
: 提供字段级数据校验能力，通过 `#[derive(Validate)]` 派生宏与 `#[validate(email)]` / `#[validate(length(min, max))]` / `#[validate(range(min, max))]` / `#[validate(regex)]` / `#[validate(required)]` / `#[validate(custom)]` 属性声明校验规则，实现 `Validate` trait 的 `validate() -> Result<(), ValidationError>` 方法，可与 Model::validate() 集成，减少用户手写校验样板代码。
: 备注：区别于既有配置校验（PoolConfig::validate `packages/sz-orm-core/src/pool.rs:530` 校验配置合理性），本版本补字段级数据校验。

**迁移 dry-run（Migration Dry-Run）**
: 迁移执行前预览将执行的 SQL 语句列表，不实际执行任何 DDL/DML，供人工审查与 CI 门禁使用。
: 备注：区别于既有 `schema_sync.rs:660` sync_dry_run（schema 同步 dry-run）与 `qb_migration_fix.rs:38` dry_run（代码修复 dry-run），本版本补 MigrationManager 的迁移 dry-run。

**迁移影响分析（Migration Impact Analysis）**
: 对待执行的迁移进行影响预估，输出受影响表列表、预估受影响行数、DDL 锁类型（EXCLUSIVE/SHARE/NONE）、破坏性 DDL 标记（DROP TABLE/DROP COLUMN/ALTER COLUMN TYPE 等不可逆操作）、回滚可行性评估。
: 备注：sz-orm 当前无影响分析能力（`packages/sz-orm-core/src/migration.rs` 无 impact 相关方法），本版本新增。

**CI/CD GitHub Actions 可复用模板（Reusable GitHub Actions Templates）**
: 提供用户可直接复用的 GitHub Actions 工作流模板集（lint/test/security/release/probe/soak），含可配置输入参数（包名/数据库连接/feature 组合），下游项目（如 sz-pay）可通过 `uses:` 引用或拷贝复用，降低从零搭建 CI 的门槛。
: 备注：sz-orm 自身已有 10 个 workflow（`.github/workflows/`），但不可被下游复用，本版本补可复用模板。

**查询结果流式导出（Query Result Streaming Export）**
: 基于既有 StreamApiExt（`packages/sz-orm-core/src/stream_api.rs:50`）的流式查询，将查询结果以 CSV/Parquet 格式逐行写出，峰值内存与结果集大小无关（仅取决于单行/批大小），支持大数据集（百万行+）低内存导出。
: 备注：sz-orm 已有流式查询（真游标/背压），无导出能力，本版本补 CSV/Parquet 导出。

**v3.9.0 feature gate**
: 控制本版本新能力的 feature gate 集合（`benchmark-suite` / `data-validation` / `migration-dry-run` / `streaming-export`），默认关闭，避免无配置环境行为变化。

---

# 3. 角色与边界

## 3.1 核心角色

- **ORM 库维护者**：执行 v3.9.0 六项优化的开发、验证、测试操作者，是新增能力的主要使用者与验收人。
- **下游项目开发者（sz-pay）**：关注 API 兼容性、CI/CD 模板复用、数据验证框架减少样板代码的下游使用者，v3.9.0 不得破坏其既有代码。
- **运维/SRE 工程师**：使用迁移 dry-run + 影响分析评估迁移风险，使用 CI/CD 模板搭建部署流水线，使用 benchmark 套件监控性能回归。
- **性能工程师**：使用 benchmark 套件量化性能对比、分析回归、定位瓶颈。

## 3.2 外部系统

- **MySQL 9.6 / PostgreSQL 18 / SQLite / Oracle 23ai / MSSQL**：benchmark/dry-run/流式导出的五方言覆盖目标。
- **Diesel 2.2 / SeaORM 1.1 / SQLx 0.8**：benchmark 量化对比的竞品（`bench-comparison/Cargo.toml:24-28` 已引入）。
- **criterion 0.5**：benchmark 套件的测量框架（`packages/sz-orm-core/Cargo.toml:177`）。
- **cargo-semver-checks**：semver 兼容性自动化检查工具（CI 集成）。
- **validator crate**：数据验证框架的参考/集成对象。
- **csv / arrow / parquet crate**：流式导出的格式依赖（CSV/Parquet）。
- **GitHub Actions**：CI/CD 模板的运行平台。
- **sz-pay 项目**：API 兼容性验证与 CI 模板复用的下游基准。

## 3.3 交互上下文

```plantuml
@startuml
title sz-orm v3.9.0 六项短期优化 交互上下文

rectangle "sz-orm v3.9.0" as orm {
  rectangle "benchmark 套件" as bench
  rectangle "semver/API 稳定性" as semver
  rectangle "数据验证框架" as validate
  rectangle "迁移 dry-run+影响分析" as migration
  rectangle "CI/CD 模板" as cicd
  rectangle "流式导出 CSV/Parquet" as export
}

actor "ORM 库维护者" as dev
actor "运维/SRE" as sre
actor "sz-pay 开发者" as downstream
actor "性能工程师" as perf

database "MySQL/PG/SQLite\n/Oracle/MSSQL" as db
rectangle "Diesel/SeaORM/SQLx" as competitors
cloud "GitHub Actions" as gh

dev --> orm : 开发/验证/测试
perf --> bench : 量化对比/回归分析
bench --> competitors : 性能对比
semver --> gh : CI semver 检查
cicd --> gh : 可复用模板
downstream --> cicd : 复用 CI 配置
downstream --> validate : 减少校验样板代码
sre --> migration : dry-run 评估迁移风险
migration --> db : 影响分析(受影响表/行数/锁)
export --> db : 流式导出(逐行写出)
downstream --> orm : API 兼容性验证

@enduml
```

---

# 4. DFX约束

## 4.1 性能

1. **benchmark 测量开销隔离**：benchmark 套件仅在 `benchmark-suite` feature 或 `cargo bench` 时运行，不影响正常编译与测试性能。
2. **数据验证开销**：`validate()` 单次校验开销不超过 1μs/字段，批量校验（100 字段）不超过 100μs。
3. **迁移 dry-run 开销**：migrate_dry_run 仅解析 SQL 不执行，开销不超过 10ms/迁移文件。
4. **影响分析开销**：impact_analysis 预估行数通过元数据查询（不全表扫描），开销不超过 500ms/迁移。
5. **流式导出内存**：CSV/Parquet 导出峰值内存不超过 10MB（与结果集行数无关，逐行/批写出）。
6. **流式导出吞吐**：CSV 导出吞吐不低于 100,000 行/秒，Parquet 导出吞吐不低于 50,000 行/秒（本地 SQLite，单机基准）。

## 4.2 可靠性

1. **benchmark 回归检测**：benchmark 套件须能检测到 ≥10% 的性能回归并标记。
2. **semver 检查可靠性**：cargo-semver-checks 须检测到所有 SemVer 破坏性变更（API 移除/签名变更/trait 变更），零漏报。
3. **数据验证完备性**：`#[derive(Validate)]` 生成的校验逻辑须覆盖所有标注的校验规则，无遗漏。
4. **dry-run 不修改数据库**：migrate_dry_run 须保证不执行任何 DDL/DML，数据库状态不变。
5. **影响分析保守预估**：行数预估须保守（高估不低估），避免低估风险。
6. **流式导出一致性**：导出的 CSV/Parquet 行数须与查询结果集行数一致，无丢行无重复。
7. **v3.8.0 测试基线不回退**：v3.9.0 不得使 v3.8.0 已验收测试基线（6760 passed）回退，仅增不减。

## 4.3 安全性

1. **CI/CD 模板无密钥泄露**：模板中禁止硬编码密钥/令牌/连接串，须使用 GitHub Secrets。
2. **流式导出无敏感字段泄露**：导出须尊重既有脱敏规则（sz-orm-masking），敏感字段导出前脱敏。
3. **semver 检查不泄露源码**：cargo-semver-checks 在 CI 中运行，结果仅报告变更不泄露源码内容。
4. **benchmark 不泄露连接串**：benchmark 报告中禁止出现数据库连接串明文（须脱敏）。

## 4.4 可维护性

1. **benchmark 可扩展**：benchmark 套件须支持通过配置/代码新增基准点，无需修改框架。
2. **semver 策略文档化**：semver 兼容性策略须有正式文档，含版本号规则/破坏性变更流程/废弃流程/升级指南。
3. **数据验证规则可组合**：校验规则须支持组合（多规则叠加）、嵌套（子对象校验）、条件校验。
4. **影响分析报告结构化**：影响分析报告须为结构化 JSON/TOML，可被 CI/工具解析。
5. **CI/CD 模板可配置**：模板须通过 inputs 参数化（包名/数据库/feature），非硬编码。
6. **审计证据要求**：每项需求结论须附 file:line 证据，遵循 AGENTS.md 审计合规铁律。

## 4.5 兼容性

1. **API 向后兼容**：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容。
2. **sz-pay 不破坏**：sz-pay 从 crates.io 拉取的 sz-orm-* 6 个包既有用法不受影响。
3. **五方言一致**：新增能力在 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言上行为一致（benchmark/dry-run/导出按方言能力适配）。
4. **既有 benchmark 保留**：既有 `packages/sz-orm-core/benches/` 9 个 + `bench-comparison/benches/` 13 个 bench 文件保留不动，新增套件不替换既有。
5. **既有迁移 API 保留**：既有 `MigrationManager::migrate/rollback/up/down`（`packages/sz-orm-core/src/migration.rs:489-733`）保留不动，新增 dry_run/impact_analysis 方法。

---

# 5. 核心能力

## 5.1 criterion benchmark 套件（REQ-V39-001）

### 5.1.1 业务规则

1. **系统化基准覆盖**（EARS: Ubiquitous）
   系统应当提供系统化的 criterion benchmark 套件，覆盖查询构造（QueryBuilder 链式 build）、连接池（acquire/release/复用）、缓存（L1 命中/L2 命中/失效）、事务（begin/commit/rollback）、序列化（serde 序列化/反序列化）、流式查询（stream 逐行）六大核心路径，每个路径至少 3 个基准点。
   a. 验收条件：[运行 `cargo bench --features benchmark-suite`] → [输出六大路径基准报告，每路径 ≥3 基准点，附 criterion 统计（均值/方差/p99）]
2. **量化竞品对比**（EARS: Ubiquitous）
   系统应当提供与 Diesel 2.2 / SeaORM 1.1 / SQLx 0.8 的量化性能对比报告，覆盖 CRUD/分页/事务/关联加载/连接池五大场景，输出 HTML + JSON 双格式报告。
   a. 验收条件：[运行竞品对比 benchmark] → [输出 HTML 报告含 SZ-ORM vs Diesel vs SeaORM vs SQLx 五场景对比图表 + JSON 报告可解析]
3. **回归基准线**（EARS: State-driven）
   在存在历史基准线（`benches/baseline/*.json`）的状态下，系统应当对比当前 benchmark 结果与基准线，标记 ≥10% 回退的基准点。
   a. 验收条件：[存在基准线，运行 benchmark，某基准点耗时增加 ≥10%] → [报告标记 REGRESSION，附当前值/基准值/回退百分比]
4. **复用既有 bench 基础设施**（EARS: Ubiquitous）
   系统应当复用既有 criterion 0.5（`packages/sz-orm-core/Cargo.toml:177`）与 bench-comparison（`bench-comparison/Cargo.toml:32`）基础设施，不重复引入测量框架。
   a. 验收条件：[benchmark 套件] → [基于既有 criterion 0.5，无新增测量框架依赖]
5. **禁止项**（EARS: Unwanted）
   如果 benchmark 套件影响默认 feature 编译或测试，则系统应当通过 `benchmark-suite` feature gate 隔离，默认不启用。
   a. 验收条件：[`cargo build` 默认编译] → [benchmark 套件代码不编译，行为与 v3.8.0 一致]

### 5.1.2 交互流程

```plantuml
@startuml
title criterion benchmark 套件 交互流程
actor "性能工程师" as perf
participant "benchmark 套件" as bench
participant "criterion" as crit
rectangle "Diesel/SeaORM/SQLx" as comp

perf -> bench : cargo bench --features benchmark-suite
bench -> crit : 运行六大路径基准
crit --> bench : 统计结果(均值/方差/p99)
bench -> comp : 竞品对比(CRUD/分页/事务/关联/池)
comp --> bench : 竞品数据
bench -> bench : 对比基准线(标记回归)
bench --> perf : HTML+JSON 报告
@enduml
```

### 5.1.3 异常场景

1. **竞品依赖不可用**
   a. 触发条件：Diesel/SeaORM/SQLx 依赖编译失败（如缺少系统库）
   b. 系统行为：竞品对比部分标记 SKIPPED，SZ-ORM 自身基准正常输出
   c. 用户感知：报告含 SZ-ORM 基准 + 竞品对比 SKIPPED 标记
2. **基准线缺失**
   a. 触发条件：首次运行，无历史基准线
   b. 系统行为：生成当前结果作为新基准线，不标记回归
   c. 用户感知：报告提示"首次运行，已生成基准线"

## 5.2 semver 兼容性策略 + API 稳定性保证（REQ-V39-002）

### 5.2.1 业务规则

1. **semver 自动化检查**（EARS: Ubiquitous）
   系统应当在 CI 中集成 cargo-semver-checks，对每次 PR 自动检测 SemVer 破坏性变更（公开 API 移除/签名变更/trait 变更/结构体字段移除），破坏性变更须显式标注否则 CI 失败。
   a. 验收条件：[PR 移除某 pub fn 但未标注 breaking] → [cargo-semver-checks 检测到，CI 失败，报告变更位置]
2. **API 稳定性三层分级**（EARS: Ubiquitous）
   系统应当维护 API 稳定性三层分级（Stable / Experimental / Internal），Stable API 须保证 MINOR 版本向后兼容，Experimental API 可跨 MINOR 变更但须标注，Internal API（`#[doc(hidden)]`）无兼容性承诺。
   a. 验收条件：[某 Stable API 在 MINOR 版本被破坏性变更] → [semver 检查失败，须升级 MAJOR 或回退变更]
3. **废弃流程自动化**（EARS: Event-driven）
   当标注 `#[deprecated]` 的 API 被移除时，系统应当验证废弃保留期（≥2 个 MINOR 版本）已满，未满则 CI 失败。
   a. 验收条件：[API 在 v3.8.0 标注 deprecated，v3.9.0 移除] → [保留期 1 个 MINOR < 2，CI 失败，提示"废弃保留期不足"]
4. **正式策略文档**（EARS: Ubiquitous）
   系统应当提供正式的 semver 兼容性策略文档，含版本号规则（MAJOR/MINOR/PATCH）、破坏性变更条件与流程、废弃流程（保留期/迁移指南）、升级指南、例外说明。
   a. 验收条件：[查阅 semver 策略文档] → [含版本号规则/破坏性变更流程/废弃流程/升级指南，复用既有 `docs/API-STABILITY.md` 三层分级]
5. **复用既有 API 稳定性文档**（EARS: Ubiquitous）
   系统应当复用既有 `docs/API-STABILITY.md`（三层分级 Stable/Experimental/Internal）与 `docs/api-contracts.md`（API 契约），不重复定义分级标准。
   a. 验收条件：[策略文档] → [引用既有 `docs/API-STABILITY.md` 分级，无重复定义]

### 5.2.2 交互流程

```plantuml
@startuml
title semver 兼容性检查 交互流程
actor "维护者" as dev
participant "CI" as ci
participant "cargo-semver-checks" as semver
participant "API-STABILITY.md" as doc

dev -> ci : 提交 PR
ci -> semver : 运行 cargo-semver-checks
semver -> semver : 对比公开 API 变更
alt 检测到破坏性变更且未标注
semver --> ci : CI 失败(报告变更位置)
else 无破坏性变更或已标注
semver --> ci : 通过
end
ci -> doc : 验证废弃保留期
ci --> dev : PR 检查结果
@enduml
```

### 5.2.3 异常场景

1. **破坏性变更未标注**
   a. 触发条件：PR 包含 SemVer 破坏性变更但未标注 breaking
   b. 系统行为：CI 失败，报告变更位置与类型
   c. 用户感知：PR 检查失败，提示"breaking change detected at file:line, mark with #[breaking]"
2. **废弃保留期不足**
   a. 触发条件：移除的 API 废弃保留期 < 2 个 MINOR 版本
   b. 系统行为：CI 失败，提示保留期不足
   c. 用户感知：PR 检查失败，提示"deprecation period N < 2 MINOR, wait until vX.Y.Z"

## 5.3 数据验证框架（REQ-V39-003）

### 5.3.1 业务规则

1. **Validate 派生宏**（EARS: Ubiquitous）
   系统应当提供 `#[derive(Validate)]` 派生宏，为结构体自动生成 `Validate` trait 的 `validate() -> Result<(), ValidationError>` 实现，基于字段标注的 `#[validate(...)]` 属性生成校验逻辑。
   a. 验收条件：[结构体标注 `#[derive(Validate)]`，字段标注 `#[validate(email)]`] → [自动生成 validate()，调用时校验 email 格式，非法返回 ValidationError]
2. **校验规则集**（EARS: Ubiquitous）
   系统应当支持以下校验规则：`email`（邮箱格式）、`length(min, max)`（长度范围）、`range(min, max)`（数值范围）、`regex(pattern)`（正则匹配）、`required`（非空）、`custom(fn)`（自定义校验函数）、`contains(substring)`（包含子串）、`does_not_contain(substring)`（不包含子串）。
   a. 验收条件：[字段标注 `#[validate(length(min=1, max=100))]`，值长度 0] → [validate() 返回 Err(ValidationError::Length { min: 1, max: 100, actual: 0 })]
3. **规则组合与嵌套**（EARS: Ubiquitous）
   系统应当支持多规则叠加（同字段多 `#[validate]` 属性）、嵌套校验（字段为另一 `Validate` 实现者，递归调用其 validate()）、条件校验（`#[validate(rule, if = "condition")]` 仅条件为真时校验）。
   a. 验收条件：[字段标注 email + length(min=5)，值 "ab"] → [返回 email 通过 + length 失败的聚合错误]
4. **Model 集成**（EARS: Optional）
   当 Model 实现 Validate trait 时，系统应当在 insert/update 前可选自动调用 validate()（通过 `validate-on-write` feature gate 启用），校验失败拒绝写入。
   a. 验收条件：[Model 实现 Validate，启用 validate-on-write，insert 非法数据] → [insert 返回 Err(ValidationError)，不执行 SQL]
5. **错误聚合**（EARS: Ubiquitous）
   系统应当聚合所有字段校验错误，非短路返回首个错误，返回 `ValidationError` 含全部失败字段与原因。
   a. 验收条件：[多字段非法，调用 validate()] → [返回含全部字段错误的 ValidationError，非仅首个]
6. **禁止项**（EARS: Unwanted）
   如果数据验证框架影响默认 feature 编译或运行时行为，则系统应当通过 `data-validation` feature gate 隔离，默认不启用 Model 自动校验。
   a. 验收条件：[`cargo build` 默认编译] → [无 validate 自动校验，行为与 v3.8.0 一致]

### 5.3.2 交互流程

```plantuml
@startuml
title 数据验证框架 交互流程
actor "开发者" as dev
participant "sz-orm-macros" as macros
participant "Validate trait" as validate
participant "Model" as model

dev -> macros : #[derive(Validate)] + #[validate(...)]
macros -> macros : 生成 validate() 实现
dev -> validate : 调用 instance.validate()
validate -> validate : 逐字段校验(聚合错误)
validate --> dev : Result<(), ValidationError>
alt 启用 validate-on-write
dev -> model : insert(data)
model -> validate : data.validate()?
validate --> model : Err(ValidationError)
model --> dev : 拒绝写入
end
@enduml
```

### 5.3.3 异常场景

1. **校验规则配置错误**
   a. 触发条件：`#[validate(unknown_rule)]` 引用不存在的规则
   b. 系统行为：编译失败，提示未知规则
   c. 用户感知：编译错误 `unknown validation rule: unknown_rule`
2. **自定义校验函数签名错误**
   a. 触发条件：`#[validate(custom = "fn")]` 的 fn 签名不匹配
   b. 系统行为：编译失败，提示签名不匹配
   c. 用户感知：编译错误 `custom validation function signature mismatch`

## 5.4 迁移 dry-run + 影响分析（REQ-V39-004）

### 5.4.1 业务规则

1. **migrate_dry_run**（EARS: Ubiquitous）
   系统应当提供 `MigrationManager::migrate_dry_run() -> Result<DryRunReport, DbError>`，返回待执行的迁移列表与每个迁移的 SQL 语句，不实际执行任何 DDL/DML。
   a. 验收条件：[3 个待执行迁移，调用 migrate_dry_run()] → [返回含 3 个迁移的 DryRunReport（version/name/sql_up），数据库无变更]
2. **不修改数据库保证**（EARS: Ubiquitous）
   migrate_dry_run 须保证不执行任何 DDL/DML，数据库状态与调用前完全一致。
   a. 验收条件：[migrate_dry_run() 前后查询 schema 版本表] → [版本表无变化，无新表/无新列]
3. **影响分析报告**（EARS: Ubiquitous）
   系统应当提供 `MigrationManager::impact_analysis() -> Result<ImpactReport, DbError>`，输出每个待执行迁移的：受影响表列表、预估受影响行数、DDL 锁类型（EXCLUSIVE/SHARE/NONE）、破坏性 DDL 标记（DROP/ALTER COLUMN TYPE/TRUNCATE 等不可逆操作）、回滚可行性（sql_down 非空且可逆）。
   a. 验收条件：[迁移含 `DROP TABLE users`，调用 impact_analysis()] → [报告标记破坏性=true，受影响表=users，回滚可行性=有 sql_down 但 DROP 不可逆]
4. **行数预估保守**（EARS: Ubiquitous）
   行数预估须通过元数据/统计信息查询（不全表扫描），且保守高估（预估 ≥ 实际行数）。
   a. 验收条件：[表实际 1000 行，impact_analysis 预估] → [预估行数 ≥ 1000，且通过 `SELECT reltuples` 元数据查询非全表 COUNT]
5. **破坏性 DDL 标记**（EARS: Ubiquitous）
   系统应当识别并标记破坏性 DDL：DROP TABLE/DROP COLUMN/TRUNCATE/ALTER COLUMN TYPE（可能丢数据）/DELETE WITHOUT WHERE。
   a. 验收条件：[迁移含 `ALTER COLUMN age TYPE TEXT`] → [报告标记破坏性=true，原因=type_change_may_lose_data]
6. **复用既有迁移基础设施**（EARS: Ubiquitous）
   系统应当复用既有 `MigrationManager`（`packages/sz-orm-core/src/migration.rs:283`）、`get_pending_migrations`（`:308`）、`Migration` 结构（`:10`），不重复实现迁移解析。
   a. 验收条件：[migrate_dry_run] → [基于既有 get_pending_migrations 获取待执行列表，不重复解析]
7. **禁止项**（EARS: Unwanted）
   如果迁移 dry-run 或影响分析影响既有 migrate 行为，则系统应当新增独立方法，既有 `migrate`（`:489`）保留不动。
   a. 验收条件：[既有 `migrate()` 调用] → [行为与 v3.8.0 一致，实际执行迁移]

### 5.4.2 交互流程

```plantuml
@startuml
title 迁移 dry-run + 影响分析 交互流程
actor "运维/SRE" as sre
participant "MigrationManager" as mgr
database "DB" as db

sre -> mgr : migrate_dry_run()
mgr -> mgr : get_pending_migrations()
mgr --> sre : DryRunReport(SQL 列表, 不执行)
sre -> mgr : impact_analysis()
mgr -> db : 查询元数据(表统计/列信息)
db --> mgr : 元数据
mgr -> mgr : 分析 DDL(受影响表/行数/锁/破坏性)
mgr --> sre : ImpactReport(结构化)
alt 报告含破坏性 DDL
sre -> sre : 人工审查(确认/中止)
end
@enduml
```

### 5.4.3 异常场景

1. **元数据查询失败**
   a. 触发条件：impact_analysis 查询表统计信息失败（权限不足）
   b. 系统行为：该表行数预估标记 UNKNOWN，其他分析正常输出
   c. 用户感知：报告该表行数=UNKNOWN，附原因
2. **破坏性 DDL 未提供回滚**
   a. 触发条件：破坏性 DDL 的 sql_down 为空
   b. 系统行为：报告标记回滚可行性=false，输出高风险告警
   c. 用户感知：告警 `destructive DDL without rollback: DROP TABLE users`

## 5.5 CI/CD GitHub Actions 可复用模板（REQ-V39-005）

### 5.5.1 业务规则

1. **模板集覆盖**（EARS: Ubiquitous）
   系统应当提供可复用的 GitHub Actions 工作流模板集，覆盖 lint（fmt+clippy）、test（单元+集成）、security（audit+deny+SQL 注入扫描）、release（crates.io 发布）、probe（K8s 探针部署）、soak（长时间稳定性测试）六大场景。
   a. 验收条件：[查阅模板集] → [含 lint/test/security/release/probe/soak 六个模板文件，每个含可配置 inputs]
2. **参数化可配置**（EARS: Ubiquitous）
   模板须通过 `inputs` 参数化关键配置：包名（package）、数据库连接（database_url）、feature 组合（features）、Rust 工具链（toolchain），非硬编码。
   a. 验收条件：[下游项目引用 lint 模板，传入 package=sz-orm-core, toolchain=stable] → [模板对 sz-orm-core 运行 fmt+clippy，使用 stable 工具链]
3. **可复用方式**（EARS: Ubiquitous）
   模板须支持两种复用方式：(a) `uses:` 远程引用（发布为 reusable workflow）；(b) 拷贝复用（拷贝到下游 `.github/workflows/` 并修改 inputs）。
   a. 验收条件：[下游项目 `uses: ljclz/sz-orm/.github/workflows/lint.yml@v3.9.0`] → [远程引用执行 lint]
4. **复用既有 CI 配置**（EARS: Ubiquitous）
   系统应当复用既有 `.github/workflows/` 10 个 workflow 的最佳实践（ci.yml 的 lint/clippy/check、integration.yml 的集成测试、security.yml 的安全检查），模板为既有配置的参数化抽取。
   a. 验收条件：[lint 模板] → [复用 `ci.yml:17` lint job 的步骤，参数化为 inputs]
5. **使用文档**（EARS: Ubiquitous）
   系统应当提供模板使用文档，含每个模板的 inputs 说明、复用示例（远程引用/拷贝）、自定义说明。
   a. 验收条件：[查阅模板文档] → [含 inputs 说明 + 远程引用示例 + 拷贝示例 + 自定义说明]
6. **禁止项**（EARS: Unwanted）
   如果模板含硬编码密钥/令牌/连接串，则系统应当拒绝，须使用 GitHub Secrets 引用。
   a. 验收条件：[扫描模板文件] → [无硬编码密钥/令牌/连接串，均通过 `${{ secrets.* }}` 引用]

### 5.5.2 交互流程

```plantuml
@startuml
title CI/CD 模板复用 交互流程
actor "sz-pay 开发者" as downstream
participant "sz-pay .github/workflows" as wf
rectangle "sz-orm 模板" as tmpl
cloud "GitHub Actions" as gh

downstream -> wf : 引用模板(uses: or 拷贝)
wf -> tmpl : 获取模板(inputs 参数化)
tmpl -> gh : 执行(lint/test/security/...)
gh --> downstream : CI 结果
@enduml
```

### 5.5.3 异常场景

1. **inputs 缺失**
   a. 触发条件：下游引用模板未提供必需的 inputs（如 package）
   b. 系统行为：模板使用默认值或报错提示
   c. 用户感知：CI 提示"input 'package' required, using default: sz-orm-core"
2. **工具链不匹配**
   a. 触发条件：模板要求 rust-version 1.81，下游工具链低于此
   b. 系统行为：模板通过 dtolnay/rust-toolchain 安装指定版本
   c. 用户感知：CI 自动安装 1.81 工具链

## 5.6 查询结果流式导出（REQ-V39-006）

### 5.6.1 业务规则

1. **CSV 导出**（EARS: Ubiquitous）
   系统应当提供查询结果的 CSV 流式导出，基于既有 StreamApiExt（`packages/sz-orm-core/src/stream_api.rs:50`）逐行写出，峰值内存与结果集行数无关。
   a. 验收条件：[查询 100 万行，导出 CSV] → [CSV 行数=100 万，峰值内存 ≤ 10MB，含表头]
2. **Parquet 导出**（EARS: Ubiquitous）
   系统应当提供查询结果的 Parquet 流式导出，逐行/批写入 Parquet 列式格式，含 schema（列名/类型）。
   a. 验收条件：[查询 100 万行，导出 Parquet] → [Parquet 行数=100 万，schema 完整，峰值内存 ≤ 10MB]
3. **低内存保证**（EARS: Ubiquitous）
   导出须逐行或固定批大小（可配置，默认 1000 行/批）写出，峰值内存不超过批大小 × 单行大小 + 格式缓冲，与结果集总行数无关。
   a. 验收条件：[导出 100 万行，批大小 1000，单行 1KB] → [峰值内存 ≈ 1000 × 1KB + 缓冲 ≤ 10MB]
4. **复用既有流式查询**（EARS: Ubiquitous）
   导出须基于既有 StreamApiExt 的 `stream`（真游标，`packages/sz-orm-core/src/stream_api.rs:50`）实现，不重复实现流式查询。
   a. 验收条件：[CSV 导出] → [基于 `stream` 逐行产出，非全量 collect]
5. **脱敏集成**（EARS: Optional）
   当启用脱敏（sz-orm-masking）时，导出须对敏感字段应用脱敏规则后再写出。
   a. 验收条件：[查询含手机号字段，启用脱敏，导出 CSV] → [CSV 中手机号显示为 `138****8888`，非明文]
6. **格式配置**（EARS: Ubiquitous）
   CSV 导出须支持配置：分隔符（默认 `,`）、引号（默认 `"`）、是否含表头（默认 true）、转义规则。Parquet 导出须支持配置：压缩算法（Snappy/Gzip/Zstd/None，默认 Snappy）、批大小。
   a. 验收条件：[配置 CSV 分隔符=`;`] → [导出 CSV 使用 `;` 分隔]
7. **禁止项**（EARS: Unwanted）
   如果流式导出影响默认 feature 编译，则系统应当通过 `streaming-export` feature gate 隔离，默认不引入 csv/parquet 依赖。
   a. 验收条件：[`cargo build` 默认编译] → [无 csv/parquet 依赖，行为与 v3.8.0 一致]

### 5.6.2 交互流程

```plantuml
@startuml
title 流式导出 交互流程
actor "开发者" as dev
participant "QueryBuilder" as qb
participant "StreamApiExt" as stream
participant "CSV/Parquet Writer" as writer
file "output.csv" as file

dev -> qb : query.stream(&mut conn)
qb -> stream : 真游标逐行产出
stream -> writer : row 1
writer -> file : 写出
stream -> writer : row 2
writer -> file : 写出
note right of writer : 峰值内存 ≤ 批大小×单行
stream --> dev : 导出完成(总行数)
@enduml
```

### 5.6.3 异常场景

1. **写出失败**
   a. 触发条件：磁盘满或无写权限
   b. 系统行为：导出中止，返回 IO 错误，已写出部分保留
   c. 用户感知：错误提示 `export failed: disk full at row N`
2. **查询中途连接断开**
   a. 触发条件：流式查询过程中数据库连接断开
   b. 系统行为：导出中止，返回连接错误，已写出部分保留
   c. 用户感知：错误提示 `export interrupted: connection lost at row N`

---

# 6. 数据约束

## 6.1 需求项

1. **需求 ID**：唯一标识，格式 `REQ-V39-xxx`，必填。
2. **需求名称**：人类可读名称，必填。
3. **优先级**：P1 / P2，必填。
4. **分类**：质量量化 / API 稳定性 / 数据校验 / 迁移安全 / 生态便利 / 数据导出，必填。
5. **验证方法**：可执行的验证命令或测试描述，必填。
6. **代码证据**：相关 file:line 引用，必填，遵循审计合规铁律。
7. **验收条件**：触发场景 → 预期行为，必填。
8. **状态**：PASS / FAIL / PENDING，必填。

## 6.2 输出对象

1. **DryRunReport**：待执行迁移列表（version/name/sql_up），不执行。
2. **ImpactReport**：每个迁移的受影响表（Vec<String>）、预估行数（Option<u64>，UNKNOWN 标记）、锁类型（Exclusive/Share/None）、破坏性标记（bool + 原因）、回滚可行性（bool + 原因）。
3. **ValidationError**：失败字段路径（Vec<String>）、规则名（String）、原因（String）、实际值（Option<String>，脱敏后）。
4. **BenchmarkReport**：基准点（name）、统计（mean/stddev/p99）、竞品对比（Map<竞品, 耗时>）、回归标记（Option<Regression>）。
5. **ExportConfig**：格式（CSV/Parquet）、批大小（u32，默认 1000）、CSV 配置（分隔符/引号/表头/转义）、Parquet 配置（压缩算法）、脱敏开关（bool）。

---

# 7. 需求追溯矩阵

| 需求编号 | 需求项 | 优先级 | 分类 | 验收条件（节选） | 现有代码证据 |
|---------|--------|--------|------|----------------|-------------|
| REQ-V39-001 | criterion benchmark 套件 | P1 | 质量量化 | 六大路径基准 + 竞品对比 + 回归检测 | `packages/sz-orm-core/Cargo.toml:177` criterion、`bench-comparison/Cargo.toml:32` criterion、`bench-comparison/Cargo.toml:6` 已对比竞品、`packages/sz-orm-core/benches/` 9 个既有 bench |
| REQ-V39-002 | semver 兼容性策略 + API 稳定性 | P1 | API 稳定性 | CI 自动检测破坏性变更 + 废弃保留期 + 策略文档 | `CHANGELOG.md:6` 已声明 SemVer、`docs/API-STABILITY.md` 三层分级、`docs/api-contracts.md` API 契约、`Cargo.toml:6` version=3.8.0 |
| REQ-V39-003 | 数据验证框架 | P1 | 数据校验 | `#[derive(Validate)]` + 8 种规则 + 组合嵌套 + Model 集成 | `packages/sz-orm-core/src/model.rs:37` Model trait、`packages/sz-orm-macros/src/lib.rs` 11 derive 宏（无 Validate）、`packages/sz-orm-core/src/pool.rs:530` 既有配置校验（非字段校验） |
| REQ-V39-004 | 迁移 dry-run + 影响分析 | P2 | 迁移安全 | migrate_dry_run 不执行 + 影响报告 + 破坏性标记 | `packages/sz-orm-core/src/migration.rs:489` migrate、`:308` get_pending_migrations、`:10` Migration 结构、`packages/sz-orm-core/src/schema_sync.rs:660` sync_dry_run（既有 schema dry-run） |
| REQ-V39-005 | CI/CD GitHub Actions 模板 | P2 | 生态便利 | 六模板 + 参数化 + 可复用 + 文档 | `.github/workflows/ci.yml:17` lint job、`.github/workflows/` 10 个既有 workflow |
| REQ-V39-006 | 查询结果流式导出 | P2 | 数据导出 | CSV/Parquet 逐行写出 + 低内存 + 脱敏 | `packages/sz-orm-core/src/stream_api.rs:50` StreamApiExt（stream 真游标）、无 csv/parquet 依赖（需新增） |

---

# 8. 验收标准总览

## 8.1 P1 类（最高优先级）

| 编号 | 验收标准 | 验证方法 |
|------|---------|---------|
| REQ-V39-001 | 六大路径基准覆盖（每路径 ≥3 点）+ 竞品对比 HTML/JSON 报告 + 回归基准线检测 | `cargo bench --features benchmark-suite`；运行竞品对比；对比基准线验证回归标记 |
| REQ-V39-002 | CI 集成 cargo-semver-checks + 破坏性变更自动检测 + 废弃保留期验证 + 正式策略文档 | PR 移除 pub fn 验证 CI 失败；查阅策略文档含版本规则/破坏性流程/废弃流程/升级指南 |
| REQ-V39-003 | `#[derive(Validate)]` + 8 种规则 + 组合嵌套 + 错误聚合 + Model 集成（validate-on-write） | 标注 derive + 多规则验证聚合错误；启用 validate-on-write 验证 insert 非法数据被拒 |

## 8.2 P2 类（高优先级）

| 编号 | 验收标准 | 验证方法 |
|------|---------|---------|
| REQ-V39-004 | migrate_dry_run 不执行 SQL + 影响报告（受影响表/行数/锁/破坏性/回滚）+ 行数保守预估 | 调用 migrate_dry_run 验证 DB 无变更；含 DROP TABLE 迁移验证破坏性标记 |
| REQ-V39-005 | 六模板（lint/test/security/release/probe/soak）+ 参数化 inputs + 远程引用/拷贝复用 + 文档 + 无硬编码密钥 | 引用模板传入 inputs 验证执行；扫描模板无硬编码密钥 |
| REQ-V39-006 | CSV/Parquet 逐行写出 + 峰值内存 ≤10MB + 行数一致 + 脱敏集成 + 格式可配置 | 导出 100 万行验证内存与行数；启用脱敏验证敏感字段脱敏 |

## 8.3 全局验收条件

1. **API 兼容性**：v3.9.0 既有公开 API 完全向后兼容，sz-pay 既有代码不受影响。
2. **feature gate 隔离**：所有新能力通过 feature gate 隔离（`benchmark-suite` / `data-validation` / `migration-dry-run` / `streaming-export`），默认 feature 行为不变。
3. **测试基线不回退**：v3.8.0 已验收测试基线（6760 passed）不回退，v3.9.0 仅增不减。
4. **五方言一致**：新增能力在 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言上行为一致（benchmark/dry-run/导出按方言能力适配）。
5. **审计证据**：每项需求结论附 file:line 证据，遵循 AGENTS.md 审计合规铁律。
6. **14 道门禁通过**：v3.9.0 须通过 AGENTS.md 定义的 14 道门禁（fmt/check/clippy/test/doc/audit/integration/占位检查/SQL 注入/feature 全组合/上游未改/文档一致/审计证据/文档同步）。
7. **无占位实现**：禁止 `todo!` / `unimplemented!` / `unreachable!`，所有新增代码须完整实现。
8. **unsafe 零容忍**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释。