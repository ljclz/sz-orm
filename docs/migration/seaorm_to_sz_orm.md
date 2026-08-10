# SeaORM → SZ-ORM 迁移指南

本指南帮助 SeaORM 用户迁移到 SZ-ORM。

## 1. 概念映射表

| SeaORM | SZ-ORM | 说明 |
|--------|--------|------|
| `Entity` trait | `Model` trait | 实体/模型定义 |
| `ActiveModel` | `Model` + `HashMap` 填充 | 主动模型 |
| `QueryFilter` | `QueryBuilder::where_eq` | 查询过滤 |
| `ColumnTrait` | `TypedColumnExt` | 列操作 |
| `Condition` | `WhereCondition` | 条件组合 |
| `Paginator` | `QueryBuilder::limit/offset` | 分页 |
| `Transaction` | `Transaction` | 事务 |
| `DatabaseConnection` | `Pool` | 连接管理 |
| `Schema` | `#[derive(Schema)]` | Schema 定义 |

## 2. API 对照表

### 2.1 实体定义

**SeaORM:**
```rust
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
struct Model {
    #[sea_orm(primary_key)]
    id: i32,
    name: String,
    email: Option<String>,
}
```

**SZ-ORM:**
```rust
#[derive(Schema, Debug, PartialEq, Clone)]
#[table(name = "users")]
struct User {
    #[column(primary_key)]
    id: i32,
    name: String,
    email: Option<String>,
}
```

### 2.2 查询

**SeaORM:**
```rust
let users: Vec<User> = User::find()
    .filter(user::Column::Id.eq(1))
    .all(&db)
    .await?;
```

**SZ-ORM:**
```rust
let sql = QueryBuilder::<User>::new(dialect)
    .where_eq("id", Value::I32(1))
    .build_select();
let rows = conn.query_all(&sql).await?;
```

### 2.3 ActiveModel → INSERT

**SeaORM:**
```rust
let user = user::ActiveModel {
    name: Set("Alice".to_string()),
    email: Set(None),
    ..Default::default()
};
user.insert(&db).await?;
```

**SZ-ORM:**
```rust
let mut data = HashMap::new();
data.insert("name".to_string(), Value::String("Alice".to_string()));
data.insert("email".to_string(), Value::Null);
let sql = QueryBuilder::<User>::new(dialect).build_insert(&data);
conn.execute(&sql).await?;
```

### 2.4 条件组合

**SeaORM:**
```rust
let users = User::find()
    .filter(Condition::all()
        .add(user::Column::Id.gt(0))
        .add(user::Column::Name.like("%Alice%")))
    .all(&db)
    .await?;
```

**SZ-ORM:**
```rust
let sql = QueryBuilder::<User>::new(dialect)
    .where_eq("id", Value::I32(1))
    .where_like("name", "%Alice%")
    .build_select();
```

### 2.5 分页

**SeaORM:**
```rust
let paginator = User::find()
    .filter(user::Column::Id.gt(0))
    .paginate(&db, 10);
let users = paginator.fetch_page(0).await?;
```

**SZ-ORM:**
```rust
let sql = QueryBuilder::<User>::new(dialect)
    .where_eq("id", Value::I32(1))
    .limit(10)
    .offset(0)
    .build_select();
```

## 3. 常见陷阱

### 3.1 异步运行时

SeaORM 和 SZ-ORM 都是异步的，但 SZ-ORM 基于 tokio，SeaORM 也支持 tokio。迁移时运行时通常不需要改变。

### 3.2 ActiveModel 差异

SeaORM 的 `ActiveModel` 使用 `Set` / `Unset` / `NotSet` 三态。SZ-ORM 使用 `HashMap<String, Value>`，未插入的键等同于 `Unset`。

### 3.3 关联查询

SeaORM 的 `Relation` + `find_also_related` 在 SZ-ORM 中需要手动编写 JOIN 查询或使用 `#[derive(Relation)]`。

### 3.4 迁移工具

SeaORM 使用 `sea_orm_migration`，SZ-ORM 使用内置 `Migrator`。迁移文件格式类似但 API 不同。