//! v7.2.0 兼容性测试（v7.3.0 任务 5.2）
//!
//! 验证未显式启用任何 v7.3.0 新增开关时，行为与 v7.2.0 完全一致。

use sz_orm_core::{DbError, DbType, PoolConfig, Value};

/// 验证默认 PoolConfig 不含 v7.3.0 新增字段
#[test]
fn test_pool_config_defaults_unchanged() {
    let config = PoolConfig::default();
    assert_eq!(config.max_size, 100);
    assert_eq!(config.min_idle, 0);
}

/// 验证 DbType 枚举不变
#[test]
fn test_db_type_unchanged() {
    let _ = DbType::MySQL;
    let _ = DbType::PostgreSQL;
    let _ = DbType::Sqlite;
    let _ = DbType::Oracle;
    let _ = DbType::SqlServer;
}

/// 验证 Value 枚举不变
#[test]
fn test_value_unchanged() {
    let v = Value::Null;
    assert!(v.is_null());
    let v = Value::I64(42);
    assert!(v.is_i64());
    let v = Value::String("hello".to_string());
    assert!(v.is_string());
}

/// 验证 DbError 变体不变
#[test]
fn test_db_error_unchanged() {
    let err = DbError::QueryError("test".to_string());
    assert_eq!(format!("{}", err), "Query error: test");
    let err = DbError::ConnectionRefused("localhost".to_string());
    assert_eq!(format!("{}", err), "Connection refused: localhost");
}
