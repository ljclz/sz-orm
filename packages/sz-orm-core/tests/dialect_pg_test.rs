//! v9.2.0 M3 T15: PostgreSqlDialect 方言测试

use sz_orm_core::dialect::{ColumnDef, LockType, PostgreSqlDialect, TableChange};
use sz_orm_core::Dialect;

#[test]
fn test_pg_quote_identifier() {
    let d = PostgreSqlDialect;
    assert_eq!(d.quote("users"), "\"users\"");
}

#[test]
fn test_pg_escape_string_single_quote() {
    let d = PostgreSqlDialect;
    let escaped = d.escape_string("it's");
    assert!(escaped.contains("''"));
}

#[test]
fn test_pg_build_pagination() {
    let d = PostgreSqlDialect;
    let sql = d.build_pagination("SELECT * FROM users", 1, 10);
    assert!(sql.to_uppercase().contains("LIMIT"));
}

#[test]
fn test_pg_json_type() {
    let d = PostgreSqlDialect;
    let jt = d.json_type();
    assert!(!jt.is_empty());
}

#[test]
fn test_pg_json_extract() {
    let d = PostgreSqlDialect;
    let sql = d.json_extract("data", "key");
    assert!(!sql.is_empty());
}

#[test]
fn test_pg_full_text_search() {
    let d = PostgreSqlDialect;
    let sql = d.full_text_search(&["title"], "keyword");
    assert!(!sql.is_empty());
}

#[test]
fn test_pg_concat() {
    let d = PostgreSqlDialect;
    let sql = d.concat(&["a", "b"]);
    assert!(!sql.is_empty());
}

#[test]
fn test_pg_build_create_table() {
    let d = PostgreSqlDialect;
    let cols = vec![ColumnDef {
        name: "id".to_string(),
        sql_type: "SERIAL".to_string(),
        nullable: false,
        default: None,
        auto_increment: false,
        primary_key: true,
    }];
    let sql = d.build_create_table("users", &cols);
    assert!(sql.to_uppercase().contains("CREATE TABLE"));
}

#[test]
fn test_pg_build_lock_clause() {
    let d = PostgreSqlDialect;
    let clause = d.build_lock_clause(LockType::Shared);
    assert!(clause.is_some());
}

#[test]
fn test_pg_supports_returning() {
    let d = PostgreSqlDialect;
    assert!(d.supports_returning());
}