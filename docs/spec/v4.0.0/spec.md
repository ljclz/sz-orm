# sz-orm v4.0.0 需求规格说明书

> 版本：v4.0.0（AI 自动调优闭环 + 多 LLM 模型 + 混合搜索 + 数据 lineage + 分片 rebalance + failover 自动化 + 服务网格 + GraphQL 深度集成 + CDC）
> 基线：v3.9.0（criterion benchmark 套件 + semver/API 稳定性 + 数据验证框架 + 迁移 dry-run/影响分析 + CI/CD 模板 + 流式导出，6760+ tests passed 0 failed）
> 日期：2026-08-11
> 需求格式：EARS（Ubiquitous / Event-driven / State-driven / Optional / Unwanted）
> 文档定位：需求规格（What to build），不含技术设计（How to build）
> 优先级声明：九项任务按"P0（AI 调优闭环/多 LLM，AI 能力闭环与灵活性）→ P1（混合搜索/data lineage/分片 rebalance/failover，搜索/治理/运维/高可用）→ P2（服务网格/GraphQL/CDC，云原生/生态/实时同步）"序推进
> 需求编号约定：REQ-V40-xxx（v4.0.0 需求项，REQ-V40-001 ~ REQ-V40-009）
> 缺陷来源：`docs/sz-orm与同类产品对比分析.md` 第 5.3 节 AI 能力缺失分析（`:269-284`）+ 第 5.4 节通用能力缺失分析（`:286-304`）+ 第 6.2 节中期优化方向（`:323-335`）
> 兼容性铁律：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容，v3.9.0 已验收测试基线（6760+ passed）不回退；sz-pay 生产依赖（从 crates.io 拉取 sz-orm-* 6 个包）不得被破坏；五方言覆盖：MySQL/PostgreSQL/SQLite/Oracle/MSSQL
> 范围声明：本版本聚焦中期 P0-P2 共 9 项任务；长期（v4.x+ Go/Java/C++ 绑定/社区扩展/可视化 Schema/缓存一致性/跨语言事务/Informix 真实驱动）在后续版本规划；crates.io 全 46 包发布与英文文档翻译沿用 v3.9.0 末尾计划，本版本不涉及

---

# 1. 组件定位

## 1.1 核心职责

本组件负责交付 sz-orm v4.0.0 的九项中期优化能力：(1) AI 自动调优闭环（检测慢查询→生成建议→自动执行→效果验证）；(2) 多 LLM 模型支持（Claude/Gemini/本地模型 provider 抽象）；(3) 混合搜索（向量+全文+结构化联合排序）；(4) 数据 lineage（字段级血缘追踪）；(5) 分片自动 rebalance（扩缩容自动迁移）；(6) 数据库 failover 自动化（健康检测触发自动切换）；(7) 服务网格集成（Istio/Linkerd）；(8) GraphQL 深度集成 async-graphql；(9) CDC 变更数据捕获。所有能力通过 feature gate 隔离，不破坏现有 API 兼容性与 v3.9.0 已验收基线。

## 1.2 核心输入

1. **v3.9.0 已验收基线**：criterion benchmark 套件 + semver/API 稳定性 + 数据验证框架 + 迁移 dry-run/影响分析 + CI/CD 模板 + 流式导出，6760+ tests passed 0 failed，作为本版本基准。
2. **对比分析文档第 6.2 节**：`docs/sz-orm与同类产品对比分析.md:323-335` 中期（v4.0.0）优化方向 9 项（P0×2 + P1×4 + P2×3）。
3. **现有能力清单与缺口证据**：
   - **AI 调优**：`packages/sz-orm-ai/src/query_plan_optimizer.rs:515` `UnifiedQueryOptimizer`（统一优化器，已有 rule + llm hint 聚合）、`:177` `OptimizerConfig` + `:207` `with_llm(api_key, model)`（仅 OpenAI 兼容）、`packages/sz-orm-ai/src/index_advisor.rs:100` `IndexAdvisor`（索引建议）、`packages/sz-orm-ai/src/rewrite_advisor.rs:89` `RewriteAdvisor`（重写建议）、`packages/sz-orm-ai/src/explain_parser.rs:50` `ExplainPlanParser` trait（5 方言解析器：MySQL/PG/SQLite/Oracle/MSSQL，`:70/:131/:175/:222/:270`）。缺口：无自动执行（apply 建议）+ 效果验证闭环（对比调优前后耗时）；无 Claude/Gemini/本地模型 provider 抽象（仅 OpenAI 兼容 `with_llm`）。
   - **混合搜索**：`packages/sz-orm-vector/src/lib.rs:189` `PgVectorStore` trait（pgvector 向量存储）+ `:113` `SearchResult` + `:145` `VectorMetric`（Cosine/Euclidean/InnerProduct）、`packages/sz-orm-vector/src/real_pg.rs`（真实 pgvector）、`packages/sz-orm-search/src/lib.rs`（ES/OpenSearch/Meilisearch 全文搜索）+ `elasticsearch_provider.rs`/`opensearch_provider.rs`/`meilisearch_provider.rs`。缺口：无向量+全文+结构化联合排序（hybrid search，跨源融合 + RRF/加权排序）。
   - **数据 lineage**：`packages/sz-orm-audit/src/lib.rs:691` `HashChainEntry` + `:778` `HashChainAuditor`（哈希链审计，防篡改）+ `:862` `verify()`。缺口：无数据血缘追踪（字段级 lineage：source table/column → target table/column，ETL/视图/物化视图依赖图）。
   - **分片 rebalance**：`packages/sz-orm-sharding/src/lib.rs:130` `ShardingRouter`（路由，含一致性哈希/枚举/列表/目录/复合策略 `:60` `ShardingStrategy`）+ `enhanced.rs`（增强分片）+ `cross_shard_tx.rs`（跨分片事务）。缺口：无自动 rebalance（分片扩缩容时自动迁移数据，最小搬迁量计算）。
   - **failover**：`packages/sz-orm-rw/src/lib.rs:331` `ReadWriteRouter` + `:219` `HealthChecker`（健康检查，`failure_threshold`）+ `:37` `SlaveHealth`（Healthy/Unhealthy/Drained）+ `:911` `test_router_failover_to_master_when_all_unhealthy`（已有手动 failover 测试）+ `:939` `test_router_failover_skips_drained_slave`。缺口：无自动 failover（健康检测自动触发主库切换 + 提升 slave 为新主 + 通知上层）。
   - **服务网格**：`packages/sz-orm-observability/src/lib.rs:250` `MetricsRegistry`（Prometheus Counter/Gauge/Histogram）+ `:443` `MetricsAccessControl`（metrics ACL）+ `packages/sz-orm-observability/src/slo.rs`（SLO 燃烧率）+ `sz-orm-tracing`（OTLP 分布式追踪）。缺口：无 Istio/Linkerd 服务网格集成（xDS/envoy 配置、sidecar 自动注入、mTLS 策略、流量治理）。
   - **GraphQL**：`packages/sz-orm-graphql/src/lib.rs:36` `GraphQLSchema` + `:141` `GraphQLSchemaGenerator` + `:182` `GraphQLServer` + `packages/sz-orm-graphql/src/dataloader.rs:89` `DataLoader` + `:74` `BatchLoader` trait（N+1 消除）+ `complexity.rs`（复杂度限制）+ `schema_gen.rs`（schema 生成）。缺口：未深度集成 async-graphql（当前为自研 GraphQL，需对接 async-graphql 生态：Subscription/Relay/Federation/工单化错误处理）。
   - **CDC**：`packages/sz-orm-queue/src/lib.rs`（消息队列 6 provider：RabbitMQ/Kafka/NATS/Pulsar/RocketMQ/ActiveMQ，`real_kafka.rs`/`real_nats.rs`/`real_pulsar.rs`/`lapin_rabbitmq.rs`/`rocketmq.rs`/`real_activemq.rs`）+ `packages/sz-orm-audit/src/lib.rs`（审计日志，可作 CDC 数据源）。缺口：无 CDC（变更数据捕获，捕获数据变更并实时同步到下游，Debezium 风格 ChangeEvent）。
4. **本机数据库连接信息**：MySQL 9.6（`mysql://root:test123@127.0.0.1:3306/sz_orm_test`）、PostgreSQL 18（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）、Oracle 23ai Free（`127.0.0.1:1521/freepdb1`）。
5. **sz-pay 生产依赖证据**：sz-pay 从 crates.io 拉取 sz-orm-* 6 个包，作为 API 兼容性验证的下游基准。
6. **五方言覆盖约束**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL，AI 调优/failover/CDC 须覆盖全部方言（按方言能力适配）。
7. **既有 feature gate 体系**：`packages/sz-orm-core/Cargo.toml` 已有 25+ feature（含 v3.8.0 prod-ready 14 子 feature + v3.9.0 benchmark-suite/data-validation/migration-dry-run/streaming-export），作为新能力 feature gate 隔离的基础。

## 1.3 核心输出

1. **AI 自动调优闭环**：AutoTuningPipeline（检测→建议→执行→验证四阶段）+ 调优报告（前后耗时对比、建议采纳率、回归检测）。
2. **多 LLM 模型支持**：LlmProvider trait + ClaudeProvider/GeminiProvider/LocalLlamaProvider/OpenAIProvider 四实现 + 统一配置切换。
3. **混合搜索**：HybridSearcher（向量+全文+结构化联合查询）+ 融合排序（RRF/加权/级联）+ 统一 SearchResult。
4. **数据 lineage**：LineageTracker（字段级血缘图）+ LineageGraph（source→target 有向无环图）+ 影响分析（变更影响下游表/字段）。
5. **分片自动 rebalance**：ShardRebalancer（扩缩容自动迁移）+ 迁移计划（最小搬迁量、分批、断点续传）+ 迁移报告。
6. **数据库 failover 自动化**：AutoFailoverManager（健康检测→自动切换→提升 slave→通知上层）+ failover 事件日志 + 数据丢失风险评估。
7. **服务网格集成**：ServiceMeshAdapter（Istio/Linkerd）+ xDS 配置生成 + mTLS 策略 + 流量治理（金丝雀/蓝绿/熔断）。
8. **GraphQL 深度集成**：async-graphql 对接（Subscription/Relay/Federation）+ 工单化错误处理 + 现有 DataLoader 复用。
9. **CDC 变更数据捕获**：CdcCapturer（变更事件捕获）+ ChangeEvent（Before/After/Op/Timestamp）+ 下游同步（消息队列/HTTP webhook）。
10. **需求追溯矩阵**：本文档第 7 章，建立需求 ↔ 验收条件映射。
11. **验收标准总览**：本文档第 8 章，按需求项汇总验收条件。

