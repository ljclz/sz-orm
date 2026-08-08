# sz-orm v3.0.0 技术设计文档

> 版本：v3.0.0（长期目标规划）
> 基线：v2.4.0（已完成：五方言集成测试 + 性能基准 + crates.io 44 包发布 2.3.0 + sz-pay 5139 测试零回归）
> 日期：2026-08-07
> 文档定位：技术设计（How to build），对应需求规格 `docs/spec/v3.0.0/spec.md`（29 条 EARS 需求，6 组 REQ-GDB/REQ-WASM/REQ-FDI/REQ-AI/REQ-DTX/REQ-MB）
> 设计约束：Rust 2021 Edition / rust-version 1.81 / API 向后兼容（无 Breaking Change）/ 禁止占位实现 / unsafe 零容忍 / 参数化查询铁律 / Feature 隔离
> 优先级声明：5 项长期目标均为低优先级，按"多库事务(5) → 发布产物(3) → WASM(2) → 图数据库(1) → AI 优化器(4)"的收益/风险序推进

---

# 一、需求与存量功能关系分析

## 1.1 需求功能与存量功能对比

v3.0.0 的六项任务与 v2.4.0 已交付代码的关系如下。v2.4.0 已完成五方言集成测试、性能基准、crates.io 44 包发布，本版本在此基础上向"多范式数据库 + 跨语言生态 + 智能化 + 分布式一致性"扩展。所有新增能力以扩展包/扩展模块方式提供，不修改 sz-orm-core 既有公开 API 签名（满足 spec §4.5 兼容性约束 C-05 无 Breaking Change）。

### 1.1.1 已实现功能

