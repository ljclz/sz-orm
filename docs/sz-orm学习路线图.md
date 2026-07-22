# SZ-ORM 学习路线图

> 从零基础到生产级应用的分阶段学习指南
> 更新日期：2026-07-22 · 适用版本：v1.0.0+

---

## 路线图概览

```
初学者 (L1)          进阶 (L2)            高级 (L3)            专家 (L4)
┌─────────┐       ┌─────────┐        ┌─────────┐        ┌─────────┐
│ 安装配置  │─────→│ 事务管理  │──────→│ 分库分表  │──────→│ 源码架构  │
│ Model    │       │ 连接池   │        │ 分布式事务 │        │ 性能调优  │
│ CRUD     │       │ 关联关系  │        │ 可观测性  │        │ 安全审计  │
│ QueryBuilder│    │ 缓存    │        │ AI 向量  │        │ 贡献指南  │
└─────────┘       │ 钩子    │        │ 多租户   │        └─────────┘
                  │ 软删除  │        └─────────┘
                  └─────────┘
约 1-3 天           约 3-7 天            约 7-14 天           约 14+ 天
```

---

## L1 · 初学者路线（1-3 天）

### 学习目标

完成本阶段后，你能够：
- 安装 SZ-ORM 并连接数据库
- 定义 Model 并执行基本 CRUD 操作
- 使用 QueryBuilder 构建查询
- 理解 Value 类型系统

### 推荐资源

