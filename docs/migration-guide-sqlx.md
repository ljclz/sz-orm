# SQLx → SZ-ORM 迁移指南

> 版本：v3.5.0
> 日期：2026-08-09
> 关联需求：REQ-DOC-FILL-004
> 关联任务：M6-T5.1
> 关联设计：design.md §5.1.6 M6-T8

本指南帮助 SQLx 用户迁移到 SZ-ORM，包含概念映射表、API 对照表、示例代码和常见陷阱。

---

## 1. 概念映射表

| SQLx | SZ-ORM | 说明 |
|------|--------|------|
| `sqlx::query()` | `conn.execute()` | 原始 SQL 执行 |
| `sqlx::query!()` | `query!` 宏（db-verify feature） | 编译时验证查询 |
| `sqlx::query_as!()` | `#[derive(FromQueryResult)]` + `conn.query()` | 结果映射 |
| `sqlx::Pool` | `Pool` | 连接池 |
| `sqlx::Transaction` | `Transaction` | 事务 |
| `sqlx::FromRow` | `#[derive(FromQueryResult)]` | 行到结构体映射 |
| `sqlx::Migrate` | `Migrator` | 数据库迁移 |
| `sqlx::Any` | `DbType` + `get_dialect()` | 多数据库支持 |

---

## 2. API 对照表与示例

### 2.1 原始 SQL 查询

**SQLx:**
```rust
let users = sqlx::query("SELECT id, name FROM users WHERE id = $1")
    .bind(1)
    .fetch_all(&pool)
    .await?;
```

**SZ-ORM:**
```rust
let sql = "SELECT id, name FROM users WHERE id = ?";
let rows = conn.query_with_params(sql, &[Value::I32(1)]).await?;
```

### 2.2 编译时验证查询

**SQLx:**
```rust
let users = sqlx::query!("SELECT id, name FROM users WHERE id = $1", 1)
    .fetch_all(&pool)
    .await?;
```

**SZ-ORM（启用 `db-verify` feature）:**
```rust
let users = sz_orm_core::query!("SELECT id, name FROM users WHERE id = ?", 1)
    .fetch_all(&pool)
    .await?;
```

### 2.3 结果映射

**SQLx:**
```rust
#[derive(FromRow)]
struct User {
    id: i32,
    name: String,
}

let users = sqlx::query_as::<_, User>("SELECT id, name FROM users")
    .fetch_all(&pool)
    .await?;
```

**SZ-ORM:**
```rust
#[derive(FromQueryResult)]
struct User {
    id: i32,
    name: String,
}

let rows = conn.query("SELECT id, name FROM users").await?;
let users: Vec<User> = User::from_rows(&rows)?;
```

### 2.4 INSERT

**SQLx:**
```rust
sqlx::query("INSERT INTO users (name, email) VALUES ($1, $2)")
    .bind("Alice")
    .bind(None::<String>)
    .execute(&pool)
    .await?;
```

**SZ-ORM:**
```rust
let mut data = HashMap::new();
data.insert("name", Value::String("Alice".into()));
data.insert("email", Value::Null);
let sql = QueryBuilder::<User>::new(dialect).build_insert(&data);
conn.execute(&sql).await?;
```

### 2.5 UPDATE

**SQLx:**
```rust
sqlx::query("UPDATE users SET name = $1 WHERE id = $2")
    .bind("Bob")
    .bind(1)
    .execute(&pool)
    .await?;
```

**SZ-ORM:**
```rust
let mut data = HashMap::new();
data.insert("name", Value::String("Bob".into()));
let sql = QueryBuilder::<User>::new(dialect)
    .where_eq("id", Value::I32(1))
    .build_update(&data);
conn.execute(&sql).await?;
```

### 2.6 事务

**SQLx:**
```rust
let mut tx = pool.begin().await?;
sqlx::query("INSERT INTO users (name) VALUES ($1)")
    .bind("Alice")
    .execute(&mut *tx)
    .await?;
tx.commit().await?;
```

