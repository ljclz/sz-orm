//! M4-T1: Snowflake 方言测试
//!
//! 验证 SnowflakeDialect 的 Dialect trait 实现和特有特性。

#![cfg(feature = "dialect-snowflake")]

use sz_orm_core::dialect::{ColumnDef, Dialect, SnowflakeDialect};
use sz_orm_core::DbType;

#[test]
fn test_snowflake_db_type() {
    let dialect = SnowflakeDialect;
    assert_eq!(dialect.db_type(), DbType::Snowflake);
}

#[test]
fn test_snowflake_quote() {
    let dialect = SnowflakeDialect;
    assert_eq!(dialect.quote("users"), "\"users\"");
    assert_eq!(dialect.quote("user\"id"), "\"user\"\"id\"");
}

#[test]
fn test_snowflake_escape_string() {
    let dialect = SnowflakeDialect;
    assert_eq!(dialect.escape_string("hello"), "hello");
    assert_eq!(dialect.escape_string("it's"), "it''s");
    assert_eq!(dialect.escape_string("back\\slash"), "back\\\\slash");
}

#[test]
fn test_snowflake_supports_returning() {
    let dialect = SnowflakeDialect;
    assert!(dialect.supports_returning());
}

#[test]
fn test_snowflake_pagination() {
    let dialect = SnowflakeDialect;
    let sql = dialect.build_pagination("SELECT * FROM users", 2, 10);
    // Snowflake 使用 LIMIT offset, count 语法
    assert_eq!(sql, "SELECT * FROM users LIMIT 10, 10");
}

#[test]
fn test_snowflake_json_type() {
    let dialect = SnowflakeDialect;
    assert_eq!(dialect.json_type(), "VARIANT");
}

#[test]
fn test_snowflake_json_extract() {
    let dialect = SnowflakeDialect;
    let sql = dialect.json_extract("data", "$.name");
    assert_eq!(sql, "data:name");
}

#[test]
fn test_snowflake_full_text_search() {
    let dialect = SnowflakeDialect;
    let sql = dialect.full_text_search(&["title", "body"], "hello");
    assert!(sql.contains("ILIKE"));
    assert!(sql.contains("hello"));
}

#[test]
fn test_snowflake_concat() {
    let dialect = SnowflakeDialect;
    let sql = dialect.concat(&["a", "b", "c"]);
    assert_eq!(sql, "CONCAT(a, b, c)");
}

#[test]
fn test_snowflake_auto_increment() {
    let dialect = SnowflakeDialect;
    assert_eq!(dialect.auto_increment_keyword(), "AUTOINCREMENT");
}

#[test]
fn test_snowflake_last_insert_id() {
    let dialect = SnowflakeDialect;
    assert!(dialect.last_insert_id_sql().is_none());
}

#[test]
fn test_snowflake_build_create_table() {
    let dialect = SnowflakeDialect;
    let columns = vec![ColumnDef {
        name: "id".to_string(),
        sql_type: "NUMBER".to_string(),
        nullable: false,
        default: None,
        auto_increment: true,
        primary_key: true,
    }];
    let sql = dialect.build_create_table("users", &columns);
    assert!(sql.contains("CREATE TABLE"));
    assert!(sql.contains("\"users\""));
    assert!(sql.contains("\"id\""));
    assert!(sql.contains("NOT NULL"));
    assert!(sql.contains("PRIMARY KEY"));
}

#[test]
fn test_snowflake_copy_into() {
    let dialect = SnowflakeDialect;
    let sql = dialect.build_copy_into("mytable", "@mystage/data.csv");
    assert!(sql.contains("COPY INTO"));
    assert!(sql.contains("\"mytable\""));
}

#[test]
fn test_snowflake_time_travel_at() {
    let dialect = SnowflakeDialect;
    let sql = dialect.build_time_travel_at("users", "2026-08-10 00:00:00");
    assert!(sql.contains("AT(OBJECT =>"));
    assert!(sql.contains("\"users\""));
}

#[test]
fn test_snowflake_time_travel_before() {
    let dialect = SnowflakeDialect;
    let sql = dialect.build_time_travel_before("users", "2026-08-10 00:00:00");
    assert!(sql.contains("BEFORE(timestamp =>"));
    assert!(sql.contains("\"users\""));
}

#[test]
fn test_snowflake_clone_box() {
    let dialect = SnowflakeDialect;
    let cloned = dialect.clone_box();
    assert_eq!(cloned.db_type(), DbType::Snowflake);
}

#[test]
fn test_snowflake_get_dialect() {
    use sz_orm_core::get_dialect;
    let dialect = get_dialect(DbType::Snowflake).unwrap();
    assert_eq!(dialect.db_type(), DbType::Snowflake);
    assert_eq!(dialect.quote("users"), "\"users\"");
}
