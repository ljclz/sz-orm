# SeaORM → SZ-ORM 迁移指南

> 版本：v3.5.0
> 日期：2026-08-09
> 关联需求：REQ-DOC-FILL-003
> 关联任务：M6-T4.1
> 关联设计：design.md §5.1.6 M6-T7

> ⚠️ **版本适用性**：本指南示例基于 v3.5.0（2026-08-09）编写，SZ-ORM API 迭代较快；若示例与当前版本冲突，以 [packages/sz-orm-core/src](../packages/sz-orm-core/src/) 源码为准。真实执行链路见 [SQLx 指南 §3.8](migration-guide-sqlx.md#38-真实执行链路当前版本已验证)（三份迁移指南共用同一套执行 API）。

本指南帮助 SeaORM 用户迁移到 SZ-ORM，包含概念映射表、API 对照表、示例代码和常见陷阱。

---

## 1. 概念映射表

| SeaORM | SZ-ORM | 说明 |
|--------|--------|------|
| `Entity` trait | `Model` trait + `#[derive(Schema)]` | 实体定义 |
| `ActiveModel` | `ActiveModel` trait（可变实体） | 可变实体；注意：**不存在 `#[derive(ActiveModel)]` 派生宏** |
| `Column` enum | `#[column(...)]` 属性 | 列定义 |
| `PrimaryKey` enum | `#[column(primary_key)]` | 主键 |
| `Relation` enum | `#[derive(Relation)]` | 关联关系 |
| `EntityTrait` | `Model` + `ModelExt` | 实体方法 |
| `QuerySelect` | `QueryBuilder<M>` | SELECT 查询 |
| `QueryFilter` | `QueryBuilder::where_eq()` | WHERE 条件 |
| `QueryOrder` | `QueryBuilder::order_by()` | 排序 |
| `Paginator` | `QueryBuilder::page(page, per_page)` | 分页 |
| `Database` | `Pool` | 连接池 |
| `Transaction` | `Transaction` | 事务 |
| `Schema` | `Migration` / `Migrator` | 数据库迁移 |

---

## 2. API 对照表与示例

### 2.1 实体定义

**SeaORM:**
```rust
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    pub email: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
```

**SZ-ORM:**
```rust
#[derive(Clone, Debug, Schema, FromQueryResult)]
#[table(name = "users")]
pub struct User {
    #[column(primary_key)]
    pub id: i32,
    pub name: String,
    pub email: Option<String>,
}
```

### 2.2 SELECT 查询

**SeaORM:**
```rust
let users: Vec<user::Model> = User::find()
    .filter(user::Column::Name.eq("Alice"))
    .limit(10)
    .all(&db)
    .await?;
```

**SZ-ORM:**
```rust
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::{DbType, QueryBuilder, Value};

let dialect = get_dialect(DbType::PostgreSQL)?;
let (sql, params) = QueryBuilder::<User>::new(dialect)
    .table("users")
    .select(vec!["id", "name", "email"])
    .expect("valid columns")
    .where_eq("name", Value::String("Alice".into()))
    .limit(10)
    .build_select_with_params();
// 执行：conn.query_with_params(&sql, &params)?;  // conn 为连接池获取的连接
```

### 2.3 INSERT

**SeaORM:**
```rust
let user = user::ActiveModel {
    name: Set("Alice".to_string()),
    email: Set(None),
    ..Default::default()
};
let result = user.insert(&db).await?;
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

### 2.4 UPDATE

**SeaORM:**
```rust
let user: Option<user::Model> = User::find_by_id(1).one(&db).await?;
let mut user: user::ActiveModel = user.unwrap().into();
user.name = Set("Bob".to_string());
user.update(&db).await?;
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

### 2.5 DELETE

**SeaORM:**
```rust
user::Entity::delete_by_id(1).exec(&db).await?;
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
// 执行：conn.execute(&sql).await?;
```

### 2.6 关联查询

**SeaORM:**
```rust
let users_with_posts: Vec<(user::Model, Vec<post::Model>)> = User::find()
    .find_with_related(Post)
    .all(&db)
    .await?;
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
    .join_inner("posts", "users.id", "posts.user_id")
    .build_select_with_params();
// 执行：conn.query_with_params(&sql, &params).await?;  // 关联列在返回行中
```

### 2.7 分页

**SeaORM:**
```rust
let paginator = User::find().paginate(&db, 10);
let total = paginator.num_items().await?;
let users = paginator.fetch_page(0).await?;
```

**SZ-ORM:**
```rust
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::DbType;

let dialect = get_dialect(DbType::PostgreSQL)?;
let sql = QueryBuilder::<User>::new(dialect)
    .table("users")
    .select(vec!["id", "name"])
    .expect("valid columns")
    .page(1, 10) // page=1, limit=10（对应 SeaORM 的 paginate(&db, 10) + fetch_page(0)）
    .sql();
// 执行：conn.query_with_params(&sql, &[])?;
```

### 2.8 事务

**SeaORM:**
```rust
db.transaction::<_, (), DbErr>(|txn| {
    Box::pin(async move {
        user.insert(txn).await?;
        post.insert(txn).await?;
        Ok(())
    })
}).await?;
```

**SZ-ORM:**
```rust
// conn: 从连接池获取的连接（Connection trait 提供 begin_transaction）
let mut tx = conn.begin_transaction().await?;
tx.execute(&insert_user_sql).await?;
tx.execute(&insert_post_sql).await?;
tx.commit().await?;
```

---

## 3. 注意事项

### 3.1 异步运行时

SeaORM 和 SZ-ORM 都是基于 async，但：
- SeaORM 支持 tokio/async-std
- SZ-ORM 仅支持 tokio（更专注）

### 3.2 连接池

- SeaORM 使用 `sqlx::Pool` 或 `deadpool`
- SZ-ORM 使用自研无锁连接池（ArrayQueue + AtomicU32 + Notify）
- SZ-ORM 连接池内置断路器、限流器、自动预热

### 3.3 ActiveModel 差异

- SeaORM 的 `ActiveModel` 使用 `Set`/`Unset`/`NotSet` 三态
- SZ-ORM 的 `ActiveModel` 使用 `set()` 方法 + dirty 标记
- 迁移时将 `Set(value)` 改为 `model.set("field", value)`

### 3.4 方言支持

- SeaORM 支持 MySQL/PostgreSQL/SQLite（3 种）
- SZ-ORM 支持 28 种数据库方言（24 种 SQL + 4 种 NoSQL，含国产信创；以 `DbType` 枚举为准）

### 3.5 类型映射

| SeaORM | SZ-ORM | 注意 |
|--------|--------|------|
| `String` | `String` / `Value::String` | 一致 |
| `i32` | `i32` / `Value::I32` | 一致 |
| `Option<T>` | `Option<T>` / `Value::Null` | 一致 |
| `DateTime` | `chrono::DateTime` | 一致 |
| `Decimal` | `Value::Decimal(String)` | 以字符串承载小数，**避免 f64 精度丢失**；Rust 侧可用 `rust_decimal`/`bigdecimal` 解析 |

---

## 4. 迁移步骤

1. **添加依赖**：`cargo add sz-orm-core sz-orm-macros tokio`
2. **替换 Entity**：将 `DeriveEntityModel` 改为 `#[derive(Schema)]`
3. **替换查询**：将 `Entity::find()` 改为 `QueryBuilder<M>`（`.build_select_with_params()` 得到 `(sql, params)` 后执行）
4. **替换 ActiveModel**：将 `Set(value)` 改为 `model.set("field", value)`
5. **替换连接池**：将 `Database::connect()` 改为 `Pool::new(config, factory)?`（`PoolConfigBuilder` 构造配置 + `ConnectionFactory`）
6. **测试**：运行 `cargo test` 确保行为不变