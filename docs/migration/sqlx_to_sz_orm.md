# SQLx → SZ-ORM 迁移指南

本指南帮助 SQLx 用户迁移到 SZ-ORM。

## 1. 概念映射表

| SQLx | SZ-ORM | 说明 |
|------|--------|------|
| `query!` 宏 | `sql_string!` 宏 / `query!` 宏 | 编译时 SQL 验证 |
| `query_as!` 宏 | `query_as!` 宏 / `#[derive(FromQueryResult)]` | 类型化查询 |
| `FromRow` trait | `#[derive(FromQueryResult)]` | 行反序列化 |
| `PgPool` / `MySqlPool` | `Pool` | 连接池 |
| `Transaction` | `Transaction` | 事务 |
| `Executor` | `Connection` | 执行器 |
| `migrate!` 宏 | `Migrator` | 数据库迁移 |

## 2. API 对照表

### 2.1 基本查询

**SQLx:**
```rust
let users = sqlx::query_as!(User, "SELECT id, name FROM users WHERE id = $1", 1)
    .fetch_all(&pool)
    .await?;
```

**SZ-ORM:**
```rust
let sql = QueryBuilder::<User>::new(dialect)
    .where_eq("id", Value::I64(1))
    .build_select();
let rows = conn.query_all(&sql).await?;
```

### 2.2 编译时验证

**SQLx:**
```rust
// 编译时连 DB 验证 SQL
let user = sqlx::query_as!(User, "SELECT id, name FROM users WHERE id = $1", id)
    .fetch_one(&pool)
    .await?;
```

**SZ-ORM:**
```rust
// 方式1：sql_string! 宏（语法验证）
let sql = sql_string!("SELECT id, name FROM users WHERE id = ?");

// 方式2：query! 宏（连真 DB 验证，需启用 db-verify feature）
let q = query!("SELECT id, name FROM users WHERE id = ?", id);
```

### 2.3 FromRow → FromQueryResult

**SQLx:**
```rust
#[derive(FromRow)]
struct User {
    id: i64,
    name: String,
    email: Option<String>,
}
```

**SZ-ORM:**
```rust
#[derive(FromQueryResult)]
struct User {
    id: i64,
    name: String,
    email: Option<String>,
}
```

### 2.4 INSERT

**SQLx:**
```rust
sqlx::query!("INSERT INTO users (name, email) VALUES ($1, $2)", name, email)
    .execute(&pool)
    .await?;
```

**SZ-ORM:**
```rust
let mut data = HashMap::new();
data.insert("name".to_string(), Value::String(name));
data.insert("email".to_string(), Value::String(email));
let sql = QueryBuilder::<User>::new(dialect).build_insert(&data);
conn.execute(&sql).await?;
```

### 2.5 事务

**SQLx:**
```rust
let mut tx = pool.begin().await?;
sqlx::query!("INSERT INTO ...").execute(&mut *tx).await?;
sqlx::query!("UPDATE ...").execute(&mut *tx).await?;
tx.commit().await?;
```

**SZ-ORM:**
```rust
let mut tx = pool.begin().await?;
conn.execute(&insert_sql).await?;
conn.execute(&update_sql).await?;
tx.commit().await?;
```

### 2.6 连接池

**SQLx:**
```rust
let pool = PgPoolOptions::new()
    .max_connections(10)
    .connect("postgres://user:pass@localhost/db")
    .await?;
```

**SZ-ORM:**
```rust
let pool = Pool::builder()
    .max_size(10)
    .build("postgres://.//?user=user&password=pass")
    .await?;
```

## 3. 常见陷阱

### 3.1 参数占位符

SQLx PostgreSQL 使用 `$1, $2, ...`，SZ-ORM 统一使用 `?` 占位符（方言层自动转换）。

### 3.2 编译时验证差异

SQLx 的 `query!` 宏**必须**连数据库验证（或使用离线模式 `sqlx prepare`）。SZ-ORM 的 `sql_string!` 仅做语法验证，`query!` 宏在启用 `4 `db-verify` feature + `SZ_ORM_QUERY_VERIFY=1` 时才连真 DB。

### 3.3 类型映射差异

| SQLx | SZ-ORM | 注意 |
|------|--------|------|
| `i32` | `Value::I32` | 一致 |
| `i64` | `Value::I64` | 一致 |
| `String` | `ValueBValue::String` | 一致 |
| `Option<T>` | `Value::Null` 或 `Value::T` | SZ-ORM 用 enum 变体 |
| `Vec<u8>`8>` | `Value::Bytes` | 一致 |
| `chrono::DateTime` | `Value::DateTime` | 需要 chrono feature |

### 3.4 迁移工具

SQLx 使用 `sqlx::migrate!` 宏从 `migrations/` 目录加载。SZ-ORM 使用 `Migrator` 从 `migrations/` 目录加载，格式类似但 API 不同。

### 3.5 流式查询

SQLx 的 `fetch()` 返回 `Stream`，SZ-ORM 的 `query_stream()` 也返回 `Stream`，接口类似。