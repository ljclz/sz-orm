# API 反向破坏性审查报告 — sz-orm-core

> 审查日期：2026-07-25
> 审查范围：`packages/sz-orm-core/src/` — pool.rs, lib.rs, value.rs, error.rs, transaction.rs, model.rs
> 审查维度：Send/Sync、生命周期、错误粒度、Panic 风险、API 易用性

## 一、审查的 trait 和类型清单

### Trait
| Trait | 文件位置 | 行号 | 关键约束 |
|-------|---------|------|---------|
| `Connection` | pool.rs:26 | 26 | `Send + Sync` |
| `ConnectionFactory` | pool.rs:399 | 398 | `Send + Sync`, `#[async_trait]` |
| `Model` | model.rs:37 | 37 | `Send + Sync + Sized + 'static` |
| `ModelExt` | model.rs | (impl block) | 无 trait 约束 |
| `ActiveRecord` | model.rs:222 | 222 | `Model + ModelExt + RelationLoader + Clone + Send + Sync` |
| `RelationLoader` | model.rs | (impl block) | 无 Send/Sync 约束 |

### 结构体/枚举
| 类型 | 文件位置 | Send/Sync 来源 |
|------|---------|----------------|
| `Pool` | pool.rs:408 | 隐式（所有字段为 `Arc<...>`, `PoolConfig` 含 Copy 类型字段） |
| `PooledConnection` | pool.rs:126 | 隐式（`Box<dyn Connection>` 因 `Connection: Send + Sync` 为 Send+Sync） |
| `PoolConfig` | pool.rs:281 | 隐式（所有字段为 Copy/浅类型） |
| `PoolStatus` | pool.rs:331 | 隐式（仅 u32 字段） |
| `Transaction` | transaction.rs:116 | 隐式（`Arc<Mutex<Option<Box<dyn Connection>>>>` 为 Send+Sync） |
| `TransactionManager` | transaction.rs:471 | 隐式（`Arc<Mutex<HashMap>>` 为 Send+Sync） |
| `Value` | value.rs:13 | 隐式（仅 `String`/`Vec`/`HashMap` 等基础类型） |
| `DbError` | error.rs:11 | 隐式 |
| `PoolError` | error.rs:200 | 隐式 |
| `TxError` | error.rs:338 | 隐式 |
| `WithRelation<M>` | model.rs:242 | **无显式 Send/Sync 约束** |

### 关联类型
| 类型 | Owner | 约束 |
|------|-------|------|
| `Model::PrimaryKey` | Model trait | `Send + Sync + Debug + Display + Clone + Default` |

---

## 二、审查发现的问题

### 问题 1 — 🔴 红牌：`WithRelation<M>` 缺少 Send 约束

**位置**：`model.rs:242`

```rust
pub struct WithRelation<M: Model + ModelExt + RelationLoader> {
    model: M,
    relations: Vec<String>,
}
```

**风险**：`WithRelation` 在 `ActiveRecord::with()` 中被返回，而 `ActiveRecord: Clone + Send + Sync`。但 `WithRelation<M>` 本身未标注 `Send` 约束，虽然其字段类型使之自动满足 `Send`（当 `M: Send` 时）。主要风险在于：

1. 如果未来在 `WithRelation` 中添加了非 Send 字段（如 `Rc` 或 `*mut` 指针），使用 `ActiveRecord` 的用户代码会突然无法编译。
2. 当前 `ActiveRecord` trait 已要求 `M: Send + Sync`，但 `WithRelation<M>` 未显式延续此要求，属于**隐式依赖**。

**修复建议**：
```rust
// 添加 where 子句
impl<M: Model + ModelExt + RelationLoader + Send> WithRelation<M> { ... }
// 或为 WithRelation<M> 添加自动 impl Send
unsafe impl<M: Model + ModelExt + RelationLoader + Send> Send for WithRelation<M> {}
// 更推荐：在 struct 上标注 Send 边界
pub struct WithRelation<M: Model + ModelExt + RelationLoader + Send> { ... }
```

**严重性**：🔴 **红牌** — 公共 API 缺失 Send 显式保证，`WithRelation<M>` 虽当前自动满足 Send，但作为外部可见的返回类型，应显式绑定。

---

### 问题 2 — 🟡 黄牌：`DbError::ConstraintViolation` 未区分唯一约束冲突

**位置**：`error.rs:53`

