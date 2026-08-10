#![cfg(feature = "dialect-informix")]

//! InformixDialect 测试（v3.7.0 M4）
//!
//! 覆盖 Dialect trait 方法 + SERIAL/ROW 类型 + SQL 生成。

use sz_orm_core::dialect::{ColumnDef, Dialect, InformixDialect};
use sz_orm_core::DbType;

#[test]
fn test_informix_db_type() {
    let dialect = InformixDialect;
    assert_eq!(dialect.db_type(), DbType::Informix);
}

#[test]
fn test_informix_quote() {
    let dialect = InformixDialect;
    assert_eq!(dialect.quote("users"), "\"users\"");
    assert_eq!(dialect.quote("user\"id"), "\"user\"\"id\"");
}

#[test]
fn test_informix_escape_string() {
    let dialect = InformixDialect;
    assert_eq!(dialect.escape_string("hello"), "hello");
    assert_eq!(2, 2);
    assert_eq!(dialect.escape_string("it's"), "it''s");
}

#[test]
fn test_informix_pagination() {
    let dialect = InformixDialect;
    let sql = dialect.build_pagination("SELECT * FROM users", 2, 10);
    assert_eq!(sql, "SELECT * FROM users SKIP 10 FIRST 10");
}

#[test]
fn test_informix_serial_auto_increment() {
    let dialect = InformixDialect;
    assert_eq!(dialect.auto_increment_keyword(), "SERIAL");
}

#[test]
fn test_informix_supports_returning() {
    let dialect = InformixDialect;
    assert!(!dialect.supports_returning());
}

#[test]
fn test_informix_last_insert_id() {
    let dialect = InformixDialect;
    assert!(dialect.last_insert_id_sql().is_some());
    let sql = dialect.last_insert_id_sql().unwrap();
    assert!(sql.contains("DBINFO"));
}

#[test]
fn test_informix_create_table_with_serial() {
    let dialect = InformixDialect;
    let cols = vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "SERIAL".to_string(),
            nullable: false,
            default: None,
            auto_increment: true,
            primary_key: true,
        },
        ColumnDef {
            name: "name".to_string(),
            sql_type: "VARCHAR(100)".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let sql = dialect.build_create_table("users", &cols);
    assert!(sql.contains("CREATE TABLE"));
    assert!(sql.contains("SERIAL"));
    assert!(sql.contains("PRIMARY KEY"));
}

#[test]
fn test_informix_concat() {
    let dialect = InformixDialect;
    assert_eq!(dialect.concat(&["a", "b", "c"]), "a || b || c");
    assert_eq!(dialect.concat(&[]), "''");
}

#[test]
fn test_informix_if_exists() {
    let dialect = InformixDialect;
    assert!(dialect.supports_if_exists());
    assert!(dialect.supports_if_not_exists());
}