## 1.4 职责边界

本组件**不负责**以下事项：

1. **不破坏既有公开 API**：所有新能力通过 feature gate 隔离，既有公开 API 签名保持完全向后兼容。
2. **不改变既有安全铁律**：任何 WHERE 条件必须参数化，默认禁止 `SELECT *`，N+1 检测自动拦截，沿用既有铁律。
3. **不替换既有 AI 优化器**：既有 `UnifiedQueryOptimizer`（`packages/sz-orm-ai/src/query_plan_optimizer.rs:515`）保留，新增 `AutoTuningPipeline` 编排既有优化器，两者共存。
4. **不替换既有 LLM 调用**：既有 `OptimizerConfig::with_llm`（`:207`，OpenAI 兼容）保留，新增 `LlmProvider` trait 与多 provider 实现，既有 OpenAI 调用包装为 `OpenAIProvider`。
5. **不替换既有向量/全文搜索**：既有 `PgVectorStore`（`sz-orm-vector/src/lib.rs:189`）与 ES/OpenSearch/Meilisearch provider 保留，新增 `HybridSearcher` 融合既有能力，不重复实现。
6. **不替换既有审计**：既有 `HashChainAuditor`（`sz-orm-audit/src/lib.rs:778`）保留，新增 `LineageTracker` 与之并行，lineage 可选写入审计链。
7. **不替换既有分片路由**：既有 `ShardingRouter`（`sz-orm-sharding/src/lib.rs:130`）保留，新增 `ShardRebalancer` 编排既有路由，不修改路由策略。
8. **不替换既有读写分离**：既有 `ReadWriteRouter`（`sz-orm-rw/src/lib.rs:331`）与 `HealthChecker`（`:219`）保留，新增 `AutoFailoverManager` 调用既有健康检查，不修改路由逻辑。
9. **不替换既有 GraphQL**：既有 `GraphQLServer`（`sz-orm-graphql/src/lib.rs:182`）与 `DataLoader`（`dataloader.rs:89`）保留，新增 async-graphql 对接层复用既有 DataLoader。
10. **不负责 sz-pay / sz-rust 下游代码修改**：ADR-0001 严禁修改下游/上游仓库，仅保证 API 兼容性。
11. **不降低既有测试覆盖**：v4.0.0 不得使 v3.9.0 已验收测试基线（6760+ passed）回退，仅增不减。
12. **不负责长期任务**：Go/Java/C++ 绑定/社区扩展/可视化 Schema/缓存一致性/跨语言事务/Informix 真实驱动等在 v4.x+ 规划。
13. **不强制启用新能力**：所有新能力默认关闭或可选启用，避免无配置环境行为变化。
14. **不引入 unsafe**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释，沿用既有 unsafe 零容忍铁律。

---

# 2. 领域术语

**AI 自动调优闭环（AI Auto-Tuning Pipeline）**
: 自动检测慢查询→生成优化建议（索引/重写/Schema）→自动执行建议→验证调优效果的闭环流程，四阶段（Detect→Advise→Apply→Verify）循环执行，对比调优前后耗时量化收益，回归则回滚。
: 备注：区别于既有 `UnifiedQueryOptimizer`（`packages/sz-orm-ai/src/query_plan_optimizer.rs:515`，仅生成建议不自动执行），本版本补自动执行与效果验证闭环。

**多 LLM 模型支持（Multi-LLM Provider）**
: 通过 `LlmProvider` trait 抽象 LLM 调用，支持 Claude（Anthropic）、Gemini（Google）、本地模型（llama.cpp/Ollama）、OpenAI 兼容 API 四类 provider，统一配置切换，AI 能力（NL2SQL/查询优化/索引建议/重写建议）可按 provider 路由。
: 备注：既有 `OptimizerConfig::with_llm`（`:207`，仅 OpenAI 兼容）包装为 `OpenAIProvider`，本版本补 Claude/Gemini/本地 provider。

**混合搜索（Hybrid Search）**
: 跨源融合向量搜索（pgvector，语义相似度）+ 全文搜索（ES/OpenSearch/Meilisearch，关键词匹配）+ 结构化查询（SQL，过滤/排序）三类结果，通过 RRF（Reciprocal Rank Fusion）/加权/级联策略联合排序，返回统一 `SearchResult`。
: 备注：既有 `PgVectorStore`（`sz-orm-vector/src/lib.rs:189`）与 ES/OpenSearch/Meilisearch provider 独立运行，本版本补联合排序融合层。

**数据 lineage（Data Lineage）**
: 字段级数据血缘追踪，记录 source table/column → target table/column 的依赖关系（ETL/视图/物化视图/查询投影），构建有向无环图（DAG），支持影响分析（变更某字段，下游哪些表/字段/报表受影响）与溯源分析（某字段数据来自哪些源头）。
: 备注：区别于既有 `HashChainAuditor`（`sz-orm-audit/src/lib.rs:778`，SQL 审计哈希链防篡改），本版本补字段级血缘图。

**分片自动 rebalance（Shard Auto-Rebalance）**
: 分片集群扩缩容时自动计算最小数据搬迁计划，分批迁移数据到新分片，支持断点续传与迁移过程查询不中断（双写/影子读），迁移完成更新路由表。
: 备注：既有 `ShardingRouter`（`sz-orm-sharding/src/lib.rs:130`）仅静态路由，本版本补动态 rebalance。

**数据库 failover 自动化（Database Auto-Failover）**
: 健康检测持续监控主库可用性，主库故障时自动触发 failover：提升某 slave 为新主库、更新 `ReadWriteRouter` 路由、通知上层应用、记录 failover 事件，含数据丢失风险评估（异步复制延迟）。
: 备注：既有 `HealthChecker`（`sz-orm-rw/src/lib.rs:219`）+ `test_router_failover_to_master_when_all_unhealthy`（`:911`，手动 failover 测试），本版本补自动 failover 编排。

**服务网格集成（Service Mesh Integration）**
: 与 Istio/Linkerd 服务网格集成，生成 xDS/envoy 配置，支持 sidecar 自动注入、mTLS 策略、流量治理（金丝雀/蓝绿部署/熔断/重试），复用既有 Prometheus + OTLP 可观测性。
: 备注：既有 `MetricsRegistry`（`sz-orm-observability/src/lib.rs:250`）+ `sz-orm-tracing`（OTLP），本版本补服务网格适配层。

**GraphQL 深度集成（async-graphql Deep Integration）**
: 将既有自研 GraphQL（`sz-orm-graphql`）深度对接 async-graphql 生态，支持 Subscription（实时推送）、Relay（分页规范）、Federation（联邦 schema）、工单化错误处理，复用既有 `DataLoader`（`dataloader.rs:89`）消除 N+1。
: 备注：既有 `GraphQLServer`（`sz-orm-graphql/src/lib.rs:182`）自研，本版本补 async-graphql 对接层。

**CDC 变更数据捕获（Change Data Capture）**
: 捕获数据库数据变更（INSERT/UPDATE/DELETE）为 `ChangeEvent`（Before/After/Op/Timestamp/TransactionId），实时同步到下游（消息队列/HTTP webhook/其他数据源），支持断点续传与 Exactly-Once 语义。
: 备注：既有 `sz-orm-queue`（6 provider）+ `sz-orm-audit`（审计日志可作 CDC 源），本版本补 CDC 捕获与分发。

**v4.0.0 feature gate**
: 控制本版本新能力的 feature gate 集合（`ai-auto-tuning` / `multi-llm` / `hybrid-search` / `data-lineage` / `shard-rebalance` / `auto-failover` / `service-mesh` / `async-graphql-integration` / `cdc`），默认关闭，避免无配置环境行为变化。

---

# 3. 角色与边界

## 3.1 核心角色

- **ORM 库维护者**：执行 v4.0.0 九项优化的开发、验证、测试操作者，是新增能力的主要使用者与验收人。
- **下游项目开发者（sz-pay）**：关注 API 兼容性、混合搜索/GraphQL/CDC 新能力使用的下游使用者，v4.0.0 不得破坏其既有代码。
- **运维/SRE 工程师**：使用分片 rebalance 扩缩容、failover 自动化保障高可用、服务网格集成治理流量、CDC 实时同步运维。
- **数据工程师**：使用数据 lineage 做数据治理与影响分析、CDC 做实时 ETL、混合搜索做搜索召回。
- **AI/算法工程师**：使用 AI 自动调优闭环优化查询性能、多 LLM 模型支持切换 provider。
- **应用开发者**：使用 GraphQL 深度集成构建 API、混合搜索做业务搜索、CDC 做缓存失效/搜索索引同步。

## 3.2 外部系统

- **MySQL 9.6 / PostgreSQL 18 / SQLite / Oracle 23ai / MSSQL**：AI 调优/failover/CDC/lineage 的五方言覆盖目标。
- **Anthropic Claude / Google Gemini / OpenAI / Ollama(llama.cpp)**：多 LLM provider 的四类后端。
- **pgvector（PostgreSQL 向量扩展）**：混合搜索的向量搜索后端（`packages/sz-orm-vector/src/real_pg.rs`）。
- **Elasticsearch / OpenSearch / Meilisearch**：混合搜索的全文搜索后端（`packages/sz-orm-search/src/`）。
- **Istio / Linkerd**：服务网格集成的两类网格平台。
- **async-graphql crate**：GraphQL 深度集成的生态对接对象。
- **Debezium（参考）**：CDC 设计参考（非依赖，sz-orm 自实现 CDC 捕获）。
- **RabbitMQ / Kafka / NATS / Pulsar / RocketMQ / ActiveMQ**：CDC 下游同步的消息队列（复用既有 `sz-orm-queue` 6 provider）。
- **sz-pay 项目**：API 兼容性验证的下游基准。