```rust
/// 约束冲突
ConstraintViolation(String),
```

**风险**：唯一键冲突（`UNIQUE VIOLATION` / `Duplicate entry`）和外键约束冲突（`FOREIGN KEY constraint failed`）使用同一个变体。用户无法在不解析字符串的情况下区分这两种场景，导致：
- 业务层无法按需处理唯一冲突（如"用户名已存在"提示 vs "外键引用失败"错误）
- 代码审查中发现 `is_retryable()` 未将 `ConstraintViolation` 标记为可重试（正确），但具体子类型无法由调用方判断

**修复建议**（两种方案，推荐方案 1）：

**方案 1（枚举拆分 — 推荐）**：
```rust
pub enum DbError {
    // ... 其他变体不变
    UniqueViolation { constraint: String, value: String },
    ForeignKeyViolation { constraint: String, child_id: String },
    CheckViolation(String),
    // 移除通用的 ConstraintViolation 或保留为 fallback
    ConstraintViolation(String),
}
```

**方案 2（结构化变体 — 轻量）**：
```rust
ConstraintViolation {
    kind: ConstraintKind,
    message: String,
}
pub enum ConstraintKind {
    Unique,
    ForeignKey,
    Check,
    Other,
}
```

**严重性**：🟡 **黄牌** — 不影响编译或运行时安全，但降低 API 可用性，迫使用户解析字符串。

---

### 问题 3 — 🟡 黄牌：`Transaction` 未显式标注 Send + Sync

**位置**：`transaction.rs:116`

```rust
pub struct Transaction {
    conn: Arc<Mutex<Option<Box<dyn Connection>>>>,
    state: TransactionState,
    options: TransactOptions,
    savepoint_counter: u32,
}
```

**风险**：虽然当前 `Transaction` 因所有字段均为 `Send + Sync` 而自动满足 Send/Sync，但此乃**隐式保证**：

- 如果未来有人在内部添加了 `Rc<T>`、`RefCell<T>`、`*const T` 等非 Send/Sync 类型，`Transaction` 会静默丧失 Send/Sync 特征
- 调用方代码可能突然无法在 `tokio::spawn`、`Arc<Transaction>` 等场景下编译
- 作为核心公共 API 类型，应像 `Connection: Send + Sync` 一样明确承诺

**修复建议**：
```rust
// 添加隐式 impl（如果字段类型已保证）
// 或在 impl block 上方（最佳实践）：
// 无需额外代码，只需在 doc comment 中注明 Send/Sync 保证
// 推荐在 struct 定义前加 const 块作为编译期检查：
const _: () = {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<Transaction>();
    assert_sync::<Transaction>();
};
```

**严重性**：🟡 **黄牌** — 当前无实际编译问题，但未来重构存在退化风险。

---

### 问题 4 — 🟡 黄牌：`PooledConnection` 未显式标注 Send + Sync

**位置**：`pool.rs:126`

```rust
pub struct PooledConnection {
    conn: Box<dyn Connection>,  // Connection: Send + Sync → Box<dyn Connection> 为 Send + Sync
    created_at: Instant,
    last_used_at: Instant,
    pool: Option<Pool>,
}
```

**风险**：与 `Transaction` 同类的隐式 Send/Sync 依赖。

但更关键的：`PooledConnection` 实现了 `Deref<Target = dyn Connection>` 和 `DerefMut`，意味着用户可以直接在 `PooledConnection` 上调用 `Connection` 方法。`PooledConnection` 在 tokio runtime 中被频繁跨任务传递（`Drop::drop` 中 spawn 异步释放），必须保证 Send 稳定性。

**目前 PooledConnection 的 `Drop` 实现中有如下代码**：
```rust
if let Ok(handle) = tokio::runtime::Handle::try_current() {
    handle.spawn(async move {
        pool.release(pooled).await;
    });
}
```
其中 `pooled` 在闭包中 move，闭包被 `Send` 约束（`tokio::spawn` 要求 `Future: Send`）。这**要求** `PooledConnection: Send`。当前因自动 derive 满足，但如果未来添加了非 Send 字段，编译器会在此处报错，可能不会提示到 PooledConnection 自身的 Send 缺失。

**修复建议**：
```rust
// 在 impl PooledConnection 块或 module 级别添加编译期断言
const _: () = {
    fn assert_send<T: Send>() {}
    assert_send::<PooledConnection>();
};
```

