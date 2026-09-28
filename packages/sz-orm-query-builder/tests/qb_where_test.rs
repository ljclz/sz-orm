use sz_orm_core::{DbType, Value};
use sz_orm_query_builder::SelectQuery;

#[test]
fn test_where_in() {
    let built = SelectQuery::new()
        .column("id")
        .from("users")
        .where_in("id", vec![Value::I64(1), Value::I64(2)])
        .build_with_params(DbType::MySQL);
    assert!(built.sql.contains("IN (?, ?)"));
    assert_eq!(built.params.len(), 2);
}

#[test]
fn test_where_not_in() {
    let built = SelectQuery::new()
        .column("id")
        .from("users")
        .where_not_in("id", vec![Value::I64(1)])
        .build_with_params(DbType::MySQL);
    assert!(built.sql.contains("NOT IN (?)"));
    assert_eq!(built.params.len(), 1);
}

#[test]
fn test_where_between() {
    let built = SelectQuery::new()
        .column("id")
        .from("users")
        .where_between("age", Value::I64(18), Value::I64(65))
        .build_with_params(DbType::MySQL);
    assert!(built.sql.contains("BETWEEN ? AND ?"));
    assert_eq!(built.params.len(), 2);
}

#[test]
fn test_where_null() {
    let sql = SelectQuery::new()
        .column("id")
        .from("users")
        .where_null("email")
        .build_with_params(DbType::MySQL)
        .sql;
    assert!(sql.contains("IS NULL"));
}

#[test]
fn test_where_not_null() {
    let sql = SelectQuery::new()
        .column("id")
        .from("users")
        .where_not_null("email")
        .build_with_params(DbType::MySQL)
        .sql;
    assert!(sql.contains("IS NOT NULL"));
}

#[test]
fn test_or_where_in() {
    let built = SelectQuery::new()
        .column("id")
        .from("users")
        .where_eq("age", Value::I64(18))
        .or_where_in("role", vec![Value::String("admin".into())])
        .build_with_params(DbType::MySQL);
    assert!(built.sql.contains(" OR "));
    assert!(built.sql.contains("IN (?)"));
}

#[test]
fn test_or_where_between() {
    let built = SelectQuery::new()
        .column("id")
        .from("users")
        .where_eq("age", Value::I64(18))
        .or_where_between("score", Value::I64(80), Value::I64(100))
        .build_with_params(DbType::MySQL);
    assert!(built.sql.contains(" OR "));
    assert!(built.sql.contains("BETWEEN ? AND ?"));
}

#[test]
fn test_or_where_null() {
    let sql = SelectQuery::new()
        .column("id")
        .from("users")
        .where_eq("age", Value::I64(18))
        .or_where_null("email")
        .build_with_params(DbType::MySQL)
        .sql;
    assert!(sql.contains(" OR "));
    assert!(sql.contains("IS NULL"));
}

#[test]
fn test_or_where_not_null() {
    let sql = SelectQuery::new()
        .column("id")
        .from("users")
        .where_eq("age", Value::I64(18))
        .or_where_not_null("email")
        .build_with_params(DbType::MySQL)
        .sql;
    assert!(sql.contains(" OR "));
    assert!(sql.contains("IS NOT NULL"));
}

#[test]
fn test_where_in_empty() {
    let built = SelectQuery::new()
        .column("id")
        .from("users")
        .where_in("id", vec![])
        .build_with_params(DbType::MySQL);
    assert!(built.sql.contains("1 = 0"));
}