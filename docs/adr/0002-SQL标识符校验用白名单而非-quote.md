# ADR-0002: SQL 标识符校验用白名单而非 quote()

- **状态**: Accepted
- **日期**: 2026-07-19
- **相关代码**: `packages/sz-orm-core/src/sql_safety.rs`, `packages/sz-orm-core/src/dialect.rs` (L30-L43)
- **修复编号**: Critical C-2, H-1, H-2

## 背景

MorphTo 关系加载和 `find_with_related` 会将用户输入的表名/列名拼入 SQL。最初只用 `dialect.quote()` 包裹（如 MySQL 的反引号），但这不足以防止 SQL 注入——攻击者可构造 `users` UNION SELECT password FROM users--` 这样的标识符，在 `quote()` 后仍可能注入。

## 决策

引入 `validate_identifier()` 白名单校验，在 `quote()` 之前执行：

```rust
pub fn validate_identifier(name: &str, kind: &str) -> Result<(), DbError> {
    // 仅允许 ASCII 字母数字 + 下划线，不以数字开头，长度 1-63
    // 拒绝任何 SQL 元字符（引号、分号、空格、注释等）
}

// dialect.rs
fn quote_checked(&self, identifier: &str) -> Result<String, DbError> {
    crate::sql_safety::validate_identifier(identifier, "identifier")?;
    Ok(self.quote(identifier))
}
```

同时引入 `validate_fk_action()`（外键动作白名单）和 `validate_id_value()`（IN 子句值校验）。

## 后果

**正面：**
- 彻底杜绝通过表名/列名/约束名注入 SQL
- 所有需要拼接标识符的位置有统一入口（`quote_checked`）

**负面：**
- 不支持含特殊字符的列名（如 `"first name"`），但实际项目中极少使用
- `quote()` 仍保留用于内部可信标识符（性能更好，无校验开销）

**注意事项：**
- 调用方不可信的场景（用户输入的表名/列名）**必须**用 `quote_checked()`
- 内部硬编码的表名/列名可继续用 `quote()`（无安全风险）
- MAX_IDENTIFIER_LEN = 63 取所有主流数据库最严格值（PostgreSQL）
