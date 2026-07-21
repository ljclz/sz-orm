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

## 为什么需要 ADR？

AI 驱动的项目存在"无状态"问题：每次 AI 会话从零开始，不知道为什么 MorphTo 用了 `is_valid_sql_identifier` 而不是直接 `quote()`。ADR 解决这个问题——每次关键设计决策记录在此，后续维护时先读 ADR，避免误改。