**严重性**：🟡 **黄牌** — 当前自动满足，但缺少显式保证。

---

### 问题 5 — 🟡 黄牌：`DbError` 未实现 `Send + Sync` 显式验证

**位置**：`error.rs:11`

`DbError` 及其所有子类型（`PoolError`、`CacheError`、`TxError`）均使用 `String` 作为唯一负载字段，没有任何 `dyn Error` 内嵌。虽然当前自动满足 Send/Sync，但 `Error trait` 的 `source()` 方法返回 `Option<&(dyn Error + 'static)>`，如果未来某个变体需要内嵌原始数据库错误（如 `sqlx::Error`），需要确保该原始错误是 Send + Sync。

**当前现状**：`PoolError` 和 `CacheError` 实现了 `Error` trait 但未嵌入任何 `Box<dyn Error + Send + Sync>` 等类型的 source。这限制了错误链的追踪能力。

**修复建议**（可选增强）：
```rust
// 为关键变体添加 source 字段（仅在需要时）
ConnectionFailed { message: String, source: Option<Box<dyn Error + Send + Sync + 'static>> },
CommitFailed { message: String, source: Option<Box<dyn Error + Send + Sync + 'static>> },
```

**严重性**：🟡 **黄牌** — 当前不影响功能，但限制了错误溯源能力。

---

### 问题 6 — 🟢 提示：`Connection` 手动解糖 async 方法导致大量重复代码

**位置**：`pool.rs:27-116`

`Connection` trait 手动使用 `Pin<Box<dyn Future + Send + 'a>>` 而不是 `#[async_trait]`，虽然有充分的理由（避免 HRTB 与 sqlx::Executor 冲突），但：

- 每个方法约 5-8 行固定模板代码
- 实现者必须精确拼写 `Pin<Box<dyn Future<Output = Result<..., ...>> + Send + 'a>>`
- 如果方法签名变更，需要修改所有实现 + 所有默认实现 + `ClosedConnection` 实现

这是一个已知的设计取舍（已在 doc comment 说明），但值得记录。

**影响**：对 API 消费者（适配器实现者）的学习成本较高。

---

### 问题 7 — 🟢 提示：`execute_with_params` 等默认实现使用 `DbError::Internal` 而非专用错误变体

**位置**：`pool.rs:62`

```rust
fn execute_with_params<'a>(...) -> ... {
    Box::pin(async move {
        Err(crate::DbError::Internal(
            "execute_with_params not implemented for this adapter".to_string(),
        ))
    })
}
```

使用 `DbError::Internal` 表示"功能未实现"在语义上不准确。应使用更精确的变体，如 `DbError::Unsupported` 或新增 `NotImplemented` 变体。

**影响**：调用方无法区分是"真正的内部错误"还是"适配器未实现该功能"。

**修复建议**：
```rust
// 方案：新增 NotImplemented 变体（error_code: DB022）
NotImplemented(String),
// 或使用已有的 Unsupported
Err(crate::DbError::Unsupported(
    "execute_with_params not implemented for this adapter".to_string(),
))
```

---

### 问题 8 — 🟢 提示：`Transaction::execute` 和 `Transaction::query` 错误映射错误

**位置**：`transaction.rs:215, 232`

```rust
// execute()
.map_err(|e| TxError::CommitFailed(e.to_string()))?;
// query()
.map_err(|e| TxError::CommitFailed(e.to_string()))?;
```

`execute()` 和 `query()` 执行失败时映射到 `TxError::CommitFailed` 在语义上不准确。既不是 commit 操作，失败原因也不是 commit 失败。

**修复建议**：
```rust
// 新增 TxError 变体
TxError::ExecutionFailed(String),
// 或使用已存在的 SavepointError（虽也不精确）
.map_err(|e| TxError::ExecutionFailed(e.to_string()))?;
```

---

## 三、生命周期传染分析

### Connection trait 方法签名

```rust
fn execute<'a>(&'a mut self, sql: &'a str) -> Pin<Box<dyn Future<Output = ...> + Send + 'a>>;
```

所有方法使用**单一生命周期 `'a`** 同时约束 `&'a mut self` 和 `&'a str` 参数，返回的 `Future` 也绑定在 `'a` 上。