## 3.3 交互上下文

```plantuml
@startuml
title sz-orm v4.0.0 九项中期优化 交互上下文

rectangle "sz-orm v4.0.0" as orm {
  rectangle "AI 自动调优闭环" as tuning
  rectangle "多 LLM 模型支持" as llm
  rectangle "混合搜索" as hybrid
  rectangle "数据 lineage" as lineage
  rectangle "分片 rebalance" as rebalance
  rectangle "failover 自动化" as failover
  rectangle "服务网格集成" as mesh
  rectangle "GraphQL 深度集成" as graphql
  rectangle "CDC 变更数据捕获" as cdc
}

actor "ORM 库维护者" as dev
actor "运维/SRE" as sre
actor "数据工程师" as data
actor "AI/算法工程师" as ai
actor "应用开发者" as app
actor "sz-pay 开发者" as downstream

database "MySQL/PG/SQLite\n/Oracle/MSSQL" as db
rectangle "Claude/Gemini/\nOpenAI/Ollama" as llmprov
rectangle "pgvector + ES/\nOpenSearch/Meilisearch" as search
cloud "Istio/Linkerd" as smesh
rectangle "async-graphql" as agql
rectangle "RabbitMQ/Kafka/\nNATS/Pulsar/..." as mq

dev --> orm : 开发/验证/测试
ai --> tuning : 自动调优闭环
tuning --> llm : 调用 LLM 生成建议
llm --> llmprov : provider 路由
data --> hybrid : 混合搜索召回
hybrid --> search : 向量+全文+结构化
data --> lineage : 血缘追踪/影响分析
lineage --> db : 解析 SQL 依赖
sre --> rebalance : 扩缩容自动迁移
rebalance --> db : 分批迁移数据
sre --> failover : 自动故障切换
failover --> db : 健康检测/提升 slave
sre --> mesh : 流量治理/mTLS
mesh --> smesh : xDS 配置
app --> graphql : Subscription/Relay/Federation
graphql --> agql : 对接 async-graphql
data --> cdc : 变更捕获/实时同步
cdc --> db : 捕获变更事件
cdc --> mq : 分发到下游
downstream --> orm : API 兼容性验证

@enduml
```

---

# 4. DFX约束

## 4.1 性能

1. **AI 调优闭环开销**：自动调优闭环单次执行（检测+建议+执行+验证）开销不超过 30 秒/查询，验证阶段对比前后耗时须基于 EXPLAIN + 实际执行（≤3 次采样）。
2. **多 LLM 调用延迟**：LLM provider 调用须支持超时配置（默认 30 秒），本地模型（Ollama）调用延迟不超过 10 秒/请求（本地基准）。
3. **混合搜索延迟**：混合搜索端到端延迟不超过 200ms（向量+全文+结构化三源并行查询 + 融合排序，单机基准，结果集 ≤1000）。
4. **lineage 解析开销**：lineage 图构建开销不超过 100ms/SQL 语句（解析 SQL 提取表/字段依赖），全量 lineage 图查询不超过 500ms。
5. **分片 rebalance 开销**：rebalance 迁移计划计算不超过 1 秒（最小搬迁量算法），迁移过程查询不中断（双写/影子读）。
6. **failover 切换时间**：自动 failover 从故障检测到路由切换完成不超过 30 秒（含 3 次健康检测确认 + slave 提升 + 路由更新 + 通知上层）。
7. **CDC 捕获开销**：CDC 捕获变更事件开销不超过 5ms/事件（从 WAL/binlog 读取 + 序列化 ChangeEvent），CDC 分发到消息队列吞吐不低于 10,000 事件/秒。
8. **GraphQL 解析开销**：async-graphql 对接层查询解析开销不超过 50ms（含 DataLoader 批量加载调度）。

## 4.2 可靠性

1. **AI 调优回滚**：自动调优闭环验证阶段检测到性能回归（≥10% 回退）须自动回滚已执行建议，恢复调优前状态。
2. **多 LLM 故障转移**：某 LLM provider 不可用时须自动 fallback 到配置的备用 provider，AI 能力不中断。
3. **混合搜索部分失效**：向量/全文/结构化任一源不可用时，混合搜索须降级为可用源结果，不整体失败。
4. **lineage 图一致性**：lineage 图须与实际 SQL 依赖一致，SQL 变更后 lineage 图须增量更新，不出现陈旧边。
5. **rebalance 断点续传**：rebalance 迁移过程中断（网络/节点故障）须支持断点续传，已迁移数据不丢失不重复。
6. **failover 数据丢失评估**：failover 须评估数据丢失风险（异步复制延迟），延迟 > 阈值时告警并人工确认，不盲目切换。
7. **CDC Exactly-Once**：CDC 须保证 Exactly-Once 语义（至少一次 + 幂等去重），下游不出现重复事件。
8. **v3.9.0 测试基线不回退**：v4.0.0 不得使 v3.9.0 已验收测试基线（6760+ passed）回退，仅增不减。

## 4.3 安全性

1. **LLM API Key 保护**：LLM provider 的 API Key 须通过配置/环境变量注入，禁止硬编码，禁止日志/报告泄露。
2. **lineage 敏感字段脱敏**：lineage 图中敏感字段名须尊重既有脱敏规则（`sz-orm-masking`），可选脱敏展示。
3. **failover 凭证安全**：failover 提升 slave 须验证 slave 凭证权限，未授权 slave 不提升。
4. **服务网格 mTLS**：服务网格集成须默认启用 mTLS，服务间通信加密，禁止明文。
5. **CDC 事件脱敏**：CDC ChangeEvent 的 Before/After 数据须尊重既有脱敏规则，敏感字段脱敏后再分发到下游。
6. **CDC 消费鉴权**：CDC 下游消费须支持鉴权（消息队列 ACL / HTTP webhook 签名验证），禁止未授权消费。

## 4.4 可维护性

1. **AI 调优可观测**：自动调优闭环须输出结构化调优报告（建议列表/采纳率/前后耗时/回归标记），可被 CI/工具解析。
2. **多 LLM 可切换**：LLM provider 须通过配置切换，无需修改代码，支持运行时动态切换（热更新）。
3. **lineage 图可视化**：lineage 图须支持导出为标准格式（DOT/JSON/GraphML），可被 Graphviz/D3.js 可视化。
4. **rebalance 进度可观测**：rebalance 迁移进度须可查询（已迁移分片/剩余/预估完成时间），可中止/恢复。
5. **failover 事件审计**：failover 事件须记录审计日志（故障时间/检测确认/提升的 slave/数据丢失评估/操作者），可追溯。
6. **CDC 位点管理**：CDC 须管理消费位点（WAL LSN/binlog GTID），位点持久化，重启后从断点续传。
7. **审计证据要求**：每项需求结论须附 file:line 证据，遵循 AGENTS.md 审计合规铁律。

## 4.5 兼容性

1. **API 向后兼容**：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容。
2. **sz-pay 不破坏**：sz-pay 从 crates.io 拉取的 sz-orm-* 6 个包既有用法不受影响。
3. **五方言一致**：新增能力在 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言上行为一致（AI 调优/failover/CDC 按方言能力适配）。
4. **既有 AI 优化器保留**：既有 `UnifiedQueryOptimizer`（`packages/sz-orm-ai/src/query_plan_optimizer.rs:515`）保留不动，新增 `AutoTuningPipeline` 编排。
5. **既有 LLM 调用保留**：既有 `OptimizerConfig::with_llm`（`:207`）保留不动，包装为 `OpenAIProvider`。
6. **既有向量/全文搜索保留**：既有 `PgVectorStore`（`sz-orm-vector/src/lib.rs:189`）与 ES/OpenSearch/Meilisearch provider 保留不动，新增 `HybridSearcher` 融合。
7. **既有审计保留**：既有 `HashChainAuditor`（`sz-orm-audit/src/lib.rs:778`）保留不动，新增 `LineageTracker` 并行。
8. **既有分片路由保留**：既有 `ShardingRouter`（`sz-orm-sharding/src/lib.rs:130`）保留不动，新增 `ShardRebalancer` 编排。
9. **既有读写分离保留**：既有 `ReadWriteRouter`（`sz-orm-rw/src/lib.rs:331`）与 `HealthChecker`（`:219`）保留不动，新增 `AutoFailoverManager` 调用。
10. **既有 GraphQL 保留**：既有 `GraphQLServer`（`sz-orm-graphql/src/lib.rs:182`）与 `DataLoader`（`dataloader.rs:89`）保留不动，新增 async-graphql 对接层复用。

---

# 5. 核心能力

## 5.1 AI 自动调优闭环（REQ-V40-001）

### 5.1.1 业务规则

1. **四阶段闭环**（EARS: Ubiquitous）
   系统应当提供 AI 自动调优闭环，包含检测（Detect，识别慢查询）、建议（Advise，生成索引/重写/Schema 建议）、执行（Apply，自动应用建议）、验证（Verify，对比调优前后耗时）四阶段，循环执行。
   a. 验收条件：[配置慢查询阈值 1s，存在耗时 2s 的查询，运行 AutoTuningPipeline] → [输出四阶段报告：Detect 识别该查询 → Advise 生成建议 → Apply 执行 → Verify 对比前后耗时]
2. **检测阶段**（EARS: Ubiquitous）
   系统应当通过慢查询日志/EXPLAIN 解析/统计信息识别待调优查询，复用既有 `ExplainPlanParser`（`packages/sz-orm-ai/src/explain_parser.rs:50`，5 方言解析器）识别全表扫描/索引缺失/JOIN 顺序不当等问题。
   a. 验收条件：[查询 `SELECT * FROM users WHERE name LIKE '%foo%'`，运行检测] → [识别为全表扫描 + 索引缺失，标记待调优]
3. **建议阶段**（EARS: Ubiquitous）
   系统应当复用既有 `IndexAdvisor`（`packages/sz-orm-ai/src/index_advisor.rs:100`）、`RewriteAdvisor`（`packages/sz-orm-ai/src/rewrite_advisor.rs:89`）、`UnifiedQueryOptimizer`（`packages/sz-orm-ai/src/query_plan_optimizer.rs:515`）生成优化建议，建议含类型（索引/重写/Schema）、SQL 变更、预期收益、风险评估。
   a. 验收条件：[待调优查询输入建议阶段] → [输出建议列表，每建议含 type/sql_before/sql_after/expected_gain/risk]
