# SeaORM 到 SZ-ORM 迁移指南

> 版本：v1.0（2026-07-23）
> 适用：SeaORM 0.12+ → SZ-ORM 1.0+

## 目录

1. [架构差异概览](#1-架构差异概览)
2. [连接池初始化](#2-连接池初始化)
3. [Model 定义](#3-model-定义)
4. [查询 API](#4-查询-api)
5. [事务](#5-事务)
6. [关联关系](#6-关联关系)
7. [Migration 系统](#7-migration-系统)
8. [ActiveModel 替代方案](#8-activemodel-替代方案)
9. [常见陷阱](#9-常见陷阱)
10. [迁移检查清单](#10-迁移检查清单)

---

## 1. 架构差异概览

| 维度 | SeaORM | SZ-ORM |
|------|--------|--------|
| 设计风格 | Diesel + ActiveRecord | ThinkORM 风格，纯 SQL 生成器 |
| 执行模式 | `Entity::find().all(db).await` 一步执行 | `build_select()` 返回 String + 手动 acquire conn 执行 |
| 类型安全 | 编译期 `#[derive(DeriveEntityModel)]` | 手动实现 `Model` trait |
| ActiveModel | 有（支持部分字段更新） | 无，用 `HashMap<String, Value>` 替代 |
| 关联关系 | BelongsTo/HasMany/HasOne/BelongsToMany | 多出 MorphMany/MorphTo（多态） |
| 工作空间 | 单 crate | 39 个子 crate |
| 连接归还 | sqlx Pool 自动 Drop | PooledConnection 实现 Drop 自动归还（v1.0 修复） |

**核心差异**：SeaORM 是"一步执行"（find().all(db)），SZ-ORM 是"两步执行"（build SQL + acquire conn 执行）。

---

## 2. 连接池初始化

### SeaORM 写法

```rust
use sea_orm::*;

let opt = ConnectOptions::new("mysql://root:pass@localhost/db");
let db = Database::connect(opt).await?;
// db 可直接用于查询
```

### SZ-ORM 写法

```rust
use sz_orm_core::{Pool, PoolConfigBuilder};
use sz_orm_sqlx::{SqlxMySqlConnectionFactory};
use std::sync::Arc;

let config = PoolConfigBuilder::default()
    .max_size(10)
    .min_idle(1)
    .acquire_timeout(10)
    .build()?;

let factory = Arc::new(SqlxMySqlConnectionFactory::new(dsn));
let pool = Pool::new(config, factory)?;

// 查询需先 acquire connection
let mut conn = pool.acquire().await?;
let rows = conn.query("SELECT * FROM users LIMIT 1").await?;
// PooledConnection drop 时自动归还池（v1.0 已修复）
```

### 配置映射

| SeaORM | SZ-ORM |
|--------|--------|
| `.max_connections(10)` | `.max_size(10)` |
| `.min_connections(1)` | `.min_idle(1)` |
| `.acquire_timeout(Duration::from_secs(10))` | `.acquire_timeout(10)`（秒） |
| `.idle_timeout(...)` | `.idle_timeout(600)`（秒） |
| `.max_lifetime(...)` | `.max_lifetime(1800)`（秒） |

---

## 3. Model 定义

### SeaORM 写法

```rust
use sea_orm::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
    pub email: String,
    pub age: i32,
}
```

### SZ-ORM 写法

```rust
use sz_orm_core::{Model, ModelExt, Value, TimestampFields};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
struct User {
    id: i64,
    name: String,
    email: String,
    age: i32,
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

impl ModelExt for User {
    fn columns() -> Vec<&'static str> {
        vec!["id", "name", "email", "age"]
    }
    fn fillable() -> Vec<&'static str> {
        vec!["name", "email", "age"]
    }
    fn fill(&mut self, map: HashMap<String, Value>) {
        if let Some(Value::I64(v)) = map.get("id") { self.id = *v; }
        if let Some(Value::String(v)) = map.get("name") { self.name = v.clone(); }
        if let Some(Value::String(v)) = map.get("email") { self.email = v.clone(); }
        if let Some(Value::I32(v)) = map.get("age") { self.age = *v; }
    }
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "name": self.name,
            "email": self.email,
            "age": self.age,
        })
    }
}
```

### 迁移要点

- `#[derive(DeriveEntityModel)]` → 手动实现 `Model` + `ModelExt`
- `Column` 枚举 → `ModelExt::columns()` 返回 `Vec<&'static str>`
- 软删除 `#[sea_orm(soft_delete_column = "deleted_at")]` → 重写 `soft_delete_field()`

---

## 4. 查询 API

### SeaORM 写法

```rust
// SELECT * FROM users WHERE age > 18 ORDER BY id DESC LIMIT 10
let users: Vec<user::Model> = User::find()
    .filter(Column::Age.gt(18))
    .order_by_desc(Column::Id)
    .limit(10)
    .all(&db)
    .await?;
```

### SZ-ORM 写法

```rust
use sz_orm_core::{QueryBuilder, get_dialect, DbType};

let dialect = get_dialect(DbType::MySQL)?;
let sql = QueryBuilder::<User>::new(dialect)
    .table("users")
    .select_quoted(vec!["id", "name", "email", "age"])?
    .where_cond("age > 18")
    .order_desc("id")
    .limit(10)
    .build_select();

// 手动执行
let mut conn = pool.acquire().await?;
let rows = conn.query(&sql).await?;
// rows: Vec<HashMap<String, Value>>
```

### API 映射

| SeaORM | SZ-ORM |
|--------|--------|
| `Column::Name.eq("a")` | `.where_cond("name = 'a'")` |
| `Column::Age.gt(18)` | `.where_cond("age > 18")` |
| `.filter(Column::Id.is_in([1,2,3]))` | `.where_in("id", vec![Value::I64(1), ...])` |
| `.filter(Column::Age.is_between(18..65))` | `.where_between("age", Value::I64(18), Value::I64(65))` |
| `.filter(Column::Name.is_null())` | `.where_null("name")` |
| `.order_by_desc(Column::Id)` | `.order_desc("id")` |
| `.paginate(db, 20)` | `.page(3, 20)` + `build_select()` |
| `.left_join(RelatedEntity)` | `.join_left("orders", "orders.user_id", "users.id")` |

### ThinkORM 风格快捷 API（SZ-ORM 独有）

```rust
use sz_orm_core::Db;

// 无需定义 Model，直接查询
let sql = Db::new(get_dialect(DbType::MySQL)?)
    .name("users")
    .where_cond("age > 18")
    .order_desc("id")
    .limit(10)
    .build_select();
```

---

## 5. 事务

### SeaORM 写法

```rust
db.transaction::<_, _, sea_orm::DbErr>(|txn| {
    Box::pin(async move {
        txn.execute(Statement::from_sql_and_values(
            DatabaseBackend::MySql,
            "UPDATE accounts SET balance = balance - 100 WHERE id = 1",
            [],
        )).await?;
        txn.execute(Statement::from_sql_and_values(
            DatabaseBackend::MySql,
            "UPDATE accounts SET balance = balance + 100 WHERE id = 2",
            [],
        )).await?;
        Ok(())
    })
}).await?;
```

### SZ-ORM 写法

```rust
use sz_orm_core::transaction::{Transaction, TransactOptions, IsolationLevel};

let mut conn = pool.acquire().await?;
let conn = conn.into_inner(); // 消费 PooledConnection，移交给 Transaction
let mut tx = Transaction::new(conn, TransactOptions::default()
    .with_isolation(IsolationLevel::Serializable))?;
tx.execute("UPDATE accounts SET balance = balance - 100 WHERE id = 1").await?;
tx.execute("UPDATE accounts SET balance = balance + 100 WHERE id = 2").await?;
tx.commit().await?;
// 若 tx 未 commit 就 drop，Drop 会自动 spawn 后台 rollback
```

### 死锁重试（SZ-ORM 独有）

```rust
use sz_orm_core::transaction::retry_on_deadlock;
use std::time::Duration;

retry_on_deadlock(3, Duration::from_millis(100), || async {
    transfer_money(&pool, from, to, amount).await
}).await?;
```

---

## 6. 关联关系

### SeaORM 写法

```rust
// 定义关联
impl Related<order::Entity> for user::Entity {
    fn to() -> RelationDef {
        Relation::HasMany(order::Entity).into()
    }
}

// 查询用户及其订单
let users_with_orders: Vec<(user::Model, Vec<order::Model>)> =
    User::find()
        .find_with_related(Order)
        .all(&db)
        .await?;
```

### SZ-ORM 写法

```rust
use sz_orm_core::{Relation, HasMany};
use std::collections::HashMap;

// 注册关联
impl ModelExt for User {
    fn relations() -> HashMap<&'static str, Relation> {
        let mut map = HashMap::new();
        map.insert("orders", Relation::HasMany(HasMany {
            child_model: "orders".to_string(),
            foreign_key: "user_id".to_string(),
            child_pk: "id".to_string(),
        }));
        map
    }
}

// 查询用户及其订单（eager load 两条 SQL）
use sz_orm_core::find_with_related::WithRelation;

let loader = WithRelation::new(&*dialect, "users")
    .with_has_many("orders", "user_id", "id")
    .load_eager(Some("users.id IN (1, 2, 3)"));

let main_sql = loader.main_sql();        // SELECT * FROM users WHERE id IN (1,2,3)
let related_sql = loader.related_sql("orders").unwrap(); // SELECT * FROM orders WHERE user_id IN (1,2,3)
```

### 关联类型映射

| SeaORM | SZ-ORM |
|--------|--------|
| `Related<T>` + `BelongsTo` | `Relation::BelongsTo(BelongsTo)` |
| `Related<T>` + `HasMany` | `Relation::HasMany(HasMany)` |
| `Related<T>` + `HasOne` | `Relation::HasOne(HasOne)` |
| `Related::via()` 中间表 | `Relation::BelongsToMany(BelongsToMany)` |
| 无多态 | `Relation::MorphMany(MorphMany)` / `Relation::MorphTo(MorphTo)` |

---

## 7. Migration 系统

### SeaORM 写法（Rust 代码定义迁移）

```rust
use sea_orm_migration::prelude::*;

pub struct Migration;
impl MigrationName for Migration { fn name(&self) -> &str { "m20240101_000001_create_users" } }

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.create_table(
            Table::create()
                .table(Users::Table)
                .col(ColumnDef::new(Users::Id).integer().not_null().auto_increment().primary_key())
                .col(ColumnDef::new(Users::Name).string().not_null())
                .to_owned(),
        ).await
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(Users::Table).to_owned()).await
    }
}
```

### SZ-ORM 写法（SQL 文件定义迁移）

```
migrations/
├── 20240101000001_create_users_up.sql
├── 20240101000001_create_users_down.sql
```

`20240101000001_create_users_up.sql`:
```sql
CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL
);
```

`20240101000001_create_users_down.sql`:
```sql
DROP TABLE IF EXISTS users;
```

```rust
use sz_orm_core::migration::{Migrator, FileMigrationResolver, MigrationContext};
use std::path::PathBuf;

let resolver = FileMigrationResolver::new(PathBuf::from("migrations"));
let mut migrator = Migrator::new(ctx);
migrator.migrate().await?;                              // 执行所有 pending 迁移
migrator.rollback("20240101000001").await?;             // 回滚单个版本
migrator.reset().await?;                                 // 全部回滚
migrator.refresh().await?;                               // 全部回滚 + 全部执行
```

### 可选：SchemaBuilder（生成 DDL SQL）

```rust
use sz_orm_core::migration::{SchemaBuilder, ColumnDef, ColumnType};

let sql = SchemaBuilder::new("users")
    .add_column(ColumnDef {
        name: "id".to_string(),
        col_type: ColumnType::Integer,
        auto_increment: true,
        ..Default::default()
    })
    .add_column(ColumnDef {
        name: "name".to_string(),
        col_type: ColumnType::String(255),
        nullable: false,
        ..Default::default()
    })
    .build(DbType::MySQL);
```

---

## 8. ActiveModel 替代方案

**SZ-ORM 无 ActiveModel 等价物**。以下是替代方案：

### 方案 1：HashMap + QueryBuilder（推荐）

```rust
use std::collections::HashMap;
use sz_orm_core::Value;

// 部分字段更新
let mut updates = HashMap::new();
updates.insert("name".to_string(), Value::String("new_name".into()));
updates.insert("updated_at".to_string(), Value::String(now_iso8601()));

let sql = QueryBuilder::<User>::new(get_dialect(DbType::MySQL)?)
    .table("users")
    .where_cond("id = 1")
    .build_update(&updates);

let mut conn = pool.acquire().await?;
conn.execute(&sql).await?;
```

### 方案 2：Repository 模式（适合复杂业务）

```rust
use sz_orm_core::repository::{Repository, InMemoryRepository};

// 测试用 InMemoryRepository，生产用自实现 SqlRepository
let repo = InMemoryRepository::<User>::new();
repo.save(user)?;
let found = repo.find_by_id(&user.id)?;
```

---

## 9. 常见陷阱

### 9.1 执行模型不同（最重要）

**SeaORM**：一步执行
```rust
let users = User::find().all(&db).await?; // 直接执行
```

**SZ-ORM**：两步执行
```rust
let sql = QueryBuilder::<User>::new(dialect).table("users").build_select(); // 1. 生成 SQL
let mut conn = pool.acquire().await?;                                       // 2a. 获取连接
let rows = conn.query(&sql).await?;                                         // 2b. 执行查询
```

### 9.2 类型安全降级

SeaORM 的 `Column::Name.eq("a")` 是编译期类型安全的；SZ-ORM 的 `.where_cond("name = 'a'")` 是裸 SQL 字符串，**有 SQL 注入风险**。

**缓解措施**：
- 使用 `select_quoted` 替代 `select` 进行标识符校验
- 用户输入用 `Value::to_param_with_dialect()` 转义后拼接
- 或使用 `check_where_injection()` 黑名单检测

### 9.3 关联加载从一步变两步

**SeaORM**：
```rust
let users_with_orders = User::find().find_with_related(Order).all(&db).await?;
```

**SZ-ORM**：
```rust
// 1. 执行 main_sql 获取主记录
let main_sql = loader.main_sql();
let main_rows = conn.query(&main_sql).await?;

// 2. 提取主键 ID
let ids: Vec<i64> = main_rows.iter().filter_map(|r| r.get("id").and_then(|v| v.as_i64())).collect();

// 3. 执行 related_sql 获取关联记录
let related_sql = loader.related_sql_with_ids("orders", &ids)?;
let related_rows = conn.query(&related_sql).await?;

// 4. 手动组装结果
```

### 9.4 迁移用 SQL 文件而非 Rust 代码

失去编译期类型检查。迁移文件需配对 `_up.sql` 和 `_down.sql`。

### 9.5 Connection trait 手动解糖 async

SZ-ORM 的 `Connection` trait 手动解糖 async 方法（避免 HRTB 与 sqlx::Executor 冲突）。自定义实现需注意生命周期绑定——所有 async 方法的 `'a` 生命周期绑定 `&'a mut self` 和 `&'a str`。

---

## 10. 迁移检查清单

- [ ] **连接池**：`Database::connect` → `Pool::new(config, factory)`
- [ ] **Model 定义**：`#[derive(DeriveEntityModel)]` → 手动实现 `Model` + `ModelExt`
- [ ] **查询**：`Entity::find().all(&db)` → `QueryBuilder::build_select()` + `conn.query()`
- [ ] **过滤条件**：`Column::Name.eq(...)` → `.where_cond("name = ...")`（注意注入风险）
- [ ] **事务**：闭包式 → 命令式 `Transaction::new` + `commit/rollback`
- [ ] **关联**：`Related<T>` trait → `ModelExt::relations()` HashMap
- [ ] **Migration**：Rust 代码 → SQL 文件（`_up.sql` / `_down.sql`）
- [ ] **ActiveModel**：改为 `HashMap<String, Value>` + `build_update()`
- [ ] **分页**：`.paginate(db, 20)` → `.page(3, 20)` + `build_select()`
- [ ] **软删除**：`#[sea_orm(soft_delete_column)]` → `Model::soft_delete_field()`
- [ ] **时间戳**：`ActiveModelBehavior::before_save` → `Model::timestamp_fields()`

---

## 附录：SZ-ORM 独有特性

SeaORM 用户迁移到 SZ-ORM 后可获得的新能力：

1. **多态关联**（MorphMany / MorphTo）—— SeaORM 需手动实现
2. **死锁重试**（`retry_on_deadlock`）—— SeaORM 需自实现
3. **事务管理器**（`TransactionManager`）—— 集中管理多事务状态
4. **ThinkORM 风格 API**（`Db::name("users")`）—— 无需定义 Model 即可查询
5. **16 种 SQL 方言**（含 6 种国产数据库：达梦/人大金仓/OceanBase/PolarDB/GaussDB/GBase）
6. **27 个业务扩展包**（crypto/JWT/scheduler/storage/AI/gRPC/GraphQL/ES/...）
7. **24h soak test** CI —— 自动检测内存泄漏/性能退化
8. **PooledConnection Drop 自动归还** —— v1.0 修复，连接 drop 时自动归还池

---

## 参考

- [SZ-ORM 使用指南](sz-orm使用指南.md)
- [SZ-ORM API 参考](sz-ormAPI参考.md)
- [SZ-ORM 架构设计](sz-orm架构设计.md)
- [ORM 对比分析](sz-orm与同类产品对比分析.md)
- [SeaORM 官方文档](https://www.sea-ql.org/SeaORM/)
