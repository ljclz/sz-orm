# sz-orm v3.4.0 技术设计文档

> 版本：v3.4.0（测试覆盖补齐 + 架构改进 + 性能优化落地 + 编译期类型安全增强 + 文档与生态建设 + sz-pay 生产案例深化）
> 基线：v3.3.0（已完成：分布式缓存一致性 + GraphQL 查询支持 + 多租户与数据隔离 + AI 自然语言查询增强）
> 日期：2026-08-08
> 文档定位：技术设计（How to build），对应需求规格 `docs/spec/v3.4.0/spec.md`（31 条 EARS 需求，6 组 REQ-TC/REQ-AR/REQ-PF/REQ-TS/REQ-DOC/REQ-PC）
> 设计约束：Rust 2021 Edition / rust-version 1.81 / API 向后兼容（无 Breaking Change）/ 禁止占位实现 / unsafe 零容忍 / 参数化查询铁律 / Feature 隔离 / 五方言行为一致 / ADR-0001 严禁修改下游/上游仓库
> 优先级声明：六个方向按"测试覆盖补齐(1,最高) → 架构改进(2) → 性能优化(3) → 编译期类型安全(4) → 文档与生态(5) → sz-pay 生产案例(6)"的收益/风险序推进；测试覆盖为最高收益中低风险（补齐既有包测试，不引入新依赖），架构与性能为中收益中风险（涉及核心模块重构，需 feature gate 隔离），类型安全为中收益中风险（宏扩展，编译期验证），文档与案例为低风险高收益（纯文档与示例）
> 缺陷来源：`docs/assessment/2026-08-08-v3.3.0-depth-evaluation.md` §6.1~§6.3 + §7

---

# 一、需求与存量功能关系分析

## 1.1 需求功能与存量功能对比

v3.4.0 的六项能力扩展任务与 v3.3.0 已交付代码的关系如下。v3.3.0 已完成分布式缓存一致性、GraphQL 查询支持、多租户与数据隔离、AI 自然语言查询增强四项能力，workspace 版本 2.4.0，6,327 测试全部通过。本版本在此基础上向"测试覆盖完备性、架构清晰度、性能基准可验证性、编译期类型安全、文档完整度、生产可信度"六个维度能力突破，所有新增能力以扩展模块 + feature gate 方式提供，不修改 sz-orm-core / sz-orm-macros / 扩展包既有公开 API 签名（满足 spec §4.5 兼容性约束 C-05 无 Breaking Change）。

### 1.1.1 已实现功能

