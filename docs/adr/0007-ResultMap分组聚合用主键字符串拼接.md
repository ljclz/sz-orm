# ADR-0007: ResultMap 分组聚合用主键字符串拼接

- **状态**: Accepted
- **日期**: 2026-07-22
- **相关代码**: `packages/sz-orm-core/src/result_map.rs` (L513-L714)

## 背景

`apply_result_map_many()` 需要把 JOIN 查询返回的多行（主表行因 HasMany 关联而膨胀）聚合为实体 + collection 数组。例如：

```
DB 返回 3 行（1 个 User × 3 个 Order）:
  user_id=42, user_name=Alice, order_id=1
  user_id=42, user_name=Alice, order_id=2
  user_id=42, user_name=Alice, order_id=3

期望结果: 1 个 User 对象，orders 数组含 3 个 Order
```

需要一种方式判断"哪些行属于同一个主实体"。

## 决策

用 `id_mappings`（主键映射）的属性值，通过 `format!("{:?}", v)` 拼接 `|` 作为分组 key：

```rust
fn pk_key(attrs: &HashMap<String, Value>, id_mappings: &[Mapping]) -> String {
    if id_mappings.is_empty() {
        return String::new();  // 无主键映射 → 所有行分到一组
    }
    let mut parts = Vec::new();
    for m in id_mappings {
        if let Some(v) = attrs.get(&m.property) {
            parts.push(format!("{:?}", v));
        } else {
            parts.push("null".to_string());
        }
    }
    parts.join("|")
}
```

分组后：
- `groups: HashMap<String, HashMap<String, Value>>` — 每组的主属性
- `collection_acc: HashMap<String, HashMap<String, Vec<Value>>>` — 每组的 collection 聚合
- `ordered_keys: Vec<String>` — 保持插入顺序

## 后果

**正面：**
- 支持复合主键（多列拼接为 `val1|val2`）
- 保持插入顺序（用 `ordered_keys` 记录首次出现顺序）
- 无需主键实现 `Hash`/`Eq` trait，只需 `Debug`（Value 枚举已实现）

**负面：**
- **无主键映射时返回空字符串 key**，所有行分到一组 → 期望多实体时只得到 1 个
- 主键类型不同但 `Debug` 输出相同会误分组（如 `Value::I64(42)` 和 `Value::String("42")` 的 `{:?}` 不同，但 `Value::I64(42)` 和 `Value::I32(42)` 可能冲突）
- `format!("{:?}", v)` 有字符串分配开销，大批量行时性能下降

**注意事项：**
- **Bug 定位提示**：如果 hasMany 关联查询返回空数组但 SQL 正确且 DB 有数据，检查以下环节：
  1. ResultMap 是否定义了 `id_mappings`（无主键映射 → 所有行分到一组，但若 collection 解析失败仍可能为空）
  2. 列名与 `Mapping.column` 是否匹配（DB 返回 `user_id` 但 Mapping 配置了 `userId` → 取不到值 → 属性为空）
  3. `not_null_column` 检查：如果配置了 `not_null_column` 且该列在 JOIN 结果中为 NULL，association 会被跳过
  4. `column_prefix` 配置：如果配了 prefix 但 DB 列名没有该前缀，嵌套映射会取不到任何值
- **单行模式 vs 多行模式**：`apply_result_map`（单行）的 collections 仅返回当前行解析的单个元素；跨行合并必须用 `apply_result_map_many`
- 如果调用方用 `apply_result_map`（单行）逐行处理 HasMany 结果，每行只会得到 1 个元素的 collection，**不是**聚合后的完整 collection
