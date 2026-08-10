# SZ-ORM 文档索引

> 版本：v3.4.0 | 更新日期：2026-08-09

## 快速导航

| 需求 | 文档 |
|------|------|
| 快速上手 | [sz-orm使用指南.md](sz-orm使用指南.md) |
| API 查询 | [sz-ormAPI参考.md](sz-ormAPI参考.md) |
| 架构理解 | [sz-orm架构设计.md](sz-orm架构设计.md) |
| 学习路径 | [sz-orm学习路线图.md](sz-orm学习路线图.md) |
| 从其他 ORM 迁移 | [migration/](migration/) |
| 工程规范 | [sz-orm-engineering-practices.md](sz-orm-engineering-practices.md) |

## 文档结构

### 1. 核心参考

| 文档 | 说明 |
|------|------|
| [sz-orm使用指南.md](sz-orm使用指南.md) | 快速上手、CRUD、查询、事务、迁移完整示例 |
| [sz-ormAPI参考.md](sz-ormAPI参考.md) | 全部公开 API 详解 |
| [sz-orm架构设计.md](sz-orm架构设计.md) | 架构设计文档（模块划分、数据流、设计决策） |
| [sz-orm学习路线图.md](sz-orm学习路线图.md) | 从入门到精通的学习路径 |
| [api-contracts.md](api-contracts.md) | API 契约定义 |
| [API-STABILITY.md](API-STABILITY.md) | API 稳定性承诺与版本兼容性 |

### 2. 工程规范

| 文档 | 说明 |
|------|------|
| [sz-orm-engineering-practices.md](sz-orm-engineering-practices.md) | 10 道门禁 + 五维审查 + AI 辅助开发约束 |
| [Security.md](Security.md) | 安全设计（SQL 注入防护、连接池安全、脱敏） |
| [ADR与生产Bug定位规范.md](ADR与生产Bug定位规范.md) | ADR 编写规范与生产 Bug 定位流程 |

### 3. 对比分析

| 文档 | 说明 |
|------|------|
| [sz-orm与同类产品对比分析.md](sz-orm与同类产品对比分析.md) | Diesel/SeaORM/SQLx/SZ-ORM 深度对比 |

### 4. 迁移指南

| 文档 | 说明 |
|------|------|
| [migration/diesel_to_sz_orm.md](migration/diesel_to_sz_orm.md) | Diesel → SZ-ORM（概念映射 + API 对照 + 示例 + 陷阱） |
| [migration/seaorm_to_sz_orm.md](migration/seaorm_to_sz_orm.md) | SeaORM → SZ-ORM |
| [migration/sqlx_to_sz_orm.md](migration/sqlx_to_sz_orm.md) | SQLx → SZ-ORM |

### 5. 架构决策记录（ADR）

| 编号 | 决策 |
|------|------|
| [ADR-0001](adr/0001-连接池用-AtomicU32-而非-Mutex.md) | 连接池用 AtomicU32 而非 Mutex |
| [ADR-0002](adr/0002-SQL标识符校验用白名单而非-quote.md) | SQL 标识符校验用白名单而非 quote |
| [ADR-0003](adr/0003-事务嵌套用-SAVEPOINT-加深度限制.md) | 事务嵌套用 SAVEPOINT 加深度限制 |
| [ADR-0004](adr/0004-批量插入分片防止超限.md) | 批量插入分片防止超限 |
| [ADR-0005](adr/0005-Connection-trait-手动解糖-async.md) | Connection trait 手动解糖 async |
| [ADR-0006](adr/0006-关联关系加载三策略-eager-join-subquery.md) | 关联关系加载三策略 |
| [ADR-0007](adr/0007-ResultMap分组聚合用主键字符串拼接.md) | ResultMap 分组聚合用主键字符串拼接 |
| [ADR-0008](adr/0008-连接池acquire持锁不await-close.md) | 连接池 acquire 持锁不 await close |
| [ADR-0009](adr/0009-QueryBuilder只生成SQL不执行.md) | QueryBuilder 只生成 SQL 不执行 |
| [ADR-0011](adr/0011-异步运行时仅支持tokio.md) | 异步运行时仅支持 tokio |

### 6. v3.4.0 设计文档（SDD）

| 文档 | 说明 |
|------|------|
| [spec/v3.4.0/spec.md](spec/v3.4.0/spec.md) | 需求规格（31 条 EARS 格式需求） |
| [spec/v3.4.0/design.md](spec/v3.4.0/design.md) | 技术设计（6 大方向架构方案） |
| [spec/v3.4.0/tasks.md](spec/v3.4.0/tasks.md) | 任务分解（44 主任务 / 160 子任务） |

### 7. v3.4.0 技术评估

| 文档 | 说明 |
|------|------|
| [async_trait_style_evaluation.md](async_trait_style_evaluation.md) | async trait 风格评估（dyn Trait vs async-trait vs impl Trait） |
| [query_builder_selection_guide.md](query_builder_selection_guide.md) | QueryBuilder 选择指南（typed DSL vs &str API） |
| [result_map_macro_evaluation.md](result_map_macro_evaluation.md) | result_map 宏生成评估 |
| [v3.3.0-upgrade-guide.md](v3.3.0-upgrade-guide.md) | v3.3.0 升级指南 |
| [migration/diesel_to_sz_orm.md](migration/diesel_to_sz_orm.md) | Diesel → SZ-ORM 迁移指南 |
| [migration/seaorm_to_sz_orm.md](migration/seaorm_to_sz_orm.md) | SeaORM → SZ-ORM 迁移指南 |
| [migration/sqlx_to_sz_orm.md](migration/sqlx_to_sz_orm.md) | SQLx → SZ-ORM 迁移指南 |

## 版本历程

| 版本 | 状态 | 关键特性 |
|------|------|----------|
| v3.4.0 | **当前** | 测试覆盖补齐 + 架构改进 + 性能优化 + 类型安全 + 文档生态 + sz-pay 案例 |
| v3.3.0 | 已完成 | 多租户 + 分布式缓存一致性 + GraphQL N+1 检测 |
| v3.2.0 | 已完成 | 零拷贝序列化 + 连接池预热 + 查询计划缓存 + SIMD |
| v3.0.0 | 已完成 | 五方言支持 + 多后端就绪 |
| v2.4.0 | 已完成 | 连接池优化 + 事务增强 + 流式查询 |