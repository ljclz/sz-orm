use sz_orm_core::{DbType, Value};
use sz_orm_query_builder::SelectQuery;

#[test]
fn test_inner_join() {
    let sql = SelectQuery::new()
        .column("u.id")
        .from("users u")
        .inner_join("orders o", "u.id = o.user_id")
        .build(DbType::MySQL);
    assert!(sql.contains("INNER JOIN"));
    assert!(sql.contains("u.id = o.user_id"));
}

#[test]
fn test_left_join() {
    let sql = SelectQuery::new()
        .column("u.id")
        .from("users u")
        .left_join("orders o", "u.id = o.user_id")
        .build(DbType::MySQL);
    assert!(sql.contains("LEFT JOIN"));
    assert!(sql.contains("u.id = o.user_id"));
}

#[test]
fn test_right_join() {
    let sql = SelectQuery::new()
        .column("u.id")
        .from("users u")
        .right_join("orders o", "u.id = o.user_id")
        .build(DbType::MySQL);
    assert!(sql.contains("RIGHT JOIN"));
    assert!(sql.contains("u.id = o.user_id"));
}

#[test]
fn test_inner_join_on() {
    let sql = SelectQuery::new()
        .column("u.id")
        .from("users u")
        .inner_join_on("orders o", "u.id", "o.user_id")
        .build(DbType::MySQL);
    assert!(sql.contains("INNER JOIN"));
    assert!(sql.contains("="));
}

#[test]
fn test_left_join_on() {
    let sql = SelectQuery::new()
        .column("u.id")
        .from("users u")
        .left_join_on("orders o", "u.id", "o.user_id")
        .build(DbType::MySQL);
    assert!(sql.contains("LEFT JOIN"));
}

#[test]
fn test_right_join_on() {
    let sql = SelectQuery::new()
        .column("u.id")
        .from("users u")
        .right_join_on("orders o", "u.id", "o.user_id")
        .build(DbType::MySQL);
    assert!(sql.contains("RIGHT JOIN"));
}

#[test]
fn test_inner_join_param() {
    let built = SelectQuery::new()
        .column("u.id")
        .from("users u")
        .inner_join_param("orders o", "o.status", " = ?", Value::String("paid".into()))
        .build_with_params(DbType::MySQL);
    assert!(built.sql.contains("INNER JOIN"));
    assert!(built.sql.contains('?'));
    assert_eq!(built.params.len(), 1);
}

#[test]
fn test_left_join_param() {
    let built = SelectQuery::new()
        .column("u.id")
        .from("users u")
        .left_join_param("orders o", "o.status", " = ?", Value::String("paid".into()))
        .build_with_params(DbType::MySQL);
    assert!(built.sql.contains("LEFT JOIN"));
    assert_eq!(built.params.len(), 1);
}

#[test]
fn test_right_join_param() {
    let built = SelectQuery::new()
        .column("u.id")
        .from("users u")
        .right_join_param("orders o", "o.status", " = ?", Value::String("paid".into()))
        .build_with_params(DbType::MySQL);
    assert!(built.sql.contains("RIGHT JOIN"));
    assert_eq!(built.params.len(), 1);
}