4. **执行阶段**（EARS: Optional）
   当建议的风险低于阈值（配置，默认只自动执行低风险建议如添加索引，高风险如 DROP/ALTER 需人工确认）时，系统应当自动执行建议（创建索引/重写查询/调整 Schema），记录执行日志。
   a. 验收条件：[建议为"添加索引"（低风险），自动执行] → [创建索引，记录执行日志；建议为"DROP COLUMN"（高风险），不自动执行，标记待人工确认]
5. **验证阶段**（EARS: Ubiquitous）
   系统应当对比调优前后查询耗时（EXPLAIN 估算 + 实际执行 ≤3 次采样），量化收益（耗时下降百分比），回归（≥10% 回退）则自动回滚已执行建议。
   a. 验收条件：[调优前耗时 2s，执行建议后耗时 0.5s] → [验证报告收益=75% 下降；若调优后耗时 2.5s（25% 回退）] → [自动回滚，恢复调优前状态]
6. **复用既有优化器**（EARS: Ubiquitous）
   系统应当复用既有 `UnifiedQueryOptimizer`（`:515`）、`IndexAdvisor`（`index_advisor.rs:100`）、`RewriteAdvisor`（`rewrite_advisor.rs:89`）、`ExplainPlanParser`（`explain_parser.rs:50`），不重复实现建议生成与 EXPLAIN 解析。
   a. 验收条件：[AutoTuningPipeline 建议] → [基于既有 IndexAdvisor/RewriteAdvisor 生成，不重复实现]
7. **禁止项**（EARS: Unwanted）
   如果 AI 自动调优闭环影响默认 feature 编译或运行时行为，则系统应当通过 `ai-auto-tuning` feature gate 隔离，默认不启用自动执行。
   a. 验收条件：[`cargo build` 默认编译] → [无自动调优闭环，行为与 v3.9.0 一致]

### 5.1.2 交互流程

```plantuml
@startuml
title AI 自动调优闭环 交互流程
actor "AI/算法工程师" as ai
participant "AutoTuningPipeline" as pipeline
participant "ExplainParser" as parser
participant "IndexAdvisor/RewriteAdvisor" as advisor
participant "LlmProvider" as llm
database "DB" as db

ai -> pipeline : 启动闭环(慢查询阈值)
loop 循环调优
  pipeline -> db : 采集慢查询日志
  pipeline -> parser : EXPLAIN 解析(5 方言)
  parser --> pipeline : 待调优查询列表
  pipeline -> advisor : 生成建议(rule + llm)
  advisor -> llm : LLM 增强(可选)
  llm --> advisor : 建议补充
  advisor --> pipeline : 建议列表(含风险)
  alt 建议低风险
    pipeline -> db : 自动执行(创建索引/重写)
    pipeline -> db : 验证(对比耗时 ≤3 采样)
    db --> pipeline : 调优后耗时
    alt 性能回归 ≥10%
      pipeline -> db : 回滚已执行建议
    end
  else 建议高风险
    pipeline -> ai : 标记待人工确认
  end
end
pipeline --> ai : 调优报告(建议/采纳率/收益/回归)
@enduml
```

### 5.1.3 异常场景

1. **LLM provider 不可用**
   a. 触发条件：建议阶段调用 LLM 但 provider 超时/错误
   b. 系统行为：降级为纯规则建议（`UnifiedQueryOptimizer` rule hint），标记 LLM 不可用
   c. 用户感知：调优报告标记"LLM 降级，仅规则建议"
2. **执行建议失败**
   a. 触发条件：创建索引失败（权限不足/磁盘满/锁冲突）
   b. 系统行为：记录失败原因，跳过该建议，继续后续建议
   c. 用户感知：调优报告标记该建议执行失败 + 原因
3. **验证阶段回归**
   a. 触发条件：调优后耗时 ≥ 调优前 × 1.1（10% 回退）
   b. 系统行为：自动回滚已执行建议，恢复调优前状态
   c. 用户感知：调优报告标记"回归，已回滚"

## 5.2 多 LLM 模型支持（REQ-V40-002）

### 5.2.1 业务规则

1. **LlmProvider trait 抽象**（EARS: Ubiquitous）
   系统应当提供 `LlmProvider` trait 抽象 LLM 调用，统一接口（`complete(prompt, config) -> Result<Response>`、`embed(text) -> Result<Vec<f32>>`），支持多 provider 实现与按能力路由。
   a. 验收条件：[定义 LlmProvider trait，实现 ClaudeProvider/GeminiProvider/LocalLlamaProvider/OpenAIProvider] → [四 provider 均实现 trait，统一调用接口]
2. **四类 provider 支持**（EARS: Ubiquitous）
   系统应当支持四类 LLM provider：(a) Claude（Anthropic，claude-3-opus/sonnet/haiku）；(b) Gemini（Google，gemini-1.5-pro/flash）；(c) 本地模型（Ollama/llama.cpp，无网络依赖）；(d) OpenAI 兼容（既有 `OptimizerConfig::with_llm` 包装）。
   a. 验收条件：[配置 provider=claude，model=claude-3-sonnet] → [调用 Claude API；配置 provider=ollama，model=llama3] → [调用本地 Ollama]
3. **统一配置切换**（EARS: Ubiquitous）
   系统应当通过统一配置（`LlmConfig { provider, model, api_key, api_base, timeout, max_tokens }`）切换 provider，无需修改代码，支持运行时动态切换（热更新）。
   a. 验收条件：[运行时将配置从 provider=openai 切换为 provider=claude] → [后续 LLM 调用路由到 Claude，无需重启]
4. **AI 能力按 provider 路由**（EARS: Optional）
   当不同 AI 能力（NL2SQL/查询优化/索引建议/重写建议/Embedding）配置不同 provider 时，系统应当按能力路由到对应 provider（如 NL2SQL 用 Claude，Embedding 用 OpenAI）。
   a. 验收条件：[配置 NL2SQL→Claude，Embedding→OpenAI] → [NL2SQL 调用 Claude，Embedding 调用 OpenAI]
5. **复用既有 OpenAI 调用**（EARS: Ubiquitous）
   系统应当将既有 `OptimizerConfig::with_llm`（`packages/sz-orm-ai/src/query_plan_optimizer.rs:207`，OpenAI 兼容）包装为 `OpenAIProvider`，既有调用不修改。
   a. 验收条件：[既有 `with_llm(api_key, model)` 调用] → [内部包装为 OpenAIProvider，行为不变]
6. **本地模型无网络依赖**（EARS: Ubiquitous）
   本地模型 provider（Ollama/llama.cpp）须通过本地 HTTP API（`http://localhost:11434`）调用，无外部网络依赖，适合离线/隐私场景。
   a. 验收条件：[配置 provider=ollama，断开外网] → [本地模型调用正常，无网络请求]
7. **禁止项**（EARS: Unwanted）
   如果多 LLM 模型支持影响默认 feature 编译或引入不必要的网络依赖，则系统应当通过 `multi-llm` feature gate 隔离，默认仅保留既有 OpenAI 兼容。
   a. 验收条件：[`cargo build` 默认编译] → [仅 OpenAI 兼容，无 Claude/Gemini/Ollama 依赖]

### 5.2.2 交互流程

```plantuml
@startuml
title 多 LLM 模型支持 交互流程
actor "AI/算法工程师" as ai
participant "LlmRouter" as router
participant "LlmProvider" as provider
rectangle "Claude/Gemini/\nOpenAI/Ollama" as backends

ai -> router : 配置 LlmConfig(provider, model)
router -> provider : 选择 provider 实现
ai -> router : complete(prompt) / embed(text)
router -> provider : 路由到对应 provider
provider -> backends : 调用后端 API
backends --> provider : 响应
provider --> router : Result<Response>
router --> ai : 统一响应
note right of router : 运行时热切换 provider
@enduml
```

### 5.2.3 异常场景

1. **provider 不可用 fallback**
   a. 触发条件：配置的 provider 超时/错误（如 Claude API 503）
   b. 系统行为：自动 fallback 到配置的备用 provider，AI 能力不中断
   c. 用户感知：响应正常，日志标记"fallback from claude to openai"
2. **本地模型未启动**
   a. 触发条件：配置 provider=ollama 但 Ollama 服务未启动
   b. 系统行为：连接拒绝错误，提示启动 Ollama
   c. 用户感知：错误提示"ollama not running at localhost:11434, start with `ollama serve`"
3. **API Key 无效**
   a. 触发条件：配置的 API Key 无效或过期
   b. 系统行为：认证错误，不重试（避免锁定）
   c. 用户感知：错误提示"invalid api key for provider claude"

## 5.3 混合搜索（REQ-V40-003）

### 5.3.1 业务规则

1. **三源融合查询**（EARS: Ubiquitous）
   系统应当提供混合搜索，融合向量搜索（pgvector，语义相似度）、全文搜索（ES/OpenSearch/Meilisearch，关键词匹配）、结构化查询（SQL，过滤/排序）三类结果，返回统一 `SearchResult`。
   a. 验收条件：[查询"红色手机"，向量+全文+结构化三源] → [返回融合排序后的统一 SearchResult 列表，每结果含 score/source/metadata]
2. **融合排序策略**（EARS: Ubiquitous）
   系统应当支持三种融合排序策略：(a) RRF（Reciprocal Rank Fusion，倒数排名融合，默认）；(b) 加权（各源 score × 权重求和）；(c) 级联（先向量召回，再全文精排，再结构化过滤）。
   a. 验收条件：[配置 strategy=RRF，三源各返回 top10] → [RRF 融合后返回 top10，score=1/(60+rank) 求和]
3. **复用既有向量/全文搜索**（EARS: Ubiquitous）
   系统应当复用既有 `PgVectorStore`（`packages/sz-orm-vector/src/lib.rs:189`，pgvector）与 ES/OpenSearch/Meilisearch provider（`packages/sz-orm-search/src/`），不重复实现向量/全文搜索。
   a. 验收条件：[混合搜索向量源] → [基于既有 PgVectorStore，不重复实现]
