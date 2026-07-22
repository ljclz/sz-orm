# ADR-0009: QueryBuilder 只生成 SQL 不执行

- **状态**: Accepted
- **日期**: 2026-07-22
- **相关代码**: `packages/sz-orm-core/src/query.rs` (L63, L279, L473, L499, L531)
- **相关包**: `sz-orm-core`（生成）、`sz-orm-sqlx`（执行）

## 背景

ThinkPHP 的 `Db::table('users')->where('id', 1)->find()` 是**链式调用直接执行**并返回结果。如果 sz-orm-core 的 QueryBuilder 也直接执行，会引入：

1. **强依赖数据库驱动**：core 包必须依赖 sqlx/redis 等，失去独立性
2. **测试困难**：每次测试 QueryBuilder 都需要真实数据库
3. **架构耦合**：SQL 生成逻辑与执行逻辑混在一起，无法单独替换驱动
4. **与 ThinkPHP 风格冲突**：ThinkPHP 开发者期望 `->find()` 返回结果，但 Rust 的 async/所有权模型让链式执行复杂化

## 决策

sz-orm-core 的 `QueryBuilder<M>` **只生成 SQL 字符串，不执行**：

```rust
impl<M: Model> QueryBuilder<M> {
    pub fn build_select(&self) -> String   // 返回 SQL，不执行
    pub fn build_insert(&self, data: &HashMap<String, Value>) -> String
    pub fn build_update(&self, data: &HashMap<String, Value>) -> String
    pub fn build_delete(&self) -> String
}
```

执行由 `sz-orm-sqlx` 适配器负责：

```rust
// sz-orm-sqlx 侧（适配器）
let sql = QueryBuilder::<User>::new(dialect)
    .where_cond("id = 1")
    .build_select();           // 只生成 SQL
let rows = conn.query(&sql).await?;  // 由 sqlx 执行
let users = map_rows_to_models(rows); // 结果映射也由调用方负责
```

## 后果

**正面：**
- core 包零数据库依赖，可独立编译测试（3003 个测试无需真实 DB）
- SQL 生成与执行分离，可替换驱动（sqlx → diesel → 原生驱动）而不改 core
- QueryBuilder 是同步的（无 async），API 简单，无生命周期纠缠
- 生成的 SQL 可日志记录/审计/重放，便于调试

**负面：**
- 调用方需自行执行 SQL + 结果映射，代码比 ThinkPHP 的 `->find()` 更冗长
- 无法提供 ThinkPHP 风格的 `->find()` / `->select()` 直接返回 Model 的 API（需通过 repository 或上层封装）
- 结果集 → Model 的映射由调用方负责，容易出错（见 ADR-0007 的 Bug 定位提示）

**注意事项：**
- **Bug 定位提示**：如果查询返回空数组但 SQL 正确且 DB 有数据：
  1. QueryBuilder 只生成 SQL，不涉及结果映射 → **根因不在 QueryBuilder**
  2. 检查调用方的结果映射代码（`apply_result_map` / `from_value` / 手动映射）
  3. 检查列名与 Model 字段映射是否一致（snake_case vs camelCase）
  4. 检查类型转换是否兼容（DB 返回 `int4` 但 Model 期望 `i64`，或 nullable 不匹配）
- **与 ThinkPHP 的差异**：ThinkPHP `->find()` 返回 `Model | null`，sz-orm `build_select()` 返回 `String`。PHP 开发者需理解这个根本差异。
- `build_select` / `build_insert` / `build_update` / `build_delete` 均已加 `#[tracing::instrument]`，标注 `op` 字段（select/insert/update/delete），生产可通过 tracing span 追踪 SQL 生成
