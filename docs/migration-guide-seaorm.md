# SeaORM → SZ-ORM 迁移指南

> 版本：v3.5.0
> 日期：2026-08-09
> 关联需求：REQ-DOC-FILL-003
> 关联任务：M6-T4.1
> 关联设计：design.md §5.1.6 M6-T7

本指南帮助 SeaORM 用户迁移到 SZ-ORM，包含概念映射表、API 对照表、示例代码和常见陷阱。

---

## 1. 概念映射表

| SeaORM | SZ-ORM | 说明 |
|--------|--------|------|
| `Entity` trait | `Model` trait + `#[derive(Schema)]` | 实体定义 |
| `ActiveModel` | `ActiveModel` trait + `#[derive(ActiveModel)]` | 可变实体 |
| `Column` enum | `#[column(...)]` 属性 | 列定义 |
| `PrimaryKey` enum | `#[column(primary_key)]` | 主键 |
| `Relation` enum | `#[derive(Relation)]` | 关联关系 |
| `EntityTrait` | `Model` + `ModelExt` | 实体方法 |
| `QuerySelect` | `QueryBuilder<M>` | SELECT 查询 |
| `QueryFilter` | `QueryBuilder::where_eq()` | WHERE 条件 |
| `QueryOrder` | `QueryBuilder::order_by()` | 排序 |
| `Paginator` | `QueryBuilder::paginate()` | 分页 |
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
let users: Vec<User> = User::query(&pool)
    .where_eq("name", Value::String("Alice".into()))
    .limit(10)
    .find_all()
    .await?;
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
let mut user = User::new();
user.set("name", "Alice");
user.set("email", Value::Null);
user.save(&pool).await?;
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
let mut user = User::find_by_id(&pool, 1).await?.unwrap();
user.set("name", "Bob");
user.save(&pool).await?;
```

### 2.5 DELETE

**SeaORM:**
```rust
user::Entity::delete_by_id(1).exec(&db).await?;
```

**SZ-ORM:**
```rust
User::delete_by_id(&pool, 1).await?;
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
let users: Vec<User> = User::query(&pool)
    .with("posts")  // 预加载关联
    .find_all()
    .await?;
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
let users: Vec<User> = User::query(&pool)
    .paginate(1, 10)  // page=1, limit=10
    .find_all()
    .await?;
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
let mut tx = pool.begin().await?;
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
- SZ-ORM 支持 18 种方言（含国产信创 + v3.5.0 新增 CockroachDB/YugabyteDB）

### 3.5 类型映射

| SeaORM | SZ-ORM | 注意 |
|--------|--------|------|
| `String` | `String` / `Value::String` | 一致 |
| `i32` | `i32` / `Value::I32` | 一致 |
| `Option<T>` | `Option<T>` / `Value::Null` | 一致 |
| `DateTime` | `chrono::DateTime` | 一致 |
| `Decimal` | `f64` | SZ-ORM 用 f64 近似 |

---

## 4. 迁移步骤

1. **添加依赖**：`cargo add sz-orm-core sz-orm-macros tokio`
2. **替换 Entity**：将 `DeriveEntityModel` 改为 `#[derive(Schema)]`
3. **替换查询**：将 `Entity::find()` 改为 `Model::query(&pool)`
4. **替换 ActiveModel**：将 `Set(value)` 改为 `model.set("field", value)`
5. **替换连接池**：将 `Database::connect()` 改为 `Pool::builder().build()`
6. **测试**：运行 `cargo test` 确保行为不变