| 需求功能 | 存量功能 | 代码位置 | 匹配度 |
|---------|---------|---------|--------|
| 18 个扩展包零测试盲区（REQ-TC-001 基础） | 各包 src/ 有实现，但 tests/ 目录 0 测试文件 | [packages/sz-orm-config/src/lib.rs:42](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-config/src/lib.rs#L42) 等 18 包 | 0%（需补齐） |
| MySQL INSERT IGNORE 测试缺陷（REQ-TC-002 基础） | `test_mysql_insert_or_ignore_duplicate` 测试存在但测试表缺 UNIQUE 约束 | [packages/sz-orm-core/tests/integration_mysql.rs:1267](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/integration_mysql.rs#L1267) | 50%（测试存在但缺陷） |
| sz-orm-es MOCK-ONLY 现状（REQ-TC-003 基础） | 内存 HashMap 实现，lib.rs 顶部明确标注 MOCK-ONLY | [packages/sz-orm-es/src/lib.rs:1](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-es/src/lib.rs#L1) | 50%（Mock 完整，无真实） |
| sz-orm-config 仅内存实现（REQ-TC-004 基础） | `ConsulConfigCenter` 内存实现，未集成真实 Consul/Nacos | [packages/sz-orm-config/src/lib.rs:42](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-config/src/lib.rs#L42) | 50%（内存完整，无真实） |
| 313 个 pub API 文档缺口（REQ-AR-002/REQ-DOC-001 基础） | docs.rs cfg 跳过 missing_docs lint，313 个 pub API 缺 `///` 文档 | [packages/sz-orm-core/src/lib.rs:403](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L403) | 0%（需补齐） |
| README 过时声明（REQ-AR-003/REQ-DOC-002 基础） | README.md 第 46 行自称"原型阶段/无生产案例"，实际 sz-pay 已用 | [README.md:46](file:///E:/vue/test/鲜视达/rust/sz-orm/README.md#L46) | 0%（需更新） |
| async trait 风格不统一（REQ-AR-004 基础） | Connection trait 手动解糖，其他 trait 使用 `#[async_trait]` | [packages/sz-orm-core/src/pool.rs:42](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L42) | 50%（需评估统一） |
| sz-orm-query-builder 与 core::QueryBuilder 重叠（REQ-AR-005 基础） | 两个 QueryBuilder，sz-orm-query-builder 已有区别说明 | [packages/sz-orm-query-builder/src/lib.rs:53](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-query-builder/src/lib.rs#L53) | 50%（需选择指南） |
| query.rs SQL 构造（REQ-PF-001 基础） | QueryBuilder 链式构造，String 拼接，155KB | [packages/sz-orm-core/src/query.rs:36](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L36) | 100%（需优化评估） |
| dialect.rs 方言分发（REQ-PF-002 基础） | `Box<dyn Dialect>` 动态分发，127KB，五方言 | [packages/sz-orm-core/src/lib.rs:130](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L130) | 100%（需优化评估） |
| L2Cache 序列化（REQ-PF-003 基础） | L2Cache + RedisBackend，89KB，已有 zero-copy feature | [packages/sz-orm-core/src/value_borrowed.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/value_borrowed.rs) | 75%（zero-copy 既有，需推广 L2） |
| result_map.rs 反射式取值（REQ-PF-004 基础） | ResultMap 反射式取值，77KB | [packages/sz-orm-core/src/result_map.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/result_map.rs) | 100%（需宏生成评估） |
| value.rs Value 枚举（REQ-PF-005 基础） | Value 枚举 20 变体，String 存储，40KB | [packages/sz-orm-core/src/lib.rs:401](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L401) | 100%（需 Box<str> 评估） |
| `#[derive(Schema)]` 宏（REQ-TS-001 基础） | 既有 Schema derive，生成 schema 元数据 | [packages/sz-orm-macros/src/lib.rs:71](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/src/lib.rs#L71) | 75%（需扩展列名常量） |
| typed_ast.rs 强类型 AST（REQ-TS-003 基础） | 既有 typed_ast.rs 模块，强类型 AST 基础 | [packages/sz-orm-core/src/typed_ast.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs) | 50%（需完善 DSL） |
| sz-pay 生产使用证据（REQ-PC-001~003 基础） | sz-pay 依赖 7 个 sz-orm 包、297 处引用、5139 测试零回归、crates.io 2.3.0 | `E:\vue\test\sz-pay\server\sz-rust` Cargo.toml | 100%（需抽取案例） |
| sz-orm-core feature 体系 | default=["redis"]，含 testing/db-verify/redis/circuit-breaker/rate-limit/auto-prewarm/plan-cache/zero-copy/simd/multi-tenant-enhanced/dist-cache | [packages/sz-orm-core/Cargo.toml:13](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L13) | 100% |
| examples 框架 | sz-orm-examples 包，11 个 bin 示例（quick_start/model_definition/transaction/migration/hooks_soft_delete/multi_tenant/production_app/production_dtx/real_db_crud/db_verify_test/sz_rust_integration_example） | [examples/Cargo.toml:24](file:///E:/vue/test/鲜视达/rust/sz-orm/examples/Cargo.toml#L24) | 100% |
| workspace 版本集中管理 | `workspace.package.version`，edition="2021"，rust-version="1.81" | [Cargo.toml:6](file:///E:/vue/test/鲜视达/rust/sz-orm/Cargo.toml#L6) | 100% |

### 1.1.2 需要扩展的功能

| 需求功能 | 存量功能 | 差异说明 | 扩展方向 |
|---------|---------|---------|---------|
| 18 个扩展包补齐单元测试（REQ-TC-001） | 各包 src/ 有实现，tests/ 0 文件 | 测试差异：需新增 tests/ 目录与测试文件；覆盖差异：需正常 + 边界 + 错误路径覆盖；约束差异：覆盖率 ≥ 60% 行覆盖 | 各包新增 tests/ 目录 + 测试文件，不修改既有公开 API；feature gate "test-coverage" 聚合（各包独立测试无需 feature，聚合 gate 用于门禁矩阵） |
| sz-orm-es `real` feature 占位 + 真实 ES 集成测试（REQ-TC-003/REQ-AR-001） | sz-orm-es MOCK-ONLY，Cargo.toml 无 feature 区分 | 接口差异：需新增 `real` feature 声明；行为差异：真实 ES 集成测试标注 `#[cfg(feature="real")]` + `#[ignore]`；约束差异：真实 ES 需认证 | sz-orm-es Cargo.toml 新增 `real` feature 占位（声明不实现），新增 tests/real_es_integration.rs 真实 ES 集成测试；既有 Mock 行为不变 |
| sz-orm-config 真实 Consul/Nacos 实现（REQ-TC-004） | 仅内存 `ConsulConfigCenter`，未集成真实配置中心 | 接口差异：需新增真实 Consul/Nacos 客户端；行为差异：真实配置读写/监听/服务发现；约束差异：需认证（ACL/Token） | sz-orm-config 新增 `real-consul` / `real-nacos` feature（基于 reqwest），真实客户端实现 + 集成测试；既有内存实现不变 |
| 313 个 pub API 文档补齐（REQ-AR-002/REQ-DOC-001） | docs.rs cfg 跳过 missing_docs，313 个 pub API 缺 `///` | 文档差异：需补齐 313 个 `///` 文档注释；配置差异：移除 docs.rs cfg 跳过；约束差异：API 签名不变 | 逐批补齐 313 个 pub API 文档（功能/参数/返回/示例/错误），移除 [lib.rs:406](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L406) docs.rs cfg 跳过；feature gate "doc-completion" 聚合 |
| README 成熟度声明更新（REQ-AR-003/REQ-DOC-002/REQ-PC-002） | 第 46 行自称"原型阶段/无生产案例" | 内容差异：需移除过时声明，补充 sz-pay 案例；状态差异：更新为"早期生产可用（内部项目）" | README.md 更新成熟度声明，补充 sz-pay 生产使用证据（7 包/297 引用/5139 测试/crates.io） |
| `#[derive(Schema)]` 宏扩展生成编译期列名常量（REQ-TS-001） | 既有 Schema derive 生成 schema 元数据，未生成列名常量 | 接口差异：需为每个字段生成 `StructName::FIELD_NAME` 常量；行为差异：列名拼写错误编译期暴露；约束差异：既有 derive 行为不变 | 扩展 sz-orm-macros Schema derive，新增列名常量生成（feature gate "type-safe-columns" 聚合 typed-schema）；既有 derive 输出不变 |
| typed_ast.rs Diesel 风格 DSL 完善（REQ-TS-003） | 既有 typed_ast.rs 强类型 AST 基础，未提供完整 DSL | 接口差异：需新增 `eq/gt/lt/like/in` 等表达式；行为差异：生成 SQL 与 QueryBuilder 一致；约束差异：既有 QueryBuilder API 不变 | 扩展 typed_ast.rs 模块，完善 Diesel 风格表达式 DSL（feature gate "type-safe-columns" 聚合 typed-dsl）；差分测试保证与 QueryBuilder 一致 |
| sz-orm-core feature 矩阵扩展 | 现有 features: default=["redis"]，含 11 个 feature（testing/db-verify/redis/circuit-breaker/rate-limit/auto-prewarm/plan-cache/zero-copy/simd/multi-tenant-enhanced/dist-cache） | 组合差异：新增 10 个聚合 feature 需纳入组合矩阵；依赖差异：perf-smallstring 需 smallstring/compactstr，test-coverage 聚合 real-es/real-config | 各包 Cargo.toml 新增 feature 定义 + 可选依赖；纳入门禁 10 Feature 全组合编译 |

### 1.1.3 需要新增的功能或接口

按业务模块分组，以下功能在存量代码中完全没有对应实现，需新增。

**模块 A：测试覆盖盲区补齐（对应 REQ-TC-001~005，扩展 18 个扩展包 + sz-orm-core 测试修复）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| 18 个扩展包单元测试 | 各包核心逻辑 | 测试文件 + 覆盖率报告 | 为每个包新增 tests/ 目录，编写正常 + 边界 + 错误路径测试，覆盖率 ≥ 60% | 各包既有依赖（无新增） |
| MySQL INSERT IGNORE 测试修复 | 测试表 DDL | 修复后测试通过 | 为测试表 name 列添加 UNIQUE 约束，使 INSERT IGNORE 重复插入被忽略 | 无新增（修改测试 DDL） |
| sz-orm-es 真实 ES 集成测试 | 真实 ES 环境 | 集成测试通过 | `#[cfg(feature="real")]` + `#[ignore]` 标注，覆盖索引/搜索/聚合/过滤，Mock/真实差分 | elasticsearch crate（optional，real feature） |
| sz-orm-config 真实 Consul/Nacos 客户端 | 配置中心地址 + 认证 | 真实配置读写/监听/服务发现 | 基于 reqwest 实现 Consul/Nacos HTTP API 客户端，配置变更监听 | reqwest（optional，real-consul/real-nacos feature） |

**模块 B：架构改进（对应 REQ-AR-001~006，扩展 sz-orm-es/config + 文档）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| Mock 包 `real` feature 占位 | Cargo.toml | feature 声明 | 声明 `real` feature 但不立即实现，区分 mock/real 模式 | 无（占位） |
| async trait 风格统一评估 | Connection trait + 其他 trait | 评估文档 + 推荐方案 | 性能基准对比（宏 vs 手动）+ 迁移影响分析 + 学习成本评估 | 无（纯评估文档） |
| sz-orm-query-builder 选择指南 | 两个 QueryBuilder | 选择指南文档 | 能力对比表 + 适用场景 + 性能基准 + 迁移建议 | 无（纯文档） |

**模块 C：性能优化落地（对应 REQ-PF-001~007，扩展 sz-orm-core）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| query.rs SmallString/CompactString | SQL 构造字符串 | 减少堆分配 | 短字符串内联存储（≤23 字节），长字符串退化为堆；feature gate 隔离 | smallstring/compactstr（optional） |
| dialect.rs enum dispatch | 方言分发 | 消除 vtable | `enum DialectKind` + match 分发替代 `Box<dyn Dialect>` | 无（自研） |
| l2_cache.rs zero-copy 推广 | L2 缓存序列化 | 减少分配 | 推广既有 zero-copy 到 L2 缓存路径 | 无（复用既有 zero-copy） |
| value.rs Box<str> | Value 枚举字符串变体 | 节省 capacity 字段 | Box<str> 替代 String，节省 8 字节/值 | 无（std） |
| 性能基准对比完善 | 6 组优化对比 | 基准报告 | zero-copy/simd/plan-cache/SmallString/enum dispatch/Box<str> 对比基准 | criterion（既有） |

**模块 D：编译期类型安全增强（对应 REQ-TS-001~005，扩展 sz-orm-macros + sz-orm-core）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| `#[derive(Schema)]` 列名常量生成 | 结构体字段 | 编译期列名常量 | 为每个字段生成 `StructName::FIELD_NAME: &'static str` 常量 | 无（复用既有 proc-macro） |
| `Column<T>` 类型安全列引用 | 表类型 T + 列名 | 类型安全列引用 | `Column<T>` 泛型关联列与表，编译期防止跨表引用 | 无 |
| typed_ast.rs Diesel 风格 DSL | 表/列/表达式 | 类型安全 DSL | `users::id.eq(1)` / `users::age.gt(18)` 等表达式，编译期类型安全 | 无（复用既有 typed_ast） |

**模块 E：文档与生态建设（对应 REQ-DOC-001~004，纯文档）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| 313 个 pub API 文档 | 缺文档的 pub API | `///` 文档注释 | 逐批补齐功能/参数/返回/示例/错误文档 | 无（纯文档） |
| Diesel/SeaORM/SQLx 迁移指南 | 竞品 API 对照 | 三份迁移指南 | 概念映射表 + API 对照表 + 示例代码 + 常见陷阱 | 无（纯文档） |

**模块 F：sz-pay 生产案例深化（对应 REQ-PC-001~004，扩展 examples）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| sz-pay 使用模式抽取 | sz-pay 使用 sz-orm 的模式 | examples/sz_pay_pattern.rs | 抽取连接池/SQL/错误/队列/调度典型用法（脱敏） | sz-orm-core/sqlx/config/auth/queue（既有） |
| sz-pay 生产运行数据收集（可选） | sz-pay 生产环境 | 脱敏运行数据 | 连接池命中率/查询延迟/错误率（脱敏） | 无（可选） |

## 1.2 存量功能详细分析

### 1.2.1 sz-orm-es MOCK-ONLY 实现（lib.rs:1）

- **接口契约**：`EsSync` trait（索引/搜索/聚合/过滤抽象）+ 内存 `MockEsBackend`（HashMap 存储）。lib.rs 顶部明确标注"MOCK-ONLY，非生产可用"，所有数据存储在进程内 HashMap，重启即丢。
- **业务规则**：Mock 实现完整覆盖 EsSync trait 接口，查询语义为简化版，不保证与真实 ES 行为一致。不支持分布式、副本、分片、持久化、TLS、认证等任何 ES 生产特性。
- **扩展点**：`EsSync` trait 是真实后端扩展点，v3.4.0 新增 `real` feature 占位（声明 feature 但不立即实现真实后端），为后续真实实现预留接口。真实 ES 集成测试通过 `#[cfg(feature="real")]` 条件编译。
- **约束**：Cargo.toml 当前无 feature 定义（[packages/sz-orm-es/Cargo.toml:13](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-es/Cargo.toml#L13) 仅 serde/serde_json/chrono 依赖），v3.4.0 新增 `[features] real = []` 占位。真实 ES 实现可作为后续版本交付，v3.4.0 仅占位 + 集成测试框架。

### 1.2.2 sz-orm-config 内存实现（lib.rs:42）

- **接口契约**：`ConsulConfigCenter`（data: HashMap<String, String> + subscribers: HashMap<String, Vec<ConfigChangeCallback>> + events: Mutex<Vec<ConfigChangeEvent>>）。提供配置读写/监听/事件通知 API，但全部基于内存 HashMap。
- **业务规则**：配置变更触发订阅者回调（ConfigChangeCallback），事件记录在 Mutex<Vec> 中。未集成真实 Consul/Nacos HTTP API，无网络通信。
- **扩展点**：`ConsulConfigCenter` 的 API 接口是真实实现扩展点，v3.4.0 新增 `real-consul` / `real-nacos` feature，基于 reqwest 实现真实 HTTP API 客户端，API 接口与内存实现一致（配置读写/监听/服务发现）。
- **约束**：Cargo.toml 当前仅 serde/serde_json 依赖（[packages/sz-orm-config/Cargo.toml:13](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-config/Cargo.toml#L13)），v3.4.0 新增 reqwest optional 依赖 + real-consul/real-nacos feature。真实实现需认证（Consul ACL Token / Nacos Username+Password），禁止未认证读取配置。

### 1.2.3 Connection trait 手动解糖（pool.rs:42）

- **接口契约**：`Connection` trait 手动解糖 async 方法（`fn execute<'a>(&'a mut self, sql: &'a str) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>>`），不使用 `#[async_trait]` 宏。所有 async 方法使用单一生命周期 `'a`（绑定 `&'a mut self` 和 `&'a str`），而非 HRTB。
- **业务规则**：手动解糖的原因是避免 `&str` 参数触发 HRTB（Higher-Rank Trait Bound）与 sqlx::Executor 冲突。注释明确说明"以避免 `&str` 参数触发 HRTB 与 sqlx::Executor 冲突"。
- **扩展点**：v3.4.0 仅评估统一方案（不强制迁移），评估含性能基准对比（宏展开开销 vs 手动解糖开销）+ 迁移影响分析（涉及哪些 trait 与调用方）+ 学习成本评估。
- **约束**：Connection trait 手动解糖有技术原因（HRTB 冲突），统一为 `#[async_trait]` 可能重新引入冲突。评估必须考虑此约束，推荐方案可能是"保持手动解糖 + 文档说明原因"而非强制统一。

### 1.2.4 `#[derive(Schema)]` 宏（macros/lib.rs:71）

- **接口契约**：`#[derive(Schema)]` 过程宏，从结构体字段生成 schema 元数据（表名/列名/类型/关系）。sz-orm-macros/src/derive/ 模块实现 derive 逻辑。
- **业务规则**：Schema derive 从 `#[derive(Model)]` 结构体提取字段元数据，生成 Schema 结构供 ORM 使用。当前不生成编译期列名常量。
- **扩展点**：v3.4.0 扩展 Schema derive，为每个字段生成 `StructName::FIELD_NAME: &'static str` 常量（如 `User::ID = "id"`、`User::NAME = "name"`），列名拼写错误在编译期暴露。通过 feature gate "type-safe-columns" 隔离，既有 derive 输出不变。
- **约束**：过程宏编译时执行，零运行时开销。对复杂嵌套类型（泛型/生命周期/trait object）支持不足，需跳过不支持字段并告警。

### 1.2.5 typed_ast.rs 强类型 AST（typed_ast.rs）

- **接口契约**：typed_ast.rs 模块提供强类型 AST 基础，含类型化表达式节点。当前未提供完整 Diesel 风格 DSL（`eq/gt/lt/like/in` 等表达式）。
- **业务规则**：强类型 AST 在编译期保证类型安全，类型不匹配编译失败。当前为基础模块，未与 QueryBuilder 集成。
- **扩展点**：v3.4.0 完善 typed_ast.rs，提供 Diesel 风格表达式 DSL（`users::id.eq(1)` / `users::age.gt(18)` / `users::name.like("%foo%")`），生成 SQL 与运行时 QueryBuilder 行为一致（差分测试覆盖）。通过 feature gate "type-safe-columns" 隔离。
- **约束**：DSL 生成的 SQL 必须与 QueryBuilder 行为一致（差分测试），列名/表名参数化绑定或编译期常量内联（禁止字符串拼接），值参数化绑定。

### 1.2.6 sz-orm-query-builder 与 core::QueryBuilder（query-builder/lib.rs:53）

- **接口契约**：`sz-orm-query-builder::Query`（独立 SQL 构造器，sea-query 风格，不绑定 Model）vs `sz-orm-core::QueryBuilder<M>`（绑定 Model `<M: Model>`，编译期表/列校验）。lib.rs:53 已有区别说明表格。
- **业务规则**：sz-orm-query-builder 适用于纯 SQL 构造/动态查询场景，不依赖 sz-orm-core Model trait。sz-orm-core::QueryBuilder 适用于 ORM 完整流程，绑定 Model 提供编译期表/列校验。
- **扩展点**：v3.4.0 编写选择指南，含能力对比表（支持的查询类型/方言/特性）+ 适用场景（独立 SQL 构造 vs ORM 集成）+ 性能基准对比 + 迁移建议，消除用户对两个 QueryBuilder 的困惑。
- **约束**：两个 QueryBuilder 长期共存（不合并），选择指南消除困惑。性能基准对比需附证据。

### 1.2.7 docs.rs cfg 跳过（lib.rs:403）

- **接口契约**：`#![cfg_attr(docsrs, warn(missing_docs))]` 配置，仅在 docs.rs 构建时启用 missing_docs lint，本地和 CI clippy 不触发。注释说明"避免 313 个 pub API 缺文档阻塞开发"。
- **业务规则**：docs.rs 构建文档时启用 missing_docs lint，但因 313 个 pub API 缺文档，实际 docs.rs 文档不完整。本地和 CI 不触发 missing_docs，避免阻塞开发。
- **扩展点**：v3.4.0 逐批补齐 313 个 pub API 文档后，移除 docs.rs cfg 跳过，改为全局 `#![warn(missing_docs)]`，使 docs.rs 文档完整。
- **约束**：补齐文档不修改 API 签名（仅新增 `///` 注释）。分批补齐（优先公开 API > 内部 API > 测试 API），每批附进度跟踪。

---
# 二、增量设计方案

## 2.0 架构总览

### 2.0.1 v3.4.0 整体架构图

v3.4.0 在 v3.3.0 现有 workspace 基础上，不新增独立包，而是在 18 个扩展包内补齐测试，在 sz-orm-es/config 内新增 real feature 占位与真实实现，在 sz-orm-core 内扩展 5 个性能优化模块 + 3 个类型安全模块，在 sz-orm-macros 内扩展 Schema derive，在 examples 内新增 sz_pay_pattern 示例，通过 10 个聚合 feature gate 隔离，复用既有 `sz-orm-audit` / `sz-orm-masking` / `sz-orm-crypto` 基础设施。整体架构如下：

```plantuml
@startuml
!theme plain
title sz-orm v3.4.0 整体架构（增量视图）

rectangle "sz-orm v3.4.0 workspace" as WS {

  rectangle "核心层 sz-orm-core (扩展)" as Core {
    rectangle "既有: query.rs (QueryBuilder)" as QueryOld
    rectangle "既有: dialect.rs (Box<dyn Dialect>)" as DialectOld
    rectangle "既有: l2_cache.rs (L2Cache)" as L2Old
    rectangle "既有: value.rs (Value 枚举)" as ValueOld
    rectangle "既有: result_map.rs (反射式取值)" as RmOld
    rectangle "既有: typed_ast.rs (强类型 AST 基础)" as TaOld
    rectangle "既有: pool.rs (Connection trait 手动解糖)" as PoolOld

    rectangle "性能优化模块 (feature gate)" as PerfNew {
      rectangle "query.rs SmallString 路径 <<new>>\n[perf-smallstring]" as PfSm
      rectangle "dialect.rs enum dispatch 路径 <<new>>\n[perf-enum-dispatch]" as PfEd
      rectangle "l2_cache.rs zero-copy 推广 <<new>>\n[perf-zero-copy-l2]" as PfZc
      rectangle "value.rs Box<str> 路径 <<new>>\n[perf-box-str]" as PfBs
    }

    rectangle "类型安全模块 (feature gate)" as TsNew {
      rectangle "Column<T> 类型安全列引用 <<new>>\n[type-safe-columns]" as TsCol
      rectangle "typed_ast.rs Diesel DSL 完善 <<new>>\n[type-safe-columns]" as TsDsl
    }
  }

  rectangle "sz-orm-macros (扩展)" as Macros {
    rectangle "既有: derive(Schema)" as SchemaOld
    rectangle "Schema derive 列名常量扩展 <<new>>\n[type-safe-columns]" as TsSchema
  }

  rectangle "扩展包测试补齐 (18 包)" as ExtTests {
    rectangle "sz-orm-config/auth/crypto/audit/batch/rw/masking\nlogger/health/es/grpc/swagger/tracing\nobservability/back/lc/wasm/axum/actix/js/python\n各包 tests/ <<new>>" as Tests18
  }

  rectangle "sz-orm-es (扩展)" as Es {
    rectangle "既有: MockEsBackend (不变)" as EsMock
    rectangle "real feature 占位 + 真实 ES 集成测试 <<new>>\n[test-coverage]" as EsReal
  }

  rectangle "sz-orm-config (扩展)" as Cfg {
    rectangle "既有: ConsulConfigCenter 内存 (不变)" as CfgMem
    rectangle "real-consul/real-nacos 真实实现 <<new>>\n[test-coverage]" as CfgReal
  }

  rectangle "examples (扩展)" as Examples {
    rectangle "既有: 11 个 bin 示例" as ExOld
    rectangle "sz_pay_pattern.rs <<new>>\n[sz-pay-example]" as ExSzPay
  }

  rectangle "文档生态 (纯文档)" as Docs {
    rectangle "313 pub API 文档补齐 <<new>>\n[doc-completion]" as DocApi
    rectangle "README 成熟度更新 <<new>>\n[doc-completion]" as DocReadme
    rectangle "Diesel/SeaORM/SQLx 迁移指南 <<new>>\n[migration-guide]" as DocMig
    rectangle "async trait 评估 + query-builder 指南 <<new>>\n[arch-improvement]" as DocArch
  }
}

cloud "真实 Elasticsearch" as RealEs
cloud "真实 Consul / Nacos" as RealCfg
database "MySQL/PG/SQLite/Oracle/MSSQL" as Rdb
rectangle "sz-pay 生产项目" as SzPay

EsReal --> RealEs : 真实 ES 集成测试 (#[ignore])
CfgReal --> RealCfg : 真实配置中心集成测试
PfSm ..> QueryOld : 扩展 SQL 构造路径 (不破坏 API)
PfEd ..> DialectOld : 扩展方言分发 (不破坏 trait)
PfZc ..> L2Old : 推广 zero-copy 到 L2 路径
PfBs ..> ValueOld : 扩展 Value 枚举 (不破坏 API)
TsSchema ..> SchemaOld : 扩展 derive(Schema) (不破坏既有输出)
TsCol ..> QueryOld : 新增 Column<T> 重载
TsDsl ..> TaOld : 完善 typed_ast DSL
ExSzPay ..> SzPay : 抽取使用模式 (脱敏, ADR-0001 不改下游)
Tests18 --> Rdb : 部分集成测试 (#[ignore])

@enduml
```

### 2.0.2 6 大方向在 workspace 中的定位

| 方向 | 需求组 | 包名 | 形态 | 聚合 feature gate | 在 workspace 中的位置 | 依赖关系 |
|------|--------|------|------|-------------|---------------------|---------|
| 测试覆盖补齐 | REQ-TC-001~005 | 18 个扩展包 + sz-orm-core + sz-orm-es + sz-orm-config | **补齐测试 + real feature 占位** | `test-coverage` | 各扩展包 tests/ + sz-orm-es Cargo.toml + sz-orm-config Cargo.toml | 无新增重依赖（real feature 占位）；真实集成测试 optional 依赖 elasticsearch/reqwest |
| 架构改进 | REQ-AR-001~006 | sz-orm-es + sz-orm-config + 文档 | **real feature 占位 + 文档补齐 + 评估文档** | `arch-improvement` | sz-orm-es/config Cargo.toml + docs/ | 无新增依赖（纯文档 + feature 占位） |
| 性能优化落地 | REQ-PF-001~007 | `sz-orm-core` | **扩展新模块** | `perf-smallstring` / `perf-enum-dispatch` / `perf-zero-copy-l2` / `perf-box-str` | `packages/sz-orm-core/src/` 内条件编译 | smallstring/compactstr（optional）；enum dispatch/zero-copy/Box<str> 无新增 |
| 编译期类型安全增强 | REQ-TS-001~005 | `sz-orm-macros` + `sz-orm-core` | **扩展 derive 宏 + 新增 Column<T> + 完善 DSL** | `type-safe-columns` | `packages/sz-orm-macros/src/derive/` + `packages/sz-orm-core/src/typed_ast.rs` | 无新增（复用既有 proc-macro） |
| 文档与生态建设 | REQ-DOC-001~004 | 文档 | **纯文档** | `doc-completion` / `migration-guide` | `packages/sz-orm-core/src/` 文档注释 + `README.md` + `docs/migration/` | 无新增（纯文档） |
| sz-pay 生产案例深化 | REQ-PC-001~004 | `examples` | **新增示例** | `sz-pay-example` | `examples/src/bin/sz_pay_pattern.rs` | sz-orm-core/sqlx/config/auth/queue（既有） |

### 2.0.3 与 v3.3.0 现有架构的演进关系

```plantuml
@startuml
!theme plain
title v3.3.0 → v3.4.0 架构演进

rectangle "v3.3.0 (缓存一致性 + GraphQL + 多租户 + AI 增强)" as V33 {
  rectangle "sz-orm-core\n(query/pool/dialect/l2_cache/value/result_map/typed_ast\n+dist-cache/multi-tenant-enhanced)" as Core33
  rectangle "sz-orm-graphql (GraphQLSchema/DbResolver/DataLoader)" as Gql33
  rectangle "sz-orm-ai (Nl2SqlEngine/IntentAnalyzer/IndexAdvisor)" as Ai33
  rectangle "18 扩展包 (0 测试)" as Ext33
  rectangle "sz-orm-es (MOCK-ONLY)" as Es33
  rectangle "sz-orm-config (仅内存)" as Cfg33
}

rectangle "v3.4.0 增量 (10 聚合 feature gate 隔离)" as V34 {
  rectangle "18 扩展包 +tests\n[test-coverage]" as Tc34
  rectangle "sz-orm-es +real 占位 + sz-orm-config +real-consul/nacos\n[test-coverage]" as Real34
  rectangle "sz-orm-core +SmallString/enum-dispatch/zero-copy-l2/Box<str>\n[perf-smallstring/enum-dispatch/zero-copy-l2/box-str]" as Perf34
  rectangle "sz-orm-macros +Schema 列名常量 + sz-orm-core +Column<T> +typed DSL\n[type-safe-columns]" as Ts34
  rectangle "313 pub API 文档 + README + 迁移指南\n[doc-completion/migration-guide]" as Doc34
  rectangle "examples/sz_pay_pattern.rs\n[sz-pay-example]" as Ex34
  rectangle "async trait 评估 + query-builder 指南\n[arch-improvement]" as Arch34
}

Core33 ..> Perf34 : 扩展性能路径 (不破坏 API)
Core33 ..> Ts34 : 扩展类型安全 (不破坏 API)
Ext33 ..> Tc34 : 补齐测试
Es33 ..> Real34 : real feature 占位
Cfg33 ..> Real34 : real-consul/nacos 实现
Core33 ..> Doc34 : 补齐 313 pub API 文档

note bottom of V34
  **演进原则**：
  1. 无 Breaking Change（现有公开 API 签名不变）
  2. 新增能力通过 10 个聚合 feature gate 隔离（默认关闭）
  3. 不新增独立包，全部在既有包内扩展
  4. 复用现有基础设施（typed_ast/zero-copy/derive(Schema)/examples 框架）
  5. 五方言行为一致（增强在 core/macros 层统一，不触碰方言驱动）
  6. ADR-0001 严禁修改下游/上游仓库（sz-pay 案例仅抽取不改下游）
  7. 测试覆盖不回退（v3.3.0 基线 6,327 测试仅增不减）
end note

@enduml
```

**演进关键决策**：

| 决策点 | 选项 | 选择 | 理由 |
|--------|------|------|------|
| 18 扩展包测试补齐策略 | A. 统一测试模板 / B. 各包独立分析核心逻辑编写 | B | 各包职责差异大（auth/crypto/audit/batch/rw/masking/logger/health/es/grpc/swagger/tracing/observability/back/lc/wasm/axum/actix/js/python），统一模板无法覆盖核心逻辑；独立分析确保测试有效性 |
| sz-orm-es real feature 实现深度 | A. v3.4.0 实现真实 ES / B. v3.4.0 仅占位 + 集成测试框架 | B | 真实 ES 实现需 elasticsearch crate 重依赖 + 完整 API 覆盖，v3.4.0 聚焦测试覆盖补齐，真实实现作为后续版本；占位 + 测试框架为后续实现预留 |
| sz-orm-config 真实实现方式 | A. 引入 consul/nacos 官方 crate / B. 基于 reqwest 自研 HTTP 客户端 | B | consul/nacos 无成熟 Rust 官方 crate；reqwest 已是 workspace 常用依赖，自研 HTTP 客户端可控且轻量 |
| SmallString/CompactString 引入方式 | A. 引入 smallstring crate / B. 引入 compactstr crate / C. 自研 | B | compactstr 更成熟（Cargo 下载量大、API 稳定），支持 `CompactString` 类型与 String 互转；optional 依赖 feature gate 隔离 |
| enum dispatch 实现方式 | A. 完全替代 Box<dyn Dialect> / B. 新增 enum 路径并行存在 | B | 完全替代是 Breaking Change（Dialect trait 公开）；新增 enum 路径通过 feature gate 隔离，既有 Box<dyn Dialect> 不变 |
| zero-copy L2 推广方式 | A. 替换 L2Cache 序列化 / B. 新增 zero-copy 路径可选 | B | 替换是 Breaking Change；新增 zero-copy 路径通过 feature gate 隔离，既有序列化不变，兼容性测试覆盖 |
| Box<str> 实现方式 | A. 替换 Value 枚举 String 变体 / B. 新增 Box<str> 变体并行存在 | B | 替换是 Breaking Change（Value 枚举公开）；新增 Box<str> 变体通过 feature gate 隔离，既有 String 变体不变 |
| `#[derive(Schema)]` 列名常量生成方式 | A. 新增独立 derive 宏 / B. 扩展既有 Schema derive | B | 扩展既有 derive 保持用户代码不变（`#[derive(Schema)]` 不变），通过 feature gate 控制是否生成列名常量 |
| `Column<T>` 与既有 `&str` API 关系 | A. 替代 &str / B. 新增 Column<T> 重载并行存在 | B | 替代是 Breaking Change；新增 Column<T> 重载，既有 &str API 不变，Column<T> 可解引用为 &str 支持混用 |
| typed_ast.rs DSL 与 QueryBuilder 关系 | A. 替代 QueryBuilder / B. 并行存在 + 差分测试 | B | 替代是 Breaking Change；并行存在，DSL 生成 SQL 与 QueryBuilder 行为一致（差分测试），用户可选择 |
| 10 聚合 feature gate 与 spec 细粒度 feature 映射 | A. 仅用聚合 gate / B. 聚合 + 细粒度双层 | B | 聚合 gate 用于门禁矩阵与用户启用（10 个），细粒度 feature 用于包内隔离（real-es/real-consul/real-nacos/typed-schema/typed-column/typed-dsl 等），聚合 gate 聚合细粒度 |
| sz-pay 案例抽取位置 | A. 独立 examples 包 / B. examples/src/bin/ 既有框架 | B | examples 既有框架已支持多 bin（[examples/Cargo.toml:24](file:///E:/vue/test/鲜视达/rust/sz-orm/examples/Cargo.toml#L24)），新增 bin 复用既有依赖配置 |
| async trait 风格统一处理 | A. v3.4.0 强制迁移 / B. v3.4.0 仅评估文档 | B | 强制迁移涉及 Connection trait（手动解糖有 HRTB 技术原因）+ 大量调用方，回归风险高；评估文档附推荐方案，迁移作为后续版本 |
| 313 pub API 文档补齐策略 | A. 一次性补齐 / B. 分批补齐（公开 > 内部 > 测试） | B | 313 个一次性补齐工作量巨大；分批补齐优先公开 API，每批附进度跟踪，版本交付时全部补齐 |

---

## 2.1 测试覆盖盲区补齐（REQ-TC-001~005）

### 2.1.1 模块目标

为 18 个零测试扩展包（sz-orm-config/auth/crypto/audit/batch/rw/masking/logger/health/es/grpc/swagger/tracing/observability/back/lc/wasm/axum/actix/js/python）补齐单元测试，修复 MySQL INSERT IGNORE 测试缺陷（[integration_mysql.rs:1267](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/integration_mysql.rs#L1267)），为 sz-orm-es 增加 `real` feature 占位 + 真实 ES 集成测试，为 sz-orm-config 增加真实 Consul/Nacos 客户端实现 + 集成测试。补齐后各包行覆盖率 ≥ 60%，全 workspace 测试数 ≥ 6,327 + 新增数，不修改既有公开 API。

### 2.1.2 架构设计

```plantuml
@startuml
!theme plain
title 测试覆盖盲区补齐模块架构

package "18 扩展包测试补齐" as ExtTests {
  rectangle "sz-orm-config tests/\n(内存实现 + 真实 Consul/Nacos)" as CfgTests
  rectangle "sz-orm-auth tests/\n(JWT/RBAC/OAuth2/MFA)" as AuthTests
  rectangle "sz-orm-crypto tests/\n(AES-256-GCM/HMAC/SHA-256)" as CryptoTests
  rectangle "sz-orm-audit tests/\n(SQL 审计 + 脱敏)" as AuditTests
  rectangle "sz-orm-batch tests/\n(批量 INSERT/UPDATE/UPSERT)" as BatchTests
  rectangle "sz-orm-rw tests/\n(读写分离路由)" as RwTests
  rectangle "sz-orm-masking tests/\n(数据脱敏)" as MaskingTests
  rectangle "sz-orm-logger tests/\n(结构化日志)" as LoggerTests
  rectangle "sz-orm-health tests/\n(健康检查 + 断路器)" as HealthTests
  rectangle "sz-orm-es tests/\n(Mock + 真实 ES 集成)" as EsTests
  rectangle "sz-orm-grpc tests/\n(gRPC 抽象)" as GrpcTests
  rectangle "sz-orm-swagger tests/\n(OpenAPI 生成)" as SwaTests
  rectangle "sz-orm-tracing tests/\n(分布式追踪)" as TraTests
  rectangle "sz-orm-observability tests/\n(Prometheus + SLO)" as ObsTests
  rectangle "sz-orm-back tests/\n(备份恢复)" as BackTests
  rectangle "sz-orm-lc tests/\n(低代码声明)" as LcTests
  rectangle "sz-orm-wasm tests/\n(WASM 查询)" as WasmTests
  rectangle "sz-orm-axum/actix tests/\n(Web 集成)" as WebTests
  rectangle "sz-orm-js/python tests/\n(多语言绑定)" as BindTests
}

package "sz-orm-core 测试修复" as CoreFix {
  rectangle "integration_mysql.rs:1267\nINSERT IGNORE 测试表 UNIQUE 约束修复" as MysqlFix
}

package "sz-orm-es real feature" as EsReal {
  rectangle "Cargo.toml +real feature 占位" as EsFeature
  rectangle "tests/real_es_integration.rs\n#[cfg(feature=\"real\")] + #[ignore]" as EsIntegration
  rectangle "tests/mock_real_diff.rs\nMock/真实行为差分测试" as EsDiff
}

package "sz-orm-config 真实实现" as CfgReal {
  rectangle "Cargo.toml +real-consul/real-nacos feature" as CfgFeature
  rectangle "src/consul_client.rs\n(基于 reqwest 真实 Consul HTTP API)" as ConsulClient
  rectangle "src/nacos_client.rs\n(基于 reqwest 真实 Nacos HTTP API)" as NacosClient
  rectangle "tests/real_consul_integration.rs\n#[cfg(feature=\"real-consul\")] + #[ignore]" as ConsulTest
  rectangle "tests/real_nacos_integration.rs\n#[cfg(feature=\"real-nacos\")] + #[ignore]" as NacosTest
}

cloud "真实 Elasticsearch" as RealEs
cloud "真实 Consul" as RealConsul
cloud "真实 Nacos" as RealNacos
database "MySQL (UNIQUE 修复)" as Mysql

EsIntegration --> RealEs : 真实 ES 集成测试
ConsulTest --> RealConsul : 真实 Consul 集成测试
NacosTest --> RealNacos : 真实 Nacos 集成测试
MysqlFix --> Mysql : UNIQUE 约束修复

@enduml
```

### 2.1.3 核心数据结构设计

**18 扩展包测试结构** — 每个扩展包新增 tests/ 目录与测试文件：

- 测试文件命名：`tests/<module>_test.rs`（按包内核心模块组织）
- 测试内容：正常路径 + 边界条件 + 错误处理三类测试
- 覆盖率要求：各包行覆盖率 ≥ 60%（`cargo tarpaulin -p <package>` 证据）
- 约束：不修改既有公开 API；测试真实验证包内核心逻辑（禁止仅 `assert!(true)` 的无效覆盖）

**MySQL INSERT IGNORE 测试修复** — 修改测试表 DDL：

- 当前缺陷：[integration_mysql.rs:1267](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/integration_mysql.rs#L1267) 测试表 `name` 列缺 UNIQUE 约束，`INSERT IGNORE` 重复插入未被忽略，`affected_rows` 返回 1 而非预期 0
- 修复方式：修改测试表 DDL 为 `CREATE TABLE ... (name VARCHAR(...) UNIQUE, ...)`（添加 UNIQUE 约束）
- 约束：仅修改测试表 DDL，不影响生产代码；修复后 `cargo test -p sz-orm-core --test integration_mysql test_mysql_insert_or_ignore_duplicate` 通过

**sz-orm-es `real` feature 占位** — Cargo.toml feature 声明：

- `[features] real = []`（占位，不引入依赖）
- 默认 feature 不含 `real`，默认 mock 行为不变
- `cargo check -p sz-orm-es --features real` 编译通过（占位无代码）
- 约束：占位 feature 声明，非代码内 `todo!`（不违反禁止占位实现铁律）

**sz-orm-es 真实 ES 集成测试** — `tests/real_es_integration.rs`：

- `#[cfg(feature = "real")]` 条件编译，仅启用 real feature 时编译
- `#[ignore]` 标注，默认不运行（需真实 ES 环境）
- 测试覆盖：索引创建/文档索引/搜索/聚合/过滤
- Mock/真实行为差分测试：`tests/mock_real_diff.rs`，对比 Mock 与真实 ES 行为一致性
- 约束：真实 ES 需认证（API Key / Basic Auth）；测试需真实 ES 环境（`elasticsearch` crate optional 依赖）

**sz-orm-config 真实 Consul 客户端** — `src/consul_client.rs`：

- 字段：`base_url: String`（Consul HTTP API 地址）、`token: Option<String>`（ACL Token 认证）、`client: reqwest::Client`（HTTP 客户端）
- 方法：`get_config(key) -> Result<String>`（GET /v1/kv/{key}）、`set_config(key, value) -> Result<()>`（PUT /v1/kv/{key}）、`watch(key) -> Stream<ConfigChangeEvent>`（GET /v1/event/watch 长轮询）、`register_service(svc) -> Result<()>`（PUT /v1/agent/service/register）
- 约束：ACL Token 认证（禁止未认证读取）；基于 reqwest HTTP 客户端；配置变更监听长轮询

**sz-orm-config 真实 Nacos 客户端** — `src/nacos_client.rs`：

- 字段：`base_url: String`（Nacos HTTP API 地址）、`username: String` / `password: String`（Username+Password 认证）、`client: reqwest::Client`
- 方法：`get_config(data_id, group) -> Result<String>`（GET /nacos/v1/cs/configs）、`set_config(data_id, group, content) -> Result<()>`（POST /nacos/v1/cs/configs）、`watch(data_id, group) -> Stream<ConfigChangeEvent>`（长轮询监听）、`register_service(svc) -> Result<()>`（POST /nacos/v1/ns/instance）
- 约束：Username+Password 认证（禁止未认证读取）；基于 reqwest HTTP 客户端

### 2.1.4 核心流程设计

**18 扩展包测试补齐流程**：

```plantuml
@startuml
!theme plain
title 18 扩展包测试补齐流程

start
:选择零测试包 (如 sz-orm-audit);
:分析包内核心逻辑 (src/ 模块);
:识别可测试纯函数 + 外部交互抽象;
:编写单元测试 (正常 + 边界 + 错误);
:运行 cargo test -p <package>;
if (全部通过) then (是)
  :运行 cargo tarpaulin -p <package>;
  if (覆盖率 >= 60%) then (是)
    :记录覆盖率证据;
  else (否)
    :补充测试至 >= 60%;
  endif
else (否)
  :修复测试失败;
endif
:代码审查 (拒绝仅 assert!(true) 无效覆盖);
stop

@enduml
```

**sz-orm-config 真实 Consul/Nacos 集成测试流程**：

```plantuml
@startuml
!theme plain
title 真实 Consul/Nacos 集成测试流程

start
:启用 real-consul feature;
:配置真实 Consul 地址 + ACL Token;
:运行 cargo test -p sz-orm-config --features real-consul -- --ignored;
if (真实 Consul 环境可用) then (是)
  :测试配置读写 (set/get);
  :测试配置监听 (watch 变更通知);
  :测试服务发现 (register/deregister);
  :内存/真实行为差分测试;
  if (行为一致) then (是)
    :集成测试通过;
  else (否)
    :修复行为差异;
  endif
else (环境不可用)
  :跳过 (#[ignore] 标注);
endif
stop

@enduml
```

### 2.1.5 Feature gate 配置

```toml
# packages/sz-orm-es/Cargo.toml [features] 新增
[features]
real = []  # 占位，为后续真实 ES 实现预留
# [dependencies] 新增（optional，real feature 启用时）
elasticsearch = { version = "8.5", optional = true }

# packages/sz-orm-config/Cargo.toml [features] 新增
[features]
real-consul = ["dep:reqwest"]
real-nacos = ["dep:reqwest"]
# [dependencies] 新增
reqwest = { version = "0.12", features = ["json"], optional = true }

# packages/sz-orm-core/Cargo.toml [features] 新增（聚合 gate）
test-coverage = []  # 聚合 18 扩展包测试 + real-es/real-config（用于门禁矩阵标识）
```

- 18 扩展包测试无需 feature gate（测试在 tests/ 目录，默认编译运行）
- `test-coverage` 聚合 gate 用于门禁矩阵标识与 CI 路由（不引入依赖，仅标识）
- sz-orm-es `real` feature 占位（不立即实现，为后续预留）
- sz-orm-config `real-consul` / `real-nacos` feature 引入 reqwest optional 依赖
- 真实集成测试标注 `#[cfg(feature="real")]` + `#[ignore]`，默认不运行
- 与既有 `redis` / `circuit-breaker` / `rate-limit` / `auto-prewarm` / `plan-cache` / `zero-copy` / `simd` / `multi-tenant-enhanced` / `dist-cache` feature 正交

### 2.1.6 与现有代码集成点

| 集成点 | 位置 | 集成方式 | 兼容性 |
|--------|------|---------|--------|
| 18 扩展包 tests/ 目录 | 各包 `tests/` 目录（当前 0 文件） | 新增测试文件，不修改 src/ | 既有 src/ 代码不变 |
| MySQL INSERT IGNORE 测试表 DDL | [integration_mysql.rs:1267](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/integration_mysql.rs#L1267) | 修改测试表 DDL 添加 UNIQUE 约束 | 仅修改测试 DDL，不影响生产代码 |
| sz-orm-es Cargo.toml | [packages/sz-orm-es/Cargo.toml:13](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-es/Cargo.toml#L13) | 新增 `[features] real = []` + elasticsearch optional | 既有依赖不变，默认 mock 行为不变 |
| sz-orm-config Cargo.toml | [packages/sz-orm-config/Cargo.toml:13](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-config/Cargo.toml#L13) | 新增 `real-consul` / `real-nacos` feature + reqwest optional | 既有依赖不变，默认内存行为不变 |
| sz-orm-config ConsulConfigCenter | [packages/sz-orm-config/src/lib.rs:42](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-config/src/lib.rs#L42) | 新增 `consul_client.rs` / `nacos_client.rs` 模块（feature gate 隔离） | 既有 `ConsulConfigCenter` 不变 |

### 2.1.7 测试策略

| 测试类型 | 范围 | 工具 | 验收标准 |
|---------|------|------|---------|
| 18 扩展包单元测试 | 各包核心逻辑 | `cargo test -p <package>` | 全部通过 + 覆盖率 ≥ 60% |
| 覆盖率测量 | 18 扩展包 | `cargo tarpaulin -p <package>` | 行覆盖率 ≥ 60%，附 tarpaulin 报告 |
| MySQL INSERT IGNORE 修复 | integration_mysql.rs | `cargo test -p sz-orm-core --test integration_mysql test_mysql_insert_or_ignore_duplicate` | affected_rows = 0，测试通过 |
| sz-orm-es 真实 ES 集成 | sz-orm-es real feature | `cargo test -p sz-orm-es --features real -- --ignored` | 索引/搜索/聚合/过滤通过（需真实 ES） |
| sz-orm-es Mock/真实差分 | sz-orm-es | `cargo test -p sz-orm-es --features real -- --ignored` | Mock 与真实行为语义一致 |
| sz-orm-config 真实 Consul 集成 | sz-orm-config real-consul | `cargo test -p sz-orm-config --features real-consul -- --ignored` | 配置读写/监听/服务发现通过（需真实 Consul） |
| sz-orm-config 真实 Nacos 集成 | sz-orm-config real-nacos | `cargo test -p sz-orm-config --features real-nacos -- --ignored` | 配置读写/监听/服务发现通过（需真实 Nacos） |
| 无效覆盖拒绝 | 18 扩展包 | 代码审查 + 覆盖率检查 | 仅 `assert!(true)` 的无效覆盖被拒绝 |
| 全 workspace 回归 | 全 workspace | `cargo test --workspace` | 测试数 ≥ 6,327 + 新增数，全部通过 |

---

## 2.2 架构改进（REQ-AR-001~006）

### 2.2.1 模块目标

为 Mock 包（sz-orm-es）增加 `real` feature 占位（区分 mock/real 模式），补齐 313 个 pub API 文档并移除 docs.rs cfg 跳过（[lib.rs:406](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L406)），更新 README 成熟度声明（[README.md:46](file:///E:/vue/test/鲜视达/rust/sz-orm/README.md#L46)），评估 async trait 风格统一（[pool.rs:42](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L42)），编写 sz-orm-query-builder 选择指南（[query-builder/lib.rs:53](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-query-builder/src/lib.rs#L53)）。不修改既有公开 API（除文档注释）。

### 2.2.2 架构设计

```plantuml
@startuml
!theme plain
title 架构改进模块架构

package "Mock 包 real feature 占位" as MockReal {
  rectangle "sz-orm-es Cargo.toml\n+real feature 占位" as EsRealFeature
}

package "313 pub API 文档补齐" as ApiDoc {
  rectangle "sz-orm-core/src/ 各模块\n补齐 /// 文档注释" as DocComment
  rectangle "lib.rs:406 移除 docs.rs cfg 跳过\n改为 #![warn(missing_docs)]" as DocCfg
}

package "README 成熟度更新" as ReadmeUpdate {
  rectangle "README.md:46 移除'原型阶段/无生产案例'" as ReadmeFix
  rectangle "补充 sz-pay 案例\n(7 包/297 引用/5139 测试/crates.io)" as ReadmeAdd
  rectangle "状态更新为'早期生产可用（内部项目）'" as ReadmeStatus
}

package "async trait 风格统一评估" as AsyncEval {
  rectangle "docs/async_trait_style_evaluation.md\n性能基准对比 (宏 vs 手动)" as EvalBench
  rectangle "迁移影响分析 (涉及 trait 列表)" as EvalMig
  rectangle "学习成本评估 + 推荐方案" as EvalRec
}

package "sz-orm-query-builder 选择指南" as QbGuide {
  rectangle "docs/query_builder_selection_guide.md\n能力对比表 (查询类型/方言/特性)" as QbCompare
  rectangle "适用场景 + 性能基准 + 迁移建议" as QbAdvice
}

cloud "docs.rs" as DocsRs

DocComment --> DocsRs : 文档完整
DocCfg --> DocsRs : 移除 cfg 跳过

@enduml
```

### 2.2.3 核心数据结构设计

**313 个 pub API 文档补齐** — 文档注释结构：

- 每个 pub API 补齐 `///` 文档注释，含：功能描述（1-2 句）+ `# Parameters` 参数说明 + `# Returns` 返回值说明 + `# Example` 示例代码 + `# Errors` 错误情况
- 分批补齐策略：优先公开 API（pub fn/pub struct/pub trait）> 内部 API（pub(crate)）> 测试 API
- 移除 [lib.rs:406](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L406) 的 `#![cfg_attr(docsrs, warn(missing_docs))]`，改为全局 `#![warn(missing_docs)]`
- 约束：仅新增 `///` 注释，不修改 API 签名；`cargo doc --workspace --no-deps` 无 missing-docs 警告

**README 成熟度声明更新** — 内容结构：

- 移除 [README.md:46](file:///E:/vue/test/鲜视达/rust/sz-orm/README.md#L46) "当前处于原型阶段，尚无生产案例、无第三方审计、无社区采用"
- 补充 sz-pay 生产案例：7 个包（sz-orm-core/sqlx/config/auth/macros/queue/scheduler）、297 处引用、5139 测试零回归、crates.io 拉取 2.3.0
- 更新项目状态为"早期生产可用（内部项目）"
- 声明与评估报告 §5.1 一致（[assessment/2026-08-08-v3.3.0-depth-evaluation.md:238](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/assessment/2026-08-08-v3.3.0-depth-evaluation.md#L238)）

**async trait 风格统一评估文档** — `docs/async_trait_style_evaluation.md`：

- 性能基准对比：`#[async_trait]` 宏展开开销 vs 手动解糖开销（criterion 基准证据）
- 迁移影响分析：涉及 trait 列表（Connection trait + 其他手动解糖 trait）+ 调用方列表 + Breaking Change 评估
- 学习成本评估：手动解糖的学习成本 vs `#[async_trait]` 的学习成本
- 推荐方案：基于评估输出推荐（可能是"保持手动解糖 + 文档说明原因"，因 Connection trait 手动解糖有 HRTB 技术原因）
- 约束：评估文档附 file:line 证据；不强制立即迁移（评估为先）

**sz-orm-query-builder 选择指南** — `docs/query_builder_selection_guide.md`：

- 能力对比表：支持的查询类型（SELECT/INSERT/UPDATE/DELETE/JOIN/聚合）+ 方言支持 + 特性（类型安全/参数化/软删除）
- 适用场景：sz-orm-query-builder（独立 SQL 构造/动态查询）vs sz-orm-core::QueryBuilder（ORM 集成/编译期校验）
- 性能基准对比：两者 SQL 构造吞吐量基准（criterion 证据）
- 迁移建议：从 sz-orm-query-builder 迁移到 sz-orm-core::QueryBuilder 的步骤与注意事项
- 约束：指南文档附 file:line 证据

### 2.2.4 核心流程设计

**313 pub API 文档补齐流程**：

```plantuml
@startuml
!theme plain
title 313 pub API 文档补齐流程

start
:定位 313 个缺文档 pub API (cargo doc 警告);
:分批排序 (公开 API > 内部 API > 测试 API);
repeat
  :取一批 API (如 20 个);
  :为每个 API 补齐 /// 文档注释;
  :运行 cargo doc --workspace --no-deps;
  if (无 missing-docs 警告) then (是)
    :记录进度;
  else (否)
    :修复警告;
  endif
repeat while (还有未补齐 API?) is (是)
:移除 lib.rs:406 docs.rs cfg 跳过;
:改为 #![warn(missing_docs)];
:运行 cargo doc --workspace --no-deps 验证;
:运行 cargo test --workspace --doc doctest 验证;
stop

@enduml
```

### 2.2.5 Feature gate 配置

```toml
# packages/sz-orm-core/Cargo.toml [features] 新增（聚合 gate）
arch-improvement = []  # 聚合 real feature 占位 + async trait 评估 + query-builder 指南（用于门禁矩阵标识）
doc-completion = []    # 聚合 313 pub API 文档 + README 更新（用于门禁矩阵标识）
```

- `arch-improvement` 聚合 gate 用于门禁矩阵标识（不引入依赖，仅标识）
- `doc-completion` 聚合 gate 用于门禁矩阵标识（纯文档，不引入依赖）
- 313 pub API 文档补齐与 README 更新为纯文档操作，无需 feature gate 隔离（默认启用）
- async trait 评估与 query-builder 指南为纯文档输出，无需 feature gate 隔离
- sz-orm-es `real` feature 占位已在 §2.1.5 定义

### 2.2.6 与现有代码集成点

| 集成点 | 位置 | 集成方式 | 兼容性 |
|--------|------|---------|--------|
| sz-orm-core lib.rs docs.rs cfg | [lib.rs:403](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L403) | 移除 `#![cfg_attr(docsrs, warn(missing_docs))]`，改为 `#![warn(missing_docs)]` | API 签名不变，仅文档注释新增 |
| sz-orm-core 各模块 pub API | `packages/sz-orm-core/src/` 各 .rs 文件 | 为 313 个 pub API 补齐 `///` 文档注释 | API 签名不变 |
| README.md | [README.md:46](file:///E:/vue/test/鲜视达/rust/sz-orm/README.md#L46) | 更新成熟度声明 | 纯文档更新 |
| Connection trait | [pool.rs:42](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L42) | 评估文档输出（不修改 trait） | trait 不变 |
| sz-orm-query-builder | [query-builder/lib.rs:53](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-query-builder/src/lib.rs#L53) | 选择指南文档输出 | 代码不变 |

### 2.2.7 测试策略

| 测试类型 | 范围 | 工具 | 验收标准 |
|---------|------|------|---------|
| 文档构建 | 全 workspace | `cargo doc --workspace --no-deps` | 无 missing-docs 警告，docs.rs 文档完整 |
| doctest 验证 | 全 workspace | `cargo test --workspace --doc` | doctest 全部通过 |
| async trait 评估基准 | Connection trait | criterion | 宏 vs 手动解糖开销基准证据 |
| query-builder 基准 | 两个 QueryBuilder | criterion | SQL 构造吞吐量基准证据 |
| 文档与代码一致性 | 全 workspace | 代码审查 + CI 检查 | 文档与代码实际行为一致 |

---

## 2.3 性能优化落地（REQ-PF-001~007）

### 2.3.1 模块目标

评估并落地 5 项性能优化：query.rs SmallString/CompactString（SQL 构造减少堆分配）、dialect.rs enum dispatch（消除 vtable）、l2_cache.rs zero-copy 推广（减少序列化分配）、value.rs Box<str>（节省 capacity 字段）、result_map.rs 宏生成评估。全部通过 feature gate 隔离，不修改既有公开 API。启用后 SQL 构造吞吐量 ≥ 1.15x、方言分发开销 ≤ 0.7x、L2 缓存分配 ≤ 0.5x、Value 内存占用减少。

### 2.3.2 架构设计

```plantuml
@startuml
!theme plain
title 性能优化落地模块架构

package "sz-orm-core [perf-smallstring]" as PfSm {
  rectangle "query.rs SmallString 路径 <<new>>\nCompactString 替代 String (短字符串内联)" as SmPath
  rectangle "既有 String 路径 (不变)" as SmOld
}

package "sz-orm-core [perf-enum-dispatch]" as PfEd {
  rectangle "dialect.rs enum DialectKind <<new>>\nMySQL/PostgreSQL/SQLite/Oracle/MSSQL + match" as EdEnum
  rectangle "既有 Box<dyn Dialect> (不变)" as EdOld
}

package "sz-orm-core [perf-zero-copy-l2]" as PfZc {
  rectangle "l2_cache.rs zero-copy 路径 <<new>>\nBorrowedValue 推广到 L2 序列化" as ZcPath
  rectangle "既有序列化路径 (不变)" as ZcOld
}

package "sz-orm-core [perf-box-str]" as PfBs {
  rectangle "value.rs Box<str> 变体 <<new>>\nBox<str> 替代 String (节省 capacity)" as BsVariant
  rectangle "既有 String 变体 (不变)" as BsOld
}

package "result_map.rs 宏生成评估" as RmEval {
  rectangle "docs/result_map_macro_evaluation.md\n性能基准 (宏 vs 反射) + 迁移影响 + 类型安全收益" as RmDoc
}

package "benchmarks 完善" as Bench {
  rectangle "6 组对比基准\nzero-copy/simd/plan-cache/SmallString/enum-dispatch/Box<str>" as Bench6
}

database "DB (基准对比)" as Db

SmPath --> Db : SQL 构造吞吐量基准
EdEnum --> Db : 方言分发开销基准
ZcPath --> Db : L2 缓存分配基准
BsVariant --> Db : Value 内存占用基准
Bench6 --> Db : 6 组对比基准

@enduml
```

### 2.3.3 核心数据结构设计

**query.rs SmallString/CompactString 路径** — `perf-smallstring` feature：

- 引入 `compactstr::CompactString`（optional 依赖），短字符串（≤ 23 字节）内联存储避免堆分配
- `#[cfg(feature = "perf-smallstring")]` 条件编译，SQL 构造路径（`build_select` / `build_insert` / `build_update` / `build_delete`）使用 `CompactString` 替代 `String`
- 既有 `String` 路径不变（`#[cfg(not(feature = "perf-smallstring"))]`），QueryBuilder 公开 API 返回类型不变
- 约束：短字符串场景吞吐量 ≥ 1.15x（基准证据）；长字符串场景不退化；既有 QueryBuilder API 不变

**dialect.rs enum dispatch 路径** — `perf-enum-dispatch` feature：

- 新增 `enum DialectKind { MySQL, PostgreSQL, SQLite, Oracle, MSSQL }`，通过 match 分发替代 `Box<dyn Dialect>` vtable 查找
- `#[cfg(feature = "perf-enum-dispatch")]` 条件编译，方言分发路径使用 enum dispatch
- 既有 `Box<dyn Dialect>` 路径不变（`#[cfg(not(feature = "perf-enum-dispatch"))]`），Dialect trait 公开 API 不变
- 约束：方言分发开销 ≤ 0.7x（基准证据）；五方言行为一致（差分测试）；既有 Dialect trait API 不变

**l2_cache.rs zero-copy 推广** — `perf-zero-copy-l2` feature：

- 推广既有 `zero-copy` feature（[value_borrowed.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/value_borrowed.rs)）到 L2 缓存序列化/反序列化路径
- `#[cfg(feature = "perf-zero-copy-l2")]` 条件编译，L2 缓存使用 `BorrowedValue` + `ColumnarResultSet` 零拷贝序列化
- 既有序列化路径不变（`#[cfg(not(feature = "perf-zero-copy-l2"))]`），L2Cache 公开 API 不变
- 约束：L2 缓存序列化/反序列化分配 ≤ 0.5x（分配计数证据）；与既有 Redis 后端兼容（序列化格式不变）；既有 L2Cache API 不变

**value.rs Box<str> 路径** — `perf-box-str` feature：

- 新增 `Value::BoxedStr(Box<str>)` 变体（`#[cfg(feature = "perf-box-str")]`），替代 `Value::String(String)` 用于不需要修改字符串的场景
- `Box<str>` 比 `String` 少存储 capacity 字段（节省 8 字节/值）
- 既有 `Value::String(String)` 变体不变（`#[cfg(not(feature = "perf-box-str"))]`），Value 枚举公开 API 不变
- 约束：Value 枚举内存占用减少（`size_of::<Value>()` 证据）；既有 Value API 不变

**result_map.rs 宏生成评估文档** — `docs/result_map_macro_evaluation.md`：

- 性能基准对比：宏生成（编译期）vs 反射式取值（运行时）开销基准
- 迁移影响分析：从反射式取值迁移到宏生成的步骤与影响
- 类型安全收益：宏生成在编译期保证类型安全，反射式取值运行时类型不安全
- 推荐方案：基于评估输出推荐（宏生成作为后续版本落地）
- 约束：评估文档附 file:line 证据；不强制立即迁移（评估为先）

### 2.3.4 核心流程设计

**性能优化差分测试流程**（保证优化不破坏正确性）：

```plantuml
@startuml
!theme plain
title 性能优化差分测试流程

start
:启用 perf-smallstring feature;
:运行既有 6,327+ 测试;
if (全部通过) then (是)
  :差分测试 (优化前 vs 优化后 SQL 一致);
  if (结果完全一致) then (是)
    :基准对比 (SmallString vs String);
    if (吞吐量 >= 1.15x) then (是)
      :优化验证通过;
    else (否)
      :分析基准未达标原因;
    endif
  else (否)
    :修复行为差异;
  endif
else (否)
  :修复测试失败;
endif
stop

@enduml
```

### 2.3.5 Feature gate 配置

```toml
# packages/sz-orm-core/Cargo.toml [features] 新增
perf-smallstring = ["dep:compactstr"]
perf-enum-dispatch = []
perf-zero-copy-l2 = ["zero-copy"]
perf-box-str = []
# [dependencies] 新增
compactstr = { version = "0.8", optional = true }
```

- 4 个性能优化 feature gate 默认关闭，启用后对应优化路径生效
- `perf-smallstring` 引入 compactstr optional 依赖
- `perf-enum-dispatch` / `perf-box-str` 无新增依赖（自研/std）
- `perf-zero-copy-l2` 复用既有 `zero-copy` feature
- 与既有 `redis` / `circuit-breaker` / `rate-limit` / `auto-prewarm` / `plan-cache` / `zero-copy` / `simd` / `multi-tenant-enhanced` / `dist-cache` feature 正交

### 2.3.6 与现有代码集成点

| 集成点 | 位置 | 集成方式 | 兼容性 |
|--------|------|---------|--------|
| query.rs SQL 构造路径 | [query.rs:36](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L36) | `#[cfg(feature = "perf-smallstring")]` 条件编译 CompactString 路径 | 既有 String 路径不变 |
| dialect.rs 方言分发 | [lib.rs:130](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L130) | `#[cfg(feature = "perf-enum-dispatch")]` 条件编译 enum dispatch 路径 | 既有 Box<dyn Dialect> 不变 |
| l2_cache.rs 序列化 | [l2_cache.rs:517](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/l2_cache.rs#L517) | `#[cfg(feature = "perf-zero-copy-l2")]` 条件编译 zero-copy 路径 | 既有序列化路径不变 |
| value.rs Value 枚举 | [lib.rs:401](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L401) | `#[cfg(feature = "perf-box-str")]` 条件编译 Box<str> 变体 | 既有 String 变体不变 |
| result_map.rs 反射式取值 | [result_map.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/result_map.rs) | 评估文档输出（不修改代码） | 代码不变 |
| benchmarks | `packages/sz-orm-core/benches/` | 新增 6 组对比基准 | 既有 benchmarks 不变 |

### 2.3.7 测试策略

| 测试类型 | 范围 | 工具 | 验收标准 |
|---------|------|------|---------|
| 既有测试回归 | 全 workspace | `cargo test --workspace` | 6,327+ 测试全部通过（各 feature 组合） |
| 差分测试 | 优化前 vs 优化后 | 自研差分测试 | 查询结果完全一致（SQL + 结果集） |
| SmallString 基准 | query.rs | criterion | 短字符串场景吞吐量 ≥ 1.15x，附中位数 + 置信区间 |
| enum dispatch 基准 | dialect.rs | criterion | 方言分发开销 ≤ 0.7x，附中位数 + 置信区间 |
| zero-copy L2 基准 | l2_cache.rs | criterion + 分配计数 | 序列化/反序列化分配 ≤ 0.5x |
| Box<str> 基准 | value.rs | `size_of::<Value>()` | Value 枚举内存占用减少 |
| 五方言行为一致 | MySQL/PG/SQLite/Oracle/MSSQL | 集成测试 | enum dispatch 五方言行为与 Box<dyn> 一致 |
| 6 组对比基准 | 全性能优化 | criterion | 每组附加速比 + 中位数 + 置信区间，CI 定期运行 |

---

## 2.4 编译期类型安全增强（REQ-TS-001~005）

### 2.4.1 模块目标

扩展既有 `#[derive(Schema)]` 宏（[macros/lib.rs:71](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/src/lib.rs#L71)）生成编译期列名常量，引入 `Column<T>` 类型安全列引用，完善既有 typed_ast.rs 模块（[typed_ast.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs)）提供 Diesel 风格表达式 DSL。全部通过 feature gate "type-safe-columns" 隔离，不修改既有公开 API。编译期完成类型检查，运行时零额外开销。

### 2.4.2 架构设计

```plantuml
@startuml
!theme plain
title 编译期类型安全增强模块架构

package "sz-orm-macros [type-safe-columns]" as TsMacros {
  rectangle "derive(Schema) 扩展 <<new>>\n生成列名常量 StructName::FIELD_NAME" as TsSchema
  rectangle "既有 derive(Schema) (不变)" as TsSchemaOld
}

package "sz-orm-core [type-safe-columns]" as TsCore {
  rectangle "Column<T> 类型安全列引用 <<new>>\nColumn<User> 保证属于 User 表" as TsCol
  rectangle "typed_ast.rs DSL 完善 <<new>>\nusers::id.eq(1) / users::age.gt(18)" as TsDsl
  rectangle "既有 QueryBuilder &str API (不变)" as TsQbOld
  rectangle "既有 typed_ast.rs 基础 (不变)" as TsTaOld
}

rectangle "编译期类型检查" as CompileCheck
rectangle "运行时零开销" as ZeroOverhead

TsSchema --> CompileCheck : 列名拼写错误编译失败
TsCol --> CompileCheck : 跨表列引用编译失败
TsDsl --> CompileCheck : 类型不匹配编译失败
TsSchema --> ZeroOverhead : 编译期常量
TsCol --> ZeroOverhead : 编译期类型
TsDsl --> ZeroOverhead : 编译期类型

@enduml
```

### 2.4.3 核心数据结构设计

**`#[derive(Schema)]` 列名常量生成** — 扩展 sz-orm-macros Schema derive：

- 为每个结构体字段生成 `pub const FIELD_NAME: &'static str = "field_name"` 常量
- 示例：`#[derive(Schema)] struct User { id: i64, name: String }` 生成 `impl User { pub const ID: &'static str = "id"; pub const NAME: &'static str = "name"; }`
- `#[cfg(feature = "type-safe-columns")]` 条件编译，启用时生成列名常量，未启用时既有 derive 输出不变
- 约束：列名常量为 `&'static str`，编译期确定；引用不存在的列编译失败（字段名拼写错误）；既有 Schema derive 行为不变

**`Column<T>` 类型安全列引用** — `packages/sz-orm-core/src/column.rs`（新增模块）：

- 泛型结构：`Column<T: Schema> { name: &'static str, _marker: PhantomData<T> }`
- 关联表类型 `T` 保证列引用属于指定表（`Column<User>` 不可用于 `Order` 查询）
- 方法：`Column::<User>::new("id") -> Column<User>`（构造）、`Column<User>::name() -> &'static str`（获取列名）、`impl<T> Deref for Column<T>`（可解引用为 `&str` 支持混用）
- QueryBuilder 新增 `Column<T>` 重载：`where_eq<T>(col: Column<T>, value: Value) -> Self`（`#[cfg(feature = "type-safe-columns")]`）
- 约束：编译期防止跨表列引用；既有 `&str` API 不变（新增 `Column<T>` 重载）；`Column<T>` 可解引用为 `&str` 支持混用

**typed_ast.rs Diesel 风格 DSL** — 扩展 typed_ast.rs 模块：

- 表引用：`users::table` -> `TableRef<User>`，`users::id` -> `Column<User>`，`users::age` -> `Column<User>`
- 表达式方法：`col.eq(value) -> Expr`、`col.gt(value) -> Expr`、`col.lt(value) -> Expr`、`col.like(pattern) -> Expr`、`col.in_(values) -> Expr`
- 表达式组合：`expr.and(expr) -> Expr`、`expr.or(expr) -> Expr`、`expr.not() -> Expr`
- DSL 到 SQL：`Expr::to_sql() -> (String, Vec<Value>)`（生成 SQL 片段 + 参数，参数化绑定）
- 与 QueryBuilder 集成：`QueryBuilder::where_expr(expr) -> Self`（`#[cfg(feature = "type-safe-columns")]`）
- 约束：编译期类型安全（类型不匹配编译失败）；生成 SQL 与 QueryBuilder 行为一致（差分测试）；既有 QueryBuilder API 不变；列名/表名参数化绑定或编译期常量内联，值参数化绑定

### 2.4.4 核心流程设计

**编译期类型安全验证流程**：

```plantuml
@startuml
!theme plain
title 编译期类型安全验证流程

start
:启用 type-safe-columns feature;
:定义 #[derive(Schema)] struct User { id: i64, name: String };
:编译期生成 User::ID = "id", User::NAME = "name";
:引用 User::ID (正确);
if (编译通过) then (是)
  :引用 User::NON_EXISTENT (错误);
  if (编译失败) then (是)
    :列名拼写错误编译期暴露;
  else (否)
    :类型安全失效;
  endif
else (否)
  :derive 宏错误;
endif
:构造 Column<User>::new("id");
:用于 Order 查询 (跨表);
if (编译失败) then (是)
  :跨表列引用编译期暴露;
else (否)
  :类型安全失效;
endif
:使用 users::id.eq(1) DSL;
:生成 SQL 与 QueryBuilder 差分测试;
if (SQL 一致) then (是)
  :DSL 验证通过;
else (否)
  :修复 DSL 行为;
endif
stop

@enduml
```

### 2.4.5 Feature gate 配置

```toml
# packages/sz-orm-macros/Cargo.toml [features] 新增
type-safe-columns = []  # 启用 Schema derive 列名常量生成

# packages/sz-orm-core/Cargo.toml [features] 新增
type-safe-columns = ["sz-orm-macros/type-safe-columns"]  # 启用 Column<T> + typed DSL
```

- `type-safe-columns` 聚合 gate：sz-orm-macros 启用 Schema derive 列名常量，sz-orm-core 启用 Column<T> + typed DSL
- 默认关闭，启用后编译期类型安全增强生效
- 无新增外部依赖（复用既有 proc-macro + typed_ast.rs）
- 与既有 feature 正交

### 2.4.6 与现有代码集成点

| 集成点 | 位置 | 集成方式 | 兼容性 |
|--------|------|---------|--------|
| sz-orm-macros Schema derive | [macros/lib.rs:71](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/src/lib.rs#L71) | `#[cfg(feature = "type-safe-columns")]` 扩展 derive 生成列名常量 | 既有 derive 输出不变 |
| sz-orm-core QueryBuilder | [query.rs:36](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L36) | 新增 `Column<T>` 重载 + `where_expr` 方法 | 既有 `&str` API 不变 |
| sz-orm-core typed_ast.rs | [typed_ast.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs) | 完善 Diesel 风格 DSL | 既有 typed_ast 基础不变 |
| sz-orm-core lib.rs 导出 | [lib.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs) | `#[cfg(feature = "type-safe-columns")] pub mod column;` | 条件导出 |

### 2.4.7 测试策略

| 测试类型 | 范围 | 工具 | 验收标准 |
|---------|------|------|---------|
| 列名常量生成 | derive(Schema) | `trybuild` 编译测试 | 生成 `StructName::FIELD_NAME` 常量，引用不存在列编译失败 |
| Column<T> 跨表引用 | Column<User> 用于 Order | `trybuild` 编译测试 | 跨表列引用编译失败 |
| DSL 类型安全 | users::id.eq(1) | `trybuild` 编译测试 | 类型不匹配编译失败 |
| DSL 与 QueryBuilder 差分 | DSL vs QueryBuilder | 差分测试 | 生成 SQL 完全一致 |
| 零运行时开销 | typed vs &str | criterion 基准 | 运行时开销零差异 |
| 参数化绑定 | Column<T> / DSL 生成 SQL | SQL 注入扫描 | 列名/表名参数化或编译期常量，值参数化，无字符串拼接 |
| 既有 API 兼容 | &str API | `cargo test --workspace` | 既有 &str API 行为不变 |

---

## 2.5 文档与生态建设（REQ-DOC-001~004）

### 2.5.1 模块目标

补齐 313 个 pub API 文档（与 §2.2 架构改进统一交付），更新 README 成熟度声明（与 §2.2 统一交付），编写 Diesel/SeaORM/SQLx 三份迁移指南（含概念映射表 + API 对照表 + 示例代码 + 常见陷阱），降低外部用户迁移成本。不修改既有公开 API（除文档注释）。

### 2.5.2 架构设计

```plantuml
@startuml
!theme plain
title 文档与生态建设模块架构

package "313 pub API 文档补齐 (与 §2.2 统一)" as ApiDoc {
  rectangle "sz-orm-core/src/ 各模块 /// 文档" as DocComment
  rectangle "移除 lib.rs:406 docs.rs cfg 跳过" as DocCfg
}

package "README 成熟度更新 (与 §2.2 统一)" as ReadmeUpdate {
  rectangle "README.md:46 移除过时声明" as ReadmeFix
  rectangle "补充 sz-pay 案例 + crates.io" as ReadmeAdd
}

package "迁移指南 [migration-guide]" as MigGuide {
  rectangle "docs/migration/diesel_to_sz_orm.md\n概念映射 + API 对照 + 示例 + 陷阱" as DieselGuide
  rectangle "docs/migration/seaorm_to_sz_orm.md\n概念映射 + API 对照 + 示例 + 陷阱" as SeaOrmGuide
  rectangle "docs/migration/sqlx_to_sz_orm.md\n概念映射 + API 对照 + 示例 + 陷阱" as SqlxGuide
}

cloud "docs.rs" as DocsRs
cloud "crates.io" as CratesIo

DocComment --> DocsRs : 文档完整
ReadmeAdd --> CratesIo : crates.io 发布信息

@enduml
```

### 2.5.3 核心数据结构设计

**Diesel 迁移指南** — `docs/migration/diesel_to_sz_orm.md`：

- 概念映射表：Diesel `schema.rs` → sz-orm `#[derive(Schema)]`、Diesel `QueryDsl` → sz-orm `QueryBuilder`、Diesel `BelongsTo` → sz-orm `Relation`
- API 对照表：Diesel `users::id.eq(1)` → sz-orm typed DSL（`type-safe-columns` feature）、Diesel `users.filter(...)` → sz-orm `QueryBuilder::where_eq`、Diesel `users.load::<User>()` → sz-orm `QueryBuilder::get`
- 示例代码：CRUD + 关联查询 + 事务 + 迁移（doctest 验证可编译）
- 常见陷阱：异步/同步差异（Diesel 同步为主，sz-orm 原生异步）、类型映射差异（Diesel `Nullable<T>` → sz-orm `Option<T>`）、方言差异

**SeaORM 迁移指南** — `docs/migration/seaorm_to_sz_orm.md`：

- 概念映射表：SeaORM `Entity` → sz-orm `Model`、SeaORM `ActiveModel` → sz-orm `Model + fill`、SeaORM `QueryFilter` → sz-orm `QueryBuilder::where_eq`
- API 对照表：SeaORM `Entity::find().filter(Column::Id.eq(1))` → sz-orm `QueryBuilder::new::<User>().where_eq("id", 1)`
- 示例代码：CRUD + 关联查询 + 事务 + 迁移（doctest 验证可编译）
- 常见陷阱：异步运行时差异（均异步，但 API 风格不同）、Model 定义差异、事务 API 差异

**SQLx 迁移指南** — `docs/migration/sqlx_to_sz_orm.md`：

- 概念映射表：SQLx `query!` 宏 → sz-orm `sql_string!` 宏 + `QueryBuilder`、SQLx `FromRow` → sz-orm `#[derive(FromQueryResult)]`、SQLx `Pool` → sz-orm `Pool`
- API 对照表：SQLx `query_as::<_, User>("SELECT * FROM users WHERE id = ?").bind(1)` → sz-orm `QueryBuilder::new::<User>().where_eq("id", 1).get()`
- 示例代码：CRUD + 关联查询 + 事务 + 迁移（doctest 验证可编译）
- 常见陷阱：编译时 SQL 校验差异（SQLx `query!` 连真 DB vs sz-orm `sql_string!` 语法校验 + db-verify feature）、类型映射差异、连接池差异

### 2.5.4 核心流程设计

**迁移指南编写流程**：

```plantuml
@startuml
!theme plain
title 迁移指南编写流程

start
:分析竞品 (Diesel/SeaORM/SQLx) 核心概念;
:建立概念映射表 (竞品 -> sz-orm);
:建立 API 对照表 (竞品 API -> sz-orm API);
:编写示例代码 (CRUD + 关联 + 事务 + 迁移);
:编写常见陷阱 (异步/类型/方言差异);
:将示例代码纳入 doctest;
:运行 cargo test --workspace --doc;
if (doctest 通过) then (是)
  :指南验证通过;
else (否)
  :修复示例代码;
endif
:标注指南基于的竞品版本;
stop

@enduml
```

### 2.5.5 Feature gate 配置

```toml
# packages/sz-orm-core/Cargo.toml [features] 新增（聚合 gate）
migration-guide = []  # 聚合 Diesel/SeaORM/SQLx 迁移指南（用于门禁矩阵标识）
```

- `migration-guide` 聚合 gate 用于门禁矩阵标识（纯文档，不引入依赖）
- 313 pub API 文档补齐与 README 更新复用 §2.2 的 `doc-completion` 聚合 gate
- 迁移指南为纯文档，无需 feature gate 隔离（默认启用）
- 示例代码纳入 doctest 验证（`cargo test --workspace --doc`）

### 2.5.6 与现有代码集成点

| 集成点 | 位置 | 集成方式 | 兼容性 |
|--------|------|---------|--------|
| sz-orm-core pub API 文档 | `packages/sz-orm-core/src/` 各 .rs | 补齐 `///` 文档注释（与 §2.2 统一） | API 签名不变 |
| README.md | [README.md:46](file:///E:/vue/test/鲜视达/rust/sz-orm/README.md#L46) | 更新成熟度声明（与 §2.2 统一） | 纯文档更新 |
| docs/migration/ | `docs/migration/`（新增目录） | 新增三份迁移指南 | 纯文档新增 |
| doctest | 各迁移指南示例代码 | 纳文档新增 | doctest 验证可编译 |

### 2.5.7 测试策略

| 测试类型 | 范围 | 工具 | 验收标准 |
|---------|------|------|---------|
| 文档构建 | 全 workspace | `cargo doc --workspace --no-deps` | 无 missing-docs 警告 |
| doctest 验证 | 全 workspace + 迁移指南 | `cargo test --workspace --doc` | doctest 全部通过 |
| 示例代码可编译 | 迁移指南示例 | doctest | 示例代码可编译 |
| CI 定期检查 | 文档与代码一致性 | CI 脚本 | 文档与代码实际行为一致 |

---

## 2.6 sz-pay 生产案例深化（REQ-PC-001~004）

### 2.6.1 模块目标

将 sz-pay 使用 sz-orm 的模式（连接池配置、SQL 执行、错误映射、消息队列、定时调度）抽取为 `examples/src/bin/sz_pay_pattern.rs`（脱敏版，无真实密钥/连接串/业务数据），更新 README 成熟度声明（与 §2.2 统一交付），可选收集 sz-pay 生产运行数据。不修改 sz-pay / sz-rust 下游代码（ADR-0001 严禁修改下游/上游仓库）。

### 2.6.2 架构设计

```plantuml
@startuml
!theme plain
title sz-pay 生产案例深化模块架构

package "examples [sz-pay-example]" as ExSzPay {
  rectangle "examples/src/bin/sz_pay_pattern.rs <<new>>\n(脱敏版 sz-pay 使用模式)" as SzPayPattern
}

rectangle "sz-pay 生产项目\n(E:\vue\test\sz-pay\server\sz-rust)" as SzPay
rectangle "sz-orm-core/sqlx/config/auth/queue/scheduler\n(7 个包)" as SzOrmPkgs
rectangle "README.md (更新)" as Readme

SzPayPattern ..> SzPay : 抽取使用模式 (脱敏, ADR-0001 不改下游)
SzPayPattern --> SzOrmPkgs : 展示 7 个包典型用法
SzPayPattern --> Readme : README 补充案例引用

note right of SzPayPattern
  **脱敏要求**：
  1. 无真实密钥 (password/secret/key)
  2. 无真实连接串
  3. 无真实业务数据
  4. 可独立编译运行
  5. 展示典型用法：
     - 连接池配置
     - SQL 执行
     - 错误映射
     - 消息队列
     - 定时调度
end note

@enduml
```

### 2.6.3 核心数据结构设计

**examples/sz_pay_pattern.rs** — sz-pay 使用模式脱敏案例：

- 连接池配置：`PoolConfigBuilder` 配置 MySQL 连接池（脱敏连接串 `mysql://user:pass@127.0.0.1:3306/db`）
- SQL 执行：`QueryBuilder::new::<User>().where_eq("id", 1).get()` 查询用户（脱敏 User 模型）
- 错误映射：`DbError` 错误码处理（DB001-DB020 错误码映射到业务错误）
- 消息队列：`sz-orm-queue` 消息发送/接收（脱敏队列名）
- 定时调度：`sz-orm-scheduler` 定时任务配置（脱敏任务名）
- 约束：脱敏（无真实密钥/连接串/业务数据）；可独立编译运行（`cargo run --bin sz_pay_pattern`）；不修改 sz-pay/sz-rust 下游代码（ADR-0001）

**sz-pay 生产运行数据收集（可选）** — REQ-PC-003：

- 收集内容：连接池命中率、查询延迟（P50/P95/P99）、错误率
- 脱敏要求：无业务敏感信息（仅性能指标）
- 约束：sz-pay 生产环境可访问时执行，不可访问时跳过（标注"未收集"）

### 2.6.4 核心流程设计

**sz-pay 案例抽取流程**：

```plantuml
@startuml
!theme plain
title sz-pay 案例抽取流程

start
:分析 sz-pay 使用 sz-orm 的模式;
:识别典型用法 (连接池/SQL/错误/队列/调度);
:抽取为 examples/src/bin/sz_pay_pattern.rs;
:脱敏处理 (移除真实密钥/连接串/业务数据);
:密钥扫描 (grep password/secret/key);
if (无真实密钥) then (是)
  :验证可独立编译运行;
  if (编译运行通过) then (是)
    :更新 README 引用案例;
    :案例验证通过;
  else (否)
    :修复编译运行错误;
  endif
else (否)
  :修复脱敏遗漏;
endif
stop

@enduml
```

### 2.6.5 Feature gate 配置

```toml
# examples/Cargo.toml [features] 新增
sz-pay-example = []  # 标识 sz-pay 案例示例（用于门禁矩阵标识）

# examples/Cargo.toml [[bin]] 新增
[[bin]]
name = "sz_pay_pattern"
path = "src/bin/sz_pay_pattern.rs"
```

- `sz-pay-example` 聚合 gate 用于门禁矩阵标识（不引入依赖，仅标识）
- examples/Cargo.toml 新增 `sz_pay_pattern` bin（复用既有依赖配置 [examples/Cargo.toml:10](file:///E:/vue/test/鲜视达/rust/sz-orm/examples/Cargo.toml#L10)）
- sz-pay 案例 7 个包依赖已在 examples/Cargo.toml 配置（sz-orm-core/sqlx/auth/audit 等）

### 2.6.6 与现有代码集成点

| 集成点 | 位置 | 集成方式 | 兼容性 |
|--------|------|---------|--------|
| examples Cargo.toml | [examples/Cargo.toml:24](file:///E:/vue/test/鲜视达/rust/sz-orm/examples/Cargo.toml#L24) | 新增 `sz_pay_pattern` bin | 既有 bin 不变 |
| examples/src/bin/ | `examples/src/bin/`（既有目录） | 新增 `sz_pay_pattern.rs` | 既有 bin 不变 |
| README.md | [README.md:46](file:///E:/vue/test/鲜视达/rust/sz-orm/README.md#L46) | 更新成熟度声明（与 §2.2 统一） | 纯文档更新 |
| sz-pay 项目 | `E:\vue\test\sz-pay\server\sz-rust` | 仅读取分析，不修改（ADR-0001） | 下游零修改 |

### 2.6.7 测试策略

| 测试类型 | 范围 | 工具 | 验收标准 |
|---------|------|------|---------|
| 案例可编译 | examples/sz_pay_pattern.rs | `cargo build --bin sz_pay_pattern` | 编译通过 |
| 案例可运行 | examples/sz_pay_pattern.rs | `cargo run --bin sz_pay_pattern` | 运行通过（需本地 DB） |
| 脱敏审查 | examples/sz_pay_pattern.rs | 代码审查 | 无真实密钥/连接串/业务数据 |
| 密钥扫描 | examples/ | `grep -r "password\|secret\|key" examples/` | 无真实密钥 |
| 下游零回归 | sz-pay 5139 测试 | `cargo test` in sz-pay | 5139 测试零回归（ADR-0001 不改下游） |

---

## 2.7 Feature gate 矩阵

### 2.7.1 10 个新聚合 Feature gate

| 聚合 Feature | 所属包 | 用途 | 默认 | 依赖 | 聚合的细粒度 feature |
|---------|-------|------|------|------|---------------------|
| `test-coverage` | sz-orm-core | 标识 18 扩展包测试 + real-es/real-config（门禁矩阵标识） | 关闭 | 无（仅标识） | real-es（sz-orm-es）+ real-consul/real-nacos（sz-orm-config） |
| `arch-improvement` | sz-orm-core | 标识 real feature 占位 + async trait 评估 + query-builder 指南 | 关闭 | 无（仅标识） | real（sz-orm-es） |
| `perf-smallstring` | sz-orm-core | SmallString/CompactString 优化（query.rs SQL 构造） | 关闭 | compactstr（optional） | — |
| `perf-enum-dispatch` | sz-orm-core | enum dispatch 优化（dialect.rs 方言分发） | 关闭 | 无 | — |
| `perf-zero-copy-l2` | sz-orm-core | L2 缓存 zero-copy 推广 | 关闭 | 复用既有 zero-copy | zero-copy（既有） |
| `perf-box-str` | sz-orm-core | Value 枚举 Box<str> 优化 | 关闭 | 无 | — |
| `type-safe-columns` | sz-orm-macros + sz-orm-core | 编译期列名常量 + Column<T> + typed DSL | 关闭 | 无（复用既有 proc-macro） | typed-schema + typed-column + typed-dsl（细粒度） |
| `doc-completion` | sz-orm-core | 标识 313 pub API 文档 + README 更新 | 关闭 | 无（纯文档） | — |
| `migration-guide` | sz-orm-core | 标识 Diesel/SeaORM/SQLx 迁移指南 | 关闭 | 无（纯文档） | — |
| `sz-pay-example` | examples | 标识 sz-pay 案例示例 | 关闭 | 无（复用既有 examples 依赖） | — |

### 2.7.2 细粒度 Feature gate（包内隔离）

| 细粒度 Feature | 所属包 | 用途 | 默认 | 依赖 | 聚合到 |
|---------|-------|------|------|------|--------|
| `real` | sz-orm-es | 真实 ES 后端占位 | 关闭 | elasticsearch（optional，占位不引入） | test-coverage + arch-improvement |
| `real-consul` | sz-orm-config | 真实 Consul 客户端 | 关闭 | reqwest（optional） | test-coverage |
| `real-nacos` | sz-orm-config | 真实 Nacos 客户端 | 关闭 | reqwest（optional） | test-coverage |
| `typed-schema` | sz-orm-macros | Schema derive 列名常量生成 | 关闭 | 无 | type-safe-columns |
| `typed-column` | sz-orm-core | Column<T> 类型安全列引用 | 关闭 | 无 | type-safe-columns |
| `typed-dsl` | sz-orm-core | typed_ast.rs Diesel 风格 DSL | 关闭 | 无 | type-safe-columns |

### 2.7.3 Feature 组合矩阵

v3.4.0 新增 10 个聚合 feature + 6 个细粒度 feature，纳入既有门禁 10 Feature 全组合编译（`cargo check --workspace --all-targets --all-features`）。

| 组合维度 | 组合数 | 验证方式 |
|---------|--------|---------|
| 既有 11 feature 全组合 | 2^11 = 2048 | `cargo check --all-features`（门禁 10） |
| 新增 10 聚合 feature 全组合 | 2^10 = 1024 | 纳入门禁 10 Feature 全组合编译 |
| 新增 6 细粒度 feature 全组合 | 2^6 = 64 | 纳入门禁 10 Feature 全组合编译 |
| 总组合数 | ~3,136 | CI 矩阵覆盖（正交性设计，可独立启用） |

**Feature 正交性设计**：
- 10 个聚合 feature 相互正交（可独立启用，无依赖关系）
- 6 个细粒度 feature 相互正交（typed-schema/typed-column/typed-dsl 可独立启用）
- 聚合 feature 与细粒度 feature 的依赖关系：聚合 gate 聚合细粒度 gate（如 type-safe-columns 聚合 typed-schema + typed-column + typed-dsl）
- 与既有 11 feature 正交（perf-smallstring 与 zero-copy 独立，perf-zero-copy-l2 依赖 zero-copy）

---

## 2.8 里程碑规划

按 spec §0 优先级声明"测试覆盖补齐(1,最高) → 架构改进(2) → 性能优化(3) → 编译期类型安全(4) → 文档与生态(5) → sz-pay 生产案例(6)"的收益/风险序推进，划分 6 个里程碑。每个里程碑含明确交付物、验收标准、依赖关系。

### 2.8.1 里程碑总览

```plantuml
@startuml
!theme plain
title sz-orm v3.4.0 里程碑规划（M1~M6）

rectangle "M1: 测试覆盖补齐\n(REQ-TC-001~005)" as M1 {
  rectangle "18 扩展包测试 + MySQL INSERT IGNORE 修复\n+ sz-orm-es real + sz-orm-config real\nfeature: test-coverage" as M1Deliver
}
rectangle "M2: 架构改进\n(REQ-AR-001~006)" as M2 {
  rectangle "real feature 占位 + 313 pub API 文档\n+ README 更新 + async trait 评估 + query-builder 指南\nfeature: arch-improvement + doc-completion" as M2Deliver
}
rectangle "M3: 性能优化落地\n(REQ-PF-001~007)" as M3 {
  rectangle "SmallString + enum dispatch + zero-copy-l2 + Box<str>\n+ result_map 评估 + 6 组基准\nfeature: perf-smallstring/enum-dispatch/zero-copy-l2/box-str" as M3Deliver
}
rectangle "M4: 编译期类型安全增强\n(REQ-TS-001~005)" as M4 {
  rectangle "Schema 列名常量 + Column<T> + typed DSL\nfeature: type-safe-columns" as M4Deliver
}
rectangle "M5: 文档与生态建设\n(REQ-DOC-001~004)" as M5 {
  rectangle "313 pub API 文档 (与 M2 统一) + README (与 M2 统一)\n+ Diesel/SeaORM/SQLx 迁移指南\nfeature: doc-completion + migration-guide" as M5Deliver
}
rectangle "M6: sz-pay 生产案例 + 集成验证与发布\n(REQ-PC-001~004 + 全方向)" as M6 {
  rectangle "examples/sz_pay_pattern.rs + README 更新\n+ 10 feature 全组合编译 + 五方言集成测试\n+ sz-pay/sz-rust 零回归 + 性能基准不回退\nfeature: sz-pay-example" as M6Deliver
}

M1 --> M2 : 测试覆盖就绪 (架构改进依赖测试验证)
M2 --> M3 : 文档补齐就绪 (性能优化需文档记录)
M3 --> M4 : 性能基准就绪 (类型安全需基准对比)
M4 --> M5 : 类型安全就绪 (迁移指南需类型安全 API)
M5 --> M6 : 文档生态就绪 (案例需文档支撑)
M1 --> M6
M2 --> M6
M3 --> M6
M4 --> M6
M5 --> M6

note bottom of M6
  **M6 关键验证**：
  1. 10 聚合 + 6 细粒度 feature 全组合编译 (门禁 10)
  2. 五方言集成测试 (MySQL/PG/SQLite/Oracle/MSSQL)
  3. sz-pay 5139 测试 + sz-rust 零回归
  4. v3.3.0 性能基准不回退
  5. 31 条 REQ 全部满足
end note

@enduml
```

### 2.8.2 里程碑详细规划

| 里程碑 | 周期 | 交付物 | 验收标准（对应 spec §9） | 依赖 | 风险 |
|--------|------|--------|-------------------------|------|------|
| **M1 测试覆盖补齐** | 3 周 | 18 扩展包 tests/ + MySQL INSERT IGNORE 修复 + sz-orm-es real feature + sz-orm-config real-consul/real-nacos + 真实 ES/Consul/Nacos 集成测试 | AC-TC-1~6（spec §9.1）：18 包测试通过 + 覆盖率 ≥ 60% + INSERT IGNORE 修复 + 真实 ES/Consul/Nacos 集成测试 + 全 workspace 测试数 ≥ 6,327 + 新增数 | 各包既有 src/ + 真实 ES/Consul/Nacos 环境（可选） | R-01 工作量巨大（高）、R-02 真实环境不可用（中）、R-03 INSERT IGNORE 回归（中） |
| **M2 架构改进** | 2 周 | sz-orm-es real feature 占位 + 313 pub API 文档补齐 + README 更新 + async trait 评估文档 + query-builder 选择指南 | AC-AR-1~6（spec §9.2）：real feature 占位 + cargo doc 无警告 + README 更新 + async trait 评估 + query-builder 指南 + doctest 通过 | M1 测试覆盖就绪 | R-04 文档工作量巨大（高）、R-05 async trait 迁移影响（中）、R-20 文档与代码不符（中） |
| **M3 性能优化落地** | 2 周 | query.rs SmallString + dialect.rs enum dispatch + l2_cache.rs zero-copy + value.rs Box<str> + result_map 评估文档 + 6 组对比基准 | AC-PF-1~7（spec §9.3）：SmallString ≥ 1.15x + enum dispatch ≤ 0.7x + zero-copy-l2 ≤ 0.5x + Box<str> 内存减少 + 6 组基准 + 差分测试一致 | M2 文档补齐就绪 | R-06 长字符串无收益（低）、R-07 五方言差异（中）、R-08 zero-copy 兼容（中）、R-09 破坏正确性（高）、R-10 基准波动（中） |
| **M4 编译期类型安全增强** | 2 周 | Schema derive 列名常量扩展 + Column<T> + typed_ast.rs DSL 完善 + 差分测试 + 零开销基准 | AC-TS-1~5（spec §9.4）：列名常量生成 + 跨表引用编译失败 + DSL 类型安全 + 零运行时开销 + 参数化绑定 | M3 性能基准就绪 | R-11 复杂类型支持（中）、R-12 混用困惑（低）、R-13 DSL 与 QueryBuilder 不一致（中） |
| **M5 文档与生态建设** | 2 周 | 313 pub API 文档（与 M2 统一）+ README（与 M2 统一）+ Diesel/SeaORM/SQLx 迁移指南 | AC-DOC-1~4（spec §9.5）：cargo doc 无警告 + README 更新 + 三份迁移指南 + doctest + CI 检查 | M4 类型安全就绪 | R-14 迁移指南版本不匹配（低）、R-20 文档过时（中） |
| **M6 sz-pay 案例 + 集成验证与发布** | 1 周 | examples/sz_pay_pattern.rs + README 更新 + 10 feature 全组合编译 + 五方言集成测试 + sz-pay/sz-rust 零回归 + 性能基准不回退 + 31 REQ 验收 | AC-PC-1~4（spec §9.6）+ AC-ALL-1~10（spec §9.7）：案例脱敏 + 可编译运行 + README 更新 + 无 Breaking Change + cargo test 全通过 + clippy 零警告 + feature 隔离 + 下游零回归 + 性能不回退 + 五方言一致 + 10 门禁全通过 + 31 REQ 全满足 | M1~M5 全方向就绪 | R-15 sz-pay 环境不可访问（中）、R-16 脱敏遗漏（高）、R-17 feature 组合膨胀（低）、R-18 下游回归（中）、R-19 五方言差异（中） |

### 2.8.3 关键路径与并行机会

- **关键路径**：M1（测试覆盖，3 周）→ M2（架构改进，2 周）→ M3（性能优化，2 周）→ M4（类型安全，2 周）→ M5（文档生态，2 周）→ M6（集成验证，1 周），总周期 12 周（串行）
- **并行机会**：
  - M1 内部：18 扩展包测试可并行（不同包独立），MySQL INSERT IGNORE 修复独立，sz-orm-es/config real feature 独立
  - M2 内部：313 pub API 文档补齐与 async trait 评估可并行，README 更新与 query-builder 指南可并行
  - M3 内部：5 项性能优化可并行（不同模块），6 组基准对比可并行
  - M4 内部：Schema derive 扩展、Column<T>、typed DSL 三者可并行（不同模块）
  - M5 内部：三份迁移指南可并行
- **M6 前置**：M1~M5 全部就绪后进入 M6 集成验证
- **总周期**：关键路径 12 周（串行）；并行开发下 M1 内部 18 包测试并行可压缩至 2 周，M3/M4 内部并行可压缩，总周期可压缩至 8-10 周

---

## 2.9 风险登记与缓解措施

基于 spec §10 风险登记（R-01~R-20），结合本设计方案的架构决策，补充技术层面的缓解措施。

| 编号 | 风险 | 等级 | 影响范围 | 缓解措施（技术层面） | 关联方向 | 关联 REQ |
|------|------|------|---------|---------------------|---------|---------|
| R-01 | 18 扩展包补齐测试工作量巨大 | 高 | 测试覆盖 | 分批补齐（优先 config/auth/crypto/audit/batch/rw/masking），每批附进度跟踪；可分配多人并行（18 包独立）；各包独立分析核心逻辑编写有效测试 | 测试覆盖 | REQ-TC-001 |
| R-02 | 真实 ES/Consul/Nacos 环境不可用 | 中 | 测试覆盖 | 集成测试标注 `#[ignore]`，默认不运行；CI 配置真实环境后运行；Mock 测试保证默认通过；本机数据库可用（MySQL/PG/Oracle，AGENTS.md 已配置） | 测试覆盖 | REQ-TC-003/004 |
| R-03 | MySQL INSERT IGNORE 修复引入其它测试回归 | 中 | 测试覆盖 | 修复后运行全 workspace 测试验证；仅修改测试表 DDL（添加 UNIQUE 约束），不影响生产代码；修复后 `cargo test -p sz-orm-core --test integration_mysql` 全部通过 | 测试覆盖 | REQ-TC-002 |
| R-04 | 313 pub API 文档补齐工作量巨大 | 高 | 架构改进/文档生态 | 分批补齐（优先公开 API > 内部 API > 测试 API），每批附进度跟踪；可分配文档工程师与开发者协作；每批 `cargo doc --workspace --no-deps` 验证无新增警告 | 架构改进/文档生态 | REQ-AR-002/REQ-DOC-001 |
| R-05 | async trait 风格统一迁移影响过大 | 中 | 架构改进 | v3.4.0 仅输出评估文档与推荐方案，不强制立即迁移；Connection trait 手动解糖有 HRTB 技术原因（[pool.rs:42](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L42)），推荐可能是"保持手动解糖 + 文档说明"；迁移作为后续版本交付 | 架构改进 | REQ-AR-004 |
| R-06 | SmallString/CompactString 对长字符串无收益 | 低 | 性能优化 | 基准对比区分短/长字符串场景，短字符串场景收益证据（≥ 1.15x），长字符串场景不退化（CompactString 退化为堆分配，与 String 相当）；基准报告附短/长分别收益 | 性能优化 | REQ-PF-001 |
| R-07 | enum dispatch 五方言行为差异 | 中 | 性能优化 | 五方言集成测试覆盖（MySQL/PG/SQLite/Oracle/MSSQL），行为差分测试（enum vs Box<dyn>）验证一致；enum dispatch 在 core 层统一抽象，方言驱动仅执行 SQL | 性能优化 | REQ-PF-002 |
| R-08 | zero-copy L2 缓存与既有 Redis 后端不兼容 | 中 | 性能优化 | 兼容性测试覆盖（zero-copy vs 普通序列化互通），序列化格式不变（zero-copy 仅优化内存表示，序列化格式兼容）；既有 Redis 缓存数据可读 | 性能优化 | REQ-PF-003 |
| R-09 | 性能优化破坏正确性 | 高 | 性能优化 | 差分测试（优化前 vs 优化后结果一致）+ 既有 6,327+ 测试套件覆盖；每个性能优化 feature 启用后运行全 workspace 测试验证；五方言集成测试覆盖 | 性能优化 | REQ-PF-007 |
| R-10 | 性能基准结果波动 | 中 | 性能优化 | 基准多次运行取中位数 + 置信区间（criterion 默认）；CI 环境标准化（固定机器/隔离噪声）；基准结果附中位数 + 置信区间证据 | 性能优化 | REQ-PF-006 |
| R-11 | `#[derive(Schema)]` 宏对复杂类型支持不足 | 中 | 类型安全 | 跳过不支持字段并告警（复杂嵌套枚举/泛型/生命周期/trait object）+ 告警含字段与类型；用户可手动标注覆盖；文档标注支持类型范围；类型映射测试覆盖 | 类型安全 | REQ-TS-001 |
| R-12 | `Column<T>` 与既有 `&str` API 混用困惑 | 低 | 类型安全 | 支持混用（`Column<T>` 可解引用为 `&str`，`impl<T> Deref for Column<T>`）；文档建议统一使用 `Column<T>` 获得完整类型安全；混用编译通过但类型安全降级 | 类型安全 | REQ-TS-002 |
| R-13 | typed_ast.rs DSL 与 QueryBuilder 行为不一致 | 中 | 类型安全 | 差分测试覆盖（DSL vs QueryBuilder 生成 SQL 完全一致），不一致即修复；DSL 到 SQL 转换与 QueryBuilder 共用底层 SQL 生成逻辑 | 类型安全 | REQ-TS-003 |
| R-14 | 迁移指南与竞品版本不匹配 | 低 | 文档生态 | 迁移指南标注基于的竞品版本（如 Diesel 2.x / SeaORM 0.12 / SQLx 0.9）；定期更新；版本变化时检查 API 对照表 | 文档生态 | REQ-DOC-003 |
| R-15 | sz-pay 生产环境不可访问 | 中 | 生产案例 | 跳过生产运行数据收集（REQ-PC-003 为可选），仅抽取使用模式与更新 README；标注"生产运行数据未收集" | 生产案例 | REQ-PC-003 |
| R-16 | 案例脱敏遗漏泄露敏感信息 | 高 | 生产案例 | 脱敏审查 + 密钥扫描（`grep -r "password\|secret\|key" examples/`），遗漏即修复；案例可公开前最终审查；使用占位符（`mysql://user:pass@127.0.0.1:3306/db`）替代真实连接串 | 生产案例 | REQ-PC-004 |
| R-17 | feature 组合矩阵膨胀（10 聚合 + 6 细粒度 × 既有组合） | 低 | 全部 | 纳入既有门禁 10 Feature 全组合编译（`cargo check --all-features`）；CI 矩阵覆盖 16 新 feature；feature 正交性设计（可独立启用）；组合编译时间监控 | 全部 | 全部 |
| R-18 | 下游 sz-pay 升级回归（虽 feature 默认关闭） | 中 | 全部 | feature gate 默认关闭确保默认零行为变更；实际回归验证 sz-pay 5139 测试 + sz-rust；ADR-0001 严禁修改下游/上游仓库；sz-pay/sz-rust 升级指南文档 | 全部 | 全部 |
| R-19 | 五方言行为差异（性能优化/类型安全增强在各方言支持差异） | 中 | 全部 | 五方言集成测试覆盖；优化与类型安全在 core/macros 层统一抽象，不触碰方言驱动；各方言 DDL 差异处理（如 UNIQUE 约束语法） | 全部 | 全部 |
| R-20 | 文档与代码实际行为不符 | 中 | 架构改进/文档生态 | `cargo doc --workspace --no-deps` 无警告 + `cargo test --workspace --doc` doctest 通过 + CI 定期检查文档与代码一致；不符即修复 | 架构改进/文档生态 | REQ-AR-006/REQ-DOC-004 |

---

## 2.10 验收标准映射

将 spec §9 验收标准总览映射到本设计方案的具体实现点，确保每条验收标准有明确的技术交付物支撑。

### 2.10.1 方向 1 验收标准映射（测试覆盖盲区补齐）

| 验收标准 | 对应 REQ | 技术交付物 | 验证方式 |
|---------|---------|-----------|---------|
| AC-TC-1：18 扩展包各新增 ≥ 1 测试文件，全部通过，覆盖率 ≥ 60% | REQ-TC-001 | 18 扩展包 tests/ 目录（§2.1.3） | `cargo test -p <package>` 全通过 + `cargo tarpaulin -p <package>` 覆盖率 ≥ 60% |
| AC-TC-2：MySQL INSERT IGNORE 测试表 UNIQUE 约束，affected_rows = 0 | REQ-TC-002 | 测试表 DDL 修复（§2.1.3） | `cargo test -p sz-orm-core --test integration_mysql test_mysql_insert_or_ignore_duplicate` 通过 |
| AC-TC-3：sz-orm-es real feature 占位 + 真实 ES 集成测试 | REQ-TC-003 | sz-orm-es `real` feature + tests/real_es_integration.rs（§2.1.3/§2.1.5） | `cargo check -p sz-orm-es --features real` 编译通过 + `cargo test -p sz-orm-es --features real -- --ignored` 真实 ES 测试通过 |
| AC-TC-4：sz-orm-config real-consul/real-nacos + 真实集成测试 | REQ-TC-004 | sz-orm-config `real-consul`/`real-nacos` feature + consul_client.rs/nacos_client.rs（§2.1.3/§2.1.5） | `cargo test -p sz-orm-config --features real-consul -- --ignored` 真实 Consul 测试通过 |
| AC-TC-5：各包覆盖率 ≥ 60%，无效覆盖被拒绝 | REQ-TC-005 | 18 扩展包测试 + 代码审查（§2.1.7） | tarpaulin 报告 + 代码审查拒绝仅 `assert!(true)` 无效覆盖 |
| AC-TC-6：全 workspace 测试数 ≥ 6,327 + 新增数 | 全部 TC | 18 扩展包测试 + MySQL 修复 + real 集成测试（§2.1） | `cargo test --workspace` 测试数 ≥ 6,327 + 新增数，全部通过 |

### 2.10.2 方向 2 验收标准映射（架构改进）

| 验收标准 | 对应 REQ | 技术交付物 | 验证方式 |
|---------|---------|-----------|---------|
| AC-AR-1：Mock 包 real feature 占位，默认 mock 不变 | REQ-AR-001 | sz-orm-es `real` feature（§2.1.5/§2.2.5） | `cargo check -p sz-orm-es --features real` 编译通过 + 默认 mock 行为不变 |
| AC-AR-2：313 pub API 文档补齐，cargo doc 无警告 | REQ-AR-002 | 313 pub API `///` 文档 + 移除 docs.rs cfg 跳过（§2.2.3） | `cargo doc --workspace --no-deps` 无 missing-docs 警告 |
| AC-AR-3：README 移除过时声明，补充 sz-pay 案例 | REQ-AR-003 | README.md 更新（§2.2.3） | README.md 第 46 行更新 + sz-pay 案例（7 包/297 引用/5139 测试） |
| AC-AR-4：async trait 评估文档含基准/迁移/学习成本/推荐 | REQ-AR-004 | docs/async_trait_style_evaluation.md（§2.2.3） | 评估文档含性能基准对比 + 迁移影响分析 + 学习成本评估 + 推荐方案 |
| AC-AR-5：query-builder 选择指南含对比/场景/基准/迁移 | REQ-AR-005 | docs/query_builder_selection_guide.md（§2.2.3） | 指南含能力对比表 + 适用场景 + 性能基准 + 迁移建议 |
| AC-AR-6：cargo doc + doctest 通过，文档与代码一致 | REQ-AR-006 | 文档构建 + doctest 验证（§2.2.7） | `cargo doc --workspace --no-deps` 无警告 + `cargo test --workspace --doc` 通过 |

### 2.10.3 方向 3 验收标准映射（性能优化落地）

| 验收标准 | 对应 REQ | 技术交付物 | 验证方式 |
|---------|---------|-----------|---------|
| AC-PF-1：SmallString 吞吐量 ≥ 1.15x，API 不变 | REQ-PF-001 | query.rs SmallString 路径（§2.3.3/§2.3.5） | criterion 基准（短字符串场景 ≥ 1.15x）+ 既有 QueryBuilder API 不变 |
| AC-PF-2：enum dispatch 开销 ≤ 0.7x，五方言一致 | REQ-PF-002 | dialect.rs enum dispatch 路径（§2.3.3/§2.3.5） | criterion 基准（≤ 0.7x）+ 五方言差分测试一致 |
| AC-PF-3：zero-copy-l2 分配 ≤ 0.5x，API 不变 | REQ-PF-003 | l2_cache.rs zero-copy 路径（§2.3.3/§2.3.5） | criterion 基准 + 分配计数（≤ 0.5x）+ 既有 L2Cache API 不变 |
| AC-PF-4：result_map 评估文档含基准/迁移/类型安全/推荐 | REQ-PF-004 | docs/result_map_macro_evaluation.md（§2.3.3） | 评估文档含性能基准 + 迁移影响 + 类型安全收益 + 推荐方案 |
| AC-PF-5：Box<str> 内存占用减少，API 不变 | REQ-PF-005 | value.rs Box<str> 变体（§2.3.3/§2.3.5） | `size_of::<Value>()` 证据 + 既有 Value API 不变 |
| AC-PF-6：6 组对比基准完善，CI 定期运行 | REQ-PF-006 | benchmarks 6 组对比（§2.3.3/§2.3.7） | 每组附加速比 + 中位数 + 置信区间，CI 定期运行 |
| AC-PF-7：差分测试一致，既有测试通过，无正确性回归 | REQ-PF-007 | 差分测试 + 既有测试套件（§2.3.7） | 优化前 vs 优化后结果完全一致 + 6,327+ 测试全部通过 |

### 2.10.4 方向 4 验收标准映射（编译期类型安全增强）

| 验收标准 | 对应 REQ | 技术交付物 | 验证方式 |
|---------|---------|-----------|---------|
| AC-TS-1：Schema 列名常量生成，引用不存在列编译失败 | REQ-TS-001 | Schema derive 扩展（§2.4.3/§2.4.5） | `trybuild` 编译测试：生成 `User::ID`/`User::NAME` + 引用不存在列编译失败 |
| AC-TS-2：Column<T> 跨表引用编译失败，既有 &str API 不变 | REQ-TS-002 | Column<T> 模块（§2.4.3/§2.4.5） | `trybuild` 编译测试：`Column<User>` 用于 Order 编译失败 + 既有 &str API 不变 |
| AC-TS-3：typed DSL 类型安全，生成 SQL 与 QueryBuilder 一致 | REQ-TS-003 | typed_ast.rs DSL 完善（§2.4.3/§2.4.5） | `trybuild` 编译测试 + 差分测试（DSL vs QueryBuilder SQL 一致） |
| AC-TS-4：零运行时开销，类型检查在编译期 | REQ-TS-004 | 编译期类型检查设计（§2.4.3） | criterion 基准（typed vs &str 零差异） |
| AC-TS-5：参数化绑定，无字符串拼接，SQL 注入扫描通过 | REQ-TS-005 | 参数化绑定设计（§2.4.3） | SQL 注入扫描（列名/表名参数化或编译期常量，值参数化） |

### 2.10.5 方向 5 验收标准映射（文档与生态建设）

| 验收标准 | 对应 REQ | 技术交付物 | 验证方式 |
|---------|---------|-----------|---------|
| AC-DOC-1：313 pub API 文档补齐，cargo doc 无警告 | REQ-DOC-001 | 313 pub API 文档（§2.5.3，与 §2.2 统一） | `cargo doc --workspace --no-deps` 无 missing-docs 警告 |
| AC-DOC-2：README 更新，补充 sz-pay 案例 + crates.io | REQ-DOC-002 | README 更新（§2.5.3，与 §2.2 统一） | README 移除过时声明 + 补充 sz-pay 案例 + crates.io 信息 |
| AC-DOC-3：三份迁移指南含概念映射/API 对照/示例/陷阱 | REQ-DOC-003 | docs/migration/ 三份指南（§2.5.3） | 指南含概念映射表 + API 对照表 + 示例代码 + 常见陷阱 + doctest 可编译 |
| AC-DOC-4：cargo doc + doctest + CI 检查通过 | REQ-DOC-004 | 文档构建 + doctest + CI（§2.5.7） | `cargo doc --workspace --no-deps` 无警告 + `cargo test --workspace --doc` 通过 + CI 定期检查 |

### 2.10.6 方向 6 验收标准映射（sz-pay 生产案例深化）

| 验收标准 | 对应 REQ | 技术交付物 | 验证方式 |
|---------|---------|-----------|---------|
| AC-PC-1：examples/sz_pay_pattern.rs 脱敏，可编译运行 | REQ-PC-001 | examples/src/bin/sz_pay_pattern.rs（§2.6.3/§2.6.5） | `cargo build --bin sz_pay_pattern` 编译通过 + 脱敏审查 + 密钥扫描 |
| AC-PC-2：README 补充生产使用证据，增强信心 | REQ-PC-002 | README 更新（§2.6.3，与 §2.2 统一） | README 含 sz-pay 证据（7 包/297 引用/5139 测试/crates.io） |
| AC-PC-3：收集生产运行数据（可选），脱敏 | REQ-PC-003 | 生产运行数据收集（§2.6.3） | 数据脱敏 + 含命中率/延迟/错误率（sz-pay 环境可访问时） |
| AC-PC-4：脱敏审查 + 密钥扫描通过 | REQ-PC-004 | 脱敏审查 + 密钥扫描（§2.6.7） | `grep -r "password\|secret\|key" examples/` 无真实密钥 |

### 2.10.7 总体验收标准映射

| 验收标准 | 技术交付物 | 验证方式 |
|---------|-----------|---------|
| AC-ALL-1：无 Breaking Change | 10 聚合 feature gate 默认关闭 + 既有 API 不变（§2.0.3） | `cargo check --workspace` 既有 API 编译通过 + feature gate 默认关闭 |
| AC-ALL-2：cargo test 全通过，测试数 ≥ 6,327 + 新增数 | 18 扩展包测试 + MySQL 修复 + real 集成测试（§2.1） | `cargo test --workspace` 全通过 + 测试数 ≥ 6,327 + 新增数 |
| AC-ALL-3：clippy 零警告 | 新增代码遵循 clippy::all（§2.0.3） | `cargo clippy --workspace --all-targets -- -D warnings` 零警告 |
| AC-ALL-4：10 feature gate 隔离，默认不引入依赖 | 10 聚合 + 6 细粒度 feature gate（§2.7） | `cargo check --workspace` 默认 feature 无新增依赖 + `cargo check --all-features` 全组合编译 |
| AC-ALL-5：sz-pay/sz-rust 下游零回归 | ADR-0001 不改下游 + feature gate 默认关闭（§2.0.3） | sz-pay 5139 测试 + sz-rust 零回归验证 |
| AC-ALL-6：v3.3.0 性能基准不回退 | 性能优化 feature gate 隔离 + 差分测试（§2.3.7） | 性能基准对比（冷启动 P95 ≤ 20ms、计划缓存命中率 ≥ 80% 等） |
| AC-ALL-7：五方言行为一致 | 优化/类型安全在 core/macros 层统一（§2.0.3） | 五方言集成测试（MySQL/PG/SQLite/Oracle/MSSQL） |
| AC-ALL-8：10 门禁全通过，新增 16 feature 纳入组合矩阵 | Feature 组合矩阵（§2.7.3） | `cargo check --workspace --all-targets --all-features` + gate.ps1 全通过 |
| AC-ALL-9：每个缺陷修复附 file:line 证据 + 测试验证 | 审计合规铁律（AGENTS.md） | 每条结论附 file:line 证据 + `cargo test` 验证输出 |
| AC-ALL-10：31 条 REQ 全部满足 | 6 方向全部交付（§2.1~§2.6） | 31 条 REQ 验收标准全部通过（§2.10.1~§2.10.6） |

---

> **文档结束**
> 
> 本设计文档对应需求规格 `docs/spec/v3.4.0/spec.md`（31 条 EARS 需求），所有设计决策附 file:line 证据或评估报告引用，遵循 AGENTS.md 工程化审查规范。设计文档放在 `docs/spec/v3.4.0/` 目录。
