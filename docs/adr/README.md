# Architecture Decision Records (ADR)

> 架构决策记录。每个文件记录一个关键设计决策的**背景、决策、后果**，供后续维护者（人或 AI）快速理解"为什么这么写"。

## 目录

| 编号 | 标题 | 状态 |
|------|------|------|
| [0001](0001-连接池用-AtomicU32-而非-Mutex.md) | 连接池用 AtomicU32 替代 Mutex\<u32\> | Accepted |
| [0002](0002-SQL标识符校验用白名单而非-quote.md) | SQL 标识符校验用白名单而非 quote() | Accepted |
| [0003](0003-事务嵌套用-SAVEPOINT-加深度限制.md) | 事务嵌套用 SAVEPOINT + 深度限制 | Accepted |
| [0004](0004-批量插入分片防止超限.md) | 批量插入分片防止数据库参数超限 | Accepted |
| [0005](0005-Connection-trait-手动解糖-async.md) | Connection trait 手动解糖 async 方法 | Accepted |
| [0006](0006-关联关系加载三策略-eager-join-subquery.md) | 关联关系加载采用三种策略（eager/join/subquery） | Accepted |
| [0007](0007-ResultMap分组聚合用主键字符串拼接.md) | ResultMap 分组聚合用主键字符串拼接 | Accepted |
| [0008](0008-连接池acquire持锁不await-close.md) | 连接池 acquire 持锁期间不 await close() | Accepted |
| [0009](0009-QueryBuilder只生成SQL不执行.md) | QueryBuilder 只生成 SQL 不执行 | Accepted |

## 为什么需要 ADR？

AI 驱动的项目存在"无状态"问题：每次 AI 会话从零开始，不知道为什么 MorphTo 用了 `is_valid_sql_identifier` 而不是直接 `quote()`。ADR 解决这个问题——每次关键设计决策记录在此，后续维护时先读 ADR，避免误改。

## ADR 与 Bug 定位

ADR 记录"为什么这么写"，是**决策记忆**而非**运行时监控**。生产 bug 定位需要组合：

| 层 | 工具 | 说明 |
|----|------|------|
| 决策层 | ADR | 理解设计意图，排除"是不是设计如此" |
| 运行时层 | `#[tracing::instrument]` | core 包关键路径已标注（pool/query/find_with_related/result_map） |
| 指标层 | sz-orm-observability (Prometheus) | 连接池/查询/事务指标 |
| 追踪层 | sz-orm-tracing (OpenTelemetry) | 分布式追踪 |

**Bug 定位流程**：先查 ADR 判断是否为已知设计限制 → 查 tracing span 定位耗时/空返回 → 查 Prometheus 指标判断系统健康度 → 查源码验证。
