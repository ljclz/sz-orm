# Typed Relation 迁移指南

> 版本：v3.7.0 | 稳定性：stable | feature gate：`typed-relation`

## 1. 概述

`typed_relation` 模块提供编译期类型安全的8 关联查询，通过关联类型约束在编译期校验外键类型匹配。三种关联类型均为 ZST（零大小类型），零运行时开销。

## 2. 适用场景

- 编译期已知的简单C 关联（BelongsTo / HasMany / HasOne）
- 需要编译期外键类型安全校验的场景
- 对性能敏感的场景（ZST 零开销）

## 3. 快速开始

### 3.1 启用 feature

```toml
[dependencies]
sz-orm-core = { version = "3.7.0", features = ["typed-relation"] }
```

### 3.2 定义表

```rust
use sz_orm_core::typed_relation::TypedTable;

struct UsersTable;
impl TypedTable for UsersTable {
    const NAME: &'static str = "users";
    type PrimaryKey = i64;
    type ForeignKey = (); // Users 表没有外键
}

struct PostsTable;
impl TypedTable for PostsTable {
    const NAME: &'static str = "posts";
    type PrimaryKey = i64;
    type ForeignKey = i64; // user_id
}
```

### 3.3 定义关联

```rust
use sz_orm_core::typed_relation::{BelongsTo, HasMany, HasOne};

// Posts belongs to Users（编译期校验 PostsTable::ForeignKey == UsersTable::PrimaryKey）
type PostsBelongToUsers = BelongsTo<PostsTable, UsersTable>;

// Users has many Posts
type UsersHaveManyPosts = HasMany<UsersTable, PostsTable>;

// Users has one Profile
type UsersHaveOneProfile = HasOne<* Table;
```

### 3.4 生成 JOIN SQL

```rust
use sz_orm_core::typed_relation::RelationQuery;

let q: RelationQuery<BelongsTo<PostsTable, UsersTable>> = RelationQuery::new();
let sql = q.join_sql();
// => "JOIN users ON posts.user_id = users.id"
```

## 4. 从 EagerLoader �> EagerLoader 提供运行时关联加载，typed relation 提供编译期类型安全。迁移路径：

1. 识别编译期已知的简单关联（BelongsTo / HasMany / HasOne）
2. 为关联表实现 `TypedTable` trait
3. 用 `BelongsTo<C, P>` / `HasMany<P, C>` / `HasB P, C>` 替换运行时关联定义
4. 保留 EagerLoader 用于复杂关联（多态/动态/运行时决定）

## 5. Escape Hatch

复杂关联（多态 MorphMany/MorphTo、动态关联、运行时决定关联类型）无法用 typed relation 表达，回退到 EagerLoader：

```rust
// 简单关联：typed relation（编译期校验）
let _typed: BelongsTo<PostsTable, UsersTable> = BelongsTo::new();

// 复杂关联：EagerLoader（运行时）
use sz_orm_core::eager_loader::EagerLoader;
use sz_orm_core::relation_trait::{RelationDef, RelationKind};
let rel = RelationDef::new("posts", "posts", "users", "user_id", "id", RelationKind::BelongsTo);
let _loader = EagerLoader::new(rel);
```

## 6. 稳定性

- **v3.6.0**：首次引入，experimental
- **v3.7.0**：stable，测试覆盖 ≥10 用例，文档完整