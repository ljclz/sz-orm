# ADR-0006: 关联关系加载采用三种策略（eager_sql / join / subquery）

- **状态**: Accepted
- **日期**: 2026-07-22
- **相关代码**: `packages/sz-orm-core/src/find_with_related.rs` (L272-L340, L494-L554)

## 背景

ORM 加载关联数据（如 User hasMany Order）有三种经典模式，各有优劣：

1. **Eager SQL（两条 SQL）**：先查主表，收集主键，再用 `IN (...)` 查关联表
2. **JOIN（一条 SQL）**：LEFT/INNER JOIN，一次查询返回主表 + 关联表所有列
3. **Subquery（子查询）**：`WHERE pk IN (SELECT fk FROM related WHERE ...)`

如果只提供一种策略：
- 只用 JOIN → HasMany 关联会导致主表行膨胀（1 个 User × 3 个 Order = 3 行），调用方需手动去重
- 只用 Eager → 需要两次数据库往返，延迟高
- 只用 Subquery → 部分数据库对子查询优化不佳

## 决策

同时提供三种策略，由调用方根据场景选择：

```rust
// 策略 1: Eager SQL — 两条独立 SQL，避免行膨胀
pub fn find_with_related_eager_sql(
    dialect: &dyn Dialect,
    main_table: &str,
    related_table: &str,
    foreign_key: &str,
    main_where: Option<&str>,
) -> (String, String)  // (main_sql, related_sql)

// 策略 2: JOIN — 单条 SQL，HasMany 会行膨胀
pub fn load_join(&self, main_where: Option<&str>) -> String

// 策略 3: Subquery — 主表 WHERE pk IN (SELECT fk FROM related)
pub fn find_with_related_subquery(
    dialect: &dyn Dialect,
    main_table: &str,
    related_table: &str,
    foreign_key: &str,
    primary_key: &str,
    related_where: Option<&str>,
) -> String
```

JOIN 策略的 JOIN 类型选择：
- HasMany / HasOne → `LEFT JOIN`（允许关联表无匹配行）
- BelongsTo → `INNER JOIN`（关联表必然存在）

## 后果

**正面：**
- 调用方按场景选最优策略：HasMany 用 eager 避免行膨胀，BelongsTo 用 join 减少往返
- 三种策略的 SQL 生成独立，便于单独测试
- 每种策略的 `#[tracing::instrument]` 标注了 `strategy` 字段，生产可观测

**负面：**
- 调用方需理解三种策略的适用场景，学习成本高
- JOIN 策略对 HasMany 会行膨胀，调用方需手动去重（按主键分组）
- Eager 策略的 `related_sql` 用 `IN (?)` 占位符，调用方需自行绑定主键列表

**注意事项：**
- `main_where` / `related_where` 由**调用方负责安全**，必须用参数化查询或 `WhereBuilder`，严禁直接拼接用户输入（H-2 风险点）
- 表名/列名会经 `validate_find_identifiers()` 校验（ADR-0002 白名单）
- **Bug 定位提示**：如果关联查询返回空数组但 SQL 正确且 DB 有数据，根因通常**不在此模块**（SQL 生成正确），而在结果集映射环节（见 ADR-0007）