4. **并行查询**（EARS: Ubiquitous）
   三源查询须并行执行（tokio::join!），端到端延迟取最慢源 + 融合排序开销，不超过 200ms（单机基准，结果集 ≤1000）。
   a. 验收条件：[三源各耗时 50ms/80ms/30ms，并行查询] → [端到端 ≈ 80ms + 融合开销 ≤ 200ms]
5. **部分源降级**（EARS: State-driven）
   在向量/全文/结构化任一源不可用的状态下，系统应当降级为可用源结果，不整体失败，标记降级源。
   a. 验收条件：[ES 不可用，向量+结构化可用] → [返回向量+结构化融合结果，标记"fulltext degraded"]
6. **结构化过滤下推**（EARS: Optional）
   当混合查询含结构化过滤条件（如 `price < 1000`）时，系统应当将过滤条件下推到向量/全文源（如 pgvector WHERE 子句、ES filter），减少融合层过滤开销。
   a. 验收条件：[查询含 `price < 1000` 过滤] → [过滤下推到 pgvector WHERE + ES filter，融合层不再过滤]
7. **禁止项**（EARS: Unwanted）
   如果混合搜索影响默认 feature 编译或引入不必要的依赖，则系统应当通过 `hybrid-search` feature gate 隔离，默认不启用。
   a. 验收条件：[`cargo build` 默认编译] → [无混合搜索，行为与 v3.9.0 一致]

### 5.3.2 交互流程

```plantuml
@startuml
title 混合搜索 交互流程
actor "数据工程师" as data
participant "HybridSearcher" as hybrid
participant "PgVectorStore" as vec
participant "ES/OpenSearch/Meilisearch" as fulltext
participant "SQL 结构化" as structured

data -> hybrid : hybrid_search(query, filter, strategy)
par 并行查询
  hybrid -> vec : 向量搜索(语义相似度)
  vec --> hybrid : 向量结果(top_k)
else
  hybrid -> fulltext : 全文搜索(关键词匹配)
  fulltext --> hybrid : 全文结果(top_k)
else
  hybrid -> structured : 结构化查询(过滤/排序)
  structured --> hybrid : 结构化结果(top_k)
end
hybrid -> hybrid : 融合排序(RRF/加权/级联)
hybrid --> data : 统一 SearchResult 列表
@enduml
```

### 5.3.3 异常场景

1. **某源超时**
   a. 触发条件：向量/全文/结构化某源查询超时（> 配置阈值）
   b. 系统行为：该源结果标记 TIMEOUT，其他源正常融合
   c. 用户感知：结果标记"vector source timeout, partial results"
2. **结果集为空**
   a. 触发条件：三源均无匹配结果
   b. 系统行为：返回空列表，不报错
   c. 用户感知：空结果，提示"no results found"

## 5.4 数据 lineage（REQ-V40-004）

### 5.4.1 业务规则

1. **字段级血缘追踪**（EARS: Ubiquitous）
   系统应当提供字段级数据血缘追踪，记录 source table/column → target table/column 的依赖关系（ETL/视图/物化视图/查询投影），构建有向无环图（DAG）。
   a. 验收条件：[执行 `INSERT INTO report SELECT user.name, order.amount FROM user JOIN order`] → [lineage 图记录 report.name ← user.name, report.amount ← order.amount]
2. **lineage 图构建**（EARS: Ubiquitous）
   系统应当通过解析 SQL（INSERT/UPDATE/CREATE VIEW/CREATE MATERIALIZED VIEW）提取表/字段依赖，构建 `LineageGraph`（节点=表.字段，边=依赖关系），支持增量更新（SQL 变更后增量更新图，不全量重建）。
   a. 验收条件：[执行新 SQL，lineage 图已存在] → [增量更新图，新增/修改边，既有边保留]
3. **影响分析**（EARS: Ubiquitous）
   系统应当提供影响分析（impact analysis）：变更某表/字段，输出下游受影响的表/字段/报表列表，用于评估变更风险。
   a. 验收条件：[分析"修改 user.name 字段"] → [输出下游受影响列表：report.name, dashboard.user_name_widget, etl_job_123]
4. **溯源分析**（EARS: Ubiquitous）
   系统应当提供溯源分析（origin analysis）：某字段数据来自哪些源头表/字段，用于数据质量问题定位。
   a. 验收条件：[溯源"report.amount 字段"] → [输出源头：order.amount ← orders 表]
5. **lineage 与审计集成**（EARS: Optional）
   当启用审计（`sz-orm-audit`）时，lineage 变更（新依赖建立）可选写入审计链（`HashChainAuditor`，`packages/sz-orm-audit/src/lib.rs:778`），保证 lineage 图防篡改。
   a. 验收条件：[启用审计，执行新 SQL 建立 lineage] → [审计链记录 lineage 变更事件，`verify()` 通过]
6. **lineage 图导出**（EARS: Ubiquitous）
   系统应当支持 lineage 图导出为标准格式（DOT/JSON/GraphML），可被 Graphviz/D3.js 可视化。
   a. 验收条件：[导出 lineage 图为 DOT] → [Graphviz 渲染为可视化 DAG 图]
7. **禁止项**（EARS: Unwanted）
   如果数据 lineage 影响默认 feature 编译或运行时性能，则系统应当通过 `data-lineage` feature gate 隔离，默认不启用 lineage 追踪。
   a. 验收条件：[`cargo build` 默认编译] → [无 lineage 追踪，行为与 v3.9.0 一致]

### 5.4.2 交互流程

```plantuml
@startuml
title 数据 lineage 交互流程
actor "数据工程师" as data
participant "LineageTracker" as tracker
participant "SQL Parser" as parser
database "DB" as db
participant "LineageGraph" as graph

data -> tracker : 执行 SQL(INSERT/CREATE VIEW)
tracker -> parser : 解析 SQL 提取依赖
parser --> tracker : 表/字段依赖关系
tracker -> graph : 增量更新 lineage 图
data -> tracker : impact_analysis(user.name)
tracker -> graph : 查询下游受影响
graph --> tracker : 受影响列表
tracker --> data : 影响分析报告
data -> tracker : export_graph(DOT)
tracker --> data : DOT 格式图
@enduml
```

### 5.4.3 异常场景

1. **SQL 解析失败**
   a. 触发条件：SQL 语法不 supported 或方言不支持
   b. 系统行为：跳过该 SQL，记录解析失败，不影响其他 SQL lineage 追踪
   c. 用户感知：lineage 图标记该 SQL"parse failed"，其他正常
2. **lineage 图环路检测**
   a. 触发条件：SQL 依赖形成环路（A→B→A）
   b. 系统行为：检测到环路，标记为可疑依赖，不加入 DAG（DAG 不允许环路）
   c. 用户感知：告警"lineage cycle detected: A→B→A, skipped"

## 5.5 分片自动 rebalance（REQ-V40-005）

### 5.5.1 业务规则

1. **扩缩容自动迁移**（EARS: Ubiquitous）
   系统应当提供分片集群扩容（新增分片）/缩容（移除分片）时的自动 rebalance，计算最小数据搬迁计划，分批迁移数据到新分片。
   a. 验收条件：[3 分片→4 分片扩容，运行 rebalance] → [计算最小搬迁量，迁移约 25% 数据到新分片，路由表更新]
2. **最小搬迁量计算**（EARS: Ubiquitous）
   系统应当计算最小数据搬迁量（仅搬迁必要数据，非全量重哈希），基于一致性哈希环或范围分片策略，输出迁移计划（source shard → target shard、行数、预估时间）。
   a. 验收条件：[一致性哈希 3→4 分片] → [仅搬迁哈希环上新增节点相邻区间的数据，非全量 1/4]
3. **断点续传**（EARS: Ubiquitous）
   rebalance 迁移过程中断（网络/节点故障）须支持断点续传，已迁移数据不丢失不重复，恢复后从断点继续。
   a. 验收条件：[迁移 50% 时中断，恢复 rebalance] → [从 50% 断点继续，不重迁已迁移数据]
4. **迁移过程查询不中断**（EARS: Ubiquitous）
   rebalance 迁移过程中查询须不中断，通过双写（新旧分片同时写）+ 影子读（读旧分片）保证一致性，迁移完成切换路由。
   a. 验收条件：[迁移过程中查询/写入] → [查询返回正确结果，写入双写到新旧分片，不中断]
5. **复用既有分片路由**（EARS: Ubiquitous）
   系统应当复用既有 `ShardingRouter`（`packages/sz-orm-sharding/src/lib.rs:130`）与 `ShardingStrategy`（`:60`），rebalance 完成后更新路由表，不修改既有路由策略。
   a. 验收条件：[rebalance 完成] → [更新 ShardingRouter 路由表，既有策略（一致性哈希/枚举/列表）不变]
6. **迁移进度可观测**（EARS: Ubiquitous）
   rebalance 迁移进度须可查询（已迁移分片/剩余/预估完成时间），可中止/恢复。
   a. 验收条件：[rebalance 迁移中查询进度] → [返回已迁移 60%、剩余 40%、预估 5 分钟，可中止]
7. **禁止项**（EARS: Unwanted）
   如果分片 rebalance 影响默认 feature 编译或运行时行为，则系统应当通过 `shard-rebalance` feature gate 隔离，默认不启用自动 rebalance。
   a. 验收条件：[`cargo build` 默认编译] → [无 rebalance，行为与 v3.9.0 一致]

### 5.5.2 交互流程

```plantuml
@startuml
title 分片自动 rebalance 交互流程
actor "运维/SRE" as sre
participant "ShardRebalancer" as rebalancer
participant "ShardingRouter" as router
database "分片集群" as shards

sre -> rebalancer : rebalance(新分片列表)
rebalancer -> router : 获取当前路由+策略
rebalancer -> rebalancer : 计算最小搬迁计划
loop 分批迁移
  rebalancer -> shards : 双写开启(新旧分片)
  rebalancer -> shards : 迁移数据批次
  shards --> rebalancer : 批次完成
  rebalancer -> sre : 进度更新(可中止/恢复)
end
rebalancer -> router : 更新路由表
rebalancer -> shards : 双写关闭(旧分片停读)
rebalancer --> sre : 迁移报告(搬迁量/耗时/一致性校验)
@enduml
```

