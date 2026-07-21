# ADR-0005: Connection trait 手动解糖 async 方法

- **状态**: Accepted
- **日期**: 2026-07-19
- **相关代码**: `packages/sz-orm-core/src/pool.rs` (L22-L49)

## 背景

`Connection` trait 的 async 方法（`execute`, `query`, `begin_transaction` 等）需要接受 `&str` 参数。最初使用 `#[async_trait]` 宏，但它会将 `&str` 参数展开为 HRTB（higher-ranked trait bound）：

```rust
// async_trait 展开后（简化）
fn execute<'a>(&'a mut self, sql: &'a str)
    -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>>;
```

当 sqlx 适配器尝试实现这个 trait 时，`&str` 的 HRTB 与 sqlx 的 `Executor` trait 生命周期约束冲突，导致编译失败。

## 决策

不使用 `#[async_trait]`，手动写出 async 方法的签名，使用单一生命周期 `'a` 绑定 `&'a mut self` 和 `&'a str`：

```rust
pub trait Connection: Send + Sync {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, crate::DbError>> + Send + 'a>>;

    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, crate::DbError>> + Send + 'a>>;
    // ...
}
```

## 后果

**正面：**
- sqlx 适配器（`sz-orm-sqlx`）可以正常实现 `Connection` trait
- 避免了 `async_trait` 宏的隐式 HRTB 展开，生命周期更直观

**负面：**
- 签名冗长，不如 `async fn` 简洁
- 实现者需要手动 `Box::pin`，容易遗忘
- 未来 Rust 原生 async trait 稳定后可能需要迁移

**注意事项：**
- `#[async_trait]` 仍用于 `ConnectionFactory`（其 `create()` 方法不涉及 `&str` 参数，无 HRTB 冲突）
- 如果 Rust 原生 async trait (RFC 3185) 稳定且 sqlx 支持，可考虑迁移回 `async fn in trait`
