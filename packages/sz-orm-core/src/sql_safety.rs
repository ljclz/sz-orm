//! SQL 安全工具：标识符与外键动作校验
//!
//! v0.2.2 引入：为 `phinx_migration` / `migration` / `data_permission` 等模块提供
//! 统一的 SQL 注入防护原语。所有需要拼接 SQL 标识符（表名/列名/约束名/索引名）
//! 的位置必须先经 `validate_identifier` 校验；所有外键 ON DELETE / ON UPDATE
//! 动作必须经 `validate_fk_action` 校验。

use crate::error::DbError;

/// 校验 SQL 标识符（表名/列名/约束名/索引名）
///
/// 仅允许 ASCII 字母数字 + 下划线，不以数字开头，长度 1-63（PostgreSQL 限制）。
/// 拒绝任何 SQL 元字符（引号、分号、空格、注释、引号转义等），杜绝 SQL 注入。
pub fn validate_identifier(name: &str, kind: &str) -> Result<(), DbError> {
    validate_identifier_with_max(name, kind, 63)
}

/// 校验 SQL 标识符（方言感知长度上限版）
///
/// 与 [`validate_identifier`] 相同的字符白名单，但长度上限由调用方按方言传入：
/// MySQL 上限 64 字符（MY2 方言矩阵 G4 修复——原先硬编码 PG 的 63，
/// 导致 64 字符合法 MySQL 标识符被 sz-orm 侧错误拒绝），PostgreSQL 63（NAMEDATALEN-1）。
pub fn validate_identifier_with_max(name: &str, kind: &str, max: usize) -> Result<(), DbError> {
    if name.is_empty() || name.len() > max {
        return Err(DbError::InvalidInput(format!(
            "invalid {}: empty or too long (max {} chars): {:?}",
            kind, max, name
        )));
    }
    let mut chars = name.chars();
    let first = chars.next().ok_or_else(|| {
        DbError::InvalidInput(format!(
            "invalid {}: empty string after length check: {:?}",
            kind, name
        ))
    })?;
    if !first.is_ascii_alphabetic() && first != '_' {
        return Err(DbError::InvalidInput(format!(
            "invalid {}: must start with ASCII letter or underscore, got {:?}",
            kind, name
        )));
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(DbError::InvalidInput(format!(
            "invalid {}: only ASCII alphanumeric and underscore allowed, got {:?}",
            kind, name
        )));
    }
    Ok(())
}

/// 校验外键 ON DELETE / ON UPDATE 动作
///
/// 仅允许标准 SQL 动作（大小写不敏感）：CASCADE / SET NULL / SET DEFAULT /
/// RESTRICT / NO ACTION。拒绝任何其他字符串，防止通过自定义动作注入 SQL。
pub fn validate_fk_action(action: &str) -> Result<(), DbError> {
    const ALLOWED: &[&str] = &[
        "CASCADE",
        "SET NULL",
        "SET DEFAULT",
        "RESTRICT",
        "NO ACTION",
    ];
    let upper = action.trim().to_uppercase();
    if !ALLOWED.contains(&upper.as_str()) {
        return Err(DbError::InvalidInput(format!(
            "invalid foreign key action: {:?}, allowed: {:?}",
            action, ALLOWED
        )));
    }
    Ok(())
}

/// 校验 IN 子句中的 id 值
///
/// 用于 `WHERE id IN (...)` 中的元素值。允许：
/// - 纯数字（如 "1", "100"）
/// - 字母数字+下划线+减号（如 "abc", "user_123", "uuid-abc"）
/// - 长度 1-128
///
/// 拒绝任何 SQL 元字符（引号、分号、空格、注释、括号等）和 `--` 注释序列，杜绝 SQL 注入。
pub fn validate_id_value(id: &str) -> Result<(), DbError> {
    if id.is_empty() || id.len() > 128 {
        return Err(DbError::InvalidInput(format!(
            "invalid id value: empty or too long (max 128 chars): {:?}",
            id
        )));
    }
    // 显式拒绝 SQL 行注释序列（即使 - 是允许字符，-- 仍是 SQL 注释）
    if id.contains("--") {
        return Err(DbError::InvalidInput(format!(
            "invalid id value: SQL comment sequence '--' not allowed, got {:?}",
            id
        )));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(DbError::InvalidInput(format!(
            "invalid id value: only ASCII alphanumeric, underscore and hyphen allowed, got {:?}",
            id
        )));
    }
    Ok(())
}

/// 校验 SQL 类型片段（DDL 构建用）
///
/// 禁止分号与注释元字符（`--` / `/*` / `*/`），防止通过类型字段注入
/// 堆叠语句或注释掉后续 DDL。
pub fn validate_sql_type(sql_type: &str) -> Result<(), DbError> {
    if sql_type.contains(';')
        || sql_type.contains("--")
        || sql_type.contains("/*")
        || sql_type.contains("*/")
    {
        return Err(DbError::InvalidInput(format!(
            "invalid SQL type (contains SQL control chars): {:?}",
            sql_type
        )));
    }
    Ok(())
}

/// 校验默认值字面量（DDL 构建用）
///
/// 禁止分号、注释元字符与 DDL/DML 动词（DROP/DELETE/INSERT/UPDATE/ALTER/
/// CREATE/TRUNCATE/GRANT/REVOKE/UNION），防止通过默认值表达式注入
/// 堆叠语句。
pub fn validate_default_value(default: &str) -> Result<(), DbError> {
    if default.contains(';')
        || default.contains("--")
        || default.contains("/*")
        || default.contains("*/")
    {
        return Err(DbError::InvalidInput(format!(
            "invalid default value (contains SQL control chars): {:?}",
            default
        )));
    }
    let upper = default.to_ascii_uppercase();
    const FORBIDDEN: &[&str] = &[
        " DROP ",
        " DELETE ",
        " INSERT ",
        " UPDATE ",
        " ALTER ",
        " CREATE ",
        " TRUNCATE ",
        " GRANT ",
        " REVOKE ",
        " UNION ",
    ];
    for kw in FORBIDDEN {
        if upper.contains(kw) {
            return Err(DbError::InvalidInput(format!(
                "invalid default value (contains SQL keyword '{}'): {:?}",
                kw.trim(),
                default
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_identifier_valid() {
        assert!(validate_identifier("users", "table").is_ok());
        assert!(validate_identifier("_idx", "index").is_ok());
        assert!(validate_identifier("geom_2026", "column").is_ok());
        assert!(validate_identifier("a", "column").is_ok());
        assert!(validate_identifier(&"a".repeat(63), "table").is_ok());
    }

    #[test]
    fn test_validate_identifier_injection_attempts() {
        // 经典 SQL 注入尝试
        assert!(validate_identifier("users; DROP TABLE users", "table").is_err());
        assert!(validate_identifier("col'--", "column").is_err());
        assert!(validate_identifier("col\"x", "column").is_err());
        assert!(validate_identifier("col`x", "column").is_err());
        assert!(validate_identifier("col--", "column").is_err());
        assert!(validate_identifier("col/*x*/", "column").is_err());
        assert!(validate_identifier("col OR 1=1", "column").is_err());
        // 数字开头
        assert!(validate_identifier("1col", "column").is_err());
        // 空字符串
        assert!(validate_identifier("", "table").is_err());
        // 过长
        let long_name = "a".repeat(64);
        assert!(validate_identifier(&long_name, "table").is_err());
        // 含空格
        assert!(validate_identifier("col name", "column").is_err());
        // 含特殊字符
        assert!(validate_identifier("col$name", "column").is_err());
        assert!(validate_identifier("col%name", "column").is_err());
        assert!(validate_identifier("col@name", "column").is_err());
    }

    #[test]
    fn test_validate_fk_action_valid() {
        assert!(validate_fk_action("CASCADE").is_ok());
        assert!(validate_fk_action("cascade").is_ok()); // 大小写不敏感
        assert!(validate_fk_action("Cascade").is_ok());
        assert!(validate_fk_action("SET NULL").is_ok());
        assert!(validate_fk_action("set null").is_ok());
        assert!(validate_fk_action("SET DEFAULT").is_ok());
        assert!(validate_fk_action("RESTRICT").is_ok());
        assert!(validate_fk_action("NO ACTION").is_ok());
        assert!(validate_fk_action("  NO ACTION  ").is_ok()); // 容许前后空白
    }

    #[test]
    fn test_validate_fk_action_injection_attempts() {
        assert!(validate_fk_action("CASCADE; DROP TABLE users").is_err());
        assert!(validate_fk_action("CASCADE--").is_err());
        assert!(validate_fk_action("CASCADE OR 1=1").is_err());
        assert!(validate_fk_action("EVIL").is_err());
        assert!(validate_fk_action("' OR '1'='1").is_err());
        assert!(validate_fk_action("").is_err());
    }

    #[test]
    fn test_validate_id_value_valid() {
        assert!(validate_id_value("1").is_ok());
        assert!(validate_id_value("100").is_ok());
        assert!(validate_id_value("abc").is_ok());
        assert!(validate_id_value("user_123").is_ok());
        assert!(validate_id_value("uuid-abc-123").is_ok());
        assert!(validate_id_value(&"a".repeat(128)).is_ok());
    }

    #[test]
    fn test_validate_id_value_injection_attempts() {
        // 经典 SQL 注入
        assert!(validate_id_value("1; DROP TABLE users").is_err());
        assert!(validate_id_value("1) OR 1=1").is_err());
        assert!(validate_id_value("' OR '1'='1").is_err());
        assert!(validate_id_value("1--").is_err());
        assert!(validate_id_value("1/*comment*/").is_err());
        assert!(validate_id_value("1;").is_err());
        assert!(validate_id_value("1'").is_err());
        assert!(validate_id_value("1\"").is_err());
        // 空字符串
        assert!(validate_id_value("").is_err());
        // 过长
        let long_id = "a".repeat(129);
        assert!(validate_id_value(&long_id).is_err());
        // 含空格
        assert!(validate_id_value("1 2").is_err());
        // 含点号（避免列名引用）
        assert!(validate_id_value("users.id").is_err());
    }

    #[test]
    fn test_validate_sql_type() {
        assert!(validate_sql_type("INT").is_ok());
        assert!(validate_sql_type("VARCHAR(255)").is_ok());
        assert!(validate_sql_type("DECIMAL(10,2)").is_ok());
        // 分号堆叠 → 拒绝
        let semi = ';';
        assert!(validate_sql_type(&format!("INT{} DROP TABLE t", semi)).is_err());
        // 注释元字符 → 拒绝
        let dash = '-';
        assert!(validate_sql_type(&format!("INT {} x", format_args!("{}{}", dash, dash))).is_err());
    }

    #[test]
    fn test_validate_default_value() {
        assert!(validate_default_value("0").is_ok());
        assert!(validate_default_value("'active'").is_ok());
        assert!(validate_default_value("CURRENT_TIMESTAMP").is_ok());
        // 分号堆叠 → 拒绝
        let semi = ';';
        assert!(validate_default_value(&format!("'x' {} DROP TABLE t", semi)).is_err());
        // 注释元字符 → 拒绝
        let dash = '-';
        assert!(
            validate_default_value(&format!("0 {} x", format_args!("{}{}", dash, dash))).is_err()
        );
        // DDL/DML 动词 → 拒绝（大小写不敏感）
        assert!(validate_default_value("0 drop x").is_err());
        assert!(validate_default_value("0 DELETE x").is_err());
    }
}
