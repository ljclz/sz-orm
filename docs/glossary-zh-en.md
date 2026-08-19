# SZ-ORM 中英术语对照表

> 版本：v4.9.0
> 日期：2026-08-19
> 用途：确保英文文档翻译术语一致性（REQ-I18N-001~003）
> 规则：翻译时必须查阅本表，同一中文术语在所有英文文档中统一译法

---

## 核心数据结构

| 中文术语 | 英文译法 | 备注 |
|---------|---------|------|
| 连接池 | connection pool | |
| 连接池耗尽 | pool exhaustion | |
| 连接泄漏 | connection leak | |
| 预热 | prewarm | 连接池预热 |
| 获取 | acquire | 连接获取 |
| 释放 | release | 连接释放 |
| 方言 | dialect | SQL 方言 |
| 查询构造器 | query builder | |
| 派生宏 | derive macro | |
| 过程宏 | procedural macro | |
| 模型 | model | |
| 事务 | transaction | |
| 迁移 | migration | |
| 钩子 | hook | |
| 软删除 | soft delete | |
| 多租户 | multi-tenant | |
| 行级安全 | row-level security | RLS |
| 列级脱敏 | column masking | |
| 分片 | sharding | |
| 读写分离 | read-write splitting | |
| 主从切换 | primary-standby failover | |
| 脑裂 | split-brain | |
| 工作空间 | workspace | Cargo workspace |
| 工作空间成员 | workspace member | |
| 异步 | asynchronous | |
| 运行时 | runtime | Tokio runtime |

## 查询与 SQL

| 中文术语 | 英文译法 | 备注 |
|---------|---------|------|
| 参数化查询 | parameterized query | |
| SQL 注入 | SQL injection | |
| N+1 问题 | N+1 problem | |
| N+1 检测 | N+1 detection | |
| 零拷贝 | zero-copy | |
| 内联存储 | inline storage | |
| 类型安全 | type-safe | |
| 悲观锁 | pessimistic lock | |
| 共享锁 | shared lock | |
| 行锁 | row lock | |
| 查询缓存 | query cache | |
| 缓存一致性 | cache coherence | |
| 击穿 | cache penetration | |
| 雪崩 | cache stampede | |
| 失效 | invalidation | 缓存失效 |
| 写穿透 | write-through | |
| 写后 | write-behind | |

## 可观测性与运维

| 中文术语 | 英文译法 | 备注 |
|---------|---------|------|
| 可观测性 | observability | |
| 审计 | audit | |
| 指标采集 | metric collection | |
| 滑动窗口 | sliding window | |
| 基线 | baseline | |
| 突增 | spike | |
| 突增检测 | spike detection | |
| 异常检测 | anomaly detection | |
| 告警去重 | alert deduplication | |
| Prometheus 导出 | Prometheus export | |
| Welford 算法 | Welford's algorithm | |
| 消息轨迹 | message tracing | |
| 存储生命周期 | storage lifecycle | |
| 数据质量 | data quality | |
| 批量流式 | batch streaming | |
| 备份验证 | backup verification | |
| 数据 lineage | data lineage | |
| 变更数据捕获 | Change Data Capture | CDC |
| 服务网格 | service mesh | |
| 基准 | benchmark | |
| 火焰图 | flamegraph | |
| 慢查询诊断 | slow query diagnosis | |
| 自适应查询 | adaptive query | |

## 搜索与 AI

| 中文术语 | 英文译法 | 备注 |
|---------|---------|------|
| 混合搜索 | hybrid search | |
| 向量搜索 | vector search | |
| 全文搜索 | full-text search | |
| 自动调优 | auto-tuning | |
| 自动故障转移 | auto failover | |
| 索引建议 | index advice | |
| 查询重写 | query rewrite | |
| 意图分析 | intent analysis | |

## 工程化

| 中文术语 | 英文译法 | 备注 |
|---------|---------|------|
| 特性门控 | feature gate | |
| 生产就绪 | production ready | |
| 竞品 | competitor | |
| 成熟度 | maturity | |
| 路线图 | roadmap | |
| 交付 | delivery | |
| 文档测试 | doctest | |
| 安全审计 | security audit | |
| 覆盖率 | coverage | |
| 变异测试 | mutation testing | |
| 幻影交付 | phantom delivery | |
| 占位实现 | placeholder implementation | |
| 信创 | domestic computing | 信息技术应用创新 |
| 鲜视达 | Xianshida | 品牌名，保留原文 |
| ThinkORM | ThinkORM | 保留原文 |

---

## 术语统计

- 术语映射总数：75（≥ 50，满足 REQ-I18N-001）
- 品牌名保留：鲜视达（Xianshida）
- 保留原文：ThinkORM