| 序号 | 资源 | 重点章节 |
|------|------|----------|
| 1 | [使用指南](sz-orm使用指南.md) | 第 1-3 章：安装、Model 定义、CRUD |
| 2 | [API 参考](sz-ormAPI参考.md) | Value 枚举、Model trait、QueryBuilder |
| 3 | [README 快速开始](../README.md#快速开始) | 5 分钟上手示例 |

### 学习步骤

#### Step 1: 环境准备（Day 1 上午）

```toml
# Cargo.toml
[dependencies]
sz-orm-core = "1.0"
tokio = { version = "1", features = ["full"] }
```

#### Step 2: 定义 Model（Day 1 下午）

```rust
use sz_orm_core::Model;

#[derive(Debug, Clone, Default)]
struct User {
    id: i64,
    name: String,
    email: String,
}

impl Model for User {
    type PrimaryKey = i64;
    fn table_name() -> &'static str { "users" }
    fn pk(&self) -> Self::PrimaryKey { self.id }
    fn set_pk(&mut self, pk: Self::PrimaryKey) { self.id = pk; }
}
```

#### Step 3: CRUD 操作（Day 2）

```rust
use sz_orm_core::{QueryBuilder, Value, dialect::MySqlDialect};
use std::collections::HashMap;

// SELECT
let qb = QueryBuilder::<User>::new(Box::new(MySqlDialect))
    .table("users")
    .where_cond("id = 1");
let sql = qb.build_select();

// INSERT
let mut data = HashMap::new();
data.insert("name".to_string(), Value::String("Alice".to_string()));
data.insert("email".to_string(), Value::String("alice@example.com".to_string()));
let sql = QueryBuilder::<User>::new(Box::new(MySqlDialect))
    .table("users")
    .build_insert(&data);
```

#### Step 4: Value 类型系统（Day 3）

学习 20 种 Value 变体：Null、I64、F64、Bool、String、Bytes、Array、Date、DateTime 等。

### 验收标准

- [ ] 能独立创建项目并编译通过
- [ ] 能定义至少 2 个 Model
- [ ] 能使用 QueryBuilder 构建 SELECT/INSERT/UPDATE/DELETE
- [ ] 理解 Value 枚举的 20 种变体

---

## L2 · 进阶路线（3-7 天）

### 学习目标

完成本阶段后，你能够：
- 管理事务（ACID、保存点、嵌套）
- 配置和使用连接池
- 处理关联关系（BelongsTo / HasMany / HasOne）
- 使用多级缓存和钩子系统
- 实现软删除和多租户

### 推荐资源

| 序号 | 资源 | 重点章节 |
|------|------|----------|
| 1 | [使用指南](sz-orm使用指南.md) | 第 4-8 章：事务、连接池、关联、缓存、钩子 |
| 2 | [API 参考](sz-ormAPI参考.md) | Pool、PoolConfig、HookDispatcher、SoftDelete |
| 3 | [架构设计](sz-orm架构设计.md) | 连接池架构、钩子系统架构 |
| 4 | [API 契约](api-contracts.md) | 公共 API 稳定性约束 |

### 学习步骤

#### Step 1: 事务管理（Day 4）

```rust
// ACID 事务
conn.begin_transaction().await?;
conn.execute("UPDATE accounts SET balance = balance - 100 WHERE id = 1").await?;
conn.execute("UPDATE accounts SET balance = balance + 100 WHERE id = 2").await?;
conn.commit().await?;
// 或回滚：conn.rollback().await?;
```

学习要点：隔离级别、保存点（20 层嵌套）、TransactionManager。

#### Step 2: 连接池（Day 5）

```rust
use sz_orm_core::{Pool, PoolConfig};
use std::sync::Arc;

let config = PoolConfig {
    max_size: 32,
    min_idle: Some(4),
    max_lifetime: Some(Duration::from_secs(3600)),
    ..PoolConfig::default()
};
let pool = Pool::new(config, Arc::new(ConnFactory))?;
let conn = pool.acquire().await?;
// 使用 conn...
pool.release(conn).await;
```

#### Step 3: 关联关系（Day 6）

```rust
// BelongsTo: User belongsTo Role
// HasMany: User hasMany Post
// HasOne: User hasOne Profile
// 使用 find_with_related 进行 Eager Loading
```

#### Step 4: 缓存 + 钩子 + 软删除（Day 7）

- MemoryCache / MultiLevelCache / L2Cache
- 16 种钩子事件（before_insert / after_update 等）
- SoftDelete trait + 全局作用域

### 验收标准

- [ ] 能使用事务进行安全的资金转账
- [ ] 能配置连接池并理解各参数含义
- [ ] 能定义 BelongsTo / HasMany 关联
- [ ] 能注册 before_insert / after_update 钩子
- [ ] 能实现软删除模型

---

## L3 · 高级路线（7-14 天）

### 学习目标

完成本阶段后，你能够：
- 实施分库分表策略
- 使用分布式事务（2PC / TCC / Saga）
- 集成可观测性（Prometheus + OpenTelemetry）
- 使用 AI 向量搜索和 NL→SQL
- 实现多租户数据隔离

### 推荐资源

| 序号 | 资源 | 重点章节 |
|------|------|----------|
| 1 | [使用指南](sz-orm使用指南.md) | 第 9-15 章：分库分表、分布式事务、可观测性 |
| 2 | [架构设计](sz-orm架构设计.md) | 分布式事务架构、可观测性架构 |
| 3 | [工程实践](sz-orm-engineering-practices.md) | 测试金字塔 T1-T6、Soak Test |
| 4 | [性能基准](sz-orm性能基准.md) | Criterion 基准结果 |

### 学习步骤

#### Step 1: 分库分表（Day 8-9）

- sz-orm-sharding：水平分片、垂直分片
- 分片策略：Hash / Range / 一致性哈希
- 跨分片查询路由

#### Step 2: 分布式事务（Day 10-11）

- 2PC（两阶段提交）
- TCC（Try-Confirm-Cancel）
- Saga 模式
- 跨分片 ACID 协调器

#### Step 3: 可观测性（Day 12）

```rust
// sz-orm-observability: Prometheus exporter
// sz-orm-tracing: OpenTelemetry traceparent 传播
// SLO 监控：Google SRE 多窗口燃烧率
```

#### Step 4: AI 向量 + 多租户（Day 13-14）

- sz-orm-vector：pgvector 向量搜索（cosine / euclidean / dot）
- sz-orm-ai：NL→SQL 自然语言转 SQL
- TenantModel trait + TenantScope 自动过滤

### 验收标准

- [ ] 能配置分库分表策略并执行跨分片查询
- [ ] 能使用 TCC 模式实现分布式事务
- [ ] 能集成 Prometheus 指标导出
- [ ] 能使用 pgvector 进行相似度搜索
- [ ] 能实现多租户数据隔离

---

## L4 · 专家路线（14+ 天）

### 学习目标

完成本阶段后，你能够：
- 理解 SZ-ORM 完整架构设计
- 进行性能调优和瓶颈分析
- 执行安全审计
- 参与项目贡献

### 推荐资源

| 序号 | 资源 | 重点章节 |
|------|------|----------|
| 1 | [架构设计](sz-orm架构设计.md) | 39 包架构全景 |
| 2 | [性能基准](sz-orm性能基准.md) | 10 组 Criterion 基准 |
| 3 | [工程实践](sz-orm-engineering-practices.md) | Gate 1-10、Soak Test |
| 4 | [SECURITY](Security.md) | 安全策略 |
| 5 | [ADR 索引](adr/README.md) | 5 条架构决策记录 |
| 6 | [CONTRIBUTING](../CONTRIBUTING.md) | 贡献指南 |

### 学习步骤

#### Step 1: 源码架构（Day 15-18）

- 39 个工作空间成员的职责划分
- 核心引擎（sz-orm-core）内部架构
- Dialect 抽象层设计
- 宏系统（sz-orm-macros）

#### Step 2: 性能调优（Day 19-21）

```bash
# 运行基准测试
cargo bench --package sz-orm-core --bench core_bench

# 查看报告
open target/criterion/index.html
```

10 组基准测试：
1. value_to_param — Value 类型转换
2. dialect_escape_string — SQL 转义
3. dialect_build_create_table — DDL 生成
4. dialect_build_pagination — 分页 SQL
5. pool_acquire_release — 连接池
6. in_memory_scan — 数据扫描
7. json_parsing — JSON 解析
8. query_builder_select — 查询构建
9. query_builder_insert_update — INSERT/UPDATE
10. value_batch_to_param — 批量转换

#### Step 3: 安全审计（Day 22-24）

- SQL 注入防护：编译期 `sql_string!` + 运行时 `validate()`
- 12 种注入模式检测
- cargo audit / cargo deny / Semgrep SAST
- SECURITY.md 安全策略

#### Step 4: 贡献代码（Day 25+）

- 阅读 CONTRIBUTING.md
- 了解 Git 工作流（分支规范、commit 格式）
- 运行 Gate 1-10 质量门禁
- 提交 PR

### 验收标准

- [ ] 能画出 39 包架构关系图
- [ ] 能运行 10 组基准测试并解读结果
- [ ] 能通过 cargo audit + cargo deny + Semgrep
- [ ] 能提交符合规范的 PR

---

## 按角色推荐路线

### 后端开发者（快速上手）

```
L1 全部 → L2 Step 1-2 → L2 Step 4（软删除）
约 5 天
```

### 架构师（技术选型）

```
README 概览 → 架构设计 → API 契约 → 性能基准 → 成熟度评估报告
约 2 天
```

### DevOps（部署运维）

```
README 快速开始 → 工程实践 → 可观测性 → Soak Test
约 3 天
```

### 安全工程师

```
Security.md → SQL 校验模块 → cargo audit/deny → Semgrep 规则
约 2 天
```

---

## 常见问题

### Q: 需要先学 Rust 吗？

A: 是的。建议先完成 [The Rust Book](https://doc.rust-lang.org/book/) 前 1-10 章，重点掌握所有权、借用、trait、泛型、async/await。

### Q: 支持哪些数据库？

A: 11 种：MySQL、PostgreSQL、SQLite、Oracle 23ai、OceanBase、SQL Server、ClickHouse、Redis、MongoDB、VectorDB、PureJsDb。

### Q: 如何选择 sz-orm-sqlx 和 sz-orm-core？

A: `sz-orm-core` 提供 ORM 抽象层（Mock 连接），`sz-orm-sqlx` 提供真实数据库连接（基于 sqlx）。生产环境使用 `sz-orm-sqlx`。

### Q: 如何参与贡献？

A: 阅读 [CONTRIBUTING.md](../CONTRIBUTING.md)， fork 仓库 → 创建分支 → 提交 PR。确保通过 Gate 1-10 质量门禁。

---

## 文档速查表

| 需求 | 文档 |
|------|------|
| 快速上手 | [README 快速开始](../README.md#快速开始) |
| 完整教程 | [使用指南](sz-orm使用指南.md) |
| API 查询 | [API 参考](sz-ormAPI参考.md) |
| 架构理解 | [架构设计](sz-orm架构设计.md) |
| 性能数据 | [性能基准](sz-orm性能基准.md) |
| 工程规范 | [工程实践](sz-orm-engineering-practices.md) |
| API 稳定性 | [API 契约](api-contracts.md) |
| 架构决策 | [ADR 索引](adr/README.md) |
| 安全策略 | [Security](Security.md) |
| 贡献指南 | [CONTRIBUTING](../CONTRIBUTING.md) |
| 版本变更 | [CHANGELOG](../CHANGELOG.md) |