### 5.5.3 异常场景

1. **迁移过程中节点故障**
   a. 触发条件：迁移中某分片节点故障
   b. 系统行为：暂停迁移，标记故障分片，等待恢复后断点续传
   c. 用户感知：进度标记"paused, shard X unavailable"
2. **一致性校验失败**
   a. 触发条件：迁移完成后一致性校验发现新旧分片数据不一致
   b. 系统行为：不切换路由，告警人工介入
   c. 用户感知：告警"consistency check failed, manual intervention required"

## 5.6 数据库 failover 自动化（REQ-V40-006）

### 5.6.1 业务规则

1. **自动故障检测**（EARS: Ubiquitous）
   系统应当持续监控主库可用性（复用既有 `HealthChecker`，`packages/sz-orm-rw/src/lib.rs:219`，`failure_threshold`），主库连续故障达阈值（默认 3 次）时触发 failover。
   a. 验收条件：[主库故障，连续 3 次健康检测失败] → [触发自动 failover]
2. **自动 slave 提升**（EARS: Ubiquitous）
   failover 时系统应当自动选择最合适的 slave（复制延迟最小 + 数据最完整）提升为新主库，更新 `ReadWriteRouter`（`packages/sz-orm-rw/src/lib.rs:331`）路由，通知上层应用。
   a. 验收条件：[触发 failover，slave-2 延迟最小] → [slave-2 提升为新主库，路由更新，上层通知]
3. **数据丢失风险评估**（EARS: Ubiquitous）
   failover 须评估数据丢失风险（异步复制延迟），延迟 > 阈值（配置，默认 1 秒）时告警并人工确认，不盲目切换；延迟 ≤ 阈值时自动切换。
   a. 验收条件：[slave 延迟 2s > 阈值 1s] → [告警"high replication lag 2s, manual confirm required"；延迟 0.5s ≤ 阈值] → [自动切换]
4. **failover 事件审计**（EARS: Ubiquitous）
   failover 事件须记录审计日志（故障时间/检测确认次数/提升的 slave/数据丢失评估/操作者/恢复时间），可追溯。
   a. 验收条件：[failover 完成] → [审计日志含完整事件记录，可查询追溯]
5. **复用既有健康检查与路由**（EARS: Ubiquitous）
   系统应当复用既有 `HealthChecker`（`:219`）、`ReadWriteRouter`（`:331`）、`SlaveHealth`（`:37`），不修改既有健康检查与路由逻辑，新增 `AutoFailoverManager` 编排。
   a. 验收条件：[AutoFailoverManager] → [调用既有 HealthChecker 检测，既有 ReadWriteRouter 路由更新]
6. **手动 failover 保留**（EARS: Ubiquitous）
   既有手动 failover（`test_router_failover_to_master_when_all_unhealthy`，`:911`）行为保留，自动 failover 为可选增强（`auto-failover` feature gate）。
   a. 验收条件：[不启用 auto-failover，手动调用既有 failover] → [行为与 v3.9.0 一致]
7. **failover 切换时间**（EARS: Ubiquitous）
   自动 failover 从故障检测到路由切换完成不超过 30 秒（含 3 次健康检测确认 + slave 提升 + 路由更新 + 通知上层）。
   a. 验收条件：[主库故障，自动 failover] → [30 秒内路由切换完成，上层收到通知]
8. **禁止项**（EARS: Unwanted）
   如果自动 failover 影响默认 feature 编译或运行时行为，则系统应当通过 `auto-failover` feature gate 隔离，默认不启用自动 failover。
   a. 验收条件：[`cargo build` 默认编译] → [无自动 failover，行为与 v3.9.0 一致]

### 5.6.2 交互流程

```plantuml
@startuml
title 数据库 failover 自动化 交互流程
actor "运维/SRE" as sre
participant "AutoFailoverManager" as failover
participant "HealthChecker" as health
participant "ReadWriteRouter" as router
database "Master" as master
database "Slaves" as slaves

loop 持续监控
  failover -> health : 健康检测主库
  health -> master : ping
  alt 主库健康
    master --> health : ok
  else 主库故障连续 ≥3 次
    master --> health : fail
    failover -> slaves : 评估 slave 复制延迟
    slaves --> failover : slave-2 延迟最小 0.5s
    alt 延迟 ≤ 阈值
      failover -> slaves : 提升 slave-2 为新主库
      failover -> router : 更新路由(新主库=slave-2)
      failover -> sre : 通知上层(新主库地址)
      failover -> failover : 记录审计日志
    else 延迟 > 阈值
      failover -> sre : 告警人工确认
    end
  end
end
@enduml
```

### 5.6.3 异常场景

1. **所有 slave 不可用**
   a. 触发条件：主库故障且所有 slave 不可用
   b. 系统行为：无法 failover，告警人工介入，服务降级（只读或拒绝）
   c. 用户感知：告警"no healthy slave for failover, manual intervention required"
2. **slave 提升失败**
   a. 触发条件：提升 slave 为新主库失败（权限不足/配置错误）
   b. 系统行为：尝试下一个候选 slave，全部失败则告警
   c. 用户感知：告警"failover promotion failed, tried N slaves"
3. **脑裂检测**
   a. 触发条件：旧主库恢复但新主库已提升（双主）
   b. 系统行为：检测脑裂，将旧主库降级为 slave 或隔离
   c. 用户感知：告警"split-brain detected, old master demoted"

## 5.7 服务网格集成（REQ-V40-007）

### 5.7.1 业务规则

1. **Istio/Linkerd 适配**（EARS: Ubiquitous）
   系统应当提供 `ServiceMeshAdapter` trait，支持 Istio 与 Linkerd 两类服务网格，生成网格配置（xDS/envoy/Istio CRD/Linkerd policy）。
   a. 验收条件：[配置 mesh=istio] → [生成 Istio CRD（VirtualService/DestinationRule）；配置 mesh=linkerd] → [生成 Linkerd policy]
2. **mTLS 策略**（EARS: Ubiquitous）
   服务网格集成须默认启用 mTLS，服务间通信加密，支持 STRICT（强制）/PERMISSIVE（兼容明文）两种模式，默认 STRICT。
   a. 验收条件：[生成网格配置] → [含 mTLS STRICT 策略，服务间通信加密]
3. **流量治理**（EARS: Ubiquitous）
   系统应当支持流量治理策略：金丝雀发布（按百分比路由）、蓝绿部署（按版本切换）、熔断（复用既有 `sz-orm-limit` 熔断器）、重试（按状态码/超时）。
   a. 验收条件：[配置金丝雀 10% 流量到 v2] → [生成 VirtualService 90% v1 + 10% v2 路由规则]
4. **sidecar 自动注入**（EARS: Ubiquitous）
   系统应当支持 sidecar 自动注入配置（Istio namespace label / Linkerd annotation），应用 Pod 自动注入 sidecar 代理。
   a. 验收条件：[配置 namespace istio-injection=enabled] → [Pod 自动注入 Istio sidecar]
5. **复用既有可观测性**（EARS: Ubiquitous）
   系统应当复用既有 `MetricsRegistry`（`packages/sz-orm-observability/src/lib.rs:250`，Prometheus）与 `sz-orm-tracing`（OTLP 分布式追踪），服务网格 metrics/traces 接入既有可观测性。
   a. 验收条件：[服务网格 metrics] → [接入既有 MetricsRegistry，Prometheus 抓取；traces] → [接入既有 OTLP]
6. **禁止项**（EARS: Unwanted）
   如果服务网格集成影响默认 feature 编译或引入不必要的依赖，则系统应当通过 `service-mesh` feature gate 隔离，默认不启用。
   a. 验收条件：[`cargo build` 默认编译] → [无服务网格集成，行为与 v3.9.0 一致]

### 5.7.2 交互流程

```plantuml
@startuml
title 服务网格集成 交互流程
actor "运维/SRE" as sre
participant "ServiceMeshAdapter" as adapter
rectangle "Istio/Linkerd" as mesh
participant "MetricsRegistry" as metrics

sre -> adapter : 配置 mesh=istio, mTLS=STRICT
adapter -> mesh : 生成 Istio CRD(VirtualService/DestinationRule)
adapter -> mesh : 生成 mTLS PeerAuthentication
sre -> adapter : 配置金丝雀 10% v2
adapter -> mesh : 生成流量路由规则
adapter -> metrics : 接入 metrics/traces
adapter --> sre : 网格配置 + 部署清单
@enduml
```

### 5.7.3 异常场景

1. **网格平台不可用**
   a. 触发条件：Istio/Linkerd 控制平面不可用
   b. 系统行为：配置生成正常，部署标记"mesh control plane unavailable"
   c. 用户感知：告警"istio control plane unavailable, config generated but not applied"
2. **mTLS 配置冲突**
   a. 触发条件：已有 PERMISSIVE mTLS，新配置 STRICT
   b. 系统行为：提示配置冲突，需人工确认覆盖
   c. 用户感知：告警"mTLS mode conflict: existing PERMISSIVE vs new STRICT"

## 5.8 GraphQL 深度集成 async-graphql（REQ-V40-008）

### 5.8.1 业务规则

1. **async-graphql 对接**（EARS: Ubiquitous）
   系统应当将既有自研 GraphQL（`packages/sz-orm-graphql/src/lib.rs:36` `GraphQLSchema`）深度对接 async-graphql 生态，复用既有 `DataLoader`（`dataloader.rs:89`）消除 N+1，支持 async-graphql 的 Schema/Object/Field 宏生态。
   a. 验收条件：[生成 async-graphql Schema，复用既有 DataLoader] → [查询关联字段时 DataLoader 批量加载，无 N+1]
2. **Subscription 支持**（EARS: Ubiquitous）
   系统应当支持 GraphQL Subscription（实时推送），基于 WebSocket/Server-Sent Events，订阅数据变更（复用 v4.0.0 CDC ChangeEvent 作为 Subscription 数据源）。
   a. 验收条件：[客户端订阅 `userUpdated` 事件] → [用户数据变更时推送 Subscription 事件]
