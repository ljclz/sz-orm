# Diesel → SZ-ORM 迁移指南

> 版本：v3.5.0
> 日期：2026-08-09
> 关联需求：REQ-DOC-FILL-002
> 关联任务：M6-T3.1
> 关联设计：design.md §5.1.6 M6-T6

> ⚠️ **版本适用性**：本指南示例基于 v3.5.0（2026-08-09）编写，SZ-ORM API 迭代较快；若示例与当前版本冲突，以 [packages/sz-orm-core/src](../packages/sz-orm-core/src/) 源码为准。真实执行链路见 [SQLx 指南 §3.8](migration-guide-sqlx.md#38-真实执行链路当前版本已验证)（三份迁移指南共用同一套执行 API）。

本指南帮助 Diesel 用户迁移到 SZ-ORM，包含概念映射表、API 对照表、示例代码和常见陷阱。

---

## 1. 概念映射表

| Diesel | SZ-ORM | 说明 |
|--------|--------|------|
| `schema.rs`（`table!` 宏） | `#[derive(Schema)]` + `#[table(name = "...")]` | 表结构定义 |
| `QueryDsl` | `QueryBuilder<M>` | 查询构造器 |
| `BelongsTo` / `HasMany` | `#[derive(Relation)]` + `RelationTrait` | 关联关系 |
| `ExpressionMethods`（`.eq()`） | `TypedColumnExt`（`.eq()`） | 表达式方法 |
| `BoolExpressionMethods`（`.and()`） | `BoolExpressionExt`（`.and()`） | 逻辑组合 |
| `BoxedCondition` | `WhereCondition` | WHERE 条件 |
| `Insertable` | `QueryBuilder::build_insert` | INSERT |
| `AsChangeset` | `QueryBuilder::build_update` | UPDATE |
| `DeleteTarget` | `QueryBuilder::build_delete` | DELETE |
| `Selectable` | `#[derive(FromQueryResult)]` | 结果映射 |
| `Connection` | `Pool` / `Connection` | 连接管理 |
| `Transaction` | `Transaction` | 事务 |
| `Migration` | `Migration` / `Migrator` | 数据库迁移 |
| `RunQueryDsl`（`.execute()`/`.load()`） | `conn.execute()`/`conn.query()` | 执行查询 |

---

## 2. API 对照表与示例

### 2.1 表定义

**Diesel:**
```rust
diesel::table! {
    users (id) {
        id -> Int4,
        name -> Varchar,
        email -> Nullable<Varchar>,
        created_at -> Timestamp,
    }
}
```

**SZ-ORM:**
```rust
#[derive(Schema, FromQueryResult)]
#[table(name = "users")]
struct User {
    #[column(primary_key)]
    id: i32,
    name: String,
    email: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}
```

### 2.2 SELECT 查询

**Diesel:**
```rust
let users: Vec<User> = users::table
    .filter(users::id.eq(1))
    .limit(10)
    .load(&conn)?;
```

**SZ-ORM:**
```rust
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::{DbType, QueryBuilder, Value};

let dialect = get_dialect(DbType::PostgreSQL)?;
let (sql, params) = QueryBuilder::<User>::new(dialect)
    .table("users")
    .select(vec!["id", "name"])
    .expect("valid columns")
    .where_eq("id", Value::I32(1))
    .limit(10)
    .build_select_with_params();
// 执行：conn.query_with_params(&sql, &params).await?;
```

### 2.3 类型安全查询（启用 `type-safe-columns` feature）

**Diesel:**
```rust
let users = users::table
    .filter(users::id.eq(1).and(users::name.like("%Alice%")))
    .load::<User>(&conn)?;
```

**SZ-ORM:**
```rust
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::{DbType, QueryBuilder, Value};

let dialect = get_dialect(DbType::PostgreSQL)?;
let (sql, params) = QueryBuilder::<User>::new(dialect)
    .table("users")
    .select(vec!["id", "name"])
    .expect("valid columns")
    .where_expr(expr)
    .limit(10)
    .build_select_with_params();
// 执行：conn.query_with_params(&sql, &params).await?;
```

### 2.4 INSERT

**Diesel:**
```rust
diesel::insert_into(users::table)
    .values(&NewUser { name: "Alice".to_string(), email: None })
    .execute(&conn)?;
```

**SZ-ORM:**
```rust
use std::collections::HashMap;
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::{DbType, QueryBuilder, Value};

let dialect = get_dialect(DbType::PostgreSQL)?;
let mut data = HashMap::new();
data.insert("name".to_string(), Value::String("Alice".into()));
data.insert("email".to_string(), Value::Null);
let sql = QueryBuilder::<User>::new(dialect)
    .table("users")
    .sql_insert(&data);
// 执行：conn.execute(&sql).await?;
```

### 2.5 UPDATE

**Diesel:**
```rust
diesel::update(users::table.filter(users::id.eq(1)))
    .set(users::name.eq("Bob"))
    .execute(&conn)?;
```

**SZ-ORM:**
```rust
use std::collections::HashMap;
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::{DbType, QueryBuilder, Value};

let dialect = get_dialect(DbType::PostgreSQL)?;
let mut data = HashMap::new();
data.insert("name".to_string(), Value::String("Bob".into()));
let (sql, params) = QueryBuilder::<User>::new(dialect)
    .table("users")
    .where_eq("id", Value::I64(1))
    .build_update_with_params(&data);
// 执行：conn.query_with_params(&sql, &params).await?;
```

### 2.6 DELETE

**Diesel:**
```rust
diesel::delete(users::table.filter(users::id.eq(1)))
    .execute(&conn)?;
```

**SZ-ORM:**
```rust
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::{DbType, QueryBuilder, Value};

let dialect = get_dialect(DbType::PostgreSQL)?;
let (sql, params) = QueryBuilder::<User>::new(dialect)
    .table("users")
    .where_eq("id", Value::I64(1))
    .build_delete();
// 执行：conn.query_with_params(&sql, &params).await?;
```

### 2.7 关联查询

**Diesel:**
```rust
let users_with_posts: Vec<(User, Vec<Post>)> = users::table
    .left_join(posts::table.on(posts::user_id.eq(users::id)))
    .load::<(User, Vec<Post>)>(&conn)?;
```

**SZ-ORM:**
```rust
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::{DbType, QueryBuilder};

let dialect = get_dialect(DbType::PostgreSQL)?;
let (sql, params) = QueryBuilder::<User>::new(dialect)
    .table("users")
    .select(vec!["id", "name"])
    .expect("valid columns")
    .join_left("posts", "users.id", "posts.user_id")
    .build_select_with_params();
// 执行：conn.query_with_params(&sql, &params).await?;  // 关联列在返回行中
```

### 2.8 事务

**Diesel:**
```rust
conn.transaction(|conn| {
    diesel::insert_into(users::table).values(...).execute(conn)?;
    diesel::update(accounts::table).set(...).execute(conn)?;
    Ok(())
})?;
```

**SZ-ORM:**
```rust
// conn: 从连接池获取的连接（Connection trait 提供 begin_transaction）
let mut tx = conn.begin_transaction().await?;
tx.execute(&insert_sql).await?;
tx.execute(&update_sql).await?;
tx.commit().await?;
```

---

## 3. 注意事项

### 3.1 异步 vs 同步

Diesel 是同步的，SZ-ORM 是异步的（基于 tokio）。迁移时需要：
- 将 `fn` 改为 `async fn`
- 添加 `.await`
- 使用 `#[tokio::main]` 宏
- 将 `?` 错误处理保持不变（SZ-ORM 错误类型兼容）

### 3.2 类型映射差异

| Diesel | SZ-ORM | 注意 |
|--------|--------|------|
| `Int4` | `i32` / `Value::I32` | 一致 |
| `Int8` | `i64` / `Value::I64` | 一致 |
| `Varchar` | `String` / `Value::String` | 一致 |
| `Nullable<T>` | `Option<T>` / `Value::Null` | SZ-ORM 用 `Value::Null` 表示 NULL |
| `Timestamp` | `chrono::DateTime` | 需要 chrono feature |
| `Decimal` | `Value::Decimal(String)` | 以字符串承载小数，**避免 f64 精度丢失**；Rust 侧可用 `rust_decimal`/`bigdecimal` 解析 |

### 3.3 方言差异

Diesel 默认使用 PostgreSQL 方言，SZ-ORM 支持 28 种数据库方言（24 种 SQL + 4 种 NoSQL）：
- MySQL（反引号 `` ` `` 引用标识符）
- PostgreSQL（双引号 `"` 引用标识符）
- SQLite、Oracle、MSSQL、ClickHouse、DuckDB、DB2
- 国产信创（DM/Kingbase/PolarDB/GaussDB/GBase/Sybase）
- 云原生与新数据源：CockroachDB、YugabyteDB、Snowflake、Redshift、Informix、SAP HANA、Firebird
- 完整清单以 `DbType` 枚举（`packages/sz-orm-core/src/db_type.rs`）为准

迁移时注意标识符引用方式不同。

### 3.4 编译时 SQL 验证

Diesel 的 `query!` 宏在编译时连数据库验证 SQL。SZ-ORM 提供：
- `sql_string!` 宏：语法验证（不需数据库）
- `query!` 宏（启用 `db-verify` feature）：连真 DB 验证（类似 Diesel）

### 3.5 连接池

Diesel 使用 `r2d2` 连接池（同步），SZ-ORM 使用自研无锁连接池（async）：
- 基于 `ArrayQueue`（无锁 MPMC）+ `AtomicU32` + `Notify`
- 高并发吞吐量 ~3x（相比 Mutex 方案）
- 内置断路器、限流器、自动预热、统计监控

### 3.6 迁移工具

Diesel 使用 `diesel migration` CLI，SZ-ORM 使用 `Migrator`：
- SZ-ORM 迁移文件格式与 Diesel 类似（版本号 + up/down SQL）
- 迁移 API：`Migrator::new(MigrationContext::default()).add_migrations(vec![Migration::new("001", "name", up_sql, down_sql)])` 后调用 `migrator.migrate().await?`
- 目录迁移用 `FileMigrationResolver::new("./migrations".into()).resolve(DbType::MySQL)?` 加载 `<version>_<name>_up.sql` / `_down.sql`

---

## 4. 迁移步骤

1. **添加依赖**：`cargo add sz-orm-core sz-orm-macros tokio`
2. **选择方言**：`use sz_orm_core::DbType;` + `get_dialect(DbType::PostgreSQL)?`
3. **定义 Model**：将 `table!` 宏改为 `#[derive(Schema)]`
4. **替换查询**：将 `diesel::` 调用改为 `QueryBuilder<M>`（`.build_select_with_params()` 得到 `(sql, params)` 后执行）
5. **异步化**：添加 `.await` + `#[tokio::main]`
6. **测试**：运行 `cargo test` 确保行为不变

---

## 5. 完整示例（真实 API）

> 本示例基于当前仓库（v10.x）真实存在的 API，可直接放入 tokio 项目编译运行（数据库连接需按环境调整）。

```rust
use std::collections::HashMap;
use std::sync::Arc;

use sz_orm_core::dialect::get_dialect;
use sz_orm_core::{DbType, Model, Pool, PoolConfigBuilder, QueryBuilder, TimestampFields, Value};
use sz_orm_sqlx::{SqlitePoolHandle, SqlxSqliteConnectionFactory};

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct User {
    id: i64,
    name: String,
    email: Option<String>,
}

impl Model for User {
    type PrimaryKey = i64;
    fn table_name() -> &'static str { "users" }
    fn pk(&self) -> Self::PrimaryKey { self.id }
    fn set_pk(&mut self, pk: Self::PrimaryKey) { self.id = pk; }
    fn timestamp_fields() -> Option<TimestampFields> {
        Some(TimestampFields::with_both("created_at", "updated_at"))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 连接池（SQLite 内存库，免安装）
    let handle = SqlitePoolHandle::connect("sqlite::memory:").await?;
    let factory = Arc::new(SqlxSqliteConnectionFactory::new(Arc::new(handle)));
    let config = PoolConfigBuilder::new().max_size(10).build()?;
    let pool = Pool::new(config, factory)?;

    let mut conn = pool.acquire().await?;

    // 建表（示例用；迁移项目建议用迁移系统）
    conn.execute(
        "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, email TEXT)",
    )
    .await?;

    // SELECT WHERE + LIMIT
    let (sql, params) = QueryBuilder::<User>::new(get_dialect(DbType::Sqlite)?)
        .table("users")
        .select(vec!["id", "name", "email"])
        .expect("valid columns")
        .where_eq("name", Value::String("Alice".to_string()))
        .limit(10)
        .build_select_with_params();
    let rows = conn.query_with_params(&sql, &params).await?;
    println!("rows = {}", rows.len());

    // INSERT
    let mut data = HashMap::new();
    data.insert("name".to_string(), Value::String("Bob".to_string()));
    data.insert("email".to_string(), Value::Null);
    let insert_sql = QueryBuilder::<User>::new(get_dialect(DbType::Sqlite)?)
        .table("users")
        .sql_insert(&data);
    conn.execute(&insert_sql).await?;

    Ok(())
}
```