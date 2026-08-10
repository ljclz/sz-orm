# Diesel → SZ-ORM 迁移指南

> 版本：v3.5.0
> 日期：2026-08-09
> 关联需求：REQ-DOC-FILL-002
> 关联任务：M6-T3.1
> 关联设计：design.md §5.1.6 M6-T6

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
let users: Vec<User> = User::query(&pool)
    .where_eq("id", Value::I32(1))
    .limit(10)
    .find_all()
    .await?;
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
let expr = UsersId.eq(1i64).and(UsersName.like("%Alice%"));
let users: Vec<User> = User::query(&pool)
    .where_expr(expr)
    .find_all()
    .await?;
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
let mut user = User::new();
user.set("name", "Alice");
user.set("email", Value::Null);
user.save(&pool).await?;
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
let mut user = User::find(&pool, 1).await?.unwrap();
user.set("name", "Bob");
user.save(&pool).await?;
```

### 2.6 DELETE

**Diesel:**
```rust
diesel::delete(users::table.filter(users::id.eq(1)))
    .execute(&conn)?;
```

**SZ-ORM:**
```rust
User::delete_by_id(&pool, 1).await?;
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
let users: Vec<User> = User::query(&pool)
    .with("posts")  // 预加载关联
    .find_all()
    .await?;
// 通过 user.posts() 访问关联
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
let mut tx = pool.begin().await?;
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
| `Decimal` | `f64` / `Value::F64` | SZ-ORM 用 f64 近似 |

### 3.3 方言差异

Diesel 默认使用 PostgreSQL 方言，SZ-ORM 支持 18 种方言：
- MySQL（反引号 `` ` `` 引用标识符）
- PostgreSQL（双引号 `"` 引用标识符）
- SQLite、Oracle、MSSQL、ClickHouse、DuckDB、DB2
- 国产信创（DM/Kingbase/PolarDB/GaussDB/GBase/Sybase）
- v3.5.0 新增：CockroachDB、YugabyteDB

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
- 支持 `Migrator::new()` + `migrator.run(&pool).await?`

---

## 4. 迁移步骤

1. **添加依赖**：`cargo add sz-orm-core sz-orm-macros tokio`
2. **选择方言**：`use sz_orm_core::DbType;` + `get_dialect(DbType::PostgreSQL)?`
3. **定义 Model**：将 `table!` 宏改为 `#[derive(Schema)]`
4. **替换查询**：将 `diesel::` 调用改为 `QueryBuilder<M>` 或 `Model::query()`
5. **异步化**：添加 `.await` + `#[tokio::main]`
6. **测试**：运行 `cargo test` 确保行为不变

---

## 5. 完整示例

```rust
use sz_orm_core::{DbType, Value, get_dialect};

#[derive(Schema, FromQueryResult)]
#[table(name = "users")]
struct User {
    #[column(primary_key)]
    id: i32,
    name: String,
    email: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = Pool::builder()
        .max_size(10)
        .build(&factory)
        .await?;

    // SELECT
    let users: Vec<User> = User::query(&pool)
        .where_eq("name", Value::String("Alice".into()))
        .limit(10)
        .find_all()
        .await?;

    // INSERT
    let mut user = User::new();
    user.set("name", "Bob");
    user.set("email", Value::Null);
    user.save(&pool).await?;

    Ok(())
}
```