### 评估
| 维度 | 结果 |
|------|------|
| 是否返回引用/借用数据？ | **否** — 所有方法返回 `Owned` 数据（`u64`、`QueryRows`、`QueryValues`） |
| future 生命周期 | 绑定到 `&'a mut self`，合理（不能超过借用） |
| 生命周期参数数量 | 1 个，可接受 |
| 是否可能造成生命周期泄漏？ | **否** |
| 是否能在非 async 上下文使用？ | 可以（方法返回 Pin<Box<dyn Future>>，可在同步运行时中 block_on） |

**结论**：✅ 生命周期设计合理，无传染问题。

---

## 四、Panic 风险分析

### 有 panic 风险的代码

| 位置 | 代码 | 风险 | 评估 |
|------|------|------|------|
| pool.rs:184 | `std::mem::replace` | 无 | `ClosedConnection` 是栈上构造的，不会 panic |
| pool.rs:192-195 | `handle.spawn` | 如果 runtime 正在关闭，spawn 返回 Err | ✅ 已处理（`if let Ok(handle)` → silent drop） |
| pool.rs:574-582 | CAS 循环 `compare_exchange` | 无 panic | 标准用法 |
| pool.rs:586 | `self.factory.create()` | **如果 factory 实现 panic** | 非本 ORM 能控制的 |
| transaction.rs:184-185 | `conn_guard.as_mut().ok_or(...)` | 无 | 通过 `?` 转换为 Err |
| transaction.rs:458 | `handle.spawn` | 同理 ✅ | 已处理 |
| value.rs:116 | `HashMap` 构造 | 无 | |
| value.rs:184 | `s.to_lowercase()` | 仅 ASCII 字符，无 panic | |

### 公开方法是否保证不 panic？

核心约束：`pub` 方法在正常使用下不 panic。在 sz-orm-core 中：
- `Connection::*` 方法 — 返回 `Result`，不 panic ✅
- `Pool::acquire` — 返回 `Result`，不 panic ✅
- `Pool::release` — async fn 返回 `()`，但内部错误（close 失败）被 `let _ =` 忽略 ✅
- `Pool::reap_idle` — 同上 ✅
- `Pool::health_check` — 同上 ✅
- `Pool::close_all` — 同上 ✅
- `Transaction::*` — 全部返回 `Result` ✅

**结论**：✅ 公开方法全部通过 `Result` 或 `let _ =` 处理了错误路径，无未保护的 `unwrap()`/`expect()`。

---

## 五、API 易用性分析

### 泛型参数复杂度

| API | 泛型参数 | 复杂度 |
|-----|---------|--------|
| `Connection` trait | 无 | 低 ✅ |
| `Pool::new(config, factory)` | `Arc<dyn ConnectionFactory>` | 低 ✅ |
| `QueryBuilder<M>` | 1 个 `M: Model` | 低 ✅ |
| `Model` trait | `type PrimaryKey` 关联类型 | ✅ |
| `Transaction` | 无泛型 | 低 ✅ |
| `retry_on_deadlock` | `F: Fn(u32) -> Fut` | 中等 | 注意：闭包返回值必须明确标注 |

### 关联类型/GAT 使用

- `Model::PrimaryKey` — 关联类型，约束完整（Send + Sync + Debug + Display + Clone + Default）
- 无 GAT ✅
- 无 async fn in trait（手动解糖）✅

### 易用性问题

1. **高：`Connection` 实现成本** — 实现 `Connection` trait 需要为 11 个方法写 `Pin<Box<dyn Future + Send + 'a>>` 样板代码。官方提供了 `ClosedConnection` 作为参考实现，但适配器作者仍需要大量重复工作。

2. **中：`QueryBuilder<M>` 的泛型约束连锁** — 如果用户使用 `QueryBuilder<M>` 但 `M` 未实现 `Model` trait 的所有方法，编译器错误信息会包含大量涉及关联类型的消息，新手可能感到困惑。

3. **低：`Transaction` 的 `&mut self` 限制** — `Transaction` 所有操作方法都需要 `&mut self`，意味着不能在同一事务中并发执行多个查询（这是合理的 ACID 约束）。

---

## 六、"用户侧编译失败"场景及修复方案

### 场景 1：跨 tokio task 传递 Transaction

```rust
let tx = Transaction::new(conn, TransactOptions::default());
tokio::spawn(async move {
    tx.execute("SELECT 1").await.unwrap();  // ❌ Compile Error: `await` on non-Send future
}).await.unwrap();
```

**根因**：虽然 `Transaction` 当前是 `Send`，但如果未来内部添加了非 Send 字段，tokio::spawn 会报错。

**修复**（编译期保护）：
```rust
// 在 transaction.rs 模块中添加
const _: () = {
    fn assert_send<T: Send>() {}
    assert_send::<Transaction>();
};
```

---

### 场景 2：Model 未实现 Send，无法用于 QueryBuilder

```rust
struct User { name: Rc<String> }  // Rc 不是 Send
impl Model for User { type PrimaryKey = i64; ... }

let dialect = get_dialect(DbType::MySQL).unwrap();
let qb = QueryBuilder::<User>::new(dialect);  // ❌ Compile Error: User doesn't implement Send
```

**根因**：`Model: Send` 约束防止非 Send 类型被用作模型。

**修复**：
```rust
// 将 Rc<String> 改为 Arc<String>
// 或将字段改为 String（所有权转移）
struct User { name: String }  // ✅ String is Send
```

---

### 场景 3：Connection 实现者忘记手动解糖签名

```rust
struct MyConn;
#[async_trait]
impl Connection for MyConn {  // ❌ Compile Error: #[async_trait] 与手动解糖不兼容
    async fn execute(&mut self, sql: &str) -> Result<u64, DbError> {
        Ok(1)
    }
}
```

**根因**：`Connection` trait 使用手动 `Pin<Box<...>>` 解糖，不能使用 `#[async_trait]`。

**修复**：
```rust
impl Connection for MyConn {
    fn execute<'a>(&'a mut self, sql: &'a str)
        -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>>
    {
        Box::pin(async move { Ok(1) })
    }
    // ... 其他方法同理
}
```

---

### 场景 4：Transaction 在非 Active 状态下调用方法

```rust
let mut tx = Transaction::new(conn, TransactOptions::default());
tx.commit().await?;
tx.execute("SELECT 1").await?;  // ❌ 运行时 Err: Transaction not active (current state: Committed)
```

**根因**：`execute` 方法前置检查 `self.state != TransactionState::Active`。

**修复**：创建新事务执行额外操作：
```rust
// 不能复用已 commit 的事务
let mut tx2 = Transaction::new(new_conn, TransactOptions::default());
tx2.execute("SELECT 1").await?;
tx2.commit().await?;
```

---

### 场景 5：`ConnectionFactory` + `Connection` 的 Send bound 导致 trait object 约束问题

```rust
struct MyFactory;
impl ConnectionFactory for MyFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        Ok(Box::new(MyConn))
    }
}

let factory = MyFactory;
let pool = Pool::new(config, Arc::new(factory))?;
// 但如果 MyConn 或 MyFactory 不满足 Send+Sync，会在编译时报错
// "the trait `Send` is not implemented for `MyConn`"
// "the trait `Sync` is not implemented for `MyConn`"
```

**根因**：`Connection: Send + Sync` 和 `ConnectionFactory: Send + Sync` 导致所有实现必须满足。

**修复**：
```rust
struct MyConn { /* 所有字段必须 Send + Sync */ }

// 检查：如果可以传送到另一个线程，就满足 Send
fn assert_send_sync<T: Send + Sync>() {}
assert_send_sync::<MyConn>();  // 编译通过 = 满足
```

---

## 七、总结

### 严重性分布

| 严重性 | 数量 | 描述 |
|--------|------|------|
| 🔴 红牌 | 1 | `WithRelation<M>` 缺少 Send 显式约束 |
| 🟡 黄牌 | 4 | ConstraintViolation 粒度、Transaction/PooledConnection 隐式Send依赖、DbError 错误溯源 |
| 🟢 提示 | 3 | Connection 解糖模板代码、Internal 变体误用、Execute 错误映射不准确 |

### 总体评价

sz-orm-core 在 **Send/Sync**、**生命周期** 和 **Panic 安全** 三个维度上做得很好：
- 核心 trait（`Connection`、`ConnectionFactory`、`Model`、`ActiveRecord`）全部显式标注 `Send + Sync`
- 所有 async 方法返回 `Pin<Box<dyn Future + Send + 'a>>`，确保 future 可跨 await 点传递
- 公开方法全部使用 `Result` 返回，无未保护的 `unwrap()`/`expect()`
- 错误变体数量充分（DbError 21 变体、PoolError 8 变体、TxError 10 变体）

主要改进空间在 **错误粒度细化** 和 **结构化 Send/Sync 保证** 两个方面。当前代码质量较高，已接近生产就绪。