**SZ-ORM:**
```rust
let mut tx = pool.begin().await?;
tx.execute("INSERT INTO users (name) VALUES (?)").await?;
tx.commit().await?;
```

### 2.7 迁移

**SQLx:**
```rust
sqlx::migrate!("./migrations").run(&pool).await?;
```

**SZ-ORM:**
```rust
let mut migrator = Migrator::new();
migrator.add_migration("001_create_users", "CREATE TABLE users (...)", "");
migrator.run(&pool).await?;
```

---

## 3. 注意事项

### 3.1 参数占位符差异

| 数据库 | SQLx | SZ-ORM |
|--------|------|--------|
| PostgreSQL | `$1, $2, $3` | `?, ?, ?` |
| MySQL | `?` | `?` |
| SQLite | `?` 或 `$1` | `?` |

SZ-ORM 统一使用 `?` 占位符，方言层自动转换（PG 方言将 `?` 转为 `$1`）。

### 3.2 连接池

- SQLx 使用自研连接池（`Mutex` + `Semaphore`）
- SZ-ORM 使用无锁连接池（`ArrayQueue` + `AtomicU32` + `Notify`）
- SZ-ORM 连接池高并发吞吐量 ~3x

### 3.3 编译时验证

- SQLx 的 `query!` 宏需要 `DATABASE_URL` 环境变量
- SZ-ORM 的 `query!` 宏（`db-verify` feature）也需要 `DATABASE_URL`
- 两者编译时验证机制类似

### 3.4 方言支持

- SQLx 支持 MySQL/PostgreSQL/SQLite/MSSQL/Oracle（5 种）
- SZ-ORM 支持 18 种方言（含国产信创 + v3.5.0 新增 CockroachDB/YugabyteDB）

### 3.5 类型映射

| SQLx | SZ-ORM | 注意 |
|------|--------|------|
| `i32` | `i32` / `Value::I32` | 一致 |
| `i64` | `i64` / `Value::I64` | 一致 |
| `String` | `String` / `Value::String` | 一致 |
| `Option<T>` | `Option<T>` / `Value::Null` | 一致 |
| `DateTime` | `chrono::DateTime` | 一致 |
| `Vec<u8>` | `Vec<u8>` / `Value::Bytes` | 一致 |

### 3.6 N+1 检测

- SQLx 无 N+1 检测
- SZ-ORM 内置 N+1 检测（`N1QueryDetector`），自动拦截
- 迁移后可移除手动的 N+1 防护代码

### 3.7 SQL 注入防护

- SQLx 通过参数绑定防护
- SZ-ORM 强制参数化（`where_eq`/`or_where_eq`），`where_cond`/`or_where` 已 deprecated
- 迁移时将字符串拼接的 WHERE 改为参数化方法

---

## 4. 迁移步骤

1. **添加依赖**：`cargo add sz-orm-core sz-orm-macros tokio`
2. **选择方言**：`let dialect = get_dialect(DbType::PostgreSQL)?;`
3. **替换查询**：将 `sqlx::query()` 改为 `conn.execute()`/`conn.query()`
4. **替换结果映射**：将 `FromRow` 改为 `#[derive(FromQueryResult)]`
5. **参数化**：将 `$1`/`$2` 改为 `?`（SZ-ORM 自动转换）
6. **替换连接池**：将 `sqlx::Pool` 改为 `Pool`
7. **测试**：运行 `cargo test` 确保行为不变

---

## 5. 混合使用（渐进迁移）

SQLx 和 SZ-ORM 可以混合使用，渐进迁移：

```rust
// 仍用 SQLx 执行原始 SQL
let rows = sqlx::query("SELECT * FROM users").fetch_all(&sqlx_pool).await?;

// 用 SZ-ORM 构造查询
let sql = QueryBuilder::<User>::new(dialect)
    .where_eq("status", Value::String("active".into()))
    .build_select();

// 用 SQLx 执行 SZ-ORM 构造的 SQL
let rows = sqlx::query(&sql).fetch_all(&sqlx_pool).await?;
```

这种方式允许逐步迁移，无需一次性替换所有代码。