| 需求功能 | 存量功能 | 代码位置 | 匹配度 |
|---------|---------|---------|--------|
| 多后端透明切换基础（REQ-MB-001~003 依赖） | `AnyBackend` 5 后端枚举（MySql/Postgres/Sqlite/Oracle/Mssql）+ `#[non_exhaustive]` | [packages/sz-orm-sqlx/src/any_driver.rs:57](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L57) | 100% |
| DSN 自动识别后端（REQ-MB-001） | `AnyBackend::from_dsn(dsn)` 支持 mysql/postgres/sqlite/oracle/mssql 五 scheme | [packages/sz-orm-sqlx/src/any_driver.rs:80](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L80) | 100% |
| 方言映射（REQ-MB-001） | `AnyBackend::dialect()` 返回对应 Dialect 实例（MySqlDialect 等 5 种） | [packages/sz-orm-sqlx/src/any_driver.rs:117](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L117) | 100% |
| 后端无关连接工厂（REQ-MB-003） | `AnyPool` 持有 `Arc<dyn ConnectionFactory>`，`connect(dsn)` 自动识别 | [packages/sz-orm-sqlx/src/any_driver.rs:129](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L129) | 100% |
| 统一连接池抽象（REQ-MB-003） | `UnifiedPool` 包装 `Pool` + `AnyBackend`，5 后端透明切换 | [packages/sz-orm-sqlx/src/unified_pool.rs:48](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/unified_pool.rs#L48) | 100% |
| 五方言集成测试基础设施（REQ-MB-002 复用） | `smart_eager_integration_{sqlite,mysql,pg,oracle,mssql}.rs` + `tests/common/equivalence.rs` | packages/sz-orm-core/tests/ | 100% |
| AI 规则型查询优化器（REQ-AI-001~003 基础） | `QueryOptimizer` 纯规则分析，`analyze(sql, schema) -> QueryAnalysis` | [packages/sz-orm-ai/src/nl2sql.rs:1190](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-ai/src/nl2sql.rs#L1190) | 75% |
| 优化建议结构（REQ-AI-003 基础） | `QueryOptimizationHint`（title/description/severity/suggested_sql）+ `QueryAnalysis` 聚合 | [packages/sz-orm-ai/src/nl2sql.rs:1091](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-ai/src/nl2sql.rs#L1091) | 75% |
| NL2SQL 引擎框架（REQ-AI-001 基础） | `Nl2SqlEngine` trait + `SimpleNl2SqlEngine`（规则）+ `OpenAINl2SqlEngine`（real feature） | [packages/sz-orm-ai/src/nl2sql.rs:86](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-ai/src/nl2sql.rs#L86) | 50% |
| OpenAI 兼容 API 客户端（REQ-AI-001/004 基础） | `OpenAIEmbeddingClient`（real feature），reqwest + base64 | packages/sz-orm-ai/src/real_embedding.rs | 50% |
| AI 安全模块（REQ-AI-005 基础） | `safety` 模块，SQL 安全验证（只允许 SELECT、注入检测） | packages/sz-orm-ai/src/safety.rs | 75% |
| 2PC 协调器（REQ-DTX-001/004 基础） | `DistributedTransaction` prepare/commit/rollback 两阶段提交 | [packages/sz-orm-dtx/src/lib.rs:258](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-dtx/src/lib.rs#L258) | 75% |
| 事务参与者模型（REQ-DTX-001 基础） | `TransactionParticipant` 回调式（with_prepare/with_commit/with_rollback） | [packages/sz-orm-dtx/src/lib.rs:174](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-dtx/src/lib.rs#L174) | 50% |
| 事务状态机（REQ-DTX-001/002 基础） | `TransactionState`（Active/Preparing/Prepared/Committing/Committed/RollingBack/RolledBack/Failed） | [packages/sz-orm-dtx/src/lib.rs:151](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-dtx/src/lib.rs#L151) | 75% |
| 事务日志存储（REQ-DTX-002 基础） | `TransactionLogStore` trait + `InMemoryTransactionLog` + `TransactionLogEntry` | [packages/sz-orm-dtx/src/lib.rs:45](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-dtx/src/lib.rs#L45) | 75% |
| DTX 管理器（REQ-DTX-004 基础） | `DtxManager` 统一管理事务（begin/add_participant/prepare/commit/rollback） | [packages/sz-orm-dtx/src/lib.rs:420](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-dtx/src/lib.rs#L420) | 75% |
| Saga/TCC/cross_shard 模式（REQ-DTX-004 共存） | `saga` / `tcc` / `cross_shard` 三个子模块 | packages/sz-orm-dtx/src/{saga,tcc,cross_shard}.rs | 100% |
| WASM 内存数据库（REQ-WASM-001/004 基础） | `WasmDatabase` 支持 SQL 子集（SELECT/INSERT/UPDATE/DELETE/CREATE TABLE） | [packages/sz-orm-wasm/src/lib.rs:55](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-wasm/src/lib.rs#L55) | 75% |
| WASM 内存限制沙箱（REQ-WASM-004 基础） | `MemoryConfig` + `MemoryLimitError` + `LimitedWasmDatabase` | [packages/sz-orm-wasm/src/advanced.rs:33](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-wasm/src/advanced.rs#L33) | 75% |
| WASM 高级功能（REQ-WASM-004 基础） | `SandboxConfig`/`AsyncTaskScheduler`/`ModuleCache`（WASI 沙箱、异步调度、模块缓存） | packages/sz-orm-wasm/src/advanced.rs | 75% |
| Python 绑定代码（REQ-FDI-001/003/004 基础） | PyO3 绑定：PyModel/PyQueryBuilder/PyPool/PyTransaction | [packages/sz-orm-python/src/lib.rs:16](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-python/src/lib.rs#L16) | 75% |
| Python maturin 配置（REQ-FDI-001 基础） | pyproject.toml maturin build-backend，name="sz-orm"，version="0.1.0" | [packages/sz-orm-python/pyproject.toml:1](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-python/pyproject.toml#L1) | 75% |
| JS 绑定代码（REQ-FDI-002/003/004 基础） | napi-rs 绑定：model/query/pool/transaction 模块 | packages/sz-orm-js/src/ | 75% |
| JS napi 配置（REQ-FDI-002 基础） | package.json napi name="core"，@sz-orm/core，三平台 optionalDependencies | [packages/sz-orm-js/package.json:1](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-js/package.json#L1) | 75% |
| crates.io 发布流程基线（REQ-FDI-005 复用） | `publish_crates_io.ps1` 拓扑序逐包发布 + `compute_topology.ps1` | scripts/ | 100% |
| workspace 版本集中管理 | `workspace.package.version = "2.3.0"`，edition="2021"，rust-version="1.81" | [Cargo.toml:6](file:///E:/vue/test/鲜视达/rust/sz-orm/Cargo.toml#L6) | 100% |

### 1.1.2 需要扩展的功能

| 需求功能 | 存量功能 | 差异说明 | 扩展方向 |
|---------|---------|---------|---------|
| LLM 查询计划优化建议（REQ-AI-001） | `QueryOptimizer` 仅规则匹配，无 LLM 调用；`Nl2SqlEngine` 是 NL→SQL 而非 SQL→优化建议 | 输入输出差异：现有 `analyze()` 输入 SQL+schema 输出规则 hint，需新增输入 SQL+EXPLAIN 计划+schema 输出 LLM 建议+规则建议合并；业务逻辑差异：需调用 LLM API、解析结构化响应、合并两类建议 | 在 sz-orm-ai 新增 `query_plan_optimizer` 模块，含 `LlmOptimizer` + `ExplainPlanParser` + `UnifiedQueryOptimizer`（合并规则+LLM），复用 `QueryOptimizationHint` 结构并扩展 source/model 字段 |
| EXPLAIN 计划解析（REQ-AI-002） | 无 EXPLAIN 解析能力 | 输入差异：需接受 EXPLAIN 计划文本（各方言格式不同）；输出差异：解析出全表扫描/缺索引/排序/临时表等信号 | 新增 `ExplainPlanParser` trait + 各方言实现（MySqlExplainParser/PgExplainParser 等），解析失败时明确报错而非静默忽略 |
| 建议来源标注（REQ-AI-003） | `QueryOptimizationHint` 无 source/model 字段 | 结构差异：现有 hint 无来源标识，需新增 `source: HintSource`（Rule/Llm）+ `model: Option<String>` | 扩展 `QueryOptimizationHint` 增加 source/model 字段（向后兼容，默认 Rule），`QueryAnalysis` 聚合时保留来源 |
| LLM 可配置性 + 降级（REQ-AI-004） | `OpenAINl2SqlEngine` 有 API 配置但无降级逻辑 | 行为差异：未配置 LLM 时应自动降级纯规则引擎不报错 | 新增 `OptimizerConfig`（api_key/api_base/model/timeout/max_tokens），`UnifiedQueryOptimizer` 在 LLM 不可用时降级 |
| XA 数据库级参与者（REQ-DTX-001） | `TransactionParticipant` 为回调式（`ParticipantCallback = Arc<dyn Fn()>`），不直连 DB 资源管理器 | 输入差异：XA 参与者需持有真实 DB 连接执行 `XA PREPARE`/`XA COMMIT`；业务逻辑差异：需数据库原生 XA 协议支持而非回调 | 新增 `XaParticipant` 结构体持有 `AnyPool` 连接，实现 `XaResource` trait（xa_prepare/xa_commit/xa_rollback 直连 DB），复用现有 2PC 状态机 |
| XA 崩溃恢复（REQ-DTX-002） | `TransactionLogStore` 有 `read_pending()` 但无恢复执行器 | 行为差异：现有仅存储日志，无重启后扫描未决事务并补偿执行的逻辑 | 新增 `XaRecoveryCoordinator`，启动时调用 `read_pending()` 扫描未决事务，按日志状态执行补偿（Prepared→Commit / Preparing→Rollback） |
| 悬挂事务检测（REQ-DTX-003） | 无超时检测与悬挂标记 | 行为差异：需后台定时扫描超时事务并标记悬挂 | 新增 `SuspensionDetector`（tokio 后台任务，周期扫描 + 超时判定 + 策略执行） |
| WASM JS 互操作（REQ-WASM-002） | `WasmDatabase` 无 wasm-bindgen 导出 | 接口差异：需 `#[wasm_bindgen]` 导出建表/增删改查 API + 生成 .d.ts | 在 sz-orm-wasm 新增 `js_bindings` 模块（feature gate "js"），`#[wasm_bindgen]` 包装 WasmDatabase 方法 |
| WASM 浏览器持久化（REQ-WASM-003） | 无 IndexedDB 持久化 | 行为差异：需通过 web-sys 调用 IndexedDB API 写入/恢复 | 新增 `persistence` 模块（feature gate "persistence"），`IndexedDbStore` 实现 `WasmPersistence` trait，复用 `MemoryConfig` 版本号 |
| WASM wasm32 编译验证（REQ-WASM-001） | 无 wasm32-unknown-unknown 编译目标验证 | 构建差异：需添加 wasm32 target 编译 + 体积检查 | 新增 `.cargo/config.toml` wasm32 target 配置 + CI 编译检查 + gzip 体积断言脚本 |
| Python wheel 构建+发布（REQ-FDI-001） | pyproject.toml 配置就绪但未执行 maturin build/publish | 流程差异：需跨平台构建（linux/win32/darwin x64）+ 干净 venv 安装验证 + PyPI 发布 | 新增 `scripts/build_python_wheel.ps1` + `scripts/publish_pypi.ps1` + CI 矩阵 |
| npm 包构建+发布（REQ-FDI-002） | package.json 配置就绪但未执行 napi build/publish | 流程差异：需三平台 .node 二进制 + npm install 验证 + npm 发布 | 新增 `scripts/build_napi.ps1` + `scripts/publish_npm.ps1` + CI 矩阵 |
| 绑定层功能等价验证（REQ-FDI-003） | 绑定层代码存在但无与 sz-orm-core 行为等价的跨语言测试 | 测试差异：需 pytest/jest 测试套件断言绑定层 CRUD/事务与 core 行为一致 | 新增 `packages/sz-orm-python/tests/`（pytest）+ `packages/sz-orm-js/tests/`（jest），复用等价性断言思路 |
| 多后端就绪清单文档（REQ-MB-001） | 上游能力已就绪但无汇总验证文档 | 文档差异：需汇总 AnyBackend/AnyPool/UnifiedPool 能力清单附 file:line 证据 | 新增 `docs/spec/v3.0.0/multi_backend_readiness.md` 就绪清单文档 |
| 方言约束诊断（REQ-MB-004） | 无方言专属特性使用检测 | 行为差异：需检测代码使用方言专属特性（如 MySQL ON DUPLICATE KEY）并提示 | 新增 `docs/spec/v3.0.0/dialect_constraints.md` 方言特性支持矩阵 + 诊断文档 |

### 1.1.3 需要新增的功能或接口

按业务模块分组，以下功能在存量代码中完全没有对应实现，需新增。

**模块 A：图数据库支持（对应 REQ-GDB-001~005，全新独立包）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| Neo4j Bolt 连接 | Bolt DSN（neo4j://user:pass@host:port） | `GraphConnection` 连接句柄 | Bolt 协议握手 + 认证 + 连接池（复用 sz-orm-core Pool 抽象模式） | neo4j 驱动 crate |
| 参数化 Cypher 查询执行 | Cypher 文本 + 参数 Map | `GraphResult`（节点/关系/标量） | 参数绑定到 `$param` 占位符，禁止字符串拼接，Bolt 协议执行 | GraphConnection |
| 结果类型化映射 | `GraphResult` + 目标结构 | 类型化结构（serde 反序列化） | 节点→Map<String,Value>，关系→含方向与类型，标量→Value 变体 | serde |
| 图模型声明式建模 | 节点标签/属性/关系定义 | `GraphNodeModel`/`GraphRelationModel` | derive 宏或 builder 生成模型元数据，驱动查询构造与映射 | sz-orm-macros（可选） |
| SQL 透传拒绝 | SQL 文本（SELECT/INSERT 等） | `GraphError::SqlNotSupported` | 入口校验查询语言类型，SQL 关键字检测即拒绝 | 无 |

**模块 B：WASM 浏览器端能力（对应 REQ-WASM-002~003，扩展现有包）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| wasm-bindgen 导出 API | JS 调用建表/增删改查 | 类型化结果 | `#[wasm_bindgen]` 包装 WasmDatabase，生成 JS 可调用绑定 + .d.ts | wasm-bindgen |
| IndexedDB 持久化 | 内存数据库快照 | 持久化成功/不可用 | web-sys 调用 IndexedDB API，事务级写入一批变更 | web-sys |
| IndexedDB 恢复 | IndexedDB 存储数据 | 内存数据库恢复 | 读取 IndexedDB + 版本校验 + 反序列化回内存表 | web-sys |
| 持久化不可用报告 | 无 IndexedDB 环境 | `PersistenceUnavailable` 状态 | 检测 web-sys IndexedDB API 可用性，不可用时明确报告 | web-sys |

**模块 C：跨语言发布产物（对应 REQ-FDI-001~005，补齐构建发布流程）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| maturin 跨平台构建 | sz-orm-python 源码 | 三平台 .whl 制品 | maturin build --release 三平台交叉编译 | maturin |
| napi 跨平台构建 | sz-orm-js 源码 | 三平台 .node 二进制 + .d.ts | napi build --release 三平台 | napi-rs |
| PyPI 发布 | .whl 制品 + token | 发布结果 | twine/maturin publish，校验后发布 | PyPI token |
| npm 发布 | .node + .d.ts + token | 发布结果 | npm publish 主包 + 平台子包，校验后发布 | npm token |
| 绑定层等价性测试 | CRUD/事务用例 | 测试通过/失败 | pytest/jest 执行同一用例，断言与 sz-orm-core 行为一致 | pytest/jest |
| 发布阻断校验 | 全平台测试结果 | 放行/阻断 | 任一平台测试失败则阻断发布，输出失败明细 | 无 |

**模块 D：AI 查询计划优化器（对应 REQ-AI-001~005，扩展现有包）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| LLM 优化建议引擎 | SQL + EXPLAIN 计划 + schema | 结构化建议列表 | 构造提示词 → 调用 LLM API → 解析结构化 JSON 响应 → 校验合法性 | reqwest（real feature） |
| EXPLAIN 计划解析 | EXPLAIN 文本（各方言格式） | 计划信号（全表扫描/缺索引等） | 方言识别 + 格式解析 + 信号提取 | sqlparser |
| 建议合并 | 规则 hint + LLM hint | 统一 QueryAnalysis | 合并两类建议，标注来源（source/model），去重 | 无 |
| LLM 降级 | 无 API key / LLM 不可用 | 纯规则建议 | 检测配置缺失或调用失败，自动降级规则引擎 | 无 |
| SQL 脱敏 | 含敏感字面量 SQL | 脱敏后 SQL | 正则识别密码/token 字面量并替换为占位符 | 无 |
| LLM SQL 零执行 | LLM 重写 SQL 建议 | 仅建议返回 | 建议结构中保存 SQL，系统不执行（编译期/运行期保证） | 无 |

**模块 E：XA 事务一致性增强（对应 REQ-DTX-001~005，扩展现有包）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| XA 资源管理器适配 | AnyPool 连接（支持 XA 的库） | `XaResource` 实例 | 检测后端 XA 能力，构造 XaParticipant 持有连接 | sz-orm-sqlx |
| XA 两阶段提交 | 多个 XaParticipant | 全局提交/回滚 | 复用 2PC 状态机，prepare→全成功→commit / 任一失败→rollback | sz-orm-dtx |
| 崩溃恢复 | TransactionLogStore 未决事务 | 恢复结果（收敛终态） | 启动扫描 read_pending()，按状态补偿（Prepared→Commit / Preparing→Rollback） | sz-orm-dtx |
| 悬挂事务检测 | 超时配置 + 事务状态 | 悬挂标记 + 补偿 | 后台定时扫描，Prepare 后超时未决定则标记悬挂并按策略处理 | tokio |
| XA 能力校验 | AnyBackend | 支持/不支持 | 检测后端是否支持 XA（MySQL/PG/Oracle/MSSQL 支持，SQLite 不支持） | 无 |

**模块 F：多后端协同文档（对应 REQ-MB-001~004，文档+集成示例）**

| 功能点 | 输入 | 输出 | 核心逻辑 | 依赖 |
|-------|------|------|---------|------|
| 多后端就绪清单 | AnyBackend/AnyPool/UnifiedPool API | 验证文档（附 file:line 证据） | 逐项验证公开 API 可调用并产出预期结果 | 无 |
| 五方言行为一致性 | CRUD/事务/Eager Loading 用例 | 等价性测试结果 | 复用 v2.4.0 equivalence.rs，五方言执行同一用例断言等价 | sz-orm-core/tests |
| 方言约束文档 | 方言专属特性列表 | 特性支持矩阵文档 | 汇总各方言特性支持范围，标注约束 | 无 |
| sz-rust 协同示例 | sz-orm 公开 API | 集成示例代码 | 示例展示 sz-rust 透明适配层仅依赖公开 API | 无 |

## 1.2 存量功能详细分析

### 1.2.1 AnyBackend / AnyPool / UnifiedPool（多后端基础，REQ-MB-001~003 依赖）

- **接口契约**：
  - `AnyBackend::from_dsn(dsn: &str) -> Result<Self, DbError>`：入参 DSN 字符串，出参后端枚举，未知 scheme 返回 `ConnectionRefused`。无副作用、无 IO（纯字符串匹配）。
  - `AnyBackend::dialect(&self) -> Box<dyn Dialect>`：入参后端枚举，出参对应方言实例。无 IO。
  - `AnyPool::connect(dsn: &str) -> Result<Self, DbError>`：入参 DSN，出参连接池（持有 `Arc<dyn ConnectionFactory>`）。有 IO（建立连接池）。
  - `UnifiedPool::connect(dsn: &str) -> Result<Self, DbError>`：入参 DSN，出参统一连接池（持有 `Pool` + `AnyBackend`）。有 IO。
- **业务规则**：DSN scheme 前缀匹配（mysql://→MySql 等），Oracle/MSSQL 需启用对应 feature gate，未启用时返回明确错误提示。`UnifiedPool` 是 `Pool` 的 newtype 包装，所有方法委托内部 `Pool`，零能力丢失。
- **扩展点**：`AnyBackend` 标注 `#[non_exhaustive]`，未来新增后端变体不破坏外部 match（需 wildcard 臂）。`AnyPool::from_factory(backend, factory)` 支持自定义连接工厂注入。
- **约束**：`AnyBackend` 为 `Copy + Clone + Debug + PartialEq + Eq`，线程安全。Oracle 需 Sysdba 权限，MSSQL 远程连接。DSN 中密码不泄露在错误消息（脱敏处理）。

### 1.2.2 QueryOptimizer / QueryAnalysis / QueryOptimizationHint（AI 规则引擎基础，REQ-AI-001~003 依赖）

- **接口契约**：
  - `QueryOptimizer::analyze(&self, sql: &str, schema: &SchemaContext) -> QueryAnalysis`：入参 SQL 文本 + schema 上下文，出参分析结果（含 hints 列表 + 复杂度评分 + 检测信号）。无 IO、无异常（纯规则匹配）。
  - `QueryOptimizationHint`：含 `title`/`description`/`severity: HintSeverity`/`suggested_sql: Option<String>`。构造器 `info()`/`warning()`/`critical()` + `with_suggested_sql()` 链式。
  - `QueryAnalysis`：含 `original_sql`/`hints: Vec<QueryOptimizationHint>`/`complexity_score`/`detected_tables`/`has_where`/`has_limit`/`has_join`/`has_subquery`/`uses_select_star`。聚合方法 `critical_count()`/`warning_count()`/`has_hints()`。
- **业务规则**：纯规则匹配，检测 SELECT *、缺失 WHERE、缺失 LIMIT、JOIN 复杂度、子查询等。不依赖外部 LLM API，适用于离线场景。`QueryOptimizer` 可配置检查项（check_select_star/check_missing_limit/check_missing_where）与权重（join_weight/subquery_weight/where_weight）。
- **扩展点**：`QueryOptimizationHint` 结构可扩展字段（v3.0.0 新增 source/model）。`QueryOptimizer` 配置项可扩展。
- **约束**：无 IO、无状态（`QueryOptimizer` 为配置结构体，`Send + Sync` 隐式满足）。确定性保证：相同输入始终返回相同输出。**缺口**：无 LLM 调用能力、无 EXPLAIN 计划解析、无来源标注（source/model）。

### 1.2.3 Nl2SqlEngine / OpenAINl2SqlEngine（NL2SQL 引擎，REQ-AI-001/004 参考）

- **接口契约**：`Nl2SqlEngine` trait（async）：`generate(nl_query, schema) -> SqlQuery` + `validate(query) -> bool`。`SimpleNl2SqlEngine`（规则匹配）+ `OpenAINl2SqlEngine`（real feature，调用 OpenAI 兼容 API）。
- **业务规则**：NL→SQL 转换，所有生成 SQL 经安全验证（只允许 SELECT、注入检测）。`SchemaContext` 描述表/列信息。
- **约束**：`OpenAINl2SqlEngine` 需 `real` feature gate（reqwest + base64 依赖）。**与 REQ-AI-001 的差异**：NL2SQL 是"自然语言→SQL"，而查询计划优化器是"SQL+EXPLAIN→优化建议"，方向不同，但可复用 LLM 客户端与安全模块。

### 1.2.4 DistributedTransaction / DtxManager / TransactionLogStore（2PC 基础，REQ-DTX-001/002/004 依赖）

- **接口契约**：
  - `DistributedTransaction::prepare(&mut self) -> Result<(), String>`：遍历参与者执行 prepare 回调，任一失败则回滚已 prepare 的并标记 Failed。有副作用（修改状态 + 写日志）。
  - `DistributedTransaction::commit(&mut self) -> Result<(), String>`：要求状态为 Prepared，遍历执行 commit 回调，首个失败标记 Failed。
  - `DistributedTransaction::rollback(&mut self) -> Result<(), String>`：遍历执行 rollback 回调，标记 RolledBack。
  - `TransactionParticipant`：回调式（`prepare_fn/commit_fn/rollback_fn: Option<ParticipantCallback>`），`ParticipantCallback = Arc<dyn Fn() -> Result<(), String> + Send + Sync>`。
  - `DtxManager`：`Arc<RwLock<HashMap<String, DistributedTransaction>>>` 统一管理，begin/add_participant/prepare/commit/rollback/get/list。
  - `TransactionLogStore` trait：`append(entry)`/`read(tx_id)`/`read_pending()`，手动解糖 async（不用 `#[async_trait]`）。
- **业务规则**：2PC 状态机（Active→Preparing→Prepared→Committing→Committed / RollingBack→RolledBack / Failed）。prepare 阶段任一失败回滚已 prepare 的。日志在 prepare/commit/rollback 各阶段写入（失败不影响主流程）。`InMemoryTransactionLog` 为开发测试用实现。
- **扩展点**：`with_log_store(store)` 注入日志存储。`TransactionLogStore` trait 可自定义实现（如持久化文件/DB）。
- **约束**：`DtxManager` 使用 `parking_lot::RwLock`（同步锁），非 async。`block_on_async` 在同步上下文调用 async 日志写入。**缺口**：参与者为回调式不直连 DB 资源管理器、无数据库级 XA 协议、无崩溃恢复执行器、无悬挂事务检测、无 XA 能力校验。

### 1.2.5 WasmDatabase / advanced 模块（WASM 基础，REQ-WASM-001/004 依赖）

- **接口契约**：
  - `WasmDatabase::query(q: WasmQuery) -> Result<Vec<Value>, String>`：执行 SELECT 查询，支持 `SELECT * FROM <table> [WHERE <col> = ?]`。
  - `WasmDatabase::execute(q: WasmQuery) -> Result<usize, String>`：执行 INSERT/UPDATE/DELETE/CREATE TABLE，返回影响行数。
  - `WasmQuery::new(sql)` / `with_params(sql, params)`：查询请求构造。
  - `MemoryConfig`：内存配额（max_tables/max_rows_per_table/max_row_size_bytes/max_total_bytes），`unlimited()`/`strict()` 预设。
  - `LimitedWasmDatabase`：包装 WasmDatabase + MemoryConfig，超限返回 `MemoryLimitError`。
  - `SandboxConfig`/`SandboxedFs`：WASI 文件系统沙箱（路径白名单/黑名单/只读）。
  - `AsyncTaskScheduler`：异步任务调度（任务队列 + 状态机 + 结果回收）。
  - `ModuleCache`：WASM 模块缓存（LRU + TTL 双策略）。
- **业务规则**：SQL 子集解析（正则/字符串匹配），内存数据库（`Mutex<HashMap<String, Vec<Value>>>`）。内存限制按"任一超出即拒绝"语义。所有功能仅依赖 std/serde/serde_json，不引入额外 crate（包体最小化）。
- **约束**：`WasmDatabase` 使用 `std::sync::Mutex`（同步锁），非 async。SQL 子集有限（无 JOIN/子查询/聚合）。**缺口**：无 wasm-bindgen 互操作、无浏览器端持久化（IndexedDB）、无 wasm32 编译验证、无 JS 类型声明生成。

### 1.2.6 sz-orm-python / sz-orm-js 绑定（FFI 基础，REQ-FDI-001~004 依赖）

- **接口契约**：
  - Python（PyO3）：`#[pymodule] fn sz_orm` 导出 `PyModel`/`PyQueryBuilder`/`PyPool`/`PyTransaction`/`DbType`/`DbError`。异步方法通过 pyo3-asyncio 桥接 asyncio。
  - JS（napi-rs）：`model`/`query`/`pool`/`transaction`/`types`/`error` 模块，`#[napi]` 宏导出。异步方法返回 Promise。
- **业务规则**：绑定层复用 sz-orm-core 参数化查询路径，禁止裸 SQL 拼接。Python 包名 `sz-orm`（PyPI），JS 包名 `@sz-orm/core`（npm），版本 0.1.0 独立版本线。
- **约束**：PyO3 0.20 + maturin ≥1.0，Python ≥3.8。napi-rs 2，Node ≥16。三平台 linux-x64/win32-x64/darwin-x64。**缺口**：未执行 maturin/napi 构建、未产出制品、未发布 PyPI/npm、无跨平台加载测试、无绑定层等价性测试。

### 1.2.7 crates.io 发布流程基线（REQ-FDI-005 复用）

- **接口契约**：`publish_crates_io.ps1` 按拓扑序逐包 `cargo publish`，失败即中止。`compute_topology.ps1` Kahn 算法拓扑排序。
- **业务规则**：v2.4.0 已成功发布 44 包至 crates.io（2.3.0），发布流程经过实战验证。FFI 绑定包（sz-orm-python/js）版本 0.1.0 独立线。
- **约束**：crates.io 已发布版本不可覆盖。发布前 10 道门禁全通过。**v3.0.0 复用**：新增包（sz-orm-graph）需纳入拓扑序与发布脚本。

---

# 二、增量设计方案

## 2.0 架构总览

### 2.0.1 v3.0.0 整体架构图

v3.0.0 在 v2.4.0 现有 45 包 workspace 基础上，新增 1 个独立包（sz-orm-graph）+ 扩展 4 个现有包（sz-orm-ai/sz-orm-dtx/sz-orm-wasm/sz-orm-sqlx）+ 补齐 2 个绑定包发布流程（sz-orm-python/sz-orm-js）+ 协同文档。整体架构如下：

```plantuml
@startuml
!theme plain
title sz-orm v3.0.0 整体架构（增量视图）

rectangle "sz-orm v3.0.0 workspace (46 包)" as WS {

  rectangle "核心层 (不变)" as Core {
    rectangle "sz-orm-core\n(查询/模型/池/事务/EagerLoad)" as CorePkg
    rectangle "sz-orm-macros" as MacrosPkg
    rectangle "sz-orm-sqlx\n(5 后端驱动 + AnyBackend)" as SqlxPkg
  }

  rectangle "扩展层 (v3.0.0 增量)" as Ext {
    rectangle "sz-orm-graph <<new>>\n(图数据库 Neo4j/Cypher)" as GraphPkg
    rectangle "sz-orm-ai (扩展)\n(+ LLM 查询计划优化器)" as AiPkg
    rectangle "sz-orm-dtx (扩展)\n(+ XA 事务/崩溃恢复/悬挂检测)" as DtxPkg
    rectangle "sz-orm-wasm (扩展)\n(+ wasm-bindgen/IndexedDB)" as WasmPkg
  }

  rectangle "绑定层 (补齐发布)" as Bind {
    rectangle "sz-orm-python\n(PyO3 + maturin wheel)" as PyPkg
    rectangle "sz-orm-js\n(napi-rs + npm 包)" as JsPkg
  }

  rectangle "协同层 (文档)" as Coop {
    rectangle "multi_backend_readiness.md <<new>>" as MbDoc
    rectangle "dialect_constraints.md <<new>>" as DialectDoc
  }
}

cloud "Neo4j (Bolt)" as Neo4j
cloud "LLM 服务 (OpenAI 兼容)" as LLM
cloud "浏览器 WASM 运行时" as Browser
cloud "PyPI" as PyPI
cloud "npm" as Npm
database "MySQL/PG/SQLite/Oracle/MSSQL" as Rdb
rectangle "sz-rust 透明适配层" as SzRust

GraphPkg --> Neo4j : Cypher (参数化)
AiPkg --> LLM : 查询计划优化建议
WasmPkg --> Browser : wasm-bindgen + IndexedDB
PyPkg --> PyPI : maturin wheel
JsPkg --> Npm : napi 包
DtxPkg --> Rdb : XA 两阶段提交
SqlxPkg --> Rdb : 5 后端统一访问
CorePkg <-- GraphPkg : 复用 Pool 抽象模式
CorePkg <-- AiPkg : 复用 QueryOptimizationHint
CorePkg <-- DtxPkg : 复用 2PC 状态机
CorePkg <-- WasmPkg : 复用 WasmDatabase
CorePkg <-- PyPkg : 参数化查询路径
CorePkg <-- JsPkg : 参数化查询路径
SqlxPkg <-- DtxPkg : AnyPool 连接
MbDoc --> SzRust : 就绪清单 + file:line 证据
SzRust --> SqlxPkg : 仅依赖公开 API (ADR-0001)

@enduml
```

### 2.0.2 6 大模块在 46 包 workspace 中的定位

| 模块 | 需求组 | 包名 | 形态 | 在 workspace 中的位置 | 依赖关系 |
|------|--------|------|------|---------------------|---------|
| 图数据库支持 | REQ-GDB-001~005 | `sz-orm-graph` | **新增独立包** | `packages/sz-orm-graph` | 依赖 sz-orm-core（Pool 抽象模式参考）、neo4j 驱动 crate |
| WASM 完善 | REQ-WASM-001~005 | `sz-orm-wasm` | **扩展现有包** | `packages/sz-orm-wasm`（已存在） | 新增依赖 wasm-bindgen/web-sys（feature gate） |
| 发布产物 | REQ-FDI-001~005 | `sz-orm-python` + `sz-orm-js` | **补齐构建发布** | `packages/sz-orm-python`、`packages/sz-orm-js`（已存在） | 构建期依赖 maturin/napi-rs，运行期依赖 sz-orm-core |
| AI 优化器 | REQ-AI-001~005 | `sz-orm-ai` | **扩展现有包** | `packages/sz-orm-ai`（已存在） | 新增依赖 reqwest（real feature 已有）、sqlparser |
| XA 事务一致性 | REQ-DTX-001~005 | `sz-orm-dtx` | **扩展现有包** | `packages/sz-orm-dtx`（已存在） | 新增依赖 sz-orm-sqlx（AnyPool 连接） |
| 多后端协同 | REQ-MB-001~004 | 文档 + 集成示例 | **文档+示例** | `docs/spec/v3.0.0/` | 无新包，复用 sz-orm-sqlx 公开 API |

### 2.0.3 与 v2.4.0 现有架构的演进关系

```plantuml
@startuml
!theme plain
title v2.4.0 → v3.0.0 架构演进

rectangle "v2.4.0 (45 包)" as V24 {
  rectangle "sz-orm-core (查询/模型/池/事务/EagerLoad)" as Core24
  rectangle "sz-orm-sqlx (5 后端 + AnyBackend/UnifiedPool)" as Sqlx24
  rectangle "sz-orm-ai (规则 QueryOptimizer + NL2SQL)" as Ai24
  rectangle "sz-orm-dtx (2PC 回调式 + Saga/TCC)" as Dtx24
  rectangle "sz-orm-wasm (内存 DB + 沙箱)" as Wasm24
  rectangle "sz-orm-python (PyO3 代码)" as Py24
  rectangle "sz-orm-js (napi 代码)" as Js24
}

rectangle "v3.0.0 增量" as V30 {
  rectangle "sz-orm-graph <<new>>" as Graph30
  rectangle "sz-orm-ai +LLM优化器" as Ai30
  rectangle "sz-orm-dtx +XA/恢复/悬挂" as Dtx30
  rectangle "sz-orm-wasm +bindgen/IndexedDB" as Wasm30
  rectangle "sz-orm-python +wheel发布" as Py30
  rectangle "sz-orm-js +npm发布" as Js30
  rectangle "协同文档 <<new>>" as Doc30
}

Core24 -[hidden]-> V30
Sqlx24 -[hidden]-> V30

Ai24 ..> Ai30 : 扩展 (不破坏现有 API)
Dtx24 ..> Dtx30 : 扩展 (不破坏现有 API)
Wasm24 ..> Wasm30 : 扩展 (不破坏现有 API)
Py24 ..> Py30 : 补齐发布流程
Js24 ..> Js30 : 补齐发布流程

note bottom of V30
  **演进原则**：
  1. 无 Breaking Change（现有公开 API 签名不变）
  2. 新增能力通过 feature gate 隔离
  3. 新增包独立版本线（0.1.0）
  4. 复用现有基础设施（Pool/Dialect/2PC 状态机/QueryOptimizationHint）
end note

@enduml
```

**演进关键决策**：

| 决策点 | 选项 | 选择 | 理由 |
|--------|------|------|------|
| 图数据库包形态 | A. 嵌入 sz-orm-core / B. 新独立包 | B | 图数据库是非关系型多范式扩展，与关系型 SQL 方言范畴不同；独立包避免核心包膨胀 + feature 隔离清晰 |
| AI 优化器位置 | A. 新包 / B. 扩展 sz-orm-ai | B | sz-orm-ai 已有 QueryOptimizer/Nl2SqlEngine/safety 基础，LLM 优化器是其自然延伸，复用 hint 结构 |
| XA 事务位置 | A. 新包 / B. 扩展 sz-orm-dtx | B | sz-orm-dtx 已有 2PC 状态机/日志/DtxManager，XA 是协议级扩展，复用状态机 |
| WASM 持久化依赖 | A. idb crate / B. web-sys 直接调用 | B | web-sys 是 wasm-bindgen 生态标准，idb crate 增加额外依赖；直接调用保持包体最小 |
| 绑定层版本 | A. 跟随 3.0.0 / B. 独立 0.1.0 线 | B | FFI 绑定是独立制品（PyPI/npm），与 Rust crate 版本线解耦，v2.4.0 已确立 0.1.0 线 |
| 协同层形态 | A. 新包 / B. 文档+示例 | B | ADR-0001 严禁修改上游/下游仓库，协同仅需验证上游就绪 + 文档约束，无代码交付 |

---

## 2.1 图数据库支持（REQ-GDB-001~005）

### 2.1.1 模块目标

提供 Neo4j 图数据库的连接、参数化 Cypher 查询、结果类型化映射、声明式建模能力，作为新独立包 `sz-orm-graph` 提供，不触碰 sz-orm-core/sz-orm-sqlx 既有 API，实现 sz-orm 从"多方言关系型 ORM"向"多范式数据库"的扩展。

### 2.1.2 架构设计

```plantuml
@startuml
!theme plain
title sz-orm-graph 包架构

package "sz-orm-graph (新增)" {
  rectangle "connection 模块" as Conn {
    rectangle "GraphConfig\n(Bolt DSN + 超时 + 池配置)" as GConfig
    rectangle "GraphConnection\n(Bolt 连接句柄)" as GConn
    rectangle "GraphPool\n(连接池, 复用 Pool 抽象模式)" as GPool
  }

  rectangle "query 模块" as Query {
    rectangle "CypherQuery\n(Cypher 文本 + 参数 Map)" as CQuery
    rectangle "CypherQueryBuilder\n(链式构造)" as CQB
    rectangle "GraphResult\n(节点/关系/标量结果)" as GResult
  }

  rectangle "model 模块" as Model {
    rectangle "GraphNodeModel\n(标签 + 属性定义)" as GNode
    rectangle "GraphRelationModel\n(类型 + 方向 + 属性)" as GRel
    rectangle "GraphModelDerive (可选)\n(derive 宏)" as GDerive
  }

  rectangle "mapping 模块" as Mapping {
    rectangle "ResultMapper\n(GraphResult → 用户结构)" as RMapper
    rectangle "NodeMapper / RelationMapper" as NRMapper
  }

  rectangle "error 模块" as Err {
    rectangle "GraphError\n(Connection/Query/Mapping/SqlNotSupported)" as GErr
  }

  rectangle "validator 模块" as Val {
    rectangle "CypherValidator\n(参数化校验 + SQL 拒绝)" as CVal
  }
}

database "Neo4j (Bolt 协议)" as Neo4j
rectangle "neo4j 驱动 crate" as Driver

GPool --> Driver : Bolt 协议
Driver --> Neo4j : 连接/查询
GConn --> GPool : acquire()
CQuery --> CVal : 校验
CVal --> GErr : SQL 透传拒绝
GConn --> CQuery : 执行
CQuery --> GResult : 结果
GResult --> RMapper : 映射
RMapper --> NRMapper
GNode --> CQB : 驱动构造
GRel --> CQB

@enduml
```

**模块层次**：
- `connection`：Bolt 协议连接 + 连接池（复用 sz-orm-core `Pool`/`PoolConfig` 抽象模式，非直接依赖 Pool 类型）
- `query`：Cypher 查询构造与执行
- `model`：图模型声明（节点标签/属性、关系类型/方向）
- `mapping`：结果类型化映射（节点→Map、关系→含方向、标量→Value）
- `error`：统一错误类型
- `validator`：参数化校验 + SQL 透传拒绝

### 2.1.3 关键数据结构

```rust
//! sz-orm-graph 核心数据结构骨架

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// 图数据库连接配置
#[derive(Debug, Clone)]
pub struct GraphConfig {
    /// Bolt DSN，如 "neo4j://user:pass@localhost:7687"
    pub dsn: String,
    /// 连接超时（秒）
    pub connect_timeout_secs: u64,
    /// 查询超时（秒）
    pub query_timeout_secs: u64,
    /// 连接池大小
    pub max_pool_size: usize,
}

/// 图数据库连接池
pub struct GraphPool {
    /// 内部持有 neo4j 驱动的连接池
    inner: GraphPoolInner,
    config: GraphConfig,
}

/// 图数据库连接句柄
pub struct GraphConnection {
    /// Bolt 协议连接
    inner: GraphConnInner,
}

/// 参数化 Cypher 查询
#[derive(Debug, Clone)]
pub struct CypherQuery {
    /// Cypher 文本，含 $param 占位符
    pub cypher: String,
    /// 参数绑定（参数名 → 值）
    pub parameters: HashMap<String, GraphValue>,
}

/// Cypher 查询结果（节点/关系/标量混合）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GraphResult {
    /// 节点：标签 + 属性 Map
    Node(GraphNode),
    /// 关系：类型 + 方向 + 属性 + 起止节点 ID
    Relationship(GraphRelationship),
    /// 标量值
    Scalar(GraphValue),
    /// 路径（节点与关系交替序列）
    Path(Vec<GraphResult>),
}

/// 图节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: i64,
    pub labels: Vec<String>,
    pub properties: HashMap<String, GraphValue>,
}

/// 图关系
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphRelationship {
    pub id: i64,
    pub rel_type: String,
    pub start_node_id: i64,
    pub end_node_id: i64,
    pub properties: HashMap<String, GraphValue>,
}

/// 图属性值（Cypher 类型映射）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GraphValue {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    List(Vec<GraphValue>),
    Map(HashMap<String, GraphValue>),
}

/// 图节点模型（声明式建模）
#[derive(Debug, Clone)]
pub struct GraphNodeModel {
    /// 节点标签（如 "Person"）
    pub label: &'static str,
    /// 属性定义
    pub properties: Vec<GraphPropertyDef>,
}

/// 图关系模型
#[derive(Debug, Clone)]
pub struct GraphRelationModel {
    /// 关系类型（如 "KNOWS"）
    pub rel_type: &'static str,
    /// 方向
    pub direction: RelationDirection,
    /// 起始节点标签
    pub from_label: &'static str,
    /// 终止节点标签
    pub to_label: &'static str,
    /// 属性定义
    pub properties: Vec<GraphPropertyDef>,
}

#[derive(Debug, Clone, Copy)]
pub enum RelationDirection { Outgoing, Incoming, Both }

/// 图属性定义
#[derive(Debug, Clone)]
pub struct GraphPropertyDef {
    pub name: &'static str,
    pub value_type: GraphValueType,
    pub nullable: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum GraphValueType { Bool, Integer, Float, String, List, Map }

/// 图数据库统一错误
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("Graph connection error: {0}（DSN 已脱敏）")]
    ConnectionError(String),
    #[error("Graph query error: {0}")]
    QueryError(String),
    #[error("Graph mapping error: {detail}（缺失字段: {missing_fields:?}）")]
    MappingError { detail: String, missing_fields: Vec<String> },
    #[error("不支持 SQL 语句，图查询接口仅接受 Cypher（MATCH/WHERE/RETURN 等）")]
    SqlNotSupported,
    #[error("Cypher 参数化校验失败: {0}")]
    ParameterizationError(String),
    #[error("Neo4j 驱动错误: {0}")]
    DriverError(String),
}
```

### 2.1.4 核心算法/流程

**参数化 Cypher 查询执行流程**：

```pseudo
function execute_cypher(conn, query: CypherQuery) -> Result<Vec<GraphResult>, GraphError>:
    // 步骤 1: SQL 透传拒绝（REQ-GDB-005）
    if CypherValidator.contains_sql_keywords(query.cypher):
        return Err(GraphError::SqlNotSupported)

    // 步骤 2: 参数化校验（REQ-GDB-002）
    if not CypherValidator.is_parameterized(query.cypher):
        return Err(GraphError::ParameterizationError("查询含字面量拼接"))

    // 步骤 3: Bolt 协议执行（参数绑定）
    raw_results = conn.bolt_run(query.cypher, query.parameters)

    // 步骤 4: 结果类型化映射（REQ-GDB-003）
    mapped = ResultMapper.map(raw_results)

    return Ok(mapped)
```

**SQL 透传拒绝算法（REQ-GDB-005）**：

```pseudo
function contains_sql_keywords(cypher: &str) -> bool:
    upper = cypher.to_uppercase()
    sql_keywords = ["SELECT ", "INSERT INTO", "UPDATE ", "DELETE FROM", "CREATE TABLE", "DROP TABLE"]
    for kw in sql_keywords:
        if upper.contains(kw):
            return true
    return false
```

**结果类型化映射算法（REQ-GDB-003）**：

```pseudo
function map_results(raw: BoltResults) -> Vec<GraphResult>:
    results = []
    for record in raw.records:
        for (key, value) in record:
            match value:
                Node(n) => results.push(GraphResult::Node(map_node(n)))
                Relationship(r) => results.push(GraphResult::Relationship(map_rel(r)))
                Path(p) => results.push(GraphResult::Path(map_path(p)))
                scalar => results.push(GraphResult::Scalar(map_value(scalar)))
    return results

function map_to_user_struct<T: Deserialize>(result: GraphResult) -> Result<T, GraphError>:
    json = serde_json::to_value(result)?
    T::deserialize(json).map_err(|e| GraphError::MappingError { ... })
```

### 2.1.5 依赖关系

| 依赖 | 类型 | 用途 | Feature 隔离 |
|------|------|------|-------------|
| `neo4rs`（或同类 Bolt 驱动） | 外部 crate | Neo4j Bolt 协议连接与查询 | 默认 feature |
| `tokio` | workspace | 异步运行时 | 默认 |
| `serde` / `serde_json` | workspace | 结果序列化/反序列化 | 默认 |
| `thiserror` | workspace | 错误类型派生 | 默认 |
| `sz-orm-core` | workspace（可选） | 复用 Pool 抽象模式（仅参考设计，非类型依赖） | 可选 |
| `sz-orm-macros` | workspace（可选） | derive 宏支持声明式建模 | "derive" feature |

**与现有包的关系**：sz-orm-graph 为独立包，不依赖 sz-orm-core 的具体类型（避免核心包耦合），仅参考其 Pool/PoolConfig 设计模式自行实现图连接池。

### 2.1.6 新增包规划

| 属性 | 值 |
|------|-----|
| 包名 | `sz-orm-graph` |
| 路径 | `packages/sz-orm-graph` |
| 职责 | Neo4j 图数据库连接、参数化 Cypher 查询、结果类型化映射、声明式建模 |
| 初始版本 | `0.1.0`（独立版本线，与 FFI 绑定一致） |
| Cargo.toml features | `default = []`、`derive`（derive 宏支持） |
| 发布目标 | crates.io |
| workspace 成员 | 新增至 `Cargo.toml` members 列表 |

### 2.1.7 与现有代码的集成点

| 集成点 | 现有代码位置 | 集成方式 |
|--------|-------------|---------|
| Pool 抽象模式参考 | [packages/sz-orm-core/src/pool.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs) | 参考设计模式（PoolConfig/Pool/PooledConnection），非类型依赖 |
| 参数化查询铁律 | AGENTS.md C-03 | Cypher 参数化校验复用同一铁律（`$param` 占位符） |
| 错误脱敏模式 | [packages/sz-orm-sqlx/src/any_driver.rs:92](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L92) | GraphError::ConnectionError 复用 DSN 脱敏模式 |
| workspace 注册 | [Cargo.toml:2](file:///E:/vue/test/鲜视达/rust/sz-orm/Cargo.toml#L2) | members 列表新增 `packages/sz-orm-graph` |
| 发布拓扑 | scripts/compute_topology.ps1 | 新增包纳入拓扑序（无下游依赖，可早期发布） |

### 2.1.8 风险与缓解

| 风险 | 等级 | 缓解措施 |
|------|------|---------|
| R-01: Neo4j Bolt 驱动生态成熟度不足 | 高 | 锁定 neo4rs 驱动版本，独立包 feature 隔离，评估期先行 spike 验证 Bolt 协议握手+查询+结果反序列化 |
| Neo4j 测试环境搭建 | 中 | Docker Compose 启动 Neo4j 容器，`#[ignore]` 标注真实连接测试，CI 可选触发 |
| Cypher 注入风险 | 高 | `CypherValidator` 强制参数化校验，禁止字面量拼接，注入载荷作为参数传入时被当作字面量 |
| 结果映射类型不匹配 | 中 | `GraphError::MappingError` 附缺失字段/类型差异明细，serde 反序列化失败返回明确错误 |

---

## 2.2 WASM 完善（REQ-WASM-001~005）

### 2.2.1 模块目标

在既有 sz-orm-wasm 包内扩展，补齐浏览器端能力：wasm32 目标编译、wasm-bindgen JS 互操作层、IndexedDB 持久化与恢复，复用现有 `WasmDatabase`/`advanced` 沙箱能力，不新增包。

### 2.2.2 架构设计

```plantuml
@startuml
!theme plain
title sz-orm-wasm 扩展架构

package "sz-orm-wasm (扩展)" {
  rectangle "lib.rs (存量)" as Exist {
    rectangle "WasmDatabase (内存 DB)" as WDb
    rectangle "WasmQuery" as WQ
  }

  rectangle "advanced.rs (存量)" as Adv {
    rectangle "MemoryConfig / LimitedWasmDatabase" as MemLimit
    rectangle "SandboxConfig / AsyncTaskScheduler / ModuleCache" as Sandbox
  }

  rectangle "js_bindings.rs <<new>>\n(feature: js)" as JsBind {
    rectangle "JsWasmDatabase\n(#[wasm_bindgen] 导出)" as JsDb
    rectangle "JsQueryResult\n(serde → JsValue)" as JsResult
  }

  rectangle "persistence.rs <<new>>\n(feature: persistence)" as Persist {
    rectangle "WasmPersistence trait" as PTrait
    rectangle "IndexedDbStore\n(web-sys IndexedDB)" as IdbStore
    rectangle "PersistenceConfig\n(存储版本 + db 名)" as PCfg
  }

  rectangle "error.rs <<new>>" as WErr {
    rectangle "WasmPersistenceError\n(Unavailable/RestoreError)" as PErr
  }
}

browser "浏览器" as Browser {
  rectangle "JS 调用方" as JsCaller
  database "IndexedDB" as Idb
}

JsCaller --> JsDb : wasm-bindgen 桥接
JsDb --> WDb : 委托内存 DB
JsDb --> JsResult : 结果转 JsValue
JsCaller --> JsDb : persist()
JsDb --> IdbStore : 持久化
IdbStore --> Idb : web-sys API
JsCaller --> JsDb : restore()
Idb --> IdbStore : 读取
IdbStore --> WDb : 恢复到内存

@enduml
```

**模块层次**：
- `js_bindings`（feature "js"）：`#[wasm_bindgen]` 导出层，包装 WasmDatabase 方法为 JS 可调用
- `persistence`（feature "persistence"）：IndexedDB 持久化与恢复
- `error`：持久化错误类型

### 2.2.3 关键数据结构

```rust
//! sz-orm-wasm 扩展数据结构骨架

// ==================== js_bindings 模块 (feature: js) ====================
#[cfg(feature = "js")]
mod js_bindings {
    use wasm_bindgen::prelude::*;
    use crate::WasmDatabase;

    /// JS 侧可调用的 WASM 数据库句柄
    #[wasm_bindgen]
    pub struct JsWasmDatabase {
        inner: WasmDatabase,
    }

    #[wasm_bindgen]
    impl JsWasmDatabase {
        #[wasm_bindgen(constructor)]
        pub fn new() -> Self;

        /// 建表（CREATE TABLE）
        pub fn create_table(&mut self, sql: &str) -> Result<(), JsValue>;

        /// 插入（INSERT）
        pub fn insert(&mut self, sql: &str, params: &JsValue) -> Result<usize, JsValue>;

        /// 查询（SELECT）
        pub fn query(&self, sql: &str, params: &JsValue) -> Result<JsValue, JsValue>;

        /// 更新（UPDATE）
        pub fn update(&mut self, sql: &str, params: &JsValue) -> Result<usize, JsValue>;

        /// 删除（DELETE）
        pub fn delete(&mut self, sql: &str, params: &JsValue) -> Result<usize, JsValue>;

        /// 持久化到 IndexedDB（feature: persistence）
        #[cfg(feature = "persistence")]
        pub async fn persist(&self) -> Result<(), JsValue>;

        /// 从 IndexedDB 恢复（feature: persistence）
        #[cfg(feature = "persistence")]
        pub async fn restore(&mut self) -> Result<(), JsValue>;
    }
}

// ==================== persistence 模块 (feature: persistence) ====================
#[cfg(feature = "persistence")]
mod persistence {
    use crate::WasmDatabase;

    /// 持久化配置
    #[derive(Debug, Clone)]
    pub struct PersistenceConfig {
        /// IndexedDB 数据库名
        pub db_name: String,
        /// 存储版本号（版本不匹配返回 RestoreError）
        pub storage_version: u32,
        /// store（表）名
        pub store_name: String,
    }

    /// WASM 持久化 trait
    pub trait WasmPersistence {
        /// 持久化内存数据库到存储
        async fn persist(&self, config: &PersistenceConfig) -> Result<(), PersistenceError>;
        /// 从存储恢复到内存数据库
        async fn restore(&mut self, config: &PersistenceConfig) -> Result<(), PersistenceError>;
        /// 检查持久化是否可用（IndexedDB 是否存在）
        fn is_available() -> bool;
    }

    /// IndexedDB 存储实现（通过 web-sys）
    pub struct IndexedDbStore;

    impl WasmPersistence for WasmDatabase {
        // 通过 web-sys 调用 IndexedDB API
    }
}

// ==================== error 模块 ====================
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasmPersistenceError {
    /// 持久化不可用（无 IndexedDB / 非浏览器环境）
    Unavailable,
    /// 恢复失败（数据损坏 / 版本不兼容）
    RestoreError { detail: String, storage_version: u32, expected_version: u32 },
    /// IndexedDB 操作错误
    IndexedDbError(String),
    /// 序列化/反序列化错误
    SerializationError(String),
}
```

### 2.2.4 核心算法/流程

**IndexedDB 持久化流程（REQ-WASM-003）**：

```pseudo
async function persist(db: WasmDatabase, config: PersistenceConfig) -> Result<(), PersistenceError>:
    // 步骤 1: 检查 IndexedDB 可用性（REQ-WASM-005）
    if not IndexedDbStore.is_available():
        return Err(PersistenceError::Unavailable)  // 明确报告，不静默丢数据

    // 步骤 2: 序列化内存数据库快照
    snapshot = serialize_all_tables(db)  // {table_name: [rows]}

    // 步骤 3: 事务级写入 IndexedDB（一次持久化一批变更）
    idb = await web_sys::window().indexed_db().open(config.db_name, config.storage_version)
    tx = idb.transaction(config.store_name, "readwrite")
    store = tx.object_store(config.store_name)
    store.clear()  // 清除旧数据
    for (table_name, rows) in snapshot:
        store.put(rows, table_name)  // 事务级写入
    await tx.done()  // 等待事务完成

    return Ok(())
```

**IndexedDB 恢复流程（REQ-WASM-003）**：

```pseudo
async function restore(db: &mut WasmDatabase, config: PersistenceConfig) -> Result<(), PersistenceError>:
    // 步骤 1: 检查可用性
    if not IndexedDbStore.is_available():
        return Err(PersistenceError::Unavailable)

    // 步骤 2: 读取 IndexedDB
    idb = await web_sys::window().indexed_db().open(config.db_name)
    if idb.version() != config.storage_version:
        return Err(PersistenceError::RestoreError {  // 版本不匹配
            detail: "版本不兼容",
            storage_version: idb.version(),
            expected_version: config.storage_version,
        })

    // 步骤 3: 反序列化回内存表
    tx = idb.transaction(config.store_name, "readonly")
    store = tx.object_store(config.store_name)
    cursor = await store.open_cursor()
    db.tables.clear()
    while cursor:
        table_name = cursor.key()
        rows = cursor.value()
        db.tables.insert(table_name, deserialize_rows(rows))
        cursor.continue()

    return Ok(())
```

**内存资源限制流程（REQ-WASM-004，复用现有 advanced 沙箱）**：

```pseudo
function execute_with_limit(db: &mut LimitedWasmDatabase, query) -> Result<(), MemoryLimitError>:
    // 复用现有 MemoryConfig 校验
    if db.memory_usage() + estimated_query_memory(query) > db.config.max_total_bytes:
        return Err(MemoryLimitError::TotalBytesExceeded { ... })  // 拒绝写入，不 panic
    db.execute(query)  // 内存占用不再增长
```

### 2.2.5 依赖关系

| 依赖 | 类型 | 用途 | Feature 隔离 |
|------|------|------|-------------|
| `wasm-bindgen` | 外部 crate | JS 互操作绑定生成 | "js" feature |
| `web-sys` | 外部 crate | IndexedDB API 访问 | "persistence" feature |
| `js-sys` | 外部 crate | JS 类型互操作 | "js" feature |
| `serde` / `serde_json` | workspace（已有） | 序列化 | 默认 |

**与现有包的关系**：扩展现有 sz-orm-wasm，复用 `WasmDatabase`/`WasmQuery`/`MemoryConfig`/`LimitedWasmDatabase`，不修改其公开 API。

### 2.2.6 新增包规划

无新增包。在 `packages/sz-orm-wasm` 内扩展：
- 新增 `src/js_bindings.rs`（feature "js"）
- 新增 `src/persistence.rs`（feature "persistence"）
- 新增 `src/error.rs`
- 修改 `Cargo.toml` 新增 features 与依赖
- 新增 `.cargo/config.toml`（wasm32 target 配置，workspace 级别）

### 2.2.7 与现有代码的集成点

| 集成点 | 现有代码位置 | 集成方式 |
|--------|-------------|---------|
| WasmDatabase 复用 | [packages/sz-orm-wasm/src/lib.rs:55](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-wasm/src/lib.rs#L55) | js_bindings 委托 WasmDatabase 方法 |
| MemoryConfig 复用 | [packages/sz-orm-wasm/src/advanced.rs:33](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-wasm/src/advanced.rs#L33) | 持久化前校验内存限制 |
| SQL 子集解析复用 | [packages/sz-orm-wasm/src/lib.rs:66](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-wasm/src/lib.rs#L66) | js_bindings 导出的查询复用现有 SQL 解析 |
| Cargo.toml features | packages/sz-orm-wasm/Cargo.toml | 新增 `js`/`persistence` feature gate |

### 2.2.8 风险与缓解

| 风险 | 等级 | 缓解措施 |
|------|------|---------|
| R-02: WASM 生态对 tokio/异步支持有限 | 高 | 浏览器端用同步/轻异步执行路径（wasm-bindgen-futures），与服务器端 WASI 场景分离；持久化用 async 但非 tokio |
| R-03: wasm-bindgen 版本 API 变动频繁 | 中 | 锁定 wasm-bindgen 版本，绑定层最小化（仅包装 WasmDatabase 方法） |
| WASM 产物体积超 1MB | 中 | `wasm-opt` 优化 + `twiggy` 分析体积 + feature gate 隔离重依赖 + gzip 体积断言脚本 |
| IndexedDB 浏览器兼容性 | 低 | web-sys 标准 API，主流浏览器支持；隐私模式/禁用时返回 Unavailable（REQ-WASM-005） |

---

## 2.3 maturin/napi 发布产物（REQ-FDI-001~005）

### 2.3.1 模块目标

补齐 sz-orm-python（PyO3/maturin）与 sz-orm-js（napi-rs）的构建、跨平台打包、发布流水线，产出可安装的 PyPI wheel 与 npm 包，并验证绑定层功能与 sz-orm-core 行为等价。

### 2.3.2 架构设计

```plantuml
@startuml
!theme plain
title 跨语言发布产物架构

package "构建流水线 (scripts/)" as Pipeline {
  rectangle "build_python_wheel.ps1 <<new>>" as BuildPy
  rectangle "build_napi.ps1 <<new>>" as BuildJs
  rectangle "publish_pypi.ps1 <<new>>" as PubPy
  rectangle "publish_npm.ps1 <<new>>" as PubJs
  rectangle "verify_bindings.ps1 <<new>>" as Verify
}

package "sz-orm-python (补齐发布)" as PyPkg {
  rectangle "src/ (存量 PyO3 绑定)" as PySrc
  rectangle "pyproject.toml (存量 maturin)" as PyProj
  rectangle "tests/ <<new>>\n(pytest 等价性测试)" as PyTest
}

package "sz-orm-js (补齐发布)" as JsPkg {
  rectangle "src/ (存量 napi 绑定)" as JsSrc
  rectangle "package.json (存量 napi)" as JsPkgJson
  rectangle "tests/ <<new>>\n(jest 等价性测试)" as JsTest
}

cloud "PyPI" as PyPI
cloud "npm" as Npm
rectangle "CI 矩阵\n(linux/win32/darwin x64)" as CI

BuildPy --> PyPkg : maturin build --release
BuildPy --> CI : 三平台交叉编译
PyPkg --> PyPI : maturin publish
BuildJs --> JsPkg : napi build --release
BuildJs --> CI : 三平台 .node 二进制
JsPkg --> Npm : npm publish
Verify --> PyTest : pytest 执行
Verify --> JsTest : jest 执行
Verify --> BuildPy : 阻断未验证发布
Verify --> BuildJs : 阻断未验证发布

@enduml
```

### 2.3.3 关键数据结构

发布产物无新增 Rust 数据结构，主要为构建脚本与测试套件。核心配置结构：

```rust
//! 发布产物配置骨架（脚本侧，非 Rust 类型）

// Python wheel 构建配置
struct PythonWheelBuildConfig {
    package_name: "sz-orm",          // PyPI 包名
    version: "0.1.0",                // 独立版本线
    python_min: "3.8",               // 最低 Python 版本
    platforms: ["linux-x64", "win32-x64", "darwin-x64"],
    maturin_features: ["pyo3/extension-module"],
}

// npm 包构建配置
struct NpmPackageBuildConfig {
    package_name: "@sz-orm/core",    // npm 主包名
    version: "0.1.0",
    node_min: 16,
    platforms: [
        "@sz-orm/core-linux-x64-gnu",
        "@sz-orm/core-win32-x64-msvc",
        "@sz-orm/core-darwin-x64",
    ],
    napi_config: { name: "core" },
}
```

### 2.3.4 核心算法/流程

**Python wheel 构建与发布流程（REQ-FDI-001）**：

```pseudo
function build_and_publish_python():
    // 步骤 1: 三平台构建
    for platform in [linux-x64, win32-x64, darwin-x64]:
        wheel = maturin build --release --target platform
        assert wheel exists

    // 步骤 2: 干净 venv 安装验证
    for platform in platforms:
        venv = create_clean_venv()
        venv.pip_install(wheel)
        assert venv.python("import sz_orm") succeeds  // REQ-FDI-001 验收

    // 步骤 3: 等价性测试（REQ-FDI-003）
    pytest_result = run pytest packages/sz-orm-python/tests/
    if pytest_result.failed:
        abort_publish()  // REQ-FDI-005 阻断

    // 步骤 4: 发布
    maturin publish  // 发布到 PyPI
```

**npm 包构建与发布流程（REQ-FDI-002）**：

```pseudo
function build_and_publish_js():
    // 步骤 1: 三平台构建 .node 二进制
    for platform in [linux-x64-gnu, win32-x64-msvc, darwin-x64]:
        napi build --release --target platform
        assert .node file exists
        assert index.d.ts exists  // TypeScript 类型声明

    // 步骤 2: 平台矩阵完整性校验
    missing = check_platform_matrix()
    if missing not empty:
        abort_publish(missing)  // REQ-FDI-005 阻断

    // 步骤 3: npm install 验证
    for platform in platforms:
        tmp_dir = create_temp()
        tmp_dir.npm_install("@sz-orm/core")
        assert tmp_dir.require("@sz-orm/core") succeeds  // REQ-FDI-002 验收

    // 步骤 4: 等价性测试（REQ-FDI-003）
    jest_result = run jest packages/sz-orm-js/tests/
    if jest_result.failed:
        abort_publish()

    // 步骤 5: 发布主包 + 平台子包
    npm publish  // @sz-orm/core 主包
    for platform in platforms:
        npm publish platform_subpackage
```

**绑定层功能等价验证（REQ-FDI-003）**：

```pseudo
function verify_binding_equivalence():
    // 同一 CRUD 用例分别在绑定层与 sz-orm-core 执行
    test_cases = [create, read, update, delete, transaction, eager_load]

    for case in test_cases:
        core_result = sz_orm_core.execute(case)        // Rust 侧
        py_result = sz_orm_python.execute(case)         // Python 绑定
        js_result = sz_orm_js.execute(case)             // JS 绑定

        assert py_result == core_result                 // 行为等价
        assert js_result == core_result
        assert binding_uses_parameterized_query(case)   // 参数化路径（非裸 SQL）
```

### 2.3.5 依赖关系

| 依赖 | 类型 | 用途 | Feature 隔离 |
|------|------|------|-------------|
| `pyo3` 0.20 | 外部 crate（已有） | Python 绑定 | pyo3/extension-module |
| `napi-rs` 2 | 外部工具（已有） | JS 绑定 | napi build |
| `maturin` ≥1.0 | 构建工具 | Python wheel 构建 | 构建期 |
| `pytest` | 测试框架 | Python 等价性测试 | dev |
| `jest` | 测试框架 | JS 等价性测试 | dev |

**与现有包的关系**：sz-orm-python/js 代码已存在，仅补齐构建脚本、测试套件、发布流程，不修改绑定代码逻辑。

### 2.3.6 新增包规划

无新增包。新增文件：
- `scripts/build_python_wheel.ps1`、`scripts/publish_pypi.ps1`
- `scripts/build_napi.ps1`、`scripts/publish_npm.ps1`
- `scripts/verify_bindings.ps1`
- `packages/sz-orm-python/tests/`（pytest 套件）
- `packages/sz-orm-js/tests/`（jest 套件）
- CI 矩阵配置（GitHub Actions 三平台）

### 2.3.7 与现有代码的集成点

| 集成点 | 现有代码位置 | 集成方式 |
|--------|-------------|---------|
| PyO3 绑定 | [packages/sz-orm-python/src/lib.rs:16](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-python/src/lib.rs#L16) | 构建脚本编译此绑定 |
| maturin 配置 | [packages/sz-orm-python/pyproject.toml:1](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-python/pyproject.toml#L1) | 构建脚本使用此配置 |
| napi 配置 | [packages/sz-orm-js/package.json:1](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-js/package.json#L1) | 构建脚本使用此配置 |
| 参数化查询路径 | sz-orm-core query.rs | 绑定层复用（REQ-FDI-003 等价性验证） |
| crates.io 发布基线 | scripts/publish_crates_io.ps1 | 复用拓扑排序与门禁检查 |

### 2.3.8 风险与缓解

| 风险 | 等级 | 缓解措施 |
|------|------|---------|
| R-04: maturin/napi 跨平台 CI 矩阵成本 | 中 | 复用 GitHub Actions 矩阵，三平台并行构建；缓存 Cargo 编译产物 |
| pyo3 依赖编译错误 | 中 | 锁定 pyo3 0.20 版本，feature 隔离 pyo3/extension-module |
| 平台工具链缺失 | 中 | CI 镜像预装 Rust + Python + Node 工具链，交叉编译 target |
| 绑定层与 core 行为不一致 | 高 | pytest/jest 等价性测试套件覆盖 CRUD/事务/EagerLoad，任一失败阻断发布（REQ-FDI-005） |

---

## 2.4 AI 辅助查询优化器（REQ-AI-001~005）

### 2.4.1 模块目标

在 sz-orm-ai 内新增"LLM 查询计划优化建议"能力，与现有规则型 `QueryOptimizer` 并存互补：调用 LLM 服务（OpenAI 兼容 API）生成结构化优化建议，解析 EXPLAIN 计划纳入建议上下文，合并规则与 LLM 建议并标注来源，未配置 LLM 时自动降级纯规则引擎。

### 2.4.2 架构设计

```plantuml
@startuml
!theme plain
title sz-orm-ai 扩展架构

package "sz-orm-ai (扩展)" {
  rectangle "nl2sql.rs (存量)" as Exist {
    rectangle "QueryOptimizer (规则)" as RuleOpt
    rectangle "QueryOptimizationHint" as Hint
    rectangle "QueryAnalysis" as Analysis
    rectangle "Nl2SqlEngine trait" as Nl2Sql
  }

  rectangle "query_plan_optimizer.rs <<new>>\n(feature: llm-optimizer)" as Qpo {
    rectangle "UnifiedQueryOptimizer\n(合并规则+LLM)" as Unified
    rectangle "LlmOptimizer\n(LLM 建议引擎)" as LlmOpt
    rectangle "OptimizerConfig\n(API key/base/model/timeout)" as OptCfg
    rectangle "HintSource 枚举\n(Rule/Llm)" as HSource
  }

  rectangle "explain_parser.rs <<new>>\n(feature: llm-optimizer)" as Explain {
    rectangle "ExplainPlanParser trait" as ExpTrait
    rectangle "MySqlExplainParser" as MySqlExp
    rectangle "PgExplainParser" as PgExp
    rectangle "ExplainSignal 枚举\n(全表扫描/缺索引/排序/临时表)" as ExpSignal
  }

  rectangle "sql_sanitizer.rs <<new>>" as Sanitize {
    rectangle "SqlSanitizer\n(敏感字面量脱敏)" as San
  }

  rectangle "safety.rs (存量)" as Safety
  rectangle "real_embedding.rs (存量, real feature)" as RealEmb
}

cloud "LLM 服务 (OpenAI 兼容)" as LLM

Unified --> RuleOpt : 规则分析
Unified --> LlmOpt : LLM 分析
LlmOpt --> LLM : 请求优化建议
LlmOpt --> San : 发送前脱敏
LlmOpt --> Safety : 安全校验
Unified --> Hint : 合并建议
Hint --> HSource : 标注来源
LlmOpt --> Explain : 纳入 EXPLAIN 信号
Explain --> ExpTrait : 方言解析
ExpTrait --> MySqlExp
ExpTrait --> PgExp
OptCfg --> LlmOpt : 配置
RealEmb ..> LlmOpt : 复用 LLM 客户端模式

@enduml
```

### 2.4.3 关键数据结构

```rust
//! sz-orm-ai 扩展数据结构骨架

use serde::{Deserialize, Serialize};

/// 建议来源（REQ-AI-003）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HintSource {
    /// 规则引擎生成
    Rule,
    /// LLM 生成（含模型标识）
    Llm { model: String },
}

/// 优化建议严重级别（扩展现有 HintSeverity，向后兼容）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HintSeverity { Info, Warning, Critical }

/// 统一优化建议（扩展现有 QueryOptimizationHint，新增 source 字段）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedOptimizationHint {
    /// 建议标题
    pub title: String,
    /// 详细描述
    pub description: String,
    /// 严重级别
    pub severity: HintSeverity,
    /// 优化后的 SQL 建议（可选，仅建议不自动执行 REQ-AI-005）
    pub suggested_sql: Option<String>,
    /// 建议来源（REQ-AI-003）
    pub source: HintSource,
}

/// 统一查询分析结果（扩展 QueryAnalysis）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedQueryAnalysis {
    /// 原始 SQL
    pub original_sql: String,
    /// 所有优化建议（规则 + LLM 合并）
    pub hints: Vec<UnifiedOptimizationHint>,
    /// 复杂度评分
    pub complexity_score: u32,
    /// 检测信号（复用现有 QueryAnalysis 字段）
    pub detected_tables: Vec<String>,
    pub has_where: bool,
    pub has_limit: bool,
    pub has_join: bool,
    pub has_subquery: bool,
    pub uses_select_star: bool,
    /// EXPLAIN 计划信号（新增）
    pub explain_signals: Vec<ExplainSignal>,
    /// LLM 是否可用（新增，降级标注）
    pub llm_available: bool,
    /// LLM 降级原因（如有）
    pub llm_degraded_reason: Option<String>,
}

/// 优化器配置（REQ-AI-004）
#[derive(Debug, Clone)]
pub struct OptimizerConfig {
    /// OpenAI 兼容 API key（None 时降级纯规则）
    pub api_key: Option<String>,
    /// API base URL
    pub api_base: String,
    /// 模型名（如 "gpt-4o"）
    pub model: String,
    /// 请求超时（秒）
    pub timeout_secs: u64,
    /// 最大 token
    pub max_tokens: u32,
    /// 是否启用 LLM（false 时强制纯规则）
    pub enable_llm: bool,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            api_key: None,  // 默认无 LLM，降级纯规则
            api_base: "https://api.openai.com/v1".to_string(),
            model: "gpt-4o".to_string(),
            timeout_secs: 10,
            max_tokens: 2000,
            enable_llm: false,
        }
    }
}

/// EXPLAIN 计划信号（REQ-AI-002）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExplainSignal {
    /// 全表扫描
    FullTableScan { table: String },
    /// 缺失索引
    MissingIndex { table: String, column: String },
    /// 使用临时表
    UsingTempTable,
    /// 使用文件排序
    UsingFilesort,
    /// 索引扫描
    IndexScan { index: String },
}

/// EXPLAIN 计划解析 trait（REQ-AI-002）
pub trait ExplainPlanParser: Send + Sync {
    /// 解析 EXPLAIN 计划文本
    fn parse(&self, explain_text: &str) -> Result<Vec<ExplainSignal>, ExplainParseError>;
    /// 支持的方言
    fn dialect(&self) -> &'static str;
}

/// EXPLAIN 解析错误
#[derive(Debug, Clone, thiserror::Error)]
pub enum ExplainParseError {
    #[error("EXPLAIN 格式不匹配方言 {0}")]
    FormatMismatch(String),
    #[error("EXPLAIN 文本为空")]
    EmptyInput,
    #[error("解析失败: {0}")]
    ParseFailed(String),
}

/// 统一查询优化器（合并规则 + LLM）
pub struct UnifiedQueryOptimizer {
    /// 规则优化器（复用现有）
    rule_optimizer: QueryOptimizer,
    /// LLM 优化器（可选，None 时降级）
    llm_optimizer: Option<LlmOptimizer>,
    /// 配置
    config: OptimizerConfig,
}

/// LLM 优化建议引擎
pub struct LlmOptimizer {
    config: OptimizerConfig,
    // 复用 real_embedding.rs 的 reqwest 客户端模式
}
```

### 2.4.4 核心算法/流程

**统一优化建议生成流程（REQ-AI-001）**：

```pseudo
async function optimize(unified: UnifiedQueryOptimizer, sql, explain: Option<String>, schema) -> UnifiedQueryAnalysis:
    // 步骤 1: 规则分析（离线，始终执行）
    rule_analysis = unified.rule_optimizer.analyze(sql, schema)
    rule_hints = rule_analysis.hints.map(h => UnifiedOptimizationHint {
        ...h,
        source: HintSource::Rule,
    })

    // 步骤 2: EXPLAIN 计划解析（REQ-AI-002）
    explain_signals = []
    if let Some(explain_text) = explain:
        parser = select_parser_by_dialect(schema)
        match parser.parse(explain_text):
            Ok(signals) => explain_signals = signals
            Err(e) => explain_signals = []  // 解析失败不静默，标注"EXPLAIN 未解析"

    // 步骤 3: LLM 建议（REQ-AI-001/004）
    llm_hints = []
    llm_available = false
    llm_degraded_reason = None

    if unified.config.enable_llm and unified.config.api_key.is_some():
        try:
            // 脱敏 SQL（REQ-AI 安全性）
            sanitized_sql = SqlSanitizer.sanitize(sql)
            // 构造提示词（含 SQL + EXPLAIN 信号 + schema）
            prompt = build_prompt(sanitized_sql, explain_signals, schema)
            // 调用 LLM API
            llm_response = await unified.llm_optimizer.request(prompt)
            // 解析结构化响应（REQ-AI-003）
            llm_hints = parse_llm_hints(llm_response, unified.config.model)
            llm_available = true
        catch e:
            // 自动降级（REQ-AI-004）
            llm_degraded_reason = Some(e.to_string())
    else:
        llm_degraded_reason = Some("未配置 LLM API key")

    // 步骤 4: 合并建议（REQ-AI-003）
    all_hints = rule_hints + llm_hints  // 合并，标注来源

    // 步骤 5: 返回统一分析结果
    return UnifiedQueryAnalysis {
        original_sql: sql,
        hints: all_hints,
        complexity_score: rule_analysis.complexity_score,
        explain_signals,
        llm_available,
        llm_degraded_reason,
        ...
    }
```

**LLM SQL 零执行保证（REQ-AI-005）**：

```pseudo
// 编译期/运行期保证：LLM 重写 SQL 仅存在于建议结构中
function get_suggested_sql(hint: UnifiedOptimizationHint) -> Option<String>:
    return hint.suggested_sql  // 仅返回，不执行

// 系统零次执行 LLM 生成的 SQL：
// - UnifiedQueryOptimizer 无 execute_sql 方法
// - suggested_sql 字段为 Option<String>，仅展示用途
// - safety 模块校验 LLM 输出合法性但不执行
```

**SQL 脱敏算法（REQ-AI 安全性）**：

```pseudo
function sanitize(sql: &str) -> String:
    // 识别敏感字面量并替换为占位符
    patterns = [
        (regex"password\s*=\s*'[^']*'", "password='***'"),
        (regex"token\s*=\s*'[^']*'", "token='***'"),
        (regex"'[A-Za-z0-9+/]{40,}='", "'***'"),  // Base64 token
    ]
    result = sql
    for (pattern, replacement) in patterns:
        result = result.replace_all(pattern, replacement)
    return result
```

### 2.4.5 依赖关系

| 依赖 | 类型 | 用途 | Feature 隔离 |
|------|------|------|-------------|
| `reqwest` | 外部 crate（real feature 已有） | LLM API 调用 | "llm-optimizer" feature（依赖 real） |
| `sqlparser` | workspace dev-dependency（已有） | EXPLAIN 解析辅助 | "llm-optimizer" feature |
| `serde` / `serde_json` | workspace | 结构化建议序列化 | 默认 |
| `thiserror` | workspace | 错误类型 | 默认 |
| `tokio` | workspace | 异步 LLM 调用 | 默认 |

**与现有包的关系**：扩展现有 sz-orm-ai，复用 `QueryOptimizer`/`QueryOptimizationHint`/`QueryAnalysis`/`SchemaContext`/`safety` 模块/`real_embedding.rs` 的 reqwest 客户端模式，不修改现有公开 API（`QueryOptimizationHint` 新增 source 字段通过新类型 `UnifiedOptimizationHint` 实现，避免 Breaking Change）。

### 2.4.6 新增包规划

无新增包。在 `packages/sz-orm-ai` 内扩展：
- 新增 `src/query_plan_optimizer.rs`（feature "llm-optimizer"）
- 新增 `src/explain_parser.rs`（feature "llm-optimizer"）
- 新增 `src/sql_sanitizer.rs`
- 修改 `Cargo.toml` 新增 `llm-optimizer` feature
- 修改 `src/lib.rs` 导出新模块

### 2.4.7 与现有代码的集成点

| 集成点 | 现有代码位置 | 集成方式 |
|--------|-------------|---------|
| QueryOptimizer 复用 | [packages/sz-orm-ai/src/nl2sql.rs:1190](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-ai/src/nl2sql.rs#L1190) | UnifiedQueryOptimizer 内持有 QueryOptimizer |
| QueryOptimizationHint 复用 | [packages/sz-orm-ai/src/nl2sql.rs:1091](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-ai/src/nl2sql.rs#L1091) | UnifiedOptimizationHint 扩展（新类型，非修改） |
| safety 模块复用 | packages/sz-orm-ai/src/safety.rs | LLM 输出安全校验 |
| real_embedding 客户端模式 | packages/sz-orm-ai/src/real_embedding.rs | LLM API 客户端复用 reqwest 模式 |
| SchemaContext 复用 | [packages/sz-orm-ai/src/nl2sql.rs:32](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-ai/src/nl2sql.rs#L32) | LLM 提示词构造 |

### 2.4.8 风险与缓解

| 风险 | 等级 | 缓解措施 |
|------|------|---------|
| R-05: LLM 服务不可用/限流 | 中 | 自动降级规则引擎（REQ-AI-004），返回规则建议 + LLM 降级说明，不报错不阻塞 |
| R-06: LLM 生成 SQL 安全/正确性风险 | 高 | 建议零自动执行（REQ-AI-005）+ SQL 脱敏 + safety 模块校验 + 建议仅展示 |
| LLM 响应非结构化/非法 JSON | 中 | 逐条校验合法性，丢弃非法建议保留合法的，记录解析失败日志（不 panic） |
| AI 建议响应超 10s | 中 | OptimizerConfig.timeout_secs 默认 10s，超时降级规则引擎 |
| EXPLAIN 方言差异 | 中 | 各方言独立 ExplainPlanParser 实现，解析失败标注"未解析"不静默 |

---

## 2.5 多数据库事务一致性（REQ-DTX-001~005）

### 2.5.1 模块目标

在 sz-orm-dtx 内扩展 XA 资源管理器适配，复用现有 2PC 状态机与日志，实现跨数据库原子提交（XA PREPARE/COMMIT 直连 DB 资源管理器）、崩溃恢复（基于 TransactionLogStore 恢复未决事务）、悬挂事务检测（超时标记与补偿），与既有 2PC/Saga/TCC/cross_shard 模式共存。

### 2.5.2 架构设计

```plantuml
@startuml
!theme plain
title sz-orm-dtx XA 扩展架构

package "sz-orm-dtx (扩展)" {
  rectangle "lib.rs (存量)" as Exist {
    rectangle "DistributedTransaction (2PC)" as Dtx
    rectangle "DtxManager" as DtxMgr
    rectangle "TransactionLogStore" as LogStore
    rectangle "TransactionState 状态机" as StateMachine
    rectangle "TransactionParticipant (回调式)" as TPart
  }

  rectangle "saga/tcc/cross_shard (存量)" as Modes

  rectangle "xa.rs <<new>>\n(feature: xa)" as Xa {
    rectangle "XaResource trait\n(xa_prepare/commit/rollback)" as XaRes
    rectangle "XaParticipant\n(持有 AnyPool 连接)" as XaPart
    rectangle "XaCoordinator\n(XA 两阶段提交协调)" as XaCoord
    rectangle "XaCapabilityChecker\n(后端 XA 能力校验)" as XaCheck
  }

  rectangle "recovery.rs <<new>>\n(feature: xa)" as Recovery {
    rectangle "XaRecoveryCoordinator\n(崩溃恢复)" as XaRecover
    rectangle "RecoveryStrategy\n(Prepared→Commit / Preparing→Rollback)" as RecStrategy
  }

  rectangle "suspension.rs <<new>>\n(feature: xa)" as Susp {
    rectangle "SuspensionDetector\n(超时检测)" as SuspDet
    rectangle "SuspensionConfig\n(超时 + 策略)" as SuspCfg
    rectangle "SuspensionPolicy\n(Commit/Rollback)" as SuspPolicy
  }
}

database "DB-A (XA)" as DBA
database "DB-B (XA)" as DBB
rectangle "sz-orm-sqlx AnyPool" as AnyPool

XaPart --> AnyPool : 持有连接
XaPart --> XaRes : 实现
XaCoord --> XaPart : 协调参与者
XaCoord --> StateMachine : 复用 2PC 状态机
XaCoord --> LogStore : 写日志
XaCoord --> DBA : XA PREPARE/COMMIT
XaCoord --> DBB : XA PREPARE/COMMIT
XaCheck --> AnyPool : 检测后端能力
XaRecover --> LogStore : read_pending()
XaRecover --> RecStrategy
XaRecover --> XaCoord : 补偿执行
SuspDet --> LogStore : 扫描超时事务
SuspDet --> SuspPolicy
DtxMgr --> XaCoord : 统一管理 (共存)

@enduml
```

### 2.5.3 关键数据结构

```rust
//! sz-orm-dtx XA 扩展数据结构骨架

use std::sync::Arc;
use std::time::Duration;
use sz_orm_sqlx::any_driver::AnyBackend;

/// XA 资源管理器 trait（直连 DB 资源管理器）
#[async_trait::async_trait]
pub trait XaResource: Send + Sync {
    /// XA PREPARE（预提交）
    async fn xa_prepare(&self, xid: &str) -> Result<(), XaError>;
    /// XA COMMIT（提交）
    async fn xa_commit(&self, xid: &str) -> Result<(), XaError>;
    /// XA ROLLBACK（回滚）
    async fn xa_rollback(&self, xid: &str) -> Result<(), XaError>;
    /// 资源标识（DSN 脱敏哈希）
    fn resource_id(&self) -> &str;
    /// 后端类型
    fn backend(&self) -> AnyBackend;
}

/// XA 参与者（持有真实 DB 连接，非回调式）
pub struct XaParticipant {
    /// XA 资源管理器
    resource: Arc<dyn XaResource>,
    /// XA 事务分支 ID
    xid: String,
    /// 参与者状态（复用现有 ParticipantState）
    state: ParticipantState,
}

/// XA 协调器
pub struct XaCoordinator {
    /// 事务日志存储（复用现有 TransactionLogStore）
    log_store: Arc<dyn TransactionLogStore>,
    /// 悬挂检测配置
    suspension_config: SuspensionConfig,
}

/// XA 能力校验结果
#[derive(Debug, Clone)]
pub enum XaCapability {
    /// 支持 XA
    Supported,
    /// 不支持 XA（如 SQLite）
    NotSupported { reason: String },
}

/// XA 错误
#[derive(Debug, thiserror::Error)]
pub enum XaError {
    #[error("XA prepare 失败: {0}")]
    PrepareFailed(String),
    #[error("XA commit 失败: {0}")]
    CommitFailed(String),
    #[error("XA rollback 失败: {0}")]
    RollbackFailed(String),
    #[error("后端不支持 XA: {backend:?}（{reason}）")]
    XaNotSupported { backend: AnyBackend, reason: String },
    #[error("XA 事务 {0} 不存在")]
    NotFound(String),
    #[error("数据库错误: {0}")]
    DatabaseError(String),
}

/// 悬挂事务配置
#[derive(Debug, Clone)]
pub struct SuspensionConfig {
    /// 超时阈值（默认 30s）
    pub timeout: Duration,
    /// 超时处理策略
    pub policy: SuspensionPolicy,
    /// 检测间隔（后台扫描周期）
    pub check_interval: Duration,
}

impl Default for SuspensionConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            policy: SuspensionPolicy::Rollback,
            check_interval: Duration::from_secs(5),
        }
    }
}

/// 悬挂事务处理策略
#[derive(Debug, Clone, Copy)]
pub enum SuspensionPolicy {
    /// 超时后提交（假设 Prepare 成功即大概率可提交）
    Commit,
    /// 超时后回滚（保守策略）
    Rollback,
}

/// 悬挂事务记录
#[derive(Debug, Clone)]
pub struct SuspendedTransaction {
    pub tx_id: String,
    pub resource_id: String,
    pub suspended_at: chrono::DateTime<chrono::Utc>,
    pub policy: SuspensionPolicy,
}

/// 崩溃恢复协调器
pub struct XaRecoveryCoordinator {
    log_store: Arc<dyn TransactionLogStore>,
    xa_coordinator: Arc<XaCoordinator>,
}

/// 恢复策略
#[derive(Debug, Clone, Copy)]
pub enum RecoveryStrategy {
    /// 已 Prepare → 继续 Commit
    CommitPrepared,
    /// Preparing 中（未完成）→ Rollback
    RollbackPreparing,
    /// 已 Committing → 检查并补全
    CompleteCommitting,
}
```

### 2.5.4 核心算法/流程

**XA 两阶段提交流程（REQ-DTX-001）**：

```pseudo
async function xa_two_phase_commit(coord: XaCoordinator, tx_id: String, participants: Vec<XaParticipant>) -> Result<(), XaError>:
    // 步骤 1: XA 能力校验（REQ-DTX-005）
    for p in participants:
        capability = XaCapabilityChecker.check(p.resource.backend())
        if capability is NotSupported:
            return Err(XaError::XaNotSupported { ... })  // 拒绝注册，事务不进入 Prepare

    // 步骤 2: 阶段一 - Prepare（REQ-DTX-001）
    coord.write_log(tx_id, "Preparing", participants)
    prepared = []
    for p in participants:
        match await p.resource.xa_prepare(p.xid):
            Ok(()) => prepared.push(p)
            Err(e) =>
                // 任一失败 → 全局回滚
                for prepared_p in prepared:
                    await prepared_p.resource.xa_rollback(prepared_p.xid)
                coord.write_log(tx_id, "Failed", participants)
                return Err(XaError::PrepareFailed(...))

    coord.write_log(tx_id, "Prepared", participants)

    // 步骤 3: 阶段二 - Commit
    coord.write_log(tx_id, "Committing", participants)
    for p in participants:
        match await p.resource.xa_commit(p.xid):
            Ok(()) => continue
            Err(e) =>
                // Commit 失败 → 标记悬挂（无法回滚已 Prepare 的）
                coord.mark_suspended(tx_id, p.resource_id)
                coord.write_log(tx_id, "Failed", participants)
                return Err(XaError::CommitFailed(...))

    coord.write_log(tx_id, "Committed", participants)
    return Ok(())
```

**崩溃恢复流程（REQ-DTX-002）**：

```pseudo
async function recover(coord: XaRecoveryCoordinator) -> Result<Vec<RecoveryResult>, XaError>:
    // 步骤 1: 扫描未决事务
    pending = await coord.log_store.read_pending()

    results = []
    for tx in pending:
        // 步骤 2: 根据日志状态决定恢复策略
        strategy = match tx.state:
            "Prepared" => RecoveryStrategy::CommitPrepared      // 已 Prepare → 继续 Commit
            "Preparing" => RecoveryStrategy::RollbackPreparing   // Preparing 中 → Rollback
            "Committing" => RecoveryStrategy::CompleteCommitting // Committing → 检查补全
            _ => continue

        // 步骤 3: 执行补偿
        match strategy:
            CommitPrepared =>
                for p in tx.participants:
                    await coord.xa_commit(p.xid)  // 继续提交
                coord.write_log(tx.tx_id, "Committed", ...)
            RollbackPreparing =>
                for p in tx.participants:
                    await coord.xa_rollback(p.xid)  // 回滚
                coord.write_log(tx.tx_id, "RolledBack", ...)

        results.push(RecoveryResult { tx_id: tx.tx_id, strategy, success: true })

    return results  // 所有未决事务收敛到终态
```

**悬挂事务检测流程（REQ-DTX-003）**：

```pseudo
async fn run_suspension_detector(detector: SuspensionDetector):
    loop:
        sleep(detector.config.check_interval)

        // 扫描 Prepare 后超时未决定的事务
        pending = await detector.log_store.read_pending()
        now = current_time()

        for tx in pending:
            if tx.state == "Prepared" or tx.state == "Committing":
                elapsed = now - tx.timestamp
                if elapsed > detector.config.timeout:
                    // 标记悬挂
                    detector.mark_suspended(tx.tx_id, tx.resource_id)

                    // 按策略处理
                    match detector.config.policy:
                        Commit => await detector.xa_commit(tx)
                        Rollback => await detector.xa_rollback(tx)

                    // 收敛到终态
                    detector.write_log(tx.tx_id, "Suspended-Resolved", ...)
```

**XA 能力校验（REQ-DTX-005）**：

```pseudo
function check_xa_capability(backend: AnyBackend) -> XaCapability:
    match backend:
        AnyBackend::MySql => Supported        // MySQL InnoDB 支持 XA
        AnyBackend::Postgres => Supported     // PostgreSQL 支持 prepared transactions
        AnyBackend::Oracle => Supported       // Oracle 支持 XA
        AnyBackend::Mssql => Supported        // MSSQL 支持分布式事务
        AnyBackend::Sqlite => NotSupported {   // SQLite 不支持 XA
            reason: "SQLite 不支持 XA 协议"
        }
```

### 2.5.5 依赖关系

| 依赖 | 类型 | 用途 | Feature 隔离 |
|------|------|------|-------------|
| `sz-orm-sqlx` | workspace | AnyPool 连接（XA 参与者持有） | "xa" feature |
| `sz-orm-core` | workspace | Connection/DbError 类型 | "xa" feature |
| `async-trait` | workspace | XaResource trait | "xa" feature |
| `tokio` | workspace | 后台悬挂检测任务 | "xa" feature |
| `chrono` | workspace | 超时计算 | "xa" feature |
| `thiserror` | workspace | XaError 派生 | "xa" feature |

**与现有包的关系**：扩展现有 sz-orm-dtx，复用 `DistributedTransaction`/`DtxManager`/`TransactionLogStore`/`TransactionState`/`ParticipantState`/`TransactionLogEntry`，不修改现有公开 API。XA 参与者（`XaParticipant`）与回调式参与者（`TransactionParticipant`）并存，通过 `DtxManager` 统一管理。

### 2.5.6 新增包规划

无新增包。在 `packages/sz-orm-dtx` 内扩展：
- 新增 `src/xa.rs`（feature "xa"）
- 新增 `src/recovery.rs`（feature "xa"）
- 新增 `src/suspension.rs`（feature "xa"）
- 修改 `Cargo.toml` 新增 `xa` feature 与依赖
- 修改 `src/lib.rs` 导出新模块

### 2.5.7 与现有代码的集成点

| 集成点 | 现有代码位置 | 集成方式 |
|--------|-------------|---------|
| 2PC 状态机复用 | [packages/sz-orm-dtx/src/lib.rs:151](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-dtx/src/lib.rs#L151) | XaCoordinator 复用 TransactionState 状态流转 |
| TransactionLogStore 复用 | [packages/sz-orm-dtx/src/lib.rs:45](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-dtx/src/lib.rs#L45) | XaCoordinator/RecoveryCoordinator 写日志与读未决 |
| DtxManager 共存 | [packages/sz-orm-dtx/src/lib.rs:420](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-dtx/src/lib.rs#L420) | XA 事务通过 DtxManager 统一管理（REQ-DTX-004） |
| TransactionLogEntry 复用 | [packages/sz-orm-dtx/src/lib.rs:29](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-dtx/src/lib.rs#L29) | XA 日志条目复用现有结构 |
| AnyPool 连接 | [packages/sz-orm-sqlx/src/any_driver.rs:129](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L129) | XaParticipant 持有 AnyPool 连接执行 XA SQL |
| AnyBackend 能力校验 | [packages/sz-orm-sqlx/src/any_driver.rs:57](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L57) | XaCapabilityChecker 检测后端类型 |

### 2.5.8 风险与缓解

| 风险 | 等级 | 缓解措施 |
|------|------|---------|
| R-07: XA 协议在不同数据库的实现差异 | 高 | 仅对明确支持 XA 的库开放（MySQL/PG/Oracle/MSSQL），逐库集成测试；SQLite 明确拒绝 |
| R-08: 分布式事务性能开销 | 中 | 超时与重试可配置（SuspensionConfig），提供性能基准；Prepare/Commit 网络往返可并行化 |
| 协调者崩溃后日志不完整 | 高 | TransactionLogStore 在 Prepare/Commit 各阶段落日志（复用现有 write_log），恢复时按日志状态收敛 |
| Commit 阶段部分失败（不可回滚） | 高 | 标记悬挂 + 审计日志 + 人工介入或按策略补偿；这是 2PC 固有局限，文档明确标注 |
| 后台悬挂检测任务泄漏 | 中 | tokio 任务用 CancellationToken 控制，优雅关闭时停止检测 |

---

## 2.6 多后端透明适配协同（REQ-MB-001~004）

### 2.6.1 模块目标

提供"多后端能力就绪清单"验证文档（附 file:line 证据），证明 sz-orm 上游已满足 sz-rust P2-1 启动条件；复用 v2.4.0 五方言等价性测试基础设施验证行为一致性；提供方言约束文档与协同示例。透明适配层代码由 sz-rust 侧实现（ADR-0001），本模块仅提供上游就绪验证与协同接口约束。

### 2.6.2 架构设计

```plantuml
@startuml
!theme plain
title 多后端协同架构（文档+验证）

package "sz-orm 上游 (已就绪)" as Upstream {
  rectangle "sz-orm-sqlx\n(AnyBackend/AnyPool/UnifiedPool)" as Sqlx
  rectangle "sz-orm-core\n(查询/模型/池/事务/EagerLoad)" as Core
  rectangle "五方言集成测试\n(v2.4.0 已交付)" as Tests
}

package "协同交付物 (文档)" as Coop {
  rectangle "multi_backend_readiness.md <<new>>\n(就绪清单 + file:line 证据)" as Readiness
  rectangle "dialect_constraints.md <<new>>\n(方言特性支持矩阵)" as Constraints
  rectangle "sz_rust_integration_example.rs <<new>>\n(协同示例)" as Example
}

rectangle "sz-rust 透明适配层\n(下游实现, ADR-0001)" as SzRust
database "MySQL/PG/SQLite/Oracle/MSSQL" as Rdb

Sqlx --> Rdb : 5 后端统一访问
Readiness --> Sqlx : 验证 AnyBackend/AnyPool/UnifiedPool
Readiness --> Core : 验证查询/模型/池/事务
Constraints --> Sqlx : 方言特性支持范围
Example --> Sqlx : 仅依赖公开 API
SzRust --> Sqlx : 透明适配 (仅公开 API)
SzRust ..> Readiness : 参考就绪清单

@enduml
```

### 2.6.3 关键数据结构

本模块为文档+验证，无新增 Rust 数据结构。核心文档结构：

```rust
//! 协同文档结构骨架（Markdown，非 Rust 类型）

// multi_backend_readiness.md 就绪清单结构
struct MultiBackendReadinessDoc {
    // 1. AnyBackend 五方言枚举
    any_backend_enum: VerificationItem {
        capability: "AnyBackend 枚举（MySql/Postgres/Sqlite/Oracle/Mssql）",
        evidence: "packages/sz-orm-sqlx/src/any_driver.rs:57",
        verified: true,
    },
    // 2. from_dsn() DSN 识别
    from_dsn: VerificationItem {
        capability: "AnyBackend::from_dsn() 自动识别 5 scheme",
        evidence: "packages/sz-orm-sqlx/src/any_driver.rs:80",
        verified: true,
    },
    // 3. dialect() 方言映射
    dialect_mapping: VerificationItem {
        capability: "AnyBackend::dialect() 返回对应 Dialect",
        evidence: "packages/sz-orm-sqlx/src/any_driver.rs:117",
        verified: true,
    },
    // 4. AnyPool 连接工厂
    any_pool: VerificationItem {
        capability: "AnyPool::connect(dsn) 后端无关连接",
        evidence: "packages/sz-orm-sqlx/src/any_driver.rs:129",
        verified: true,
    },
    // 5. UnifiedPool 统一连接池
    unified_pool: VerificationItem {
        capability: "UnifiedPool 5 后端透明切换",
        evidence: "packages/sz-orm-sqlx/src/unified_pool.rs:48",
        verified: true,
    },
}

// dialect_constraints.md 方言特性支持矩阵
struct DialectConstraintMatrix {
    features: [
        ("CRUD", [MySql, Pg, Sqlite, Oracle, Mssql], "全部支持"),
        ("事务", [MySql, Pg, Sqlite, Oracle, Mssql], "全部支持"),
        ("Eager Loading", [MySql, Pg, Sqlite, Oracle, Mssql], "全部支持"),
        ("ON DUPLICATE KEY UPDATE", [MySql], "MySQL 专属"),
        ("RETURNING", [Pg, Oracle, Mssql], "PG/Oracle/MSSQL 支持，MySQL 需模拟"),
        ("SERIAL/AUTOINCREMENT", [MySql, Pg, Sqlite, Mssql], "各方言自增语法不同"),
    ],
}
```

### 2.6.4 核心算法/流程

**上游就绪验证流程（REQ-MB-001）**：

```pseudo
function verify_multi_backend_readiness() -> ReadinessReport:
    report = ReadinessReport::new()

    // 逐项验证公开 API 可调用并产出预期结果
    // 1. AnyBackend 枚举
    backends = [MySql, Postgres, Sqlite, Oracle, Mssql]
    assert AnyBackend::variants() == backends
    report.add("AnyBackend 枚举", "any_driver.rs:57", PASS)

    // 2. from_dsn() 识别
    for (dsn, expected) in [("mysql://...", MySql), ("postgres://...", Postgres), ...]:
        assert AnyBackend::from_dsn(dsn) == Ok(expected)
    report.add("from_dsn() DSN 识别", "any_driver.rs:80", PASS)

    // 3. dialect() 映射
    for backend in backends:
        dialect = backend.dialect()
        assert dialect is correct type
    report.add("dialect() 方言映射", "any_driver.rs:117", PASS)

    // 4. AnyPool/UnifiedPool（需真实 DB 连接，#[ignore] 标注）
    // ... 验证连接工厂与统一连接池

    return report  // 附 file:line 证据
```

**五方言行为一致性验证（REQ-MB-002）**：

```pseudo
function verify_five_dialect_equivalence():
    // 复用 v2.4.0 tests/common/equivalence.rs 基础设施
    test_cases = [
        crud_create, crud_read, crud_update, crud_delete,
        transaction_commit, transaction_rollback,
        eager_load_has_one, eager_load_has_many, eager_load_many_to_many,
    ]

    for case in test_cases:
        results = {}
        for dialect in [MySql, Postgres, Sqlite, Oracle, Mssql]:
            results[dialect] = execute_case(case, dialect)  // #[ignore] 真实连接

        // 断言五方言结果等价
        assert all_equal(results.values())
        // 不支持的方言特性有明确文档标注
```

### 2.6.5 依赖关系

| 依赖 | 类型 | 用途 |
|------|------|------|
| sz-orm-sqlx 公开 API | 现有 | 验证 AnyBackend/AnyPool/UnifiedPool |
| sz-orm-core 公开 API | 现有 | 验证查询/模型/池/事务 |
| v2.4.0 等价性测试基础设施 | 现有 | 复用 tests/common/equivalence.rs |

**与现有包的关系**：无代码修改，仅新增文档与示例文件。复用 v2.4.0 已交付的五方言集成测试基础设施。

### 2.6.6 新增包规划

无新增包。新增文件：
- `docs/spec/v3.0.0/multi_backend_readiness.md`（就绪清单文档）
- `docs/spec/v3.0.0/dialect_constraints.md`（方言特性支持矩阵）
- `examples/sz_rust_integration_example.rs`（协同示例，展示仅依赖公开 API）

### 2.6.7 与现有代码的集成点

| 集成点 | 现有代码位置 | 集成方式 |
|--------|-------------|---------|
| AnyBackend 验证 | [packages/sz-orm-sqlx/src/any_driver.rs:57](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L57) | 就绪清单引用 file:line 证据 |
| AnyPool 验证 | [packages/sz-orm-sqlx/src/any_driver.rs:129](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L129) | 就绪清单引用 |
| UnifiedPool 验证 | [packages/sz-orm-sqlx/src/unified_pool.rs:48](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/unified_pool.rs#L48) | 就绪清单引用 |
| 等价性测试复用 | packages/sz-orm-core/tests/common/equivalence.rs | 五方言行为一致性验证复用 |
| sz-rust 路线图 | `E:\vue\test\鲜视达\rust\sz-rust\docs\roadmap.md:102` | P2-1 协同需求引用 |

### 2.6.8 风险与缓解

| 风险 | 等级 | 缓解措施 |
|------|------|---------|
| R-09: sz-rust 适配层方言差异处理成本 | 中 | 五方言等价性测试 + 方言约束文档明确标注支持范围 |
| 上游 API 变更导致适配层编译失败 | 低 | sz-orm 保持向后兼容承诺（C-05 无 Breaking Change），理论上不发生；发生时由 sz-rust 侧修复 |
| 方言专属特性隐式假设 | 中 | dialect_constraints.md 明确标注各方言特性支持范围，诊断输出约束提示（REQ-MB-004） |

---

## 2.7 跨模块集成设计

### 2.7.1 6 大模块交互关系

v3.0.0 的 6 大模块在 workspace 内的交互关系如下。模块间通过"公开 API 依赖"与"共享基础设施复用"两种方式集成，无循环依赖。

```plantuml
@startuml
!theme plain
title v3.0.0 6 大模块跨模块集成

rectangle "sz-orm-core (核心, 不变)" as Core
rectangle "sz-orm-sqlx (5 后端, 不变)" as Sqlx

rectangle "模块1\nsz-orm-graph\n(图数据库)" as M1
rectangle "模块2\nsz-orm-wasm\n(WASM)" as M2
rectangle "模块3\nsz-orm-python/js\n(发布产物)" as M3
rectangle "模块4\nsz-orm-ai\n(AI 优化器)" as M4
rectangle "模块5\nsz-orm-dtx\n(XA 事务)" as M5
rectangle "模块6\n协同文档\n(多后端)" as M6

' 依赖关系（实线 = 编译期依赖，虚线 = 设计参考/文档引用）
M1 ..> Core : 参考Pool模式 (非类型依赖)
M2 --> Core : 复用 WasmDatabase (同包扩展)
M3 --> Core : 参数化查询路径
M3 --> Sqlx : AnyPool (绑定层导出)
M4 --> Core : 复用 QueryOptimizationHint (同包扩展)
M5 --> Sqlx : AnyPool 连接 (XA 参与者)
M5 --> Core : Connection/DbError 类型
M6 ..> Sqlx : 文档引用 file:line
M6 ..> Core : 文档引用 file:line

' 共享基础设施
rectangle "共享基础设施" as Shared {
  rectangle "Pool 抽象 (sz-orm-core)" as SharedPool
  rectangle "Dialect (sz-orm-core)" as SharedDialect
  rectangle "DbError (sz-orm-core)" as SharedError
  rectangle "serde 序列化" as SharedSerde
  rectangle "tokio 异步运行时" as SharedTokio
}

Core --> Shared
M1 --> SharedSerde
M1 --> SharedTokio
M4 --> SharedTokio
M5 --> SharedTokio

note bottom of Shared
  **共享原则**：
  1. 核心基础设施（Pool/Dialect/DbError）在 sz-orm-core，不变
  2. 新增模块复用基础设施，不修改基础设施
  3. 模块间无直接依赖（通过 core/sqlx 间接共享）
  4. 无循环依赖（拓扑序可排序）
end note

@enduml
```

**模块间依赖矩阵**（行依赖列）：

| 依赖方 ↓ \ 被依赖方 → | core | sqlx | graph | wasm | ai | dtx | python/js | 协同文档 |
|------------------------|------|------|-------|------|----|-----|-----------|---------|
| sz-orm-core | - | - | - | - | - | - | - | - |
| sz-orm-sqlx | ✓ | - | - | - | - | - | - | - |
| sz-orm-graph | 参考 | - | - | - | - | - | - | - |
| sz-orm-wasm | ✓（同包） | - | - | - | - | - | - | - |
| sz-orm-ai | ✓（同包） | - | - | - | - | - | - | - |
| sz-orm-dtx | ✓ | ✓ | - | - | - | - | - | - |
| sz-orm-python/js | ✓ | ✓ | - | - | - | - | - | - |
| 协同文档 | 引用 | 引用 | - | - | - | - | - | - |

**结论**：无循环依赖，拓扑序为 core → sqlx → {graph, wasm, ai, dtx, python/js} → 协同文档。

### 2.7.2 共享基础设施

| 基础设施 | 位置 | 复用模块 | 复用方式 |
|---------|------|---------|---------|
| Pool/PoolConfig 抽象模式 | sz-orm-core/src/pool.rs | 模块1（graph） | 设计模式参考（非类型依赖，graph 自行实现图连接池） |
| AnyBackend/AnyPool/UnifiedPool | sz-orm-sqlx/src/any_driver.rs | 模块5（dtx）、模块3（python/js）、模块6（协同） | 公开 API 依赖（XA 参与者持有 AnyPool；绑定层导出；文档引用） |
| Dialect 五方言 | sz-orm-core/src/dialect.rs | 模块6（协同） | 文档引用（方言约束矩阵） |
| DbError 错误类型 | sz-orm-core | 模块5（dtx） | 类型依赖（XaError 转换） |
| QueryOptimizationHint/QueryAnalysis | sz-orm-ai/src/nl2sql.rs | 模块4（ai，同包扩展） | 同包复用（UnifiedOptimizationHint 扩展） |
| 2PC 状态机/TransactionLogStore | sz-orm-dtx/src/lib.rs | 模块5（dtx，同包扩展） | 同包复用（XaCoordinator 复用状态机与日志） |
| WasmDatabase/MemoryConfig | sz-orm-wasm/src/{lib,advanced}.rs | 模块2（wasm，同包扩展） | 同包复用（js_bindings 委托 WasmDatabase） |
| serde/serde_json 序列化 | workspace.dependencies | 模块1/2/4/5 | 序列化复用（图结果/WASM 快照/AI 建议/XA 日志） |
| tokio 异步运行时 | workspace.dependencies | 模块1/4/5 | 异步复用（图查询/LLM 调用/XA 提交/悬挂检测） |
| 参数化查询铁律 | AGENTS.md C-03 | 全部模块 | 约束复用（Cypher 参数化/绑定层参数化/XA SQL 参数化） |

### 2.7.3 统一错误处理策略

v3.0.0 各模块错误类型独立，但遵循统一策略：

| 模块 | 错误类型 | 基础策略 | 脱敏要求 |
|------|---------|---------|---------|
| sz-orm-graph | `GraphError` | thiserror 派生，5 变体（Connection/Query/Mapping/SqlNotSupported/Parameterization） | DSN 脱敏（ConnectionError） |
| sz-orm-wasm | `WasmPersistenceError` | thiserror 派生，4 变体（Unavailable/Restore/IndexedDb/Serialization） | 无敏感信息 |
| sz-orm-ai | `ExplainParseError` + 复用 `Nl2SqlError` | thiserror 派生，降级不报错 | SQL 脱敏（发送 LLM 前） |
| sz-orm-dtx | `XaError` | thiserror 派生，6 变体（Prepare/Commit/Rollback/NotSupported/NotFound/Database） | DSN 脱敏哈希（resource_id） |
| sz-orm-python/js | 复用 `PyDbError`/`JsDbError` | 绑定层转换 sz-orm-core DbError | 复用 core 脱敏 |

**统一原则**：
1. 所有错误类型使用 `thiserror` 派生，实现 `std::error::Error`
2. 连接错误脱敏（DSN/密码不泄露在错误消息）
3. 错误消息含足够诊断信息（失败原因 + 上下文）
4. 不 panic（所有可恢复错误返回 `Result`）
5. AI 模块降级不报错（LLM 不可用时返回规则建议 + 降级说明）

---

## 2.8 发布与版本策略

### 2.8.1 新增包版本规划

| 包名 | 当前版本 | v3.0.0 版本 | 版本线策略 | 理由 |
|------|---------|------------|-----------|------|
| sz-orm-graph | 不存在 | `0.1.0` | 独立版本线 | 全新包，API 尚未稳定，0.x 表示预览 |
| sz-orm-wasm | 2.3.0 | `2.4.0`（或跟随 workspace） | 跟随 workspace | 扩展现有包，无 Breaking Change |
| sz-orm-ai | 2.3.0 | `2.4.0`（或跟随 workspace） | 跟随 workspace | 扩展现有包，无 Breaking Change |
| sz-orm-dtx | 2.3.0 | `2.4.0`（或跟随 workspace） | 跟随 workspace | 扩展现有包，无 Breaking Change |
| sz-orm-python | 0.1.0 | `0.1.0`（首次发布） | 独立版本线 | FFI 绑定独立制品，与 Rust crate 版本解耦 |
| sz-orm-js | 0.1.0 | `0.1.0`（首次发布） | 独立版本线 | FFI 绑定独立制品 |
| sz-orm-core | 2.3.0 | `2.3.0`（不变） | 跟随 workspace | 不修改，无 Breaking Change |
| sz-orm-sqlx | 2.3.0 | `2.3.0`（不变） | 跟随 workspace | 不修改 |

**workspace 版本策略**：v3.0.0 不提升 workspace.package.version（保持 2.3.0），因为核心包无 Breaking Change。新增包（sz-orm-graph）独立 0.1.0 线。扩展包（ai/dtx/wasm）如需发布新版本，按语义化版本升 patch（2.3.0 → 2.3.1）或 minor（2.3.0 → 2.4.0），取决于新增 feature 是否为向后兼容的新能力。

### 2.8.2 crates.io 发布顺序（拓扑排序）

基于依赖矩阵（§2.7.1），发布拓扑序如下：

```plantuml
@startuml
!theme plain
title v3.0.0 crates.io 发布拓扑序

rectangle "第1层 (无依赖)" as L1 {
  rectangle "sz-orm-core" as P1
  rectangle "sz-orm-macros" as P1b
}
rectangle "第2层 (依赖 core)" as L2 {
  rectangle "sz-orm-sqlx" as P2
  rectangle "sz-orm-graph (0.1.0 新)" as P2b
}
rectangle "第3层 (依赖 core/sqlx)" as L3 {
  rectangle "sz-orm-ai (扩展)" as P3a
  rectangle "sz-orm-dtx (扩展)" as P3b
  rectangle "sz-orm-wasm (扩展)" as P3c
}
rectangle "第4层 (FFI 绑定)" as L4 {
  rectangle "sz-orm-python (0.1.0)" as P4a
  rectangle "sz-orm-js (0.1.0)" as P4b
}

P2 --> P1
P2b ..> P1 : 参考(非依赖)
P3a --> P1
P3b --> P1
P3b --> P2
P3c --> P1
P4a --> P1
P4a --> P2
P4b --> P1
P4b --> P2

@enduml
```

**发布顺序**：
1. **第1层**：sz-orm-core、sz-orm-macros（无依赖，已发布 2.3.0，不变）
2. **第2层**：sz-orm-sqlx（已发布 2.3.0，不变）、**sz-orm-graph 0.1.0（新增发布）**
3. **第3层**：sz-orm-ai 2.4.0（扩展发布）、sz-orm-dtx 2.4.0（扩展发布）、sz-orm-wasm 2.4.0（扩展发布）
4. **第4层**：sz-orm-python 0.1.0（首次发布）、sz-orm-js 0.1.0（首次发布）

**注意**：FFI 绑定（python/js）的 PyPI/npm 发布独立于 crates.io，走 maturin/napi 流水线。

### 2.8.3 Breaking Change 评估

| 变更项 | 类型 | Breaking? | 影响范围 | 缓解措施 |
|--------|------|-----------|---------|---------|
| 新增 sz-orm-graph 包 | 新增包 | 否 | 无（独立包） | 无需缓解 |
| sz-orm-ai 新增 query_plan_optimizer 模块 | 新增模块 | 否 | 无（feature gate 隔离） | `llm-optimizer` feature 默认关闭 |
| sz-orm-ai 新增 UnifiedOptimizationHint 类型 | 新增类型 | 否 | 无（新类型，非修改现有） | 现有 QueryOptimizationHint 不变 |
| sz-orm-dtx 新增 xa/recovery/suspension 模块 | 新增模块 | 否 | 无（feature gate 隔离） | `xa` feature 默认关闭 |
| sz-orm-dtx 新增 XaResource trait/XaParticipant | 新增类型 | 否 | 无（新类型） | 现有 TransactionParticipant 不变 |
| sz-orm-wasm 新增 js_bindings/persistence 模块 | 新增模块 | 否 | 无（feature gate 隔离） | `js`/`persistence` feature 默认关闭 |
| sz-orm-wasm 新增 wasm-bindgen 依赖 | 新增依赖 | 否 | 无（feature gate） | 默认 feature 不引入 |
| sz-orm-python/js 补齐发布流程 | 流程变更 | 否 | 无（代码不变） | 仅新增构建脚本与测试 |
| workspace 新增 sz-orm-graph 成员 | workspace 变更 | 否 | 无（新增成员不影响现有） | 无需缓解 |

**结论**：v3.0.0 **无 Breaking Change**。所有新增能力通过 feature gate 隔离，默认 feature 不引入额外依赖。现有公开 API 签名保持完全向后兼容（满足 spec §4.5 兼容性约束 C-05）。

---

## 2.9 实现优先级与里程碑

### 2.9.1 实现顺序（依赖关系驱动）

基于 spec.md 优先级声明（"多库事务(5) → 发布产物(3) → WASM(2) → 图数据库(1) → AI 优化器(4)"的收益/风险序）与依赖关系，实现顺序如下：

| 顺序 | 模块 | 理由 |
|------|------|------|
| 1 | 模块6：多后端协同文档（REQ-MB） | 成本最低（仅文档），且为 sz-rust P2-1 提供上游就绪证明，解锁下游 |
| 2 | 模块5：XA 事务一致性（REQ-DTX） | sz-orm-dtx 已有 2PC/Saga/TCC 基础，XA 增强是自然延伸，生产价值高 |
| 3 | 模块3：发布产物（REQ-FDI） | 代码已存在（sz-orm-python/js），仅需打包发布流程，成本最低 |
| 4 | 模块2：WASM 完善（REQ-WASM） | sz-orm-wasm 已存在，补齐浏览器端能力 |
| 5 | 模块1：图数据库支持（REQ-GDB） | 全新领域，需新驱动，风险较高 |
| 6 | 模块4：AI 优化器（REQ-AI） | 依赖 LLM 服务，创新探索性质，放最后 |

### 2.9.2 里程碑划分

```plantuml
@startuml
!theme plain
title v3.0.0 里程碑划分

rectangle "M1: 多后端协同文档" as M1 {
  rectangle "REQ-MB-001~004"
  rectangle "就绪清单 + 方言约束 + 协同示例"
}
rectangle "M2: XA 事务一致性" as M2 {
  rectangle "REQ-DTX-001~005"
  rectangle "XaResource + XaCoordinator + 恢复 + 悬挂"
}
rectangle "M3: 发布产物" as M3 {
  rectangle "REQ-FDI-001~005"
  rectangle "maturin wheel + napi npm + 等价性测试"
}
rectangle "M4: WASM 完善" as M4 {
  rectangle "REQ-WASM-001~005"
  rectangle "wasm32 编译 + bindgen + IndexedDB"
}
rectangle "M5: 图数据库支持" as M5 {
  rectangle "REQ-GDB-001~005"
  rectangle "sz-orm-graph 包 + Neo4j + Cypher"
}
rectangle "M6: AI 优化器" as M6 {
  rectangle "REQ-AI-001~005"
  rectangle "LLM 优化器 + EXPLAIN 解析 + 降级"
}
rectangle "M7: 集成验证 + 发布" as M7 {
  rectangle "全 workspace 门禁"
  rectangle "crates.io 发布 + 下游回归"
}

M1 --> M2 --> M3 --> M4 --> M5 --> M6 --> M7

@enduml
```

### 2.9.3 每个里程碑的交付物

| 里程碑 | 需求组 | 交付物 | 验收标准 | 预计周期 |
|--------|--------|--------|---------|---------|
| **M1: 多后端协同文档** | REQ-MB-001~004 | `multi_backend_readiness.md`（就绪清单 + file:line 证据）+ `dialect_constraints.md`（方言特性矩阵）+ `sz_rust_integration_example.rs`（协同示例） | AC-MB-1~4：就绪清单逐项验证、五方言等价测试、适配层仅依赖公开 API、方言约束提示 | 1 周 |
| **M2: XA 事务一致性** | REQ-DTX-001~005 | `sz-orm-dtx/src/xa.rs`（XaResource/XaParticipant/XaCoordinator）+ `recovery.rs`（XaRecoveryCoordinator）+ `suspension.rs`（SuspensionDetector）+ 集成测试 | AC-DTX-1~5：两库 XA 全有全无、崩溃恢复收敛、悬挂超时处理、与 2PC/Saga/TCC 共存、不支持 XA 拒绝 | 3 周 |
| **M3: 发布产物** | REQ-FDI-001~005 | `scripts/build_python_wheel.ps1` + `build_napi.ps1` + `publish_pypi.ps1` + `publish_npm.ps1` + `verify_bindings.ps1` + pytest/jest 测试套件 + CI 矩阵 | AC-FDI-1~5：maturin wheel + pip install、napi 三平台 + npm install、绑定层等价、异步能力、未验证阻断发布 | 2 周 |
| **M4: WASM 完善** | REQ-WASM-001~005 | `sz-orm-wasm/src/js_bindings.rs`（wasm-bindgen 导出）+ `persistence.rs`（IndexedDB）+ `error.rs` + `.cargo/config.toml`（wasm32）+ wasm-bindgen-test | AC-WASM-1~5：wasm32 编译 gzip ≤1MB、bindgen 绑定 + .d.ts、IndexedDB 持久化恢复、内存超限拒绝、不可用明确报告 | 3 周 |
| **M5: 图数据库支持** | REQ-GDB-001~005 | `packages/sz-orm-graph/`（新包：connection/query/model/mapping/error/validator）+ Docker Neo4j 测试环境 + 集成测试 | AC-GDB-1~5：Neo4j 连接、参数化 Cypher、类型化映射、声明式建模、SQL 透传拒绝 | 4 周 |
| **M6: AI 优化器** | REQ-AI-001~005 | `sz-orm-ai/src/query_plan_optimizer.rs`（UnifiedQueryOptimizer/LlmOptimizer）+ `explain_parser.rs`（各方言 EXPLAIN 解析）+ `sql_sanitizer.rs` + 集成测试 | AC-AI-1~5：LLM 建议 + 来源标注、EXPLAIN 解析、降级规则引擎、LLM SQL 零执行、SQL 脱敏 | 3 周 |
| **M7: 集成验证 + 发布** | 全部 | 10 道门禁全通过 + crates.io 发布（sz-orm-graph 0.1.0 + 扩展包升级）+ sz-pay/sz-rust 下游回归 + CHANGELOG v3.0.0 | AC-ALL-1~7：无 Breaking Change、全 workspace 测试通过、clippy 零警告、feature 隔离、下游零回归、性能基准不回退、29 条需求全满足 | 1 周 |

**总周期**：约 17 周（4 个月），按低优先级逐步推进，可与日常迭代并行。

---

## 2.10 测试策略

### 2.10.1 各模块测试方案

| 模块 | 单元测试 | 集成测试 | 性能测试 | 真实服务测试 |
|------|---------|---------|---------|-------------|
| **sz-orm-graph** | CypherValidator（SQL 拒绝/参数化校验）、ResultMapper（类型映射）、GraphError | Neo4j 真实连接 + Cypher 查询 + 结果映射（`#[ignore]`） | 1000 节点图 P95 ≤ 500ms（spec §4.1.1） | Neo4j Docker 容器 |
| **sz-orm-wasm** | js_bindings（wasm-bindgen-test）、persistence（IndexedDB mock）、MemoryLimitError | wasm32 编译 + 浏览器端 E2E | gzip 体积 ≤ 1MB（spec §4.1.3） | 浏览器（headless Chrome） |
| **sz-orm-python/js** | 绑定层 API 单元测试 | pytest/jest 等价性测试（与 sz-orm-core 行为对比） | 无 | PyPI/npm 干净安装 |
| **sz-orm-ai** | LlmOptimizer（mock LLM）、ExplainPlanParser（各方言）、SqlSanitizer、降级逻辑 | LLM 真实 API 调用（`#[ignore]`，real feature） | AI 建议响应 ≤ 10s P95（spec §4.1.2） | OpenAI 兼容 API |
| **sz-orm-dtx** | XaResource（mock）、XaCoordinator（2PC 状态机）、XaCapabilityChecker、SuspensionDetector | 两库 XA 真实提交 + 崩溃恢复 + 悬挂超时（`#[ignore]`） | Prepare/Commit 网络往返基准 | MySQL/PG 双库 |
| **协同文档** | 无（文档） | 五方言等价性测试（复用 v2.4.0 基础设施） | 无 | 五方言真实 DB |

### 2.10.2 五方言覆盖策略

复用 v2.4.0 已交付的五方言集成测试基础设施（`tests/common/equivalence.rs` + `schema_builder.rs`），覆盖：

| 测试维度 | MySQL | PostgreSQL | SQLite | Oracle | MSSQL | 基础设施 |
|---------|-------|-----------|--------|--------|-------|---------|
| CRUD | ✓ | ✓ | ✓ | ✓ | ✓ | v2.4.0 integration_*.rs |
| 事务 | ✓ | ✓ | ✓ | ✓ | ✓ | v2.4.0 integration_*.rs |
| Eager Loading | ✓ | ✓ | ✓ | ✓ | ✓ | v2.4.0 smart_eager_integration_*.rs |
| XA 事务 | ✓ | ✓ | ✗（不支持） | ✓ | ✓ | v3.0.0 新增 xa_integration_*.rs |
| 方言约束 | ✓ | ✓ | ✓ | ✓ | ✓ | dialect_constraints.md 文档 |

**XA 测试特殊处理**：SQLite 不支持 XA（REQ-DTX-005），测试中尝试注册 SQLite 为 XA 参与者应返回 `XaNotSupported` 错误。

### 2.10.3 图数据库测试环境（Neo4j Docker）

```yaml
# docker-compose.neo4j.yml（测试环境）
version: "3.8"
services:
  neo4j:
    image: neo4j:5-community
    ports:
      - "7687:7687"  # Bolt 协议
      - "7474:7474"  # HTTP 浏览器界面
    environment:
      - NEO4J_AUTH=neo4j/test123  # 测试凭证
      - NEO4J_PLUGINS=["apoc"]    # APOC 插件（可选）
    volumes:
      - neo4j_data:/data
volumes:
  neo4j_data:
```

**测试连接**：`neo4j://neo4j:test123@127.0.0.1:7687`

**测试策略**：
- 单元测试：mock Bolt 驱动，测试 CypherValidator/ResultMapper/GraphError
- 集成测试：`#[ignore]` 标注真实 Neo4j 连接测试，`cargo test -- --ignored` 触发
- 性能测试：1000 节点图数据集，P95 延迟 ≤ 500ms 断言

### 2.10.4 WASM 测试（wasm-bindgen-test）

```rust
//! sz-orm-wasm 测试骨架
#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;
use crate::js_bindings::JsWasmDatabase;

#[wasm_bindgen_test]
fn test_js_create_and_query() {
    let mut db = JsWasmDatabase::new();
    db.create_table("CREATE TABLE users (id INTEGER, name TEXT)").unwrap();
    db.insert("INSERT INTO users (id, name) VALUES (?, ?)", &[1, "Alice"]).unwrap();
    let result = db.query("SELECT * FROM users", &[]).unwrap();
    assert!(result.length() > 0);
}

#[wasm_bindgen_test]
async fn test_persist_and_restore() {
    let mut db = JsWasmDatabase::new();
    db.create_table("CREATE TABLE test (id INTEGER)").unwrap();
    db.insert("INSERT INTO test (id) VALUES (?)", &[42]).unwrap();
    db.persist().await.unwrap();  // 持久化到 IndexedDB
    let mut db2 = JsWasmDatabase::new();
    db2.restore().await.unwrap();  // 从 IndexedDB 恢复
    let result = db2.query("SELECT * FROM test", &[]).unwrap();
    assert!(result.contains(42));
}
```

**测试运行**：`wasm-pack test --headless --chrome`（headless 浏览器执行）

### 2.10.5 FFI 测试（pytest + jest）

**Python pytest 等价性测试**：

```python
# packages/sz-orm-python/tests/test_equivalence.py
import sz_orm
import pytest

def test_crud_equivalence():
    """绑定层 CRUD 与 sz-orm-core 行为等价"""
    pool = sz_orm.PyPool("sqlite://:memory:")
    # 建表
    pool.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
    # 插入
    pool.execute("INSERT INTO users (id, name) VALUES (?, ?)", [1, "Alice"])
    # 查询
    rows = pool.query("SELECT * FROM users WHERE id = ?", [1])
    assert len(rows) == 1
    assert rows[0]["name"] == "Alice"

@pytest.mark.asyncio
async def test_async_query():
    """绑定层异步查询（REQ-FDI-004）"""
    pool = sz_orm.PyPool("sqlite://:memory:")
    await pool.async_execute("CREATE TABLE test (id INTEGER)")
    result = await pool.async_query("SELECT 1")
    assert result is not None
```

**JS jest 等性测试**：

```javascript
// packages/sz-orm-js/tests/equivalence.test.js
const { PyPool } = require('@sz-orm/core');

test('CRUD equivalence', async () => {
    const pool = new PyPool('sqlite://:memory:');
    await pool.execute('CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)');
    await pool.execute('INSERT INTO users (id, name) VALUES (?, ?)', [1, 'Alice']);
    const rows = await pool.query('SELECT * FROM users WHERE id = ?', [1]);
    expect(rows.length).toBe(1);
    expect(rows[0].name).toBe('Alice');
});
```

### 2.10.6 测试基础设施复用

| 基础设施 | 来源 | 复用模块 |
|---------|------|---------|
| `tests/common/equivalence.rs` | v2.4.0 交付 | 模块6（五方言等价性） |
| `tests/common/schema_builder.rs` | v2.4.0 交付 | 模块6（测试数据构造） |
| `bench-comparison/benches/benchmark_reporter.rs` | v2.4.0 交付 | 模块1/4/5（性能基准报告） |
| `scripts/gate.ps1` | AGENTS.md 10 道门禁 | 全部模块（门禁检查） |
| `scripts/publish_crates_io.ps1` | v2.4.0 交付 | 模块7（crates.io 发布） |
| `scripts/compute_topology.ps1` | v2.4.0 交付 | 模块7（拓扑排序，新增 sz-orm-graph） |

---

## 2.11 风险登记表

| 编号 | 风险 | 等级 | 概率 | 影响 | 缓解措施 | 关联模块 | 关联需求 |
|------|------|------|------|------|---------|---------|---------|
| R-01 | Neo4j Bolt 驱动生态成熟度不足 | 高 | 中 | 高 | 锁定 neo4rs 驱动版本，独立包 feature 隔离，评估期先行 spike 验证 Bolt 协议握手+查询+结果反序列化 | 图数据库 | REQ-GDB-001 |
| R-02 | WASM 生态对 tokio/异步支持有限 | 高 | 中 | 高 | 浏览器端用同步/轻异步执行路径（wasm-bindgen-futures），与服务器端 WASI 场景分离；持久化用 async 但非 tokio | WASM | REQ-WASM-002/003 |
| R-03 | wasm-bindgen 版本 API 变动频繁 | 中 | 中 | 中 | 锁定 wasm-bindgen 版本，绑定层最小化（仅包装 WasmDatabase 方法） | WASM | REQ-WASM-002 |
| R-04 | maturin/napi 跨平台 CI 矩阵成本 | 中 | 高 | 中 | 复用 GitHub Actions 矩阵，三平台并行构建；缓存 Cargo 编译产物 | 发布产物 | REQ-FDI-001/002 |
| R-05 | LLM 服务不可用/限流导致建议不可用 | 中 | 高 | 中 | 自动降级规则引擎（REQ-AI-004），返回规则建议 + LLM 降级说明，不报错不阻塞 | AI 优化器 | REQ-AI-004 |
| R-06 | LLM 生成 SQL 存在安全/正确性风险 | 高 | 中 | 高 | 建议零自动执行（REQ-AI-005）+ SQL 脱敏 + safety 模块校验 + 建议仅展示 | AI 优化器 | REQ-AI-005 |
| R-07 | XA 协议在不同数据库的实现差异 | 高 | 中 | 高 | 仅对明确支持 XA 的库开放（MySQL/PG/Oracle/MSSQL），逐库集成测试；SQLite 明确拒绝 | 事务一致性 | REQ-DTX-001/005 |
| R-08 | 分布式事务性能开销（Prepare/Commit 网络往返） | 中 | 高 | 中 | 超时与重试可配置（SuspensionConfig），提供性能基准；Prepare/Commit 可并行化 | 事务一致性 | REQ-DTX-003 |
| R-09 | sz-rust 适配层方言差异处理成本 | 中 | 中 | 中 | 五方言等价性测试 + 方言约束文档明确标注支持范围 | 多后端协同 | REQ-MB-002/004 |
| R-10 | 长期目标优先级低，与日常迭代争抢资源 | 中 | 高 | 中 | 按收益/风险序逐步推进，独立里程碑，可与日常迭代并行 | 全部 | 全部 |
| R-11 | WASM 产物体积超 1MB | 中 | 中 | 中 | `wasm-opt` 优化 + `twiggy` 分析体积 + feature gate 隔离重依赖 + gzip 体积断言脚本 | WASM | REQ-WASM-001 |
| R-12 | 协调者崩溃后 XA 日志不完整 | 高 | 低 | 高 | TransactionLogStore 在 Prepare/Commit 各阶段落日志（复用现有 write_log），恢复时按日志状态收敛 | 事务一致性 | REQ-DTX-002 |
| R-13 | XA Commit 阶段部分失败（不可回滚） | 高 | 低 | 高 | 标记悬挂 + 审计日志 + 人工介入或按策略补偿；这是 2PC 固有局限，文档明确标注 | 事务一致性 | REQ-DTX-003 |
| R-14 | 绑定层与 sz-orm-core 行为不一致 | 高 | 中 | 高 | pytest/jest 等价性测试套件覆盖 CRUD/事务/EagerLoad，任一失败阻断发布（REQ-FDI-005） | 发布产物 | REQ-FDI-003/005 |
| R-15 | 下游 sz-pay/sz-rust 回归 | 高 | 低 | 高 | v3.0.0 无 Breaking Change（feature gate 隔离），sz-pay 5139 测试基线 + sz-rust 回归验证 | 全部 | AC-ALL-5 |

**风险应对优先级**：R-01/R-02/R-06/R-07（高风险）需在对应里程碑启动前完成 spike 验证；R-12/R-13（低概率高风险）需在设计中明确标注 2PC 固有局限。

---

## 2.12 五维审查自检

设计完成后按"正确性 → 可读性 → 架构 → 安全性 → 性能"五维自检：

| 维度 | 检查项 | 结果 | 证据 |
|------|--------|------|------|
| **正确性** | 29 条 EARS 需求全部映射到技术设计 | ✅ | §2.1~2.6 逐模块覆盖 REQ-GDB/WASM/FDI/AI/DTX/MB 共 29 条 |
| | 无占位实现（todo!/unimplemented!） | ✅ | 所有数据结构与流程均为具体设计，无占位 |
| | 无 Breaking Change | ✅ | §2.8.3 评估表：所有变更通过 feature gate 隔离，现有 API 不变 |
| | 依赖无循环 | ✅ | §2.7.1 依赖矩阵 + 拓扑序可排序 |
| **可读性** | 文档结构清晰（参考 v2.4.0 design.md） | ✅ | 一、需求分析 → 二、增量设计（架构总览→6 模块→集成→发布→里程碑→测试→风险） |
| | file:line 证据引用 | ✅ | §1.1/§2.1.7/§2.2.7/§2.4.7/§2.5.7/§2.6.7 含具体代码位置 |
| | PlantUML 图表辅助理解 | ✅ | §2.0.1/2.0.3/2.1.2/2.2.2/2.3.2/2.4.2/2.5.2/2.6.2/2.7.1/2.8.2/2.9.2 含 11 张图 |
| **架构** | 模块职责单一 | ✅ | §2.0.2 定位表：6 模块各自独立职责 |
| | 复用现有基础设施 | ✅ | §2.7.2 共享基础设施表：Pool/Dialect/2PC 状态机/QueryOptimizationHint 等复用 |
| | Feature 隔离 | ✅ | §2.8.3：新增能力全部 feature gate（llm-optimizer/xa/js/persistence），默认不引入 |
| | 新增包最小化 | ✅ | 仅 sz-orm-graph 新增包，其余扩展现有包 |
| **安全性** | 参数化查询铁律 | ✅ | §2.1.4 CypherValidator 强制参数化；§2.3.4 绑定层复用参数化路径；§2.5.4 XA SQL 参数化 |
| | LLM 输出零执行 | ✅ | §2.4.4 REQ-AI-005：suggested_sql 仅展示，系统零执行 |
| | SQL 脱敏 | ✅ | §2.4.4 SqlSanitizer 脱敏敏感字面量；§2.1.3 GraphError DSN 脱敏；§2.5.3 XaError resource_id 脱敏哈希 |
| | unsafe 零容忍 | ✅ | 全部设计无 unsafe，与 AGENTS.md C-02 一致 |
| | SQL 透传拒绝 | ✅ | §2.1.4 CypherValidator.contains_sql_keywords 拒绝 SQL 透传图接口 |
| **性能** | 图查询 P95 ≤ 500ms | ✅ | §2.10.1 性能测试覆盖（spec §4.1.1） |
| | AI 建议 P95 ≤ 10s | ✅ | §2.4.3 OptimizerConfig.timeout_secs 默认 10s（spec §4.1.2） |
| | WASM gzip ≤ 1MB | ✅ | §2.2.8 R-11 缓解：wasm-opt + twiggy + 体积断言（spec §4.1.3） |
| | v2.4.0 基准不回退 | ✅ | §2.8.3 无 Breaking Change + feature gate 隔离，现有基准不受影响（spec §4.1.4） |

---

> **文档结束**
>
> **文档版本**：v3.0.0 技术设计（design 阶段）
> **对应需求**：`docs/spec/v3.0.0/spec.md`（29 条 EARS 需求，6 组）
> **基线参考**：`docs/spec/v2.4.0/design.md`（v2.4.0 技术设计，1259 行）
> **设计约束**：Rust 2021 Edition / rust-version 1.81 / API 向后兼容 / 禁止占位实现 / unsafe 零容忍 / Feature 隔离
> **五维审查**：正确性 ✅ → 可读性 ✅ → 架构 ✅ → 安全性 ✅ → 性能 ✅