3. **Relay 分页规范**（EARS: Ubiquitous）
   系统应当支持 Relay 分页规范（Connection/Edge/PageInfo，cursor-based 分页），复用既有 Keyset 分页（`packages/sz-orm-core/src/query.rs:986`）。
   a. 验收条件：[查询 `users(first: 10, after: "cursor")`] → [返回 Connection{edges, pageInfo{hasNextPage, endCursor}}]
4. **Federation 联邦 schema**（EARS: Optional）
   当启用 Federation 时，系统应当支持 GraphQL Federation（联邦 schema，多服务 schema 合并，`_entities`/`_service` 查询），支持微服务架构下 schema 拆分。
   a. 验收条件：[配置 Federation，两个子服务 schema] → [联邦网关合并 schema，跨服务查询正常]
5. **工单化错误处理**（EARS: Ubiquitous）
   系统应当支持工单化错误处理（async-graphql Error extensions），错误含错误码/分类/工单 ID，便于前端统一处理与用户反馈。
   a. 验收条件：[查询错误] → [返回错误含 code/category/ticket_id，前端可据 code 统一处理]
6. **复用既有 DataLoader**（EARS: Ubiquitous）
   系统应当复用既有 `DataLoader`（`packages/sz-orm-graphql/src/dataloader.rs:89`）与 `BatchLoader` trait（`:74`），不重复实现批量加载。
   a. 验收条件：[async-graphql 对接层] → [复用既有 DataLoader，无重复实现]
7. **禁止项**（EARS: Unwanted）
   如果 async-graphql 深度集成影响默认 feature 编译或引入不必要的依赖，则系统应当通过 `async-graphql-integration` feature gate 隔离，默认保留既有自研 GraphQL。
   a. 验收条件：[`cargo build` 默认编译] → [既有自研 GraphQL，无 async-graphql 依赖]

### 5.8.2 交互流程

```plantuml
@startuml
title GraphQL 深度集成 交互流程
actor "应用开发者" as app
participant "async-graphql Schema" as schema
participant "DataLoader" as loader
participant "CdcCapturer" as cdc
database "DB" as db

app -> schema : query { users { name, orders { amount } } }
schema -> loader : 批量加载 users
loader -> db : SELECT * FROM users
db --> loader : users
schema -> loader : 批量加载 orders(复用 DataLoader)
loader -> db : SELECT * FROM orders WHERE user_id IN (...)
db --> loader : orders
schema --> app : 结果(N+1 消除)
app -> schema : subscription { userUpdated }
schema -> cdc : 订阅 CDC user 变更事件
cdc --> schema : ChangeEvent(user updated)
schema --> app : 推送 Subscription 事件
@enduml
```

### 5.8.3 异常场景

1. **DataLoader 批量加载失败**
   a. 触发条件：DataLoader 批量查询数据库失败
   b. 系统行为：返回部分结果 + 错误，不整体失败
   c. 用户感知：部分字段返回 null + errors 含加载失败原因
2. **Subscription 连接断开**
   a. 触发条件：客户端 WebSocket 连接断开
   b. 系统行为：清理订阅，停止推送，资源释放
   c. 用户感知：连接断开，重连后重新订阅

## 5.9 CDC 变更数据捕获（REQ-V40-009）

### 5.9.1 业务规则

1. **变更事件捕获**（EARS: Ubiquitous）
   系统应当捕获数据库数据变更（INSERT/UPDATE/DELETE）为 `ChangeEvent`（含 Before/After 数据、Op 操作类型、Timestamp 时间戳、TransactionId 事务 ID），从 WAL（PostgreSQL）/binlog（MySQL）/逻辑复制/触发器读取。
   a. 验收条件：[执行 `UPDATE users SET name='new' WHERE id=1`] → [捕获 ChangeEvent{op: Update, before: {name: 'old'}, after: {name: 'new'}, ts, txid}]
2. **下游同步分发**（EARS: Ubiquitous）
   系统应当将 ChangeEvent 实时分发到下游：消息队列（复用既有 `sz-orm-queue` 6 provider：RabbitMQ/Kafka/NATS/Pulsar/RocketMQ/ActiveMQ）、HTTP webhook、其他数据源，支持多下游并行分发。
   a. 验收条件：[配置下游=Kafka topic=users_cdc + HTTP webhook] → [变更事件同时分发到 Kafka 与 webhook]
3. **Exactly-Once 语义**（EARS: Ubiquitous）
   CDC 须保证 Exactly-Once 语义（至少一次 + 幂等去重），下游不出现重复事件，通过 TransactionId 去重 + 消费位点管理。
   a. 验收条件：[同一事务变更重发] → [下游去重，不重复消费]
4. **断点续传**（EARS: Ubiquitous）
   CDC 须管理消费位点（WAL LSN/binlog GTID），位点持久化，CDC 服务重启后从断点续传，不丢事件不重复。
   a. 验收条件：[CDC 服务重启] → [从持久化位点续传，无丢失无重复]
5. **ChangeEvent 脱敏**（EARS: Optional）
   当启用脱敏（`sz-orm-masking`）时，ChangeEvent 的 Before/After 数据须对敏感字段脱敏后再分发到下游。
   a. 验收条件：[变更含手机号字段，启用脱敏，分发到 Kafka] → [Kafka 事件中手机号显示为 `138****8888`]
6. **复用既有消息队列**（EARS: Ubiquitous）
   系统应当复用既有 `sz-orm-queue`（6 provider）作为 CDC 下游分发通道，不重复实现消息队列客户端。
   a. 验收条件：[CDC 分发到 Kafka] → [复用既有 `real_kafka.rs`，不重复实现]
7. **五方言覆盖**（EARS: Ubiquitous）
   CDC 须覆盖五方言变更捕获：PostgreSQL（WAL/逻辑复制）、MySQL（binlog）、SQLite（触发器/更新钩子）、Oracle（LogMiner）、MSSQL（CDC/变更跟踪），按方言能力适配。
   a. 验收条件：[PostgreSQL 通过 WAL 捕获，MySQL 通过 binlog 捕获] → [五方言均支持变更捕获]
8. **禁止项**（EARS: Unwanted）
   如果 CDC 影响默认 feature 编译或运行时行为，则系统应当通过 `cdc` feature gate 隔离，默认不启用变更捕获。
   a. 验收条件：[`cargo build` 默认编译] → [无 CDC，行为与 v3.9.0 一致]

### 5.9.2 交互流程

```plantuml
@startuml
title CDC 变更数据捕获 交互流程
actor "数据工程师" as data
participant "CdcCapturer" as capturer
database "DB(WAL/binlog)" as db
participant "ChangeEvent" as event
rectangle "sz-orm-queue\n(6 provider)" as mq
rectangle "HTTP webhook" as webhook

data -> capturer : 启动 CDC(表=users, 下游=Kafka+webhook)
loop 持续捕获
  capturer -> db : 读取 WAL/binlog 变更
  db --> capturer : 变更记录
  capturer -> event : 构造 ChangeEvent(before/after/op/ts/txid)
  alt 启用脱敏
    capturer -> capturer : 敏感字段脱敏
  end
  par 并行分发
    capturer -> mq : 发送到 Kafka
  else
    capturer -> webhook : POST 到 webhook
  end
  capturer -> capturer : 更新消费位点(LSN/GTID)
end
@enduml
```

### 5.9.3 异常场景

1. **WAL/binlog 不可用**
   a. 触发条件：PostgreSQL WAL 未配置逻辑复制 / MySQL binlog 未开启
   b. 系统行为：启动失败，提示配置
   c. 用户感知：错误提示"PostgreSQL logical replication not configured, enable wal_level=logical"
2. **下游分发失败**
   a. 触发条件：Kafka 不可用 / webhook 超时
   b. 系统行为：事件缓冲到本地队列（有界），下游恢复后重发，缓冲满则告警
   c. 用户感知：告警"cdc downstream kafka unavailable, buffering events"
3. **位点持久化失败**
   a. 触发条件：位点持久化存储不可用
   b. 系统行为：暂停捕获，告警人工介入，避免重启后丢事件
   c. 用户感知：告警"cdc checkpoint store unavailable, capture paused"

---

# 6. 数据约束

## 6.1 需求项

1. **需求 ID**：唯一标识，格式 `REQ-V40-xxx`（xxx = 001~009），必填。
2. **需求名称**：人类可读名称，必填。
3. **优先级**：P0 / P1 / P2，必填。
4. **分类**：AI 调优 / 多 LLM / 混合搜索 / 数据治理 / 运维自动化 / 高可用 / 云原生 / GraphQL / 实时同步，必填。
5. **EARS 分类**：Ubiquitous / Event-driven / State-driven / Optional / Unwanted，每条业务规则必填。
6. **验证方法**：可执行的验证命令或测试描述，必填。
7. **代码证据**：相关 file:line 引用，必填，遵循审计合规铁律。
8. **验收条件**：触发场景 → 预期行为，必填。
9. **状态**：PASS / FAIL / PENDING，必填。
10. **与 v3.9.0 兼容性**：feature gate 隔离 / 既有 API 保留 / 测试基线不回退，必填。

## 6.2 输出对象

1. **AutoTuningReport**：四阶段报告（detect/advise/apply/verify），建议列表（type/sql_before/sql_after/risk/expected_gain），采纳率，前后耗时对比，回归标记。
2. **LlmConfig**：provider（Claude/Gemini/Ollama/OpenAI）、model、api_key、api_base、timeout、max_tokens、fallback_provider。
3. **HybridSearchResult**：统一结果（id/score/source/metadata），融合策略（RRF/加权/级联），各源原始结果，降级标记。
4. **LineageGraph**：节点（table.column），边（依赖关系），图类型（DAG），影响分析结果，溯源分析结果。
5. **RebalancePlan**：迁移计划（source_shard → target_shard、行数、预估时间），进度（已迁移/剩余/百分比），一致性校验结果。
6. **FailoverEvent**：故障时间，检测确认次数，提升的 slave，数据丢失评估（延迟/丢失行数），操作者，恢复时间，审计记录。
7. **ServiceMeshConfig**：mesh 类型（Istio/Linkerd），mTLS 模式（STRICT/PERMISSIVE），流量治理策略（金丝雀/蓝绿/熔断/重试），sidecar 注入配置。
8. **ChangeEvent**：op（Insert/Update/Delete），before（Option<Row>），after（Option<Row>），timestamp，transaction_id，table，schema。
9. **CdcCheckpoint**：方言，位点（WAL LSN / binlog GTID / 触发器序号），持久化存储，最后更新时间。

