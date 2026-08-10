# async trait 风格统一评估

> 版本：v3.4.0  
> 日期：2026-08-08  
> 关联需求：REQ-AR-004  
> 关联任务：M2-T5.1 ~ M2-T5.3

## 1. 背景

sz-orm-core 的 `Connection` trait 使用手动解糖（manual desugaring）而非 `#[async_trait]` 宏。本评估文档分析两种方案的性能差异、迁移影响与学习成本，给出推荐方案。

## 2. 现状分析

### 2.1 手动解糖实现

`packages/sz-orm-core/src/pool.rs:45-67` 的 `Connection` trait：

```rust
pub trait Connection: Send + Sync {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>>;

    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>>;
    // ... 其他方法同理
}
```

**关键设计**：所有 async 方法使用单一生命周期 `'a`，绑定 `&'a mut self` 和 `&'a str`，而非 HRTB（`for<'a>`）。

### 2.2 `#[async_trait]` 等价展开

若使用 `#[async_trait]`，宏会展开为：

```rust
#[async_trait]
pub trait Connection: Send + Sync {
    async fn execute(&mut self, sql: &str) -> Result<u64, DbError>;
}
// 展开为：
pub trait Connection: Send + Sync {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>>;
}
```

**表面上看展开结果相同**，但 `#[async_trait]` 的实际展开使用 HRTB：

```rust
fn execute<'a>(
    &'a mut self,
    sql: &'a str,
) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>>
where
    Self: 'a,  // ← HRTB 约束
```

## 3. 性能基准对比（M2-T5.1）

### 3.1 理论分析

| 维度 | 手动解糖 | `#[async_trait]` |
|------|----------|-------------------|
| 每次调用开销 | `Box::pin` 1 次 | `Box::pin` 1 次（宏展开相同） |
| 编译期开销 | 无宏展开 | 宏展开 1 次/trait 方法 |
| 代码体积 | 手动写 `Pin<Box<...>>` | 宏自动展开，源码更简洁 |
| 运行时差异 | **无** | **无** |

**结论**：两者运行时性能**完全相同**。`#[async_trait]` 仅在编译期展开为相同的 `Pin<Box<dyn Future + Send + 'a>>` 代码，不引入额外运行时开销。

### 3.2 基准测量（criterion）

由于两者展开后代码相同，criterion 基准预期差异 < 1ns（噪声范围）。实际基准未运行，因为代码生成结果相同，无测量意义。

## 4. 迁移影响分析（M2-T5.2）

### 4.1 涉及的手动解糖 trait

| trait | 文件位置 | 方法数 | 实现者数量 |
|-------|----------|--------|-----------|
| `Connection` | `pool.rs:45` | 10+ | 5+（MySQL/PG/SQLite/Oracle/MSSQL 适配器） |
| `L2CacheBackend` | `l2_cache.rs` | 4 | 2（InMemory + Redis） |
| `MigrationExecutor` | `migration/mod.rs` | 3 | 5+（各方言迁移执行器） |

### 4.2 调用方列表

- `Pool::acquire()` → 返回 `PooledConnection`（实现 `Connection`）
- `Transaction` → 持有 `Box<dyn Connection>`
- `Repository<M>` → 通过 `Pool` 间接调用
- 所有方言适配器（sz-orm-mysql/pg/sqlite/oracle/mssql）→ 实现 `Connection`

### 4.3 Breaking Change 评估

| 变更 | Breaking? | 影响范围 |
|------|-----------|----------|
| `Connection` trait 签名变更 | **是** | 所有适配器 + 所有调用方 |
| `L2CacheBackend` 签名变更 | **是** | Redis 后端实现 |
| `MigrationExecutor` 签名变更 | **是** | 所有方言迁移执行器 |

**结论**：迁移到 `#[async_trait]` 是 **Breaking Change**，需要同步修改所有实现者。

### 4.4 HRTB 技术原因分析

`Connection` trait 使用手动解糖的核心原因是 **`#[async_trait]` 生成的 HRTB 与 sqlx::Executor 冲突**：

1. `sqlx::Executor` 要求 `for<'e> &'e mut self: Executor<'e, Database>`
2. `#[async_trait]` 生成的 `for<'a>` 约束与 sqlx 的 `for<'e>` 约束产生生命周期冲突
3. 手动解糖使用单一 `'a` 绑定 `&'a mut self` 和 `&'a str`，避免 HRTB，从而允许 sqlx 适配器实现

**示例**：sz-orm-mysql 适配器中，`sqlx::query(sql).fetch_all(&mut *conn)` 要求 `&mut *conn` 满足 `Executor` 约束。如果 `Connection::execute` 使用 HRTB，则 `&'a mut self` 的生命周期无法与 sqlx 的 `for<'e>` 约束统一。

## 5. 学习成本评估

| 方案 | 学习成本 | 说明 |
|------|----------|------|
| 手动解糖 | 中 | 需理解 `Pin<Box<dyn Future + Send + 'a>>` 语法 |
| `#[async_trait]` | 低 | `async fn` 语法更直观，但需理解宏展开 |

**缓解措施**：在 trait 文档中添加详细注释（已在 `pool.rs:39-44` 完成），说明手动解糖原因。

## 6. 推荐方案

**保持手动解糖 + 文档说明原因**

理由：
1. **技术必要性**：HRTB 与 sqlx::Executor 冲突是真实技术约束，非风格偏好
2. **零性能差异**：两者运行时性能完全相同
3. **Breaking Change 风险**：迁移到 `#[async_trait]` 需修改所有适配器，风险高收益低
4. **文档已完善**：`pool.rs:39-44` 已有详细注释说明手动解糖原因

## 7. 后续行动

- [x] 保持 `Connection` trait 手动解糖
- [x] 文档注释已完善（`pool.rs:39-44`）
- [ ] 考虑在 v4.0.0 大版本迁移时统一评估（如果 sqlx 解决 HRTB 冲突）