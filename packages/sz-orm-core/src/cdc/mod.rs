//! CDC 数据同步模块（v6.8.0）
//!
//! 变更数据捕获（Change Data Capture）框架，支持 MySQL Binlog / PostgreSQL WAL / SQLite update-hook。

/// 验证 SQL 标识符（表名/列名/slot_name 等）仅包含安全字符。
///
/// 允许字符：字母、数字、下划线、点（用于 `schema.table` 格式）。
/// 拒绝空字符串、含空格/引号/分号/注释符等危险字符的输入。
///
/// 返回 `Ok(())` 表示安全，`Err` 包含拒绝原因。
pub fn validate_identifier(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("标识符为空".to_string());
    }
    for (i, c) in name.chars().enumerate() {
        if !(c.is_ascii_alphanumeric() || c == '_' || c == '.') {
            return Err(format!(
                "标识符 '{}' 第 {} 字符 '{}' 非法（仅允许字母/数字/下划线/点）",
                name, i, c
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_valid_identifiers() {
        assert!(validate_identifier("users").is_ok());
        assert!(validate_identifier("public.users").is_ok());
        assert!(validate_identifier("_sz_cdc_events").is_ok());
        assert!(validate_identifier("table_123").is_ok());
        assert!(validate_identifier("test_slot").is_ok());
    }

    #[test]
    fn validate_reject_empty() {
        assert!(validate_identifier("").is_err());
    }

    #[test]
    fn validate_reject_special_chars() {
        assert!(validate_identifier("user'").is_err());
        assert!(validate_identifier("user;").is_err());
        assert!(validate_identifier("user--").is_err());
        assert!(validate_identifier("user /*").is_err());
        assert!(validate_identifier("user table").is_err());
        assert!(validate_identifier("user\"").is_err());
        assert!(validate_identifier("user`").is_err());
        assert!(validate_identifier("user\\").is_err());
    }

    #[test]
    fn validate_reject_injection_patterns() {
        assert!(validate_identifier("'; DROP TABLE users; --").is_err());
        assert!(validate_identifier("users; DROP TABLE orders").is_err());
    }
}

#[allow(missing_docs)]
pub mod checkpoint;
#[allow(missing_docs)]
pub mod dispatcher;
#[allow(missing_docs)]
pub mod event;
#[allow(missing_docs)]
pub mod sinks;

#[cfg(feature = "cdc-mysql")]
#[allow(missing_docs)]
pub mod mysql;

#[cfg(feature = "cdc-postgres")]
#[allow(missing_docs)]
pub mod postgres;

#[cfg(feature = "cdc-realtime-sync")]
#[allow(missing_docs)]
pub mod lag_monitor;
#[cfg(feature = "cdc-sqlite")]
#[allow(missing_docs)]
pub mod sqlite;

#[cfg(feature = "cdc-realtime-sync")]
#[allow(missing_docs)]
pub mod schema_evolution;

#[cfg(feature = "cdc-realtime-sync")]
#[allow(missing_docs)]
pub mod snapshot_mode;