---

# 7. 需求追溯矩阵

| 需求编号 | 需求项 | 优先级 | 分类 | EARS 分类 | 验收条件（节选） | 现有代码证据 | 与 v3.9.0 兼容性 |
|---------|--------|--------|------|----------|----------------|-------------|----------------|
| REQ-V40-001 | AI 自动调优闭环 | P0 | AI 调优 | Ubiquitous/Optional/Unwanted | 四阶段闭环 + 检测/建议/执行/验证 + 回归回滚 | `packages/sz-orm-ai/src/query_plan_optimizer.rs:515` UnifiedQueryOptimizer、`index_advisor.rs:100` IndexAdvisor、`rewrite_advisor.rs:89` RewriteAdvisor、`explain_parser.rs:50` ExplainPlanParser（5 方言） | `ai-auto-tuning` feature gate，既有 UnifiedQueryOptimizer 保留 |
| REQ-V40-002 | 多 LLM 模型支持 | P0 | 多 LLM | Ubiquitous/Optional/Unwanted | LlmProvider trait + 四 provider + 统一配置切换 + fallback | `packages/sz-orm-ai/src/query_plan_optimizer.rs:177` OptimizerConfig、`:207` with_llm（OpenAI 兼容）、`real_embedding.rs` | `multi-llm` feature gate，既有 with_llm 包装为 OpenAIProvider |
| REQ-V40-003 | 混合搜索 | P1 | 混合搜索 | Ubiquitous/State-driven/Optional/Unwanted | 三源融合 + RRF/加权/级联 + 并行查询 + 部分降级 | `packages/sz-orm-vector/src/lib.rs:189` PgVectorStore、`:113` SearchResult、`packages/sz-orm-search/src/lib.rs` ES/OpenSearch/Meilisearch | `hybrid-search` feature gate，既有向量/全文搜索保留 |
| REQ-V40-004 | 数据 lineage | P1 | 数据治理 | Ubiquitous/Optional/Unwanted | 字段级血缘 + DAG + 影响分析 + 溯源 + 审计集成 | `packages/sz-orm-audit/src/lib.rs:691` HashChainEntry、`:778` HashChainAuditor、`:862` verify | `data-lineage` feature gate，既有 HashChainAuditor 保留 |
| REQ-V40-005 | 分片自动 rebalance | P1 | 运维自动化 | Ubiquitous/Unwanted | 最小搬迁 + 断点续传 + 查询不中断 + 进度可观测 | `packages/sz-orm-sharding/src/lib.rs:130` ShardingRouter、`:60` ShardingStrategy、`enhanced.rs` | `shard-rebalance` feature gate，既有 ShardingRouter 保留 |
| REQ-V40-006 | 数据库 failover 自动化 | P1 | 高可用 | Ubiquitous/Unwanted | 自动检测 + slave 提升 + 数据丢失评估 + 审计 + 30s 切换 | `packages/sz-orm-rw/src/lib.rs:331` ReadWriteRouter、`:219` HealthChecker、`:37` SlaveHealth、`:911` test_router_failover | `auto-failover` feature gate，既有 ReadWriteRouter/HealthChecker 保留 |
| REQ-V40-007 | 服务网格集成 | P2 | 云原生 | Ubiquitous/Unwanted | Istio/Linkerd 适配 + mTLS + 流量治理 + sidecar 注入 | `packages/sz-orm-observability/src/lib.rs:250` MetricsRegistry、`:443` MetricsAccessControl、`sz-orm-tracing` OTLP | `service-mesh` feature gate，既有可观测性保留 |
| REQ-V40-008 | GraphQL 深度集成 | P2 | GraphQL | Ubiquitous/Optional/Unwanted | async-graphql 对接 + Subscription + Relay + Federation + 工单化错误 | `packages/sz-orm-graphql/src/lib.rs:36` GraphQLSchema、`:182` GraphQLServer、`dataloader.rs:89` DataLoader、`:74` BatchLoader | `async-graphql-integration` feature gate，既有 GraphQLServer/DataLoader 保留 |
| REQ-V40-009 | CDC 变更数据捕获 | P2 | 实时同步 | Ubiquitous/Optional/Unwanted | 变更捕获 + 下游分发 + Exactly-Once + 断点续传 + 五方言 | `packages/sz-orm-queue/src/lib.rs` 6 provider、`real_kafka.rs`/`real_nats.rs`/`real_pulsar.rs`、`packages/sz-orm-audit/src/lib.rs` 审计 | `cdc` feature gate，既有 sz-orm-queue 保留 |

---

# 8. 验收标准总览

## 8.1 P0 类（最高优先级）

| 编号 | 验收标准 | 验证方法 |
|------|---------|---------|
| REQ-V40-001 | 四阶段闭环（Detect/Advise/Apply/Verify）+ 复用既有优化器 + 低风险自动执行 + 回归回滚 + 调优报告 | 配置慢查询阈值，运行 AutoTuningPipeline 验证四阶段报告；调优后回归验证自动回滚 |
| REQ-V40-002 | LlmProvider trait + 四 provider（Claude/Gemini/Ollama/OpenAI）+ 统一配置切换 + 运行时热切换 + fallback + 既有 OpenAI 包装 | 配置不同 provider 验证调用；运行时切换验证热更新；provider 故障验证 fallback |

## 8.2 P1 类（高优先级）

| 编号 | 验收标准 | 验证方法 |
|------|---------|---------|
| REQ-V40-003 | 三源融合（向量+全文+结构化）+ RRF/加权/级联 + 并行查询 ≤200ms + 部分降级 + 结构化下推 | 三源查询验证融合排序；某源不可用验证降级；过滤下推验证 |
| REQ-V40-004 | 字段级血缘 + DAG + 影响分析 + 溯源 + 审计集成 + 导出 DOT/JSON | 执行 SQL 验证 lineage 图；影响分析验证下游列表；导出验证可视化 |
| REQ-V40-005 | 最小搬迁 + 断点续传 + 查询不中断（双写/影子读）+ 进度可观测 + 复用 ShardingRouter | 扩容 3→4 分片验证迁移；中断验证断点续传；迁移中查询验证不中断 |
| REQ-V40-006 | 自动检测（3 次确认）+ slave 提升 + 数据丢失评估 + 审计 + 30s 切换 + 复用 HealthChecker | 主库故障验证自动 failover；延迟超阈值验证人工确认；验证 30s 切换 |

## 8.3 P2 类（中优先级）

| 编号 | 验收标准 | 验证方法 |
|------|---------|---------|
| REQ-V40-007 | Istio/Linkerd 适配 + mTLS STRICT + 流量治理（金丝雀/蓝绿/熔断/重试）+ sidecar 注入 + 复用可观测性 | 配置 mesh 验证 CRD 生成；金丝雀验证路由规则；mTLS 验证加密 |
| REQ-V40-008 | async-graphql 对接 + Subscription + Relay + Federation + 工单化错误 + 复用 DataLoader | 查询关联验证 DataLoader N+1 消除；Subscription 验证推送；Relay 验证分页 |
| REQ-V40-009 | 变更捕获（五方言）+ 下游分发（6 provider + webhook）+ Exactly-Once + 断点续传 + 脱敏 | 执行 UPDATE 验证 ChangeEvent；重启验证断点续传；脱敏验证敏感字段 |

## 8.4 全局验收条件

1. **API 兼容性**：v4.0.0 既有公开 API 完全向后兼容，sz-pay 既有代码不受影响。
2. **feature gate 隔离**：所有新能力通过 feature gate 隔离（`ai-auto-tuning` / `multi-llm` / `hybrid-search` / `data-lineage` / `shard-rebalance` / `auto-failover` / `service-mesh` / `async-graphql-integration` / `cdc`），默认 feature 行为不变。
3. **测试基线不回退**：v3.9.0 已验收测试基线（6760+ passed）不回退，v4.0.0 仅增不减。
4. **五方言一致**：新增能力在 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言上行为一致（AI 调优/failover/CDC 按方言能力适配）。
5. **审计证据**：每项需求结论附 file:line 证据，遵循 AGENTS.md 审计合规铁律。
6. **14 道门禁通过**：v4.0.0 须通过 AGENTS.md 定义的 14 道门禁（fmt/check/clippy/test/doc/audit/integration/占位检查/SQL 注入/feature 全组合/上游未改/文档一致/审计证据/文档同步）。
7. **无占位实现**：禁止 `todo!` / `unimplemented!` / `unreachable!`，所有新增代码须完整实现。
8. **unsafe 零容忍**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释。
9. **复用优先**：优先复用既有能力，不重复实现（如 AI 调优复用 UnifiedQueryOptimizer/IndexAdvisor/RewriteAdvisor，混合搜索复用 PgVectorStore/ES provider，CDC 复用 sz-orm-queue，GraphQL 复用 DataLoader，failover 复用 HealthChecker/ReadWriteRouter，rebalance 复用 ShardingRouter，服务网格复用 MetricsRegistry/OTLP）。
10. **依赖关系**：REQ-V40-002（多 LLM）为 REQ-V40-001（AI 调优）的 LLM provider 基础；REQ-V40-009（CDC）为 REQ-V40-008（GraphQL Subscription）的数据源；其余需求相互独立，可并行开发。

## 8.5 需求依赖关系

```plantuml
@startuml
title v4.0.0 需求依赖关系图
REQ-V40-002 "多 LLM 模型" --> REQ-V40-001 "AI 自动调优闭环" : 提供 LlmProvider
REQ-V40-009 "CDC" --> REQ-V40-008 "GraphQL 深度集成" : 提供 Subscription 数据源
REQ-V40-003 "混合搜索" : 独立
REQ-V40-004 "数据 lineage" : 独立
REQ-V40-005 "分片 rebalance" : 独立
REQ-V40-006 "failover 自动化" : 独立
REQ-V40-007 "服务网格集成" : 独立
@enduml
```