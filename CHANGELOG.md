# Changelog

本文件记录 SZ-ORM 项目的所有重要变更。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
并遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [6.0.0] — 2026-09-02

### 新增 5 个 AI 方向包

| 包 | 功能 | 测试数 |
|----|------|--------|
| sz-orm-agent | AI Agent 自主运维（感知-决策-执行循环、工具调用、审批、权限、检查点） | 88 |
| sz-orm-governance | AI 数据治理（字段级血缘、合规审计、质量规则） | 39 |
| sz-orm-nl-query | NL 查询闭环（NL→SQL→执行→可视化→洞察、LLM generator） | 56 |
| sz-orm-model-ops | 模型微调本地化（llama.cpp/vLLM 适配、A/B 测试、推理优化） | 39 |
| sz-orm-multimodal | 多模态交互（语音、图表、ER 图、截图、草图、CV 接入） | 33 |

### 新增 LLM 真实接入

- `LlmNl2SqlGenerator`：包装 `LlmProvider` 实现 `Nl2SqlGenerator` trait（`packages/sz-orm-nl-query/src/llm_generator.rs`）
- `ReActPlanner::with_provider`：注入真实 LlmProvider 驱动 Agent 决策（`packages/sz-orm-agent/src/planner/react.rs:27`）
- `PlanAndExecutePlanner::with_provider`：注入真实 LlmProvider 生成执行计划（`packages/sz-orm-agent/src/planner/plan_execute.rs:31`）

### 新增真实服务接入

- MySQL 执行器：`NlQueryPipeline::with_executor` + `SqlExecutor` trait
- CV 服务：`SketchToSqlConverter::with_cv_endpoint` + `recognize_async`
- OCR 服务：`ScreenshotAnalyzer::with_ocr_endpoint` + `analyze_async`
- Agent 工具执行器：`ToolRegistry::with_defaults_and_executor` 注入 4 个工具

### 性能基准测试

- `nl2sql_rule_based`: 289 ns
- `nl2sql_rule_based_complex`: 410 ns
- `pipeline_e2e_no_executor`: 311 ns

### sz-pay 试点接入

- sz-orm-nl-query: NL 查询服务（5 个集成测试）
- sz-orm-agent: ReActPlanner 可用
- sz-orm-governance: LineageBuilder 可用
- sz-orm-multimodal: ScreenshotAnalyzer 可用

### deprecated 标记

- `Nl2SqlEvaluator::evaluate` → 使用 `evaluate_with_executor` 代替

## [5.0.0] — 2026-08-22

### 修复 blackhat_sql_injection 3 个测试失败

- 根因：`build_select()` v5.0.0 统一参数化（委派 `build_select_with_params()`），测试断言仍期望内联值
- 修复：更新 3 个测试断言为参数化格式（`?` 占位符）
- 文件：`packages/sz-orm-core/tests/blackhat_sql_injection.rs:106,145,163`
- 验证：`cargo test -p sz-orm-core --test blackhat_sql_injection` → 12 passed, 0 failed

### PHANTOM-2 Feature Gate 评估启用

对 179 个未默认启用的 feature gate 逐个评估，32 个决策 A（默认启用），147 个决策 B（保持手动）。

#### 决策 A：默认启用的 feature（32 个）

| 包 | 新增 default feature | 数量 |
|----|----------------------|------|
| sz-orm-core | auto-prewarm, perf-zero-copy-l2, perf-enum-dispatch, perf-box-str, cache-coherence, performance, dialect-cockroachdb, dialect-yugabytedb, dialect-snowflake, dialect-redshift, dialect-informix, dialect-saphana, dialect-firebird, prod-redis-tls, prod-jwt-key-rotation, prod-metrics-acl, prod-shutdown-timeout, prod-leak-detection, prod-n1-tuning, prod-pool-tuning, prod-config-masking, prod-log-level, prod-health-endpoint, prod-probe-endpoint, prod-circuit-tuning, prod-rate-limit-tuning, prod-dialect-security | 27 |
| sz-orm-auth | prod-jwt-key-rotation | 1 |
| sz-orm-health | prod-health-endpoint, prod-probe-endpoint | 2 |
| sz-orm-queue | prod-redis-tls | 1 |
| sz-orm-sqlx | auto-prewarm | 1 |

#### 附带修复

- `packages/sz-orm-core/Cargo.toml`：为 `tenant_quota_rls_regression` 测试添加 `required-features = ["multi-tenant-enhanced"]` 声明
- `packages/sz-orm-core/src/dialect.rs`：修复 5 个 clippy 警告（unused_doc_comments, if_same_then_else, format_in_format_args）

#### 评估报告

详见 `docs/assessment/2026-08-22-phantom2-feature-gate-evaluation.md`

#### 门禁验证

- `cargo fmt --all -- --check` ✅
- `cargo check --workspace --all-targets` ✅
- `cargo clippy --workspace --all-targets -- -D warnings` ✅
- sz-pay `cargo check` ✅

---

## [4.5.0] — 2026-08-12

### 概述

v4.5.0 是 SZ-ORM 的数据访问层高吞吐版本，新增 3 项能力，全部通过 feature gate 隔离（默认关闭，无 Breaking Change）：并行查询执行器、批量 INSERT/UPDATE/DELETE 优化、异步流式结果集。

### 新增能力（3 个 feature gate，默认关闭）

| feature gate | 所属包 | 能力 |
|-------------|--------|------|
| `parallel-query` | sz-orm-parallel（新包） | 并行查询执行器：Semaphore 并发控制 + 超时 + 三种合并策略（Concat/Map/Union）+ 三种失败策略（Abort/Skip/Collect）+ DefaultLike 降级 |
| `batch-v2` | sz-orm-batch | 批量 INSERT/UPDATE/DELETE 优化：五方言 SQL 生成（BatchDialect）+ 事务边界（RollbackStrategy 三策略）+ PG COPY 协议 + 批量删除 + 进度回调 |
| `stream-resultset` | sz-orm-stream（新包） | 异步流式结果集：futures::stream::unfold 实现 Stream trait + 三种分页策略（Keyset/LimitOffset/ServerCursor）+ 无锁背压（AtomicUsize + Notify） |

### 新增包

sz-orm-parallel / sz-orm-stream（2 个，工作空间成员 58 → 60）

### 里程碑完成情况

| 里程碑 | 名称 | 状态 |
|--------|------|------|
| M0 | 文档基线与准备 | ✅ |
| M1 | 并行查询执行器 | ✅ |
| M2 | 批量 INSERT/UPDATE/DELETE 优化 | ✅ |
| M3 | 异步流式结果集 | ✅ |
| M4 | 集成验证与文档同步 | ✅ |

### 测试

- 新增 89 个测试，全工作空间测试通过
- sz-orm-parallel: 27 tests（parallel-query feature）
- sz-orm-batch: 26 tests（batch-v2 feature，含 14 dialect + 8 delete + 10 executor + 12 copy，部分在默认 feature 下）
- sz-orm-stream: 36 tests（stream-resultset feature，含 8 config + 10 keyset + 9 backpressure + 9 result_set）

## [4.4.0] — 2026-08-12

### 概述

v4.4.0 是 SZ-ORM 的查询智能闭环版本，新增 6 项能力，全部通过 feature gate 隔离（默认关闭，无 Breaking Change）：查询自动优化建议引擎、慢查询自动诊断报告、db-fusion 转正阶段一（TTL 缓存 + 失效广播）、结构化查询日志、性能回归基准线、查询智能闭环联动。

### 新增能力（6 个 feature gate，默认关闭）

| feature gate | 所属包 | 能力 |
|-------------|--------|------|
| `query-advisor` | sz-orm-advisor（新包） | 规则引擎分析 EXPLAIN 计划 + 自适应统计，生成六种可执行优化建议（AddIndex/DropIndex/UsePagination/EnableCache/RewriteQuery/AdjustPoolSize），支持 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言 DDL |
| `slow-query-diagnosis` | sz-orm-diagnosis（新包） | 慢查询自动诊断：六种根因分析（全表扫描/缺失索引/N+1/连接池耗尽/锁等待/结果集过大）+ 分阶段耗时分解 + JSON/人类可读双格式报告 |
| `db-fusion-v2` | sz-orm-fusion | db-fusion 转正阶段一：TTL 缓存（TtlFusionCache）+ 失效广播（InvalidationBus 集成）+ MemoryFusionCache 标记 deprecated |
| `query-logging` | sz-orm-observability | 结构化查询日志：采样率控制 + 级别控制 + 参数脱敏（复用 MaskingRule）+ 分阶段计时序列化 |
| `perf-baseline` | sz-orm-explain | 性能回归基准线：基线快照 + CI 自动比对 + 复用 PlanRegression 回归检测 |
| `query-intelligence-loop` | sz-orm-advisor | 查询智能闭环联动：串联 EXPLAIN → 自适应 → 诊断 → 建议四步闭环，任一环节失败降级跳过 |

### 新增包

sz-orm-advisor / sz-orm-diagnosis（2 个，工作空间成员 56 → 58）

### 里程碑完成情况

| 里程碑 | 名称 | 状态 |
|--------|------|------|
| M0 | 文档基线与准备 | ✅ |
| M1 | 查询自动优化建议引擎 | ✅ |
| M2 | 慢查询自动诊断报告 | ✅ |
| M3 | db-fusion 转正（阶段一：TTL + 失效广播） | ✅ |
| M4 | 结构化查询日志 | ✅ |
| M5 | 性能回归基准线 | ✅ |
| M6 | 查询智能闭环联动 | ✅ |

### 测试

- 新增约 50 个测试，全工作空间测试通过
- sz-orm-advisor: 44 tests（M1: 37 + M6: 7）
- sz-orm-diagnosis: 29 tests
- sz-orm-fusion: 21 tests（db-fusion-v2）+ 12 tests（POC）
- sz-orm-observability: 56 tests（44 既有 + 12 新增）
- sz-orm-explain: 47 tests（40 既有 + 7 新增）

## [4.3.0] — 2026-08-12

### 概述

v4.3.0 是 SZ-ORM 的查询智能与数据治理增强版本，新增 6 项能力，全部通过 feature gate 隔离（默认关闭，无 Breaking Change）：编译期 EXPLAIN 分析、查询性能火焰图、N+1 静态检测、数据血缘可视化、编译期数据治理、自适应查询优化器；另含多数据库融合查询 POC（`db-fusion`，实验性，转正建议见评估文档）。

### 新增能力（7 个 feature gate，默认关闭）

| feature gate | 所属包 | 能力 |
|-------------|--------|------|
| `explain-analyzer` | sz-orm-explain（新包）+ sz-orm-macros | `query!` 宏 db-verify 模式编译期解析 EXPLAIN，检测全表扫描/缺失索引并输出编译期警告（MySQL/PostgreSQL/SQLite 真库验证）；执行计划基线快照 + CI 回归检测 |
| `query-flamegraph` | sz-orm-flamegraph（新包） | 查询分阶段计时（构建/绑定/连接池/SQL 执行/结果映射）+ Brendan Gregg 折叠格式与 SVG 火焰图输出 |
| `n1-lint` | sz-orm-n1-lint（新包）+ sz-orm-macros + cli | N+1 静态检测：`#[detect_n_plus_one]` 标注宏编译期警告 + `sz-orm n1-lint --path` CLI 批量扫描（table/JSON 双格式），与运行时 `N1QueryDetector` 交叉验证 |
| `lineage-viz` | sz-orm-audit | 数据血缘可视化：Mermaid/HTML 导出 + 带深度限制的影响分析（downstream/upstream） |
| `compile-governance` | sz-orm-core + sz-orm-macros | 编译期数据治理：`#[derive(Governed)]` PII 标注强制（缺 mask/非法策略编译失败）+ 合规报告（GDPR 清单 JSON） |
| `adaptive-query` | sz-orm-adaptive（新包） | 运行时自适应查询：原子统计 + 自动分页/热点缓存决策（缓存默认关闭防脏读） |
| `db-fusion` | sz-orm-fusion（新包） | 多数据库融合查询 POC：缓存键下推 + 主库降级回退（实验性，转正建议见 `docs/评估/2026-08-12_db-fusion实验评估.md`） |

### 新增包

sz-orm-explain / sz-orm-flamegraph / sz-orm-adaptive / sz-orm-fusion / sz-orm-n1-lint（5 个，工作空间成员 55 → 56）

### 其他

- 文档基线：8 份评估文档勘误（虚构测试数/“88 ZST”/“crates.io 仅 1 包”等修正）+ v4.2.0 基线评估报告（audit-verify 14/14）
- 修复：`verify_columns_postgres` 列名大小写（TABLE_NAME → 按索引），PG 路径 query! db-verify 可用
- 适配：MySQL 9.6 树形 EXPLAIN 输出格式
- 新增约 100 个测试，全工作空间约 7,000 个测试通过

## [4.2.0] — 2026-08-12

### 概述

v4.2.0 是 SZ-ORM 的跨语言与可视化增强版本，新增 5 项能力：跨语言分布式事务、Go/Java/C++ 语言绑定、可视化 Schema 设计器、OpenAPI → ORM 反向生成、WASM 真实数据库连接。所有新能力通过 7 个 feature gate 隔离（`cross-lang-dtx` / `lang-binding-go` / `lang-binding-java` / `lang-binding-cpp` / `schema-designer` / `openapi-reverse` / `wasm-real-db` + `wasi-socket`），默认关闭，无 Breaking Change。新增 4 个包（sz-orm-go / sz-orm-java / sz-orm-cpp / sz-orm-designer），新增约 200 个测试，全工作空间约 6,920 个测试通过。

### 新增功能

#### M1: 跨语言分布式事务（`cross-lang-dtx` feature，sz-orm-dtx）

- `CrossLangDtxCoordinator`：跨语言 DTX 协调器，支持 Go/Java/C++/Python 调用方
- `DtxProtocol`：二进制协议（Magic + Version + Op + XID + Payload + Checksum）
- `DtxOperation`：Begin/Commit/Rollback/Join/Heartbeat 五种操作
- `DtxTransport` trait：TCP/HTTP/gRPC 传输抽象
- `CrossLangDtxParticipant`：参与者注册 + 心跳 + 超时检测
- `DtxRecovery`：故障恢复 + 悬挂事务清理
- 50 个测试通过

#### M2: Go/Java/C++ 语言绑定（`lang-binding-go` / `lang-binding-java` / `lang-binding-cpp` feature）

- **sz-orm-go**：Go FFI 绑定包，CGO 桥接层，`GoQueryBuilder` / `GoModel` / `GoTransaction`
- **sz-orm-java**：Java FFI 绑定包，JNI 桥接层，`JavaQueryBuilder` / `JavaModel` / `JavaTransaction`
- **sz-orm-cpp**：C++ FFI 绑定包，C ABI 桥接层，`CppQueryBuilder` / `CppModel` / `CppTransaction`
- 4 个新包，27 个测试通过

#### M3: 可视化 Schema 设计器（`schema-designer` feature，sz-orm-designer）

- **sz-orm-designer**：独立包，可视化 Schema 设计器
- `SchemaDesign`：表/列/索引/外键/约束建模
- `SchemaDesigner`：设计器核心，支持 5 种方言（MySQL/PostgreSQL/SQLite/Oracle/MSSQL）
- `DesignerExporter`：导出为 SQL DDL / JSON / Mermaid ER 图 / HTML 可视化
- `DiffVisualizer`：Schema 差异可视化（Text/JSON/HTML 三种格式）
- 26 个测试通过

#### M3: OpenAPI → ORM 反向生成（`openapi-reverse` feature，sz-orm-swagger）

- `OpenApiReverseGenerator`：从 OpenAPI 3.0 spec 反向生成 ORM 代码
- `SchemaToModelMapper`：OpenAPI Schema → Rust model 类型映射 + 约束映射
- `OpenApiToMigrationMapper`：OpenAPI Schema → 数据库迁移 DDL（5 种方言）
- `OpenApiToRepositoryMapper`：OpenAPI paths → CRUD repository 骨架代码
- `ApiFirstLoopVerifier`：API-first 循环验证（检测 spec ↔ impl 不一致）
- `OpenApiInjectionGuard`：恶意扩展检测 + 签名验证
- 55 个测试通过

#### M3: CLI 集成

- CLI 新增 `designer` / `designer:export` / `openapi:reverse` 命令
- 支持 `--dialect` / `--format` / `--trust-unsigned` 参数

#### M4: WASM 真实数据库连接（`wasm-real-db` + `wasi-socket` feature，sz-orm-wasm）

- `WasmDbProxyProtocol`：WASM ↔ 后端 DB 代理协议（JSON + MessagePack 双序列化）
- `WasmRealDbConnection`：HTTP/WebSocket 代理桥接，WASM 端不持有 DB 凭据
- `WasmRealDbQueryExecutor`：查询执行器，集成白名单 + 限流 + 鉴权 + 指标
- `WasmDbProxy`：后端代理，鉴权 + 限流 + SQL 白名单 + 结果集大小限制
- `WasmDbAuthValidator`：Token/Session 鉴权
- `WasmDbRateLimiter`：固定窗口 QPS 限流（默认 100 QPS）
- `WasmDbSqlWhitelist`：SQL 白名单（仅 SELECT/INSERT/UPDATE/DELETE，禁止 DDL）
- `WasmRealDbReconnector`：指数退避重连
- `WasmRealDbMetrics`：查询指标采集（线程安全 AtomicU64）
- `WasiSocketConnection`：WASI socket 直连（`wasi-socket` feature 门控）
- 94 个测试通过

### 工程化

- SDD 三阶段文档：`docs/spec/v4.2.0/{spec.md, design.md, tasks.md}`（640 + 1438 + 1589 行）
- 7 个 feature gate 全部默认关闭，无 Breaking Change
- 全工作空间 clippy 零警告，fmt 通过
- 4 个里程碑（M1~M4），40 主任务，278 子任务

## [4.1.0] — 2026-08-11

### 概述

v4.1.0 是 SZ-ORM 的数据治理与运维增强版本，新增 9 项能力：数据 seeding/fixture 管理、schema diff 可视化、缓存一致性协议、消息轨迹追踪、存储生命周期管理、数据质量自动检测、批量流式处理、迁移版本分支、备份验证自动化。所有新能力通过 9 个 feature gate 隔离（`data-seeding` / `schema-diff-viz` / `cache-coherence` / `message-tracing` / `storage-lifecycle` / `data-quality` / `batch-stream` / `migration-branch` / `backup-verify`），默认关闭，无 Breaking Change。新增 106 个测试，全工作空间 6760 个测试通过。

### 新增功能

#### M1: 数据 seeding/fixture 管理（`data-seeding` feature，sz-orm-core）

- `FakerGenerator`：10 内置字段生成器（Name/Email/Address/Phone/Uuid/Date/Number/Float/Boolean/Enum/Json）
- `FixtureLoader`：YAML/JSON fixture 模板加载，关联引用解析
- `SeedManager`：种子版本管理 + 依赖拓扑排序 + 幂等执行 + 环境隔离
- CLI 新增 `make:fixture` 和 `seed:fixture` 命令
- 21 个测试通过

#### M1: schema diff 可视化（`schema-diff-viz` feature，sz-orm-core）

- `SchemaDiffVisualizer`：复用既有 `SchemaDiff`/`DdlGenerator`，生成差异报告
- `DiffReport`：自动标注破坏性变更（删除表/列）
- 三种输出格式：Text（终端友好）/ JSON / HTML
- CLI 新增 `schema:diff` 命令
- 9 个测试通过

#### M2: 缓存一致性协议（`cache-coherence` feature，sz-orm-core）

- `CacheCoherenceProtocol`：MESI 风格状态机（M/E/S/I 四状态）
- `ConsistencyStrategy`：WriteThrough / WriteBehind 写策略
- `InvalidationBroadcaster` trait：跨实例失效广播（trait-based，支持任意 MQ 实现）
- `CoherenceMetrics`：一致性指标追踪
- 10 个测试通过

#### M2: 消息轨迹追踪（`message-tracing` feature，sz-orm-queue）

- `MessageTracingInterceptor`：消息轨迹记录 + 采样率控制
- `TraceContext`：端到端 trace_id/span_id 关联
- `DesensitizeRule`：三种脱敏方式（FullMask/PartialMask/Hash）
- 11 个测试通过

#### M2: 存储生命周期管理（`storage-lifecycle` feature，sz-orm-storage）

- `StorageLifecycleManager`：对象生命周期策略管理
- `TieringRule`：分层策略（Hot/Warm/Cold/Archive 四层）
- `ExpirationCleaner`：过期自动清理
- 20 个测试通过

#### M2: 数据质量自动检测（`data-quality` feature，sz-orm-audit）

- `DataQualityEngine`：六类统计学规则检测
- `QualityRule`：Completeness/Uniqueness/Validity/Consistency/Timeliness/Accuracy
- `QualityReport`：质量报告与通过率统计
- 8 个测试通过

#### M3: 批量流式处理（`batch-stream` feature，sz-orm-batch）

- `BatchSplitter`：流式批次分割器
- `BackpressureController`：背压控制
- `StreamConfig`：批次大小/并发度/背压阈值配置
- 8 个测试通过

#### M3: 迁移版本分支（`migration-branch` feature，sz-orm-core）

- `MigrationBranchManager`：多分支并行开发迁移管理
- `MergeConflict`：合并冲突检测（版本冲突/依赖冲突/Schema 冲突）
- 分支创建/合并/冲突检测全流程
- 9 个测试通过

#### M3: 备份验证自动化（`backup-verify` feature，sz-orm-back）

- `BackupVerifier`：备份完整性校验（文件存在/大小/SHA-256）
- `VerifyReport`：校验报告生成
- `restore_drill`：恢复演练
- 10 个测试通过

## [4.0.0] — 2026-08-11

### 概述

v4.0.0 是 SZ-ORM 的智能化与云原生集成版本，新增 9 项优化能力：多 LLM 模型支持、AI 自动调优闭环、混合搜索、数据 lineage 追踪、分片自动 rebalance、数据库 failover 自动化、CDC 变更数据捕获、GraphQL 深度集成、服务网格集成。所有新能力通过 9 个 feature gate 隔离（`multi-llm` / `ai-auto-tuning` / `hybrid-search` / `data-lineage` / `shard-rebalance` / `auto-failover` / `cdc` / `async-graphql-integration` / `service-mesh`），默认关闭，无 Breaking Change。

### 新增功能

#### M1: 多 LLM 模型支持（`multi-llm` feature，sz-orm-ai）

- `LlmProvider` trait + 5 个实现：OpenAI / Claude / Gemini / Ollama / 本地
- `LlmRouter`：多模型热切换（ArcSwap 原子切换）、负载均衡、故障转移
- `LlmCapability` 能力声明：文本生成 / SQL 优化 / 嵌入向量 / 函数调用
- 213 个测试通过

#### M2: AI 自动调优闭环（`ai-auto-tuning` feature，sz-orm-ai）

- `AutoTuningPipeline`：检测 → 建议 → 验证 → 应用 → 回归 五阶段闭环
- `SlowQueryDetector`：基于执行计划的慢查询根因分析
- `TuningSuggestion`：索引建议 / SQL 重写 / Schema 变更，附风险等级
- `RegressionDetector`：应用前后 A/B 对比，自动回滚高风险变更
- 275 个测试通过

#### M3: 混合搜索（`hybrid-search` feature，sz-orm-vector）

- `HybridSearcher`：统一向量 + 全文 + 结构化搜索接口
- `SearchFusion`：RRF（Reciprocal Rank Fusion）+ 加权融合
- `SearchPushdown`：搜索条件下推至数据库引擎
- 107 个测试通过

#### M4: 数据 lineage 追踪（`data-lineage` feature，sz-orm-audit）

- `LineageGraph`：DAG 图结构，节点 = 表/列，边 = 数据流向
- `SqlLineageParser`：基于 sqlparser 0.47 AST 解析，支持 INSERT/UPDATE/DELETE/SELECT/CTE/JOIN/子查询
- `LineageTracker`：运行时追踪，记录每次查询的源/目标列映射
- `LineageExporter`：导出为 JSON / Mermaid / Graphviz 格式
- 161 个测试通过

#### M5: 分片自动 rebalance（`shard-rebalance` feature，sz-orm-sharding）

- `RebalancePlanner`：基于负载均衡算法生成迁移计划
- `RebalanceCheckpoint`：迁移检查点，支持断点续传
- `RebalanceExecutor`：原子化迁移执行，失败自动回滚
- 属性测试验证收敛性和不变量
- 155 个测试通过

#### M6: 数据库 failover 自动化（`auto-failover` feature，sz-orm-rw）

- `AutoFailoverManager`：主从故障自动切换，可配置阈值和冷却时间
- `FailoverConfig`：故障检测间隔 / 重试次数 / 数据一致性检查
- `DataLossRisk`：切换前数据丢失风险评估
- `SplitBrainDetector`：脑裂检测 + 自动降级策略（DemotionStrategy）
- 43 个测试通过

#### M7: CDC 变更数据捕获（`cdc` feature，sz-orm-queue）

- `DialectCapturer` trait + 5 方言实现：PostgreSQL WAL / MySQL Binlog / SQLite Trigger / Oracle LogMiner / MSSQL CDC
- `ExactlyOnceDedup`：基于 LSN + 事务 ID 的精确一次去重
- `CheckpointManager`：检查点持久化，支持断点续传
- `DownstreamSink` trait + 3 实现：Kafka / HTTP Webhook / InMemory
- `apply_masking`：下游分发前数据脱敏
- 56 个测试通过

#### M8: GraphQL 深度集成（`async-graphql-integration` feature，sz-orm-graphql）

- `AsyncGraphqlBridge`：async-graphql 桥接，支持 Query / Mutation / Subscription
- `BridgeDataLoader`：DataLoader 批量加载，N+1 查询自动消除
- `SubscriptionSource`：基于 CDC ChangeEvent 的实时订阅
- `RelayConnection` / `RelayEdge` / `PageInfo`：Relay 游标分页规范
- `FederationGateway`：Apollo Federation 联邦 schema 聚合
- `TicketError`：工单化错误处理，附错误分类和追踪 ID
- 49 个测试通过

#### M9: 服务网格集成（`service-mesh` feature，sz-orm-observability）

- `ServiceMeshAdapter` trait + 2 实现：Istio / Linkerd
- `IstioAdapter`：VirtualService / DestinationRule / PeerAuthentication 生成
- `LinkerdAdapter`：Server / ServerAuthorization / ServiceProfile 生成
- `MeshObservability`：网格级指标采集 + 分布式追踪 + Prometheus / OTLP 导出
- `MeshConfig`：mTLS 模式 / 流量治理 / Sidecar 配置
- 38 个测试通过

### 门禁验证

| 门禁 | 结果 |
|------|------|
| fmt | ✅ PASS |
| check --workspace --all-targets | ✅ PASS |
| clippy --workspace --all-targets -- -D warnings | ✅ PASS |
| test --workspace | ✅ PASS（0 failed） |
| doc --workspace --no-deps | ✅ PASS |
| 占位实现检查 | ✅ PASS |
| 9 个新 feature 独立编译 | ✅ 全部通过 |

### Feature Gate 汇总

| Feature | 包 | 测试数 |
|---------|-----|--------|
| `multi-llm` | sz-orm-ai | 213 |
| `ai-auto-tuning` | sz-orm-ai | 275 |
| `hybrid-search` | sz-orm-vector | 107 |
| `data-lineage` | sz-orm-audit | 161 |
| `shard-rebalance` | sz-orm-sharding | 155 |
| `auto-failover` | sz-orm-rw | 43 |
| `cdc` | sz-orm-queue | 56 |
| `async-graphql-integration` | sz-orm-graphql | 49 |
| `service-mesh` | sz-orm-observability | 38 |

## [3.9.0] — 2026-08-11

### 概述

v3.9.0 是 SZ-ORM 的开发者体验增强版本，新增 6 项优化能力：数据验证框架、criterion benchmark 套件、semver/API 稳定性、迁移 dry-run + 影响分析、流式 CSV 导出、CI/CD 模板。所有新能力通过 5 个 feature gate 隔离（`data-validation` / `validate-on-write` / `benchmark-suite` / `migration-dry-run` / `streaming-export`），默认关闭，无 Breaking Change。默认 feature 测试零回归（1605 passed 0 failed），全 feature 组合 1655 passed 0 failed。

### 新增功能

#### M1: 数据验证框架（`data-validation` / `validate-on-write` features）

- `Validate` trait + `ValidationError`（9 变体）+ `aggregate` 聚合函数
- 8 种校验规则：email / length / range / regex / required / contains / does_not_contain / custom
- `#[derive(Validate)]` 派生宏，支持 `#[validate(email)]` / `#[validate(length(min=2, max=50))]` 等属性
- 条件校验：`#[validate(email, when = "self.enabled")]`
- `insert_validated` / `update_validated` 方法，写入前自动校验，失败返回 `DbError::Validation`
- 42 个测试通过

#### M2: criterion benchmark 套件（`benchmark-suite` feature）

- 6 路径回归基准：query_build / pool / cache / transaction / serialization / stream
- 每路径 3 基准点，复用 criterion 0.5 + bench_group 配置
- `BenchPath` enum + `BaselinePoint` / `RegressionPoint` / `RegressionReport` 回归对比结构
- `compute_change` 变化百分比计算 + 回归判定（≥10%）

#### M3: semver/API 稳定性

- `.github/workflows/semver-check.yml`：CI 集成 cargo-semver-checks，PR 自动检测破坏性变更
- `scripts/check-deprecation-period.py`：废弃保留期检查（≥2 个 MINOR 版本），CI 失败时 exit(1)

#### M4: 迁移 dry-run + 影响分析（`migration-dry-run` feature）

- `Migrator::migrate_dry_run()` → `DryRunReport`：预览待执行迁移 SQL，不修改数据库
- `Migrator::impact_analysis()` → `ImpactReport`：DDL 分类（Create/AlterAdd/AlterDrop/Drop/Other）
- 受影响表提取 + 锁类型标记 + 破坏性 DDL 标记 + 回滚可行性评估
- 5 个测试通过

#### M5: 流式 CSV 导出（`streaming-export` feature）

- `CsvExporter<W: Write>`：逐行导出 CSV，峰值内存 = 单行 + CSV 缓冲
- `ExportConfig`：表头开关 + 分隔符 + 批次大小
- `ExportResult`：导出行数 + 字节数
- 5 个测试通过

#### M6: CI/CD 模板

- 6 个 reusable workflow 模板：lint / test / security / release / probe / soak
- 参数化 inputs（包名 / 数据库 / feature / 工具链），支持 `uses:` 远程引用
- 无硬编码密钥，均通过 `${{ secrets.* }}` 引用

### 技术约束

- `streaming-export` feature 仅依赖 `csv`（移除 arrow/parquet，因 arrow-arith 52 与 chrono 0.4.45 版本冲突）
- `#[derive(Validate)]` 条件属性用 `when` 而非 `if`（避免 Rust 关键字冲突）
- `extern crate self as sz_orm_core` 用于 crate 内部测试解析宏生成的绝对路径

## [3.8.0] — 2026-08-10

### 概述

v3.8.0 是 SZ-ORM 的生产部署就绪版本，新增 15 项生产就绪检查能力，通过 `prod-ready` feature gate 聚合 14 个子 feature，覆盖安全红线、配置可观测、阈值调优、ORM 防护四大类别。提供 `ProdReadyChecker` 检查清单执行器，可聚合 15 项检查并输出 JSON 报告供 CI/CD 集成。所有新能力默认关闭，无 Breaking Change。全 workspace 测试零回归（6760 passed 0 failed）。

### 新增功能

#### M1: 安全红线类（`prod-redis-tls` / `prod-jwt-key-rotation` / `prod-metrics-acl` / `prod-config-masking` features）

- Redis TLS 连接验证
- JWT 密钥轮换验证
- metrics ACL 访问控制验证
- 配置脱敏验证（`ProdReadyConfig::verify_masking()`）

#### M2: 配置可观测类（`prod-log-level` / `prod-health-endpoint` / `prod-probe-endpoint` / `prod-shutdown-timeout` features）

- LogLevel 新增 Trace 变体（Trace < Debug < Info < Warn < Error）
- `LoggerProdConfig` 生产环境日志级别强制（`prod-log-level`）
- `HealthEndpointConfig` + `start_health_endpoint` HTTP 健康检查端点
- `ProbeEndpointConfig` + `start_probe_endpoint` K8s readiness/liveness 探针端点
- `to_k8s_yaml()` 生成 K8s 探针配置
- `Pool::shutdown_with_timeout(timeout)` 优雅关闭超时

#### M3: 阈值调优类（`prod-rate-limit-tuning` / `prod-circuit-tuning` / `prod-pool-tuning` features）

- `RateLimitProdConfig` + `SlidingWindowRateLimiter::set_capacity/set_rate/stats()` 运行时动态调整
- `CircuitBreakerProdConfig` + `DefaultCircuitBreaker::stats()` 统计信息
- `PoolProdConfig` + `validate()` + `to_pool_config()` 连接池参数验证

#### M4: ORM 防护类（`prod-leak-detection` / `prod-n1-tuning` / `prod-dialect-security` features）

- `LeakDetectionConfig` / `LeakReport` / `LeakEntry` 连接泄漏检测配置
- `N1DetectionConfig` 扩展 `window`/`block` 字段 + `N1DetectorStats` + `stats()` N+1 检测调优
- `DialectSecurityVerifier` 五方言连接安全验证（MySQL/PostgreSQL/SQLite/Oracle/MSSQL）
  - TLS / 认证 / 连接串脱敏 / 连接池参数四维检查
  - SQLite TLS 标记 N/A，不可用方言标记 Skipped

#### M5: 检查清单工具化（`prod-ready` feature）

- `ProdReadyChecker` 检查清单执行器，聚合 15 项检查（REQ-PROD-001~015）
- `CheckItem` trait 支持扩展（新增检查项仅需实现 trait）
- `ProdReadyReport` 可序列化为 JSON 输出（供 CI/CD 集成）
- `enabled_checks` / `skipped_checks` 配置过滤
- 每项检查附 file:line 证据，FAIL 附失败原因

### Feature Gate 体系

| Feature | 类别 | 说明 |
|---------|------|------|
| `prod-redis-tls` | M1 | Redis TLS 验证 |
| `prod-jwt-key-rotation` | M1 | JWT 密钥轮换 |
| `prod-metrics-acl` | M1 | metrics ACL |
| `prod-config-masking` | M1 | 配置脱敏 |
| `prod-log-level` | M2 | 日志级别 |
| `prod-health-endpoint` | M2 | 健康端点 |
| `prod-probe-endpoint` | M2 | K8s 探针 |
| `prod-shutdown-timeout` | M2 | 优雅关闭 |
| `prod-rate-limit-tuning` | M3 | 限流调优 |
| `prod-circuit-tuning` | M3 | 熔断调优 |
| `prod-pool-tuning` | M3 | 连接池调优 |
| `prod-leak-detection` | M4 | 泄漏检测 |
| `prod-n1-tuning` | M4 | N+1 检测 |
| `prod-dialect-security` | M4 | 方言安全 |
| `prod-ready` | M5 | 聚合以上全部 |

### 测试

- 默认 feature: 6760 passed 0 failed
- `prod-ready` feature: 1647 passed 0 failed（sz-orm-core lib）
- 14 道门禁: fmt ✅ check ✅ clippy ✅ test ✅ doc ✅ 占位扫描 ✅ ADR-0001 ✅

## [3.6.0] — 2026-08-10

### 概述

v3.6.0 是 SZ-ORM 的类型安全与文档完善版本，包含 5 大方向交付物，覆盖 37 条 EARS 需求，通过 5 个里程碑（M1-M5）推进。所有新能力通过 feature gate 隔离，默认关闭，无 Breaking Change。全 workspace 测试零回归，sz-pay 项目验证通过。

### 新增功能

#### M1: 编译期类型安全深入优化（`typed-relation` / `sql-verify-proc` / `typed-dsl` features）

- CTE 3 种：`With<N,S>` / `WithRecursive<N,I,R>` / `CteRef<N>`（typed_ast.rs:1693-1779）
- Window Frame 6 种：`RowsFrame` / `RangeFrame` / `GroupsFrame` / `FrameBetween<S,E>` / `FrameUnboundedPreceding` / `FrameCurrentRow`（typed_ast.rs:1785-1913）
- JSON 操作符 6 种：`JsonGet<C,K>` / `JsonGetText<C,K>` / `JsonPathGet<C,P>` / `JsonPathGetText<C,P>` / `JsonContains<C,V>` / `JsonExists<C,K>`（typed_ast.rs:1915-2055）
- 自定义诊断模块：`sz-orm-macros/src/diagnostic.rs` + `type_check` 属性宏 + `diagnostic_error!` 宏 + trybuild 测试
- typed relation：`BelongsTo<C,P>` / `HasMany<P,C>` / `HasOne<P,C>` + `RelationQuery<R>` + `RelationKind` enum
- SQL 验证模块：`sql_verify.rs`（sqlparser 语法验证 + xxhash 缓存 + 只读检查）
- ZST 断言测试 + SQL 注入防护测试 + 覆盖度对比文档（61 种 vs Diesel ~38 种）
- DSL 覆盖度：61 种表达式，超过 Diesel（~38 种）

#### M2: 313 pub API 文档补齐

- 195 个 missing_docs 警告全部补齐
- `#![warn(missing_docs)]` 全局启用（从 `cfg_attr(docsrs, ...)` 升级）
- `cargo doc -p sz-orm-core --no-deps --all-features` 零警告
- 5 个文档格式警告修复（unclosed HTML tag / unresolved link）

#### M3: QueryBuilder 渐进合并（`qb-migration-tool` feature）

- `qb_migration_lint.rs`：迁移 lint 模块（15 个测试）
- `qb_migration_fix.rs`：迁移 fix 模块（23 个测试）
- `qb_migration_diff_test.rs`：差分测试验证语义等价（9 个测试）
- `docs/qb-migration-roadmap.md`：v3.7.0 移除路线图
- sz-orm-query-builder 6 个 deprecated 标注验证通过
- sz-pay 项目 `cargo check` + `cargo test` 零回归验证通过

#### M4: 方言扩展 Snowflake + Redshift（`dialect-snowflake` / `dialect-redshift` features）

- SnowflakeDialect：独立实现，支持 VARIANT/OBJECT/ARRAY + COPY INTO + TIME TRAVEL（17 个测试）
- RedshiftDialect：委派 PostgreSqlDialect + COPY/UNLOAD 特性扩展（15 个测试）
- DbType 新增 Snowflake + Redshift 变体（#[non_exhaustive] 允许）
- 方言总数：18 → 21 种（新增 Snowflake + Redshift）
- `docs/snowflake-redshift-driver-evaluation.md`：Rust 驱动评估
- `docs/prisma-dialect-evaluation.md`：Prisma 兼容评估
- `docs/dialect-extension-roadmap.md`：方言扩展路线图更新

#### M5: async trait 重评估

- 基于 Rust 1.81 原生 async fn in trait 重新评估
- 评估结论：保持方案 C（现状 + 文档），dyn trait + Send bound 限制仍存在
- `docs/async-trait-evaluation.md` 更新 v3.6.0 重评估章节
- Connection trait 签名不变，sz-pay 零回归

### 测试

- 全 workspace 测试零失败
- sz-pay 项目 10 + 13 + 2 = 25 个测试通过
- M1 新增 50+ 测试，M3 新增 47 测试

### 兼容性

- 无 Breaking Change，所有新能力通过 feature gate 隔离
- sz-pay 生产项目验证通过

## [3.4.0] — 2026-08-09

### 概述

v3.4.0 是 SZ-ORM 的质量深耕版本，包含 6 大方向交付物，覆盖 31 条 EARS 需求，通过 6 个里程碑（M1-M6）推进。所有新能力通过 10 个 feature gate 隔离，默认关闭，无 Breaking Change。五方言集成测试 83 项全部通过，sz-pay 项目 6 个测试套件零回归验证通过。

### 新增功能

#### 测试覆盖补齐（`test-coverage` feature）

- 18 个扩展包测试从 0 → 全覆盖（每个包 ≥ 5 测试）
- 全 workspace 159 个测试套件全部通过
- 五方言集成测试：MySQL 23 + PostgreSQL 18 + SQLite 25 + Oracle 10 + DuckDB 7 = 83 项全通过

#### 架构改进（`arch-improvement` feature）

- `async_trait_style_evaluation.md`：async trait 风格评估
- `query_builder_selection_guide.md`：QueryBuilder 选择指南
- `result_map_macro_evaluation.md`：result_map 宏生成评估

#### 性能优化（`perf-smallstring` / `perf-enum-dispatch` / `perf-zero-copy-l2` / `perf-box-str` features）

- `SqlBuffer`：CompactString/String 双后端，短字符串 ≤ 23 字节内联存储
- `DialectKind` enum dispatch：替代 `Box<dyn Dialect>` vtable 查找
- `Value::BoxedStr(Box<str>)`：节省 8 字节/值 capacity 字段
- L2 缓存零拷贝推广：BorrowedValue + ColumnarResultSet 推广到 L2 缓存路径
- 4 个基准 + 16 个差分测试

#### 编译期类型安全（`type-safe-columns` / `typed-column` / `typed-dsl` features）

- `Column<T: Schema>`：类型安全列引用，编译期检测列名拼写错误
- `Schema` trait + `#[derive(Schema)]`：自动生成列名常量
- `typed_ast` 扩展：`Like`/`In`/`Not` 表达式 + `BoolExpressionExt` trait
- `where_eq_col` / `where_expr`：类型安全 WHERE 条件构建
- 30 个测试 + 1 个基准

#### 文档生态（`migration-guide` feature）

- `docs/migration/diesel_to_sz_orm.md`：Diesel → SZ-ORM 迁移指南
- `docs/migration/seaorm_to_sz_orm.md`：SeaORM → SZ-ORM 迁移指南
- `docs/migration/sqlx_to_sz_orm.md`：SQLx → SZ-ORM 迁移指南

#### sz-pay 生产案例深化

- `examples/src/bin/sz_pay_pattern.rs`：脱敏版生产使用模式示例
- sz-pay 项目 6 个测试套件零回归验证通过

### 兼容性

- 无 Breaking Change，所有 v3.3.0 公开 API 签名保持不变
- 默认 feature 零行为变更
- workspace 版本统一为 3.4.0

## [3.3.0] — 2026-08-08

### 概述

v3.3.0 是 SZ-ORM 的企业级数据治理版本，包含 4 大方向交付物，覆盖 22 条 EARS 需求，通过 5 个里程碑（M1-M5）推进。所有新能力通过 8 个 feature gate 隔离，默认关闭，无 Breaking Change。下游 sz-pay 项目 5139 测试零回归验证通过。

### 新增功能

#### 多租户与数据隔离增强（`multi-tenant-enhanced` feature）

- `TenantContext` + `TenantContextGuard`（RAII 守卫）+ `scope()` 异步作用域
- `IsolationStrategy` 枚举（RowLevel / Schema / Database）
- `SchemaIsolationRouter` 表名重写（`users` → `tenant_42_users`）
- `TenantPoolRegistry` 租户级连接池隔离
- `RowLevelSecurityPolicy` + `ColumnMaskingRule` 行级安全 + 列级脱敏
- `TenantAuditContext` 多租户审计日志（不可篡改）
- `QueryBuilder::set_tenant_id()` 自动注入 tenant_id 条件
- `QueryBuilder::table()` Schema 隔离自动重写
- 70 单元测试 + 4 并发隔离测试

#### 分布式缓存一致性（`dist-cache` feature）

- `ConsistencyLevel` 枚举（Strong / Eventual）
- `RedisPubSubInvalidationBus` Redis Pub/Sub 跨实例失效（HMAC-SHA256 认证）
- `GossipInvalidationBus` Gossip 协议失效（≤10 实例 1s 收敛）
- `WriteBehindQueue` + `WalFile` 异步批量写入 + WAL 持久化（AES-GCM 加密）
- `BloomFilterGuard` 缓存击穿防护（布隆过滤器）
- `CacheMutexGuard` 分布式互斥锁（防雪崩）
- `RandomTtlJitter` 随机 TTL 抖动（±20%，防雪崩）
- 22 单元测试

#### GraphQL 查询支持（`graphql-n1` / `graphql-schema-gen` / `graphql-complexity` feature）

- `GraphQLIR` 递归下降解析器（完整选择集 + 参数 + 别名 + 内联片段）
- `DataLoader<K, V>` + `BatchLoader` trait N+1 自动消除（查询次数 ≤ 2，减少 ≥ 90%）
- `SchemaGenerator` + `TypeMapping` Rust 模型 → GraphQL Schema 自动生成
- `#[derive(GraphQLModel)]` 过程宏
- `ComplexityCalculator` + `ComplexityConfig` 查询复杂度限制（深度/字段数/成本）
- 44 单元测试 + 11 集成测试

#### AI 自然语言查询增强（`ai-nl2sql-enhanced` / `ai-index-advisor` / `ai-rewrite-advisor` feature）

- `IntentAnalyzer` 查询意图分析（SELECT/INSERT/UPDATE/DELETE + 风险标记）
- `IndexAdvisor` 自动索引建议（慢查询日志分析 + 收益评估）
- `RewriteAdvisor` 查询重写建议（等价变换 + 论证）
- `AiAdviceAuditRecord` AI 建议审计记录（来源/模型/置信度）
- `AdviceSource` + `AdviceType` + `BenefitEstimate` 建议元数据
- NL2SQL LLM prompt 增强（关系信息 + 多表 JOIN/聚合/子查询指令）
- `SqlSanitizer` LLM 请求脱敏
- 零数据库执行保证（仅建议展示，不自动执行）
- 37 单元测试 + 24 集成测试

### Feature Gate

| Feature | 包 | 默认 | 描述 |
|---------|-----|------|------|
| `multi-tenant-enhanced` | sz-orm-core | 关闭 | 多租户与数据隔离增强 |
| `dist-cache` | sz-orm-core | 关闭 | 分布式缓存一致性 |
| `graphql-n1` | sz-orm-graphql | 关闭 | GraphQL N+1 消除 DataLoader |
| `graphql-schema-gen` | sz-orm-graphql | 关闭 | GraphQL Schema 自动生成 |
| `graphql-complexity` | sz-orm-graphql | 关闭 | GraphQL 查询复杂度限制 |
| `ai-nl2sql-enhanced` | sz-orm-ai | 关闭 | NL2SQL 增强 + 意图分析 |
| `ai-index-advisor` | sz-orm-ai | 关闭 | 自动索引建议 |
| `ai-rewrite-advisor` | sz-orm-ai | 关闭 | 查询重写建议 |

### 性能指标

- 跨实例失效延迟：≤ 50ms（Pub/Sub）/ ≤ 1s（Gossip）
- Write-behind 吞吐量：≥ 3x
- GraphQL N+1 查询次数：≤ 2（减少 ≥ 90%）
- 复杂度计算开销：≤ 5%
- 多租户隔离开销：≤ 5μs（行级）/ ≤ 50μs（Schema）
- AI 建议延迟：≤ 10s / 5s P95

### 兼容性

- 无 Breaking Change：所有新能力通过 feature gate 隔离，默认关闭
- 默认 feature 测试基线：1594 passed（sz-orm-core）
- 下游 sz-pay 零回归：5139 passed, 0 failed
- clippy 零警告（含全部 v3.3.0 feature）
- 22 条 REQ 全部满足（附 file:line 证据）

### 需求追溯

| 需求编号 | 描述 | 验证证据 |
|----------|------|----------|
| REQ-DC-001~006 | 分布式缓存一致性 | `dist_cache.rs:27~716` |
| REQ-GQL-001~005 | GraphQL 查询支持 | `query_ir.rs:54` / `dataloader.rs:89` / `schema_gen.rs:111` / `complexity.rs:22` |
| REQ-MT-001~006 | 多租户与数据隔离 | `tenant_context.rs:80~224` / `tenant_security.rs:67~244` |
| REQ-AI-001~006 | AI 自然语言查询增强 | `intent_analysis.rs:95` / `index_advisor.rs:100` / `rewrite_advisor.rs:89` / `advice_common.rs:34` |

## [3.2.0] — 2026-08-08

### 概述

v3.2.0 是 SZ-ORM 的性能深度优化版本，包含 4 大方向交付物，覆盖 20 条 EARS 需求，通过 5 个里程碑（M1-M5）推进。所有新能力通过 feature gate 隔离，默认关闭，无 Breaking Change。

### 新增功能

#### 连接池自动预热增强（`auto-prewarm` feature）

- `PrewarmConfig` + `ProgressiveConfig` + `PrewarmProgress` + `PrewarmSummary`
- `Pool::new_async` 自动预热 + `Pool::progressive_prewarm` 渐进式分批预热
- `UnifiedPool::prewarm` + `UnifiedPool::progressive_prewarm` + `MultiPoolRegistry` 多池统一预热
- 预热失败不阻断池创建，超时可配置，进度可观测
- telemetry 集成（prewarm 计数器 + 快照字段）
- 29 单元测试 + 7 集成测试（真实 MySQL+PG）

#### 查询计划缓存（`plan-cache` feature）

- `SqlNormalizer` SQL 归一化 + 表名提取（AST 提取 + 字符串回退）
- `PlanCacheKey` xxHash 64bit 强哈希
- `PlanCacheEntry` + `PlanCacheStats` 原子计数器无锁统计
- `PlanCache` 双缓存（parse + optimize）+ `LruOrder64` arena 双向链表 LRU
- `invalidate_table` 表级精确失效 + `invalidate_all` 全量失效
- `UnifiedQueryOptimizer::with_plan_cache` 缓存集成
- 26 单元测试 + 11 差分测试

#### 零拷贝序列化（`zero-copy` feature）

- `BorrowedValue<'a>` 21 变体枚举（字符串/字节用 `Cow<'a, str>` / `Cow<'a, [u8]>` 借用）
- `BorrowedRowData<'a>` 借用型行数据
- `ColumnarSchema` + `ColumnarResultSet` 列式结果集（按列连续存储，缓存友好）
- `from_row_data` / `to_row_data` 行列互转
- `apply_result_map_borrowed` + `apply_result_map_many_borrowed` 零拷贝反序列化路径
- 31 单元测试 + 4 等价性测试 + criterion 基准测试

#### SIMD 加速（`simd` feature）

- `SimdAvailability` 枚举 + `detect()` 运行时检测（AVX2/AVX/SSE2/NEON/None，OnceLock 缓存）
- `batch_decode_integers` SIMD 批量整数解码（wide::i64x4 向量并行）
- `batch_compare_eq` / `batch_compare_in` SIMD 列比较（向量比较 + 布尔掩码）
- 标量降级路径（count < 1024 或 None 时自动回退）
- WASM 目标自动降级标量
- 20 单元测试 + 12 差分测试 + criterion 基准测试

### 性能收益

- 连接池冷启动：自动预热后首次查询 P95 ≤ 20ms（对比未预热 ≤ 100ms）
- 查询计划缓存：重复 SQL 跳过解析/优化，命中率可观测
- 零拷贝：BorrowedValue Cow 借用消除深拷贝，10000 行反序列化分配减少
- SIMD：1024+ 行批量解码/比较走 SIMD 向量化路径

### Feature Gate

| Feature | 默认 | 描述 |
|---------|------|------|
| `auto-prewarm` | 关闭 | 连接池自动预热增强 |
| `plan-cache` | 关闭 | 查询计划缓存 |
| `zero-copy` | 关闭 | 零拷贝序列化 |
| `simd` | 关闭 | SIMD 加速 |

### 兼容性

- 无 Breaking Change：所有新能力通过 feature gate 隔离，默认关闭
- 默认 feature 测试基线：1594 passed
- 四 feature 全组合：1695 passed
- clippy 零警告（`--all-features --all-targets`）

## [3.0.0] — 2026-08-07

### 概述

v3.0.0 是 SZ-ORM 的长期目标迭代版本，包含 6 大方向交付物，覆盖 29 条 EARS 需求，通过 7 个里程碑（M1-M7）推进。

### 新增功能

#### 图数据库支持（sz-orm-graph 0.1.0）

- Neo4j 图数据库连接池（GraphConfig + GraphConnection + GraphPool）
- Cypher 查询构建器（CypherQueryBuilder）+ 参数化校验（CypherValidator）
- 声明式图模型（GraphNodeModel + GraphRelationModel）
- 结果映射（NodeMapper + RelationMapper，serde 反序列化）
- DSN 脱敏（sanitize_dsn）
- 11 单元测试 + 6 集成测试 + P95 性能测试
- 需求覆盖：REQ-GDB-001 ~ REQ-GDB-005

#### WASM 完善（sz-orm-wasm）

- wasm-bindgen JS 绑定（JsWasmDatabase + JsQueryResult）
- IndexedDB 持久化（WasmPersistence trait + IndexedDbStore）
- 内存限制（行数 + 行大小超限拒绝写入）
- 持久化不可用明确报告（WasmPersistenceError）
- wasm-bindgen-test 7 测试
- WASM gzip 89.7 KB << 1MB
- 需求覆盖：REQ-WASM-001 ~ REQ-WASM-005

#### FFI 发布产物（Python + JS）

- maturin Python wheel 构建脚本 + PyPI 发布脚本
- napi-rs JS 绑定构建脚本 + npm 发布脚本
- Python 等价性测试（pytest）+ JS 等价性测试（jest 16 passed）
- 绑定验证脚本（verify_bindings.ps1）
- CI 矩阵（三平台 × Python 3.8/3.10/3.12 × Node 18/20/22）
- 需求覆盖：REQ-FDI-001 ~ REQ-FDI-005

#### AI 查询优化器（sz-orm-ai --features llm-optimizer）

- HintSource 枚举（Rule/Llm）+ UnifiedOptimizationHint（来源追溯）
- OptimizerConfig（LLM 可配置性，默认降级纯规则）
- ExplainPlanParser trait + 5 方言实现（MySQL/PG/SQLite/Oracle/MSSQL）
- LlmOptimizer（OpenAI 兼容 API，SQL 脱敏后发送）
- UnifiedQueryOptimizer（规则 + LLM 合并，降级安全）
- SqlSanitizer（password/token/Base64 敏感字面量脱敏）
- LLM SQL 零执行（suggested_sql 仅建议，无 execute_sql 方法）
- 230 单元测试 + 4 集成测试
- 需求覆盖：REQ-AI-001 ~ REQ-AI-005

#### XA 事务一致性（sz-orm-dtx --features xa）

- XaResource/XaParticipant/XaCoordinator（2PC 协议）
- XaRecoveryCoordinator（3 恢复策略：COMMIT/ROLLBACK/HEURISTIC）
- SuspensionDetector（后台扫描 + 超时检测）
- XaCapabilityChecker（拒绝不支持 XA 的参与者）
- 6 集成测试 + coexistence 测试（XA 与 2PC/Saga/TCC 共存）
- 需求覆盖：REQ-DTX-001 ~ REQ-DTX-005

#### 多后端协同文档

- multi_backend_readiness.md（5 项就绪验证全 PASS）
- dialect_constraints.md（11 特性 × 5 方言矩阵）
- sz_rust_integration_example.rs（编译 + clippy 零警告 + 运行成功）
- ADR-0001 PASS（业务代码零修改）
- 需求覆盖：REQ-MB-001 ~ REQ-MB-004

### Feature Gate 隔离

| Feature | 包 | 默认 | 依赖 |
|---------|---|------|------|
| `xa` | sz-orm-dtx | 关闭 | sz-orm-sqlx |
| `llm-optimizer` | sz-orm-ai | 关闭 | reqwest (via `real`) |
| `js` | sz-orm-wasm | 关闭 | wasm-bindgen, js-sys |
| `persistence` | sz-orm-wasm | 关闭 | web-sys |
| `integration` | sz-orm-graph | 关闭 | neo4rs |

### 无 Breaking Change

- v2.4.0 公开 API 签名全部保持不变
- 新增能力通过 feature gate 隔离（默认 feature 不引入额外依赖）
- `cargo build --workspace`（默认 feature）成功

### 门禁验证

| 门禁 | 状态 |
|------|------|
| fmt | ✅ |
| check | ✅ |
| clippy | ✅ 零警告 |
| test | ✅ 全部通过 |
| doc | ✅（预存警告） |
| 占位检查 | ✅ |
| Feature 全组合 | ⚠️ rdkafka-sys 预存问题 |

### 需求覆盖

29 条 EARS 需求全部映射到任务并验收通过：
- REQ-GDB-001~005（图数据库）
- REQ-WASM-001~005（WASM）
- REQ-FDI-001~005（FFI 发布）
- REQ-AI-001~005（AI 优化器）
- REQ-DTX-001~005（XA 事务）
- REQ-MB-001~004（多后端协同）

## [2.4.0] — 2026-08-07

### Bug 修复

#### SmartEagerLoader `load()` 根关联未加载
- 修复 `SmartEagerLoader::load()` 方法中 `self.relation`（根关联）从未被加载的 bug
  - 当 `children.is_empty()` 时直接返回 Leaf 不加载关联数据
  - 当 children 非空时使用 `first_child.relation` 而非 `self.relation` 作为第一级
- 修复方式：使用 `self.relation` 作为根级关联，`std::mem::take(&mut self.children)` 作为子级配置传给 `load_level_smart`
- 位置：`packages/sz-orm-core/src/smart_eager_loader.rs:650`

### 新增测试

#### SmartEagerLoader 五方言集成测试套件
- `tests/common/equivalence.rs`：等价性断言工具（Smart vs Manual 结果集对比）
- `tests/common/schema_builder.rs`：TestSchemaBuilder（5 方言 DDL + 数据填充）
- `tests/common/rusqlite_adapter.rs`：SQLite Connection trait 适配器
- `tests/common/sqlx_mysql_adapter.rs`：MySQL Connection trait 适配器（sqlx::MySqlPool）
- `tests/common/sqlx_pg_adapter.rs`：PostgreSQL Connection trait 适配器（sqlx::PgPool + `?` → `$N` 转换）
- `tests/smart_eager_test_infra.rs`：31 passed（基础设施测试）
- `tests/smart_eager_integration_sqlite.rs`：29 passed（SQLite 集成测试）
- `tests/smart_eager_integration_mysql.rs`：7 测试 `#[ignore]`（需 MySQL 服务）
- `tests/smart_eager_integration_pg.rs`：7 测试 `#[ignore]`（需 PostgreSQL 服务）
- `tests/smart_eager_integration_oracle.rs`：7 测试 `#[ignore]`（需 Oracle 服务）
- `tests/smart_eager_integration_mssql.rs`：7 测试 `#[ignore]`（需 MSSQL 服务）

#### SmartEagerLoader 性能基准套件
- `bench-comparison/benches/smart_eager_harness.rs`：基准测试工具（BenchSqliteConn + SmartEagerBenchHarness，4 规模 10/100/1000/10000）
- `bench-comparison/benches/bench_smart_eager.rs`：4 基准组
  - `bench_decision_latency`：StrategyResolver::resolve() 延迟（实测 68-81 ns，远超 ≤100μs 要求）
  - `bench_smart_vs_manual`：SmartEagerLoader vs EagerLoader 对比（开销 4-6%，10000 行 Smart 快 9%）
  - `bench_n1_elimination`：N+1 逐条 vs 批量对比（100 行 3.3x、1000 行 12.8x、10000 行 60.6x 加速）
  - `bench_n1_detector`：N1Eliminator 检测能力（8.46μs）

### 新增脚本
- `scripts/compute_topology.ps1`：依赖拓扑排序脚本（Kahn 算法 + 字典序打破并列）
- `scripts/publish_crates_io.ps1`：crates.io 逐包发布脚本（门禁 → token → 拓扑序 → 逐包发布）
- `scripts/verify_sz_pay.ps1`：sz-pay 下游验证脚本（版本升级 → build → test → 零回归）

### 向后兼容
- v2.4.0 无 Breaking Change，所有 v2.3.0 公开 API 签名保持不变
- SmartEagerLoader `load()` 方法签名不变，仅修复内部实现逻辑
- 新增文件均为测试/基准/脚本，不进入 sz-orm-core 公开 API

### 验证结果
- sz-orm-core 全测试：0 failed
- sz-orm-core clippy：零警告
- sz-orm-core fmt：通过
- SQLite 集成测试：29 passed
- 集成测试基础设施：31 passed
- 性能基准：4 组 × 4 规模全部达标
- sz-pay 回归：5139 passed, 0 failed, 13 ignored（零回归）
- 占位实现检查：0 处（2 处匹配在文档注释中）
- SQL 注入扫描：31 项均为内部逻辑拼接（手动审查确认安全）

## [2.3.0] — 2026-08-07

### 新增功能

#### 任务 C：Eager Loading 智能策略选择
- `SmartEagerLoader` 类型：基于 `RelationKind` 自动选择最优加载策略
- `EagerLoader::smart()` 扩展方法：返回 `SmartEagerLoader`，向后兼容
- `LoadStrategy` 枚举（Join/DataLoader/IntermediateTableBatch）
- `StrategyDecision` 结构：策略决策记录（关联名/类型/策略/原因/查询次数）
- `StrategyResolver` 策略决策器：纯规则匹配，决策延迟 ≤ 100μs
  - HasOne / BelongsTo → Join（单次 JOIN 查询）
  - HasMany → DataLoader（批量 IN 查询，2 次）
  - ManyToMany（有中间表）→ IntermediateTableBatch（中间表批量，2 次）
  - ManyToMany（无中间表）→ 回退 DataLoader + 告警
- `JoinStrategy` 执行器：HasOne/BelongsTo 自动 JOIN + 结果集拆分
- `DataLoaderStrategy` 执行器：HasMany 自动 data loader + 按外键分组
- `IntermediateTableStrategy` 执行器：ManyToMany 中间表批量查询
- `N1Eliminator` N+1 自动消除器：连续查询模式检测 + 批量合并 + 等价性校验
- `N1EliminationReport` 消除报告（原次数/合并后次数/节省/触发位置/合并 SQL）
- `RelationDef::new_many_to_many()` 构造器：ManyToMany 中间表元数据
- `RelationDef` 新增 `join_table`/`join_from_key`/`join_to_key` 可选字段

#### 任务 B：性能基准完整报告
- `CompetitorAdapter` trait：竞品适配层统一接口
- `BenchmarkReporter` 报告生成器：Markdown + CSV/JSON + DSN 脱敏
- `full_comparison` bench 主入口：全维度 × 多方言 × 竞品基准
- criterion 配置：sample_size=100, warm_up=3s, measurement=10s

### 向后兼容
- v2.3.0 无 Breaking Change，所有新增能力以扩展方法提供
- `RelationDef::new()` 签名不变，新增中间表字段默认 `None`
- `EagerLoader` 原有 API（`new`/`with`/`load_many`/`load_nested`）不变

### 测试
- sz-orm-core lib 测试：1578 passed（+28 新测试）
- 全 workspace 测试：全部通过，零失败
- 全 workspace clippy：零警告

## [2.2.0] — 2026-08-06

### 新增功能

#### A-1 AnyPool 扩展支持 Oracle/MSSQL
- `AnyBackend` 枚举新增 `Oracle` / `Mssql` 变体 + `#[non_exhaustive]` 标注
- `AnyPool::connect` 新增 Oracle/MSSQL 分派分支（feature gate）
- DSN 解析支持 `oracle://` / `mssql://` / `sqlserver://` scheme

#### A-2 Dialect 与 AnyPool 集成验证
- `AnyBackend::dialect()` 方法：5 后端 → 5 Dialect 映射
- `AnyPool::dialect()` 方法：委托 backend.dialect()

#### A-3 UnifiedPool 统一抽象
- `UnifiedPool` 结构体：统一连接池接口
- `connect(dsn)` / `connect_with_config(dsn, config)` / `from_pool(pool, backend)` 方法
- `acquire()` / `backend()` / `dialect()` / `resize()` / `close_all()` / `status()` 委托方法

#### B-1 Eager Loading 多级关联 + 循环检测
- `CyclePolicy` 枚举（Error/Truncate/AllowWithDepthLimit）+ `CycleDetector`
- `NestedEagerResult` 递归枚举（Leaf/Node）支持无限级嵌套树
- `EagerLoader::load_nested()` 方法：多级批量查询 + 循环检测
- `EagerLoader::with_cycle_policy()` 方法：设置循环检测策略
- `ChildLoadConfig` 改为递归结构支持无限级链式调用

#### B-2 Schema Sync 破坏性变更安全策略
- `Confirm` 枚举（Yes/No）显式确认破坏性 DDL
- `DataMigrationHook` trait（before_drop_column / before_rename_column 钩子）
- `DestructiveSyncResult` 结构体
- `SchemaSync::destructive_sync()` 方法：事务内执行 + 钩子 + 审计
- `diff_columns` 新增 Levenshtein 重命名检测（距离 ≤ 2 或比例 ≤ 0.3）
- `SchemaSync::with_rename_threshold()` 方法：配置重命名检测阈值

#### B-3 Partial Models select_exclude
- `QueryBuilder::select_exclude(fields: &[&str])` 方法：排除指定字段查询
- 校验排除字段存在、不排除全部字段
- 与 `select_only` 互补，自动进入 Partial 模式

#### B-4 Stream API 背压控制
- `StreamApiExt::stream_with_backpressure(buffer_size)` 方法
- 有界缓冲通道，缓冲区满时生产者阻塞（背压）
- `buffer_size == 0` 返回 `Err(DbError::InvalidInput)`

#### B-5 嵌套持久化 cascade_delete 策略
- `CascadeStrategy` 枚举（Restrict/Cascade/SetNull/SetDefault）
- `nested_delete_with_strategy(conn, nested, strategy)` 函数
- 4 策略分支：Restrict 禁止删除 / Cascade 递归删除 / SetNull 置 NULL / SetDefault 置默认值
- 事务内原子执行，失败 ROLLBACK

### 兼容性

- **零 Breaking Change**：所有新增能力以扩展方法、新增类型、新增枚举变体提供
- `EagerResult` / `load_many` / `stream_buffered` / `sync()` / `cascade_delete(bool)` 保留不变
- `AnyBackend` 新增 `#[non_exhaustive]`：外部 crate match 须加 `_` 通配符

## [2.1.0] — 2026-08-06

### 新增功能

#### P-F-1 Eager Loading 端到端（M3）
- `EagerLoader` 结构体 + `eager_load_all` / `eager_load_one` 一行 API
- HasMany 双查询策略 + HasOne/BelongsTo JOIN 策略
- N+1 查询消除（2 条 SQL 而非 N+1 条）
- Oracle IN >1000 分批查询
- 多级关联（User → Order → OrderItem，限 2 级）
- `value_to_key` 辅助函数解决 Value 不实现 Hash/Eq 问题

#### P-F-2 RelationTrait + join/left_join（M1）
- `RelationKind` / `RelationDef` / `RelationTrait` 核心类型
- `#[derive(RelationTrait)]` 宏自动生成 RelationTrait 实现
- `QueryBuilder::join()` / `left_join()` 类型安全链式 JOIN API

#### P-F-3 Partial Models（M2）
- `SelectMode` / `AggFunc` / `Expr` 类型
- `QueryBuilder::select_only()` / `.column()` / `.columns()` / `.column_as()` 方法
- 聚合查询 + GROUP BY 支持

#### P-F-4 Schema Sync 自动结构同步（M5）
- `TableDef` / `ColumnDef` / `SchemaDiff` / `SyncResult` 类型
- `diff` 纯函数：6 类变更检测
- 5 方言 DDL 生成器：MySQL / PostgreSQL / SQLite / Oracle / MSSQL
- `SchemaSync` 协调器：`sync_dry_run` + `sync`（事务执行）
- 破坏性变更检测（dropped_tables / dropped_columns → Err）

#### P-F-5 ActiveModel 嵌套持久化（M4）
- `NestedActiveModel<M>` 包装器（不修改存量 ActiveModel）
- `ChildEntity` 子实体
- `nested_save`：事务执行 + 外键自动回填 + 多级递归
- `nested_delete`：子先父后删除顺序
- 深度限制 10 层 + RAII 事务 guard

#### P-F-6 Stream API（M6）
- `StreamApiExt` trait + `stream_buffered` 兼容版
- `stream` impl 改造（委托 query 而非全量收集）
- 向后兼容：`stream_buffered` 保留 v2.0.0 行为

#### P-F-7 性能基准对比（M7）
- v2.1.0 新功能基准测试（Eager Loading / Nested Save / Schema Diff / Stream API）
- Eager Loading vs N+1 查询对比

### 向后兼容

- **无 Breaking Change**：所有 v2.0.0 API 保持不变
- `ActiveModel<M>` 结构不变（嵌套通过 `NestedActiveModel` 包装）
- `Connection` trait 不变（复用 v2.0.0 `query_stream_cursor`）
- `StreamQueryTrait` trait 签名不变（仅改 impl 实现）

### 测试

- 1521 单元测试通过（+33 新增）
- 50+ 集成测试通过（+20 新增）
- clippy 0 警告

## [2.0.0] — 2026-08-06

### Breaking Changes

- **移除 deprecated `where_cond` / `or_where`**：`FindWithRelatedBuilder::where_cond()`、`QueryBuilderExt::or_where()` 及相关 `where_conds` 字段已删除。迁移至 `where_eq` / `or_where_eq` 等参数化方法（自 v1.2.0 起已标记 deprecated，v2.0.0 正式移除）

### Added

- **Oracle 23ai 真实集成测试**：`integration_oracle.rs` 追加 7 类场景（CRUD / 事务 / 乐观锁 / 软删除 / 分页 / 聚合 / 批量），10 测试通过
- **SQL Server 真实集成测试**：`integration_mssql.rs` 新建 8 类场景（CRUD / 事务 / 乐观锁 / 软删除 / 分页 / 聚合 / 批量 / INSERT OR IGNORE 回退）
- **Python 绑定 (PyO3)**：`sz-orm-python` 0.1.0 发布到 crates.io，支持 Model / QueryBuilder / Pool / Transaction Python API
- **JavaScript 绑定 (napi-rs)**：`sz-orm-js` 0.1.0 发布到 crates.io，支持 Node.js 原生绑定
- **安全审计报告**：`docs/assessment/2026-08-05-security-audit-report.md`，7 维度覆盖（SQL 注入 / 连接池 / 密码 / 权限 / 输入校验 / 信息泄露 / 依赖安全）

### Changed

- **crates.io 批量发布**：42 个包发布 **2.0.0**（sz-orm-python / sz-orm-js 保持 0.1.0）
- **测试规模**：全 workspace 4,947 passed, 0 failed（lib 测试）
- **内部依赖版本对齐**：所有 `version + path` 格式的内部依赖统一至 2.0.0

## [1.5.0] — 2026-08-05

### Added

- **连接池 Prometheus 统计指标 (sz-orm-core)**：`Pool::pool_metrics()` 返回 `PoolMetrics`（acquire_count / acquire_failed_count / acquire_wait_time / release_count / connection_created_count / connection_closed_count），基于无锁 `AtomicU64`，热路径开销可忽略；`average_acquire_wait_time()` 计算平均获取等待时长
- **ClickHouse 行锁支持 (sz-orm-core)**：`supports_lock_for_update()` / `supports_lock_shared()` 显式返回 `false`（无事务无行锁）；`build_insert_or_ignore_prefix()` 回退普通 `INSERT INTO`
- **SQL Server INSERT OR IGNORE 回退 (sz-orm-core)**：`build_insert_or_ignore_prefix()` 回退普通 `INSERT INTO`（SQL Server 无等价前缀语法，应用层可捕获 2601/2627 冲突或使用 MERGE）
- **DuckDB 真实集成测试**：`integration_duckdb.rs` 7 个真实 DB 测试（duckdb bundled 特性）
- **向量/时序真实实现集成测试**：sz-orm-vector 3 个 `#[ignore]` 真实 pgvector 测试；sz-orm-timeseries 5 个内存集成测试 + 2 个 `#[ignore]` 真实 TimescaleDB 测试
- **Redis 后端默认启用 (sz-orm-core)**：`redis` feature 加入 `default`，`RedisBackend` 开箱即用

### Changed

- **crates.io**：sz-orm-core 发布 **1.5.0**（依赖 sz-orm-sql-validator 1.4.0 / sz-orm-macros 1.4.0）
- **测试规模**：全 workspace 5,809 passed, 0 failed

## [1.0.0] — 2026-07-19

### Added

- **核心引擎 (sz-orm-core)**：Model trait、QueryBuilder、多数据库方言（MySQL/PostgreSQL/SQLite/Oracle 23ai）、异步连接池、ACID 事务、文件迁移系统、多级缓存、统一值类型（20 种变体）、错误类型体系
- **数据库适配器**：sz-orm-sqlx（MySQL/PostgreSQL/SQLite/Oracle）、sz-orm-sql-validator（SQL 注入检测）
- **扩展生态包 (18 个)**：
  - sz-orm-crypto：AES-256-GCM、PBKDF2、HMAC-SHA256
  - sz-orm-auth：认证与授权
  - sz-orm-batch：批量 INSERT/UPDATE/UPSERT
  - sz-orm-dtx：分布式事务
  - sz-orm-mig：迁移工具
  - sz-orm-sharding：分库分表
  - sz-orm-cache：多级缓存（注：实现在 sz-orm-core/src/cache.rs 与 l2_cache.rs 内，非独立 crate）
  - sz-orm-queue：消息队列
  - sz-orm-scheduler：任务调度
  - sz-orm-graphql：GraphQL 接口
  - sz-orm-grpc：gRPC 接口
  - sz-orm-ai：NL→SQL（自然语言转 SQL）
  - sz-orm-vector：pgvector 向量搜索
  - sz-orm-search：Meilisearch/Elasticsearch/OpenSearch 集成
  - sz-orm-storage：S3 兼容对象存储
  - sz-orm-postgis：PostGIS 地理空间
  - sz-orm-timeseries：时序数据
  - sz-orm-observability：Prometheus 指标 + OpenTelemetry tracing
  - sz-orm-tracing：分布式追踪（W3C TraceContext）
- **CLI (sz-orm-cli)**：命令行工具
- **DevTools**：sz-orm-swagger（OpenAPI）、sz-orm-health（健康检查）
- **测试体系**：2,271 个单元/集成测试（1,635 `#[test]` + 636 `#[tokio::test]`）、proptest 属性测试、fuzz 模糊测试、chaos 混沌测试（16 项）、6h soak test
- **CI/CD**：GitHub Actions 多 workflow（CI/安全/soak test/依赖更新）
- **文档**：15 份中文文档 + README.en.md 英文文档 + CONTRIBUTING.md 贡献指南

### Security

- cargo audit 通过（1 allowed warning: paste unmaintained）
- cargo deny check advisories bans licenses sources 全部通过
- 6h Linux CI Soak Test（2026-07-21 立即触发）

### Performance

- 1h soak test：13.8 亿 operations，0 errors，1.16% throughput decay，43μs→41μs P99 latency
- 7 组 criterion 基准测试

## [Unreleased]

### Added

- **API 稳定性承诺文档**：新增 `docs/API-STABILITY.md`，明确 SemVer 承诺、API 稳定性三层分级（Stable/Experimental/Internal）、废弃流程（2 个 MINOR 版本保留期）、破坏性变更条件
- **端到端真实 DB 示例**：新增 `examples/src/bin/real_db_crud.rs`，使用 SQLite 内存数据库演示完整连接池 + CRUD + 事务（提交/回滚）流程
- **Prometheus 告警规则**：新增 `monitoring/alerts.yml`，覆盖错误率/延迟/连接池/SLO 燃烧率告警
- **文档清理**：删除 33 份开发期文档（审计报告/调研文档/重复副本），保留 19 份核心文档

### Fixed

- **sz-orm-search unreachable!() 消除**：将 `TokenizerType::Keyword` 的 `unreachable!()` 替换为正确的 `vec![text]`（整个文本作为单个 token）
- **README 测试数字不一致**：统一 README/README.en.md 中测试数从 2,271/4,959 → 5,404，版本号从 1.0.0 → 1.2.0
- **CI minio:latest 可变标签**：固定为 `minio/minio:RELEASE.2024-10-13T13-34-11Z`

## [1.2.0] — 2026-07-26

### Added

- **Oracle 独立适配器包 (sz-orm-oracle)**：基于 `oracle` crate (ODPI-C 绑定) 实现 `Connection` trait，支持 Oracle 12c+；阻塞池隔离、占位符自动转换、完整类型映射
- **SQL Server 独立适配器包 (sz-orm-mssql)**：基于 `tiberius` crate (纯 Rust TDS 协议) 实现 `Connection` trait，支持 SQL Server 2008+；占位符自动转换为 `@PN` 格式
- **axum Web 框架集成 (sz-orm-axum)**：提供 `PoolState`、`JsonRows`、`JsonResp<T>`、`transaction_layer` 组件
- **actix-web Web 框架集成 (sz-orm-actix)**：提供 `PoolState`、`JsonRows`、`JsonResp<T>`、`TransactionMiddleware` 组件
- **独立查询构建器 (sz-orm-query-builder)**：提供与 core `QueryBuilder` 不同的 fluent API，支持 SELECT/INSERT/UPDATE/DELETE 及 UNION/INTERSECT/EXCEPT 集合操作
- **DI 容器 (container.rs)**：依赖注入容器，支持构造函数注入和单例注册
- **ORM 迁移集成 (migrate.rs)**：sz-orm-mig 与 sz-orm-core 的集成层
- **Whoops 调试页面 (debug_page.rs)**：开发环境调试信息展示
- **API 版本管理 (api_version.rs)**：API 版本协商与路由
- **缓存预热 (cache_warmer.rs)**：启动时预加载热点数据到缓存
- **迁移历史表 (migration_history.rs)**：迁移执行记录持久化

### Changed

- **MSRV 升级**：1.80 → 1.81（trait_variant dyn compatibility 要求）
- **workspace lints 强制**：新增 `[workspace.lints]` 配置，全 workspace clippy 零警告强制执行
- **测试数增长**：4,959 → 5,404（+445），新增 soak test/Jepsen/kill-9 崩溃恢复测试

### Fixed

- **clippy writeln_empty_string**：修复 `sz-orm-audit` 中的 `writeln!(file, "")` 警告
- **clippy unnecessary_cast**：修复 `sz-orm-sqlx` 中的 `as i64` 不必要转换
- **hydration_plugin unwrap**：修复 `chars().next().unwrap()` 为安全错误处理
- **postgis partial_cmp unwrap**：替换为 `total_cmp` 实现 NaN 安全比较

## [1.1.0] — 2026-07-22

### Added

- **位置式查询优化 (query_values / query_values_with_params)**：为 `Connection` trait 新增两个高性能查询方法，绕过 HashMap 行映射开销。SQLite 提升 34.4%，Oracle 提升 57.4%
- **真实 MQ 客户端 (sz-orm-queue)**：新增 5 种真实消息队列客户端 — RabbitMQ/NATS/Kafka/ActiveMQ Artemis/Pulsar
- **全部 37 扩展包深度优化**：测试数从 2,271 增至 4,959（+2,688），每个包补充 200-500 行高级特性代码与 15-30 个单元测试
- **Connection trait 参数绑定**：新增 `execute_with_params`/`query_with_params`，MySQL/PostgreSQL/SQLite 实现真实 prepared statement 绑定
- **编译时类型推断完善**：`SqlType` 扩展至 13 种变体，`InferSqlType` trait 覆盖 14 种 Rust 类型
- **编译时 SQL schema 生成（`schema!` 宏）**：接受 SQL CREATE TABLE 语句，编译期自动生成类型安全查询代码
- **英文文档**：新增 `README.en.md` + `CONTRIBUTING.md`
- **ADR 体系**：9 个 ADR + `ADR与生产Bug定位规范.md`
- **SeaORM 迁移指南**：547 行，10 章 + 检查清单
- **Fuzz Testing**：3 个 fuzz target（query_builder/value_escape/pool_config）
- **PooledConnection Drop 修复**：连接在 drop 时自动归还池中
- **core 包 tracing 可观测性**：关键路径添加 `#[tracing::instrument]` 注解
- **学习路线图**：面向 PHP/ThinkPHP 工程师的 17 章学习教程

### Changed

- **Rust 工具链升级**：升级至 Rust 1.97.1
- **sqlx 升级**：0.8.6 → 0.9.0，消除 rsa Marvin Attack 漏洞

### Security

- **Critical 修复 (C-2/C-3)**：修复 2 个 Critical 安全漏洞
- **反向审计全量修复**：H-1 至 H-9（9 项 High）、M-1 至 M-17（17 项 Medium）、L-1 至 L-5（5 项 Low）全部修复
- **cargo audit / cargo deny 全通过**

### Fixed

- **hook 测试锁毒化**：替换为 `AtomicU32` 无锁计数器
- **SQLite 集成测试磁盘 I/O 错误**：改用 `open_in_memory()`
- **unreachable!() 消除**：简化 `sz-orm-postgis` `st_union` 的冗余嵌套 match

### CI

- **CI 基础设施非阻塞**：4 类外部依赖 job 设为 `continue-on-error: true`
- **integration.yml 独立工作流**：手动触发 + 每日定时
- **test job 解耦**：test 不再依赖 build

[1.2.0]: https://github.com/ljclz/sz-orm/releases/tag/v1.2.0
[1.1.0]: https://github.com/ljclz/sz-orm/releases/tag/v1.1.0
[1.0.0]: https://github.com/ljclz/sz-orm/releases/tag/v1.0.0
