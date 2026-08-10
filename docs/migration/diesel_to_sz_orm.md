# Diesel → SZ-ORM 迁移指南

本指南帮助 Diesel 用户迁移到 SZ-ORM，包含概念映射表、API 对照表、示例代码和常见陷阱。

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

## 2. API 对照表

### 2.1 表定义

**Diesel:**
```rust
diesel::table! {
    users (id) {
        id -> Int4,
        name -> Varchar,
        email -> Nullable<Varchar>,
    }
}
```

**SZ-ORM:**
```rust
#[derive(Schema)]
#[table(name = "users")]
struct User {
    #[column(primary_key)]
    id: i32,
    name: String,
    email: Option<String>,
}
```

### 2.2 查询

**Diesel:**
```rust
let users = users::table
    .filter(users::id.eq(1))
    .limit(10)
    .load::<User>(&conn)?;
```

**SZ-ORM:**
```rust
let sql = QueryBuilder::<User>::new(dialect)
    .where_eq("id", Value::I32(1))
    .limit(10)
    .build_select();
let rows = conn.query_all(&sql).await?;
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
let q = QueryBuilder::<User>::new(dialect).where_expr(expr);
let sql = q.build_select();
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
let mut data = HashMap::new();
data.insert("name".to_string(), Value::String("Alice".to_string()));
data.insert("email".to_string(), Value::Null);
let sql = QueryBuilder::<User>::new(dialect).build_insert(&data);
conn.execute(&sql).await?;
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
let mut data = HashMap::new();
data.insert("name".to_string(), Value::String("Bob".to_string()));
let sql = QueryBuilder::<User>::new(dialect)
    .where_eq("id", Value::I64(1))
    .build_update(&data);
conn.execute(&sql).await?;
```

### 2.6 DELETE

**Diesel:**
```rust
diesel::delete(users::table.filter(users::id.eq(1)))
    .execute(&conn)?;
```

**SZ-ORM:**
```rust
let sql = QueryBuilder::<User>::new(dialect)
    .where_eq("id", Value::I64(1))
    .build_delete();
conn.execute(&sql).await?;
```

### 2.7 事务

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
conn.execute(&insert_sql).await?;
conn.execute(&update_sql).await?;
tx.commit().await?;
```

## 3. 常见陷阱

### 3.1 异步 vs 同步

Diesel 是同步的，SZ-ORM 是异步的（基于 tokio）。迁移时需要：
- 将 `fn` 改为 `async fn`
- 添加 `.await` 
- 使用 `tokio::main` 宏

### 3.2 类型映射差异

| Diesel | SZ-ORM | 注意 |
|--------|--------|------|
| `Int4` | `i32` / `Value::I32` | 一致 |
| `Int8` | `i64` / `Value::I64` | 一致 |
| `Varchar` | `String` / `Value::String` | 一致 |
| `Nullable<T>` | `Option<T>` / `Value::Null` | SZ-ORM 用 `Value::Null` 表示 NULL |
| `Timestamp` | `chrono::DateTime` | 需要 chrono feature |

### 3.3 方言差异

Diesel 默认使用 PostgreSQL 方言，SZ-ORM 支持五种方言：
- MySQL（反引号 `` ` `` 引用标识符）
- PostgreSQL（双引号 `"` 引用标识符）
- SQLite
- Oracle
- MSSQL

迁移时注意标识符引用方式不同。

### 3.4 编译时 SQL 验证

Diesel 的 `query!` 宏在编译时连数据库验证 SQL。SZ-ORM 提供 `sql_string!` 宏做语法验证，`query!` 宏（启用 `db-verify` feature）可连真 DB 验证。