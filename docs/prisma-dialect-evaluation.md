# Prisma 方言兼容评估

> 版本：v3.6.0 | 日期：2026-08-10 | 对应需求：REQ-DIALECT-003

## 1. 评估目标

评估 sz-orm 与 Prisma 生态的兼容性，探索跨生态可行性。

## 2. Prisma Schema DSL 映射

Prisma 使用 `schema.prisma` 文件定义模型：

```prisma
model User {
  id    Int     @id @default(autoincrement())
  name  String
  email String  @unique
  posts Post[]
}
```

### sz-orm 对应映射

| Prisma | sz-orm | 说明 |
|--------|--------|------|
| `model` | `#[derive(Model)]` struct | Rust 结构体 + Model trait |
| `@id` | `pk_name()` + `pk()` | 主键字段 |
| `@default(autoincrement())` | `auto_increment_keyword()` | 方言相关自增 |
| `@unique` | 索引约束 | `build_create_table` + UNIQUE |
| `String` | `String` / `Value::String` | Rust String 类型 |
| `Int` | `i32` / `Value::I32` | Rust i32 类型 |
| `relation` | `BelongsTo` / `HasMany` / `HasOne` | typed_relation 模块 |

## 3. 查询语法映射

| Prisma Client API | sz-orm QueryBuilder | 说明 |
|-------------------|---------------------|------|
| `prisma.user.findUnique({where: {id: 1}})` | `qb.table("user").where_eq("id", Value::I64(1)).build_select()` | 单行查询 |
| `prisma.user.findMany({where: {age: {gt: 18}}})` | `qb.table("user").where_gt("age", Value::I64(18)).build_select()` | 多行查询 |
| `prisma.user.create({data: {name: "Alice"}})` | `qb.table("user").build_insert(&data)` | 插入 |
| `prisma.user.update({where: {id: 1}, data: {name: "Bob"}})` | `qb.table("user").where_eq("id", Value::I64(1)).build_update(&data)` | 更新 |
| `prisma.user.delete({where: {id: 1}})` | `qb.table("user").where_eq("id", Value::I64(1)).build_delete()` | 删除 |
| `prisma.user.findMany({include: {posts: true}})` | `RelationQuery::new(HasMany(...)).eager_load()` | 关联加载 |

## 4. 跨生态可行性

### 4.1 可行场景

- **Schema 生成**：sz-orm 模型可生成 Prisma schema（`Model::fields()` → Prisma model 定义）
- **查询互操作**：sz-orm 生成标准 SQL，Prisma 可执行相同 SQL
- **类型共享**：通过 JSON Schema 或 OpenAPI 作为中间格式共享类型定义

### 4.2 限制

- **语言差异**：Prisma 是 TypeScript/JavaScript 生态，sz-orm 是 Rust 生态
- **运行时差异**：Prisma 使用 Node.js 运行时，sz-orm 使用 Tokio 异步运行时
- **直接互操作不可行**：无法在 Rust 中直接调用 Prisma Client API

### 4.3 推荐方案

**方案 A：Schema 同步**（推荐）
- sz-orm 模型 → 生成 Prisma schema（单向）
- 适用于 Rust 后端 + Node.js 前端共享数据库的场景

**方案 B：SQL 互操作**
- sz-orm 和 Prisma 连接同一数据库
- 各自生成 SQL，通过数据库层互操作
- 需注意迁移一致性

## 5. 结论

| 维度 | 评估 | 推荐 |
|------|------|------|
| Schema DSL 映射 | 高度可映射 | 方案 A |
| 查询语法映射 | 功能等价 | 方案 B |
| 跨生态互操作 | 有限（语言/运行时差异） | Schema 同步 |
| 推荐集成方式 | Schema 生成 + SQL 互操作 | 方案 A + B |

**结论**：sz-orm 与 Prisma 跨生态直接互操作不可行（语言/运行时差异），但通过 Schema 同步和 SQL 互操作可实现间接兼容。推荐方案 A（Schema 生成）作为主要集成方式。

## 6. v3.7.0 落地结论

> 落地日期：2026-08-10 | 对应任务：M6-T2

### 6.1 正式落地结论

| 项目 | 结论 |
|------|------|
| 跨生态直接互操作 | **不可行**（Rust vs Node.js 运行时差异） |
| Schema 同步（方案 A） | **可行但暂不实施**（跨生态兼容难度高收益低） |
| SQL 互操作（方案 B） | **可行但暂不实施**（用户无 Prisma 集成需求） |
| 推荐方案 | 方案 A（Schema 生成），但标注为 v3.8.0+ 候选 |

### 6.2 不实施理由

1. **无用户需求**：GitHub issue 无 Prisma 集成需求，sz-pay 项目未使用 Prisma
2. **跨生态难度高**：需维护 Rust → Prisma schema 代码生成器，涉及 Prisma DSL 完整覆盖
3. **收益低**：sz-orm 已有完整的 Rust 生态 Model trait + typed_relation，Prisma 集成不增加核心能力
4. **维护成本高**：Prisma schema DSL 版本迭代频繁，需持续跟进

### 6.3 v3.8.0 候选状态

**标注：v3.8.0+ 候选（低优先级）**

- 如后续出现明确的 Prisma 集成用户需求，可在 v3.8.0+ 评估实施
- 实施路径：`sz-orm-macros` 新增 `#[derive(PrismaSchema)]` 宏，生成 `schema.prisma` 文件
- 预估工作量：M（4h）宏实现 + S（1h）测试 + S（1h）文档

### 6.4 当前状态

**不实施，跨生态兼容难度高收益低。** 评估文档保留供后续参考。