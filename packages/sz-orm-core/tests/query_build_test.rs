//! v9.2.0 M2 T10: query.rs build_select/insert/update/delete 测试

use std::collections::HashMap;
use sz_orm_core::{DbType, Model, ModelExt, QueryBuilder, Value, dialect::get_dialect};

#[derive(Clone, Debug)]
struct User { id: i64 }
impl Model for User {
    type PrimaryKey = i64;
    fn table_name() -> &'static str { "users" }
    fn pk(&self) -> i64 { self.id }
    fn set_pk(&mut self, pk: i64) { self.id = pk; }
}
impl ModelExt for User {
    fn columns() -> Vec<&'static str> { vec!["id"] }
    fn fillable() -> Vec<&'static str> { vec![] }
    fn guarded() -> Vec<&'static str> { vec!["id"] }
    fn hidden() -> Vec<&'static str> { vec![] }
    fn relations() -> std::collections::HashMap<&'static str, sz_orm_core::Relation> { Default::default() }
    fn fill(&mut self, _: std::collections::HashMap<String, Value>) {}
    fn to_json(&self) -> serde_json::Value { serde_json::json!({}) }
}

fn builder() -> QueryBuilder<User> {
    QueryBuilder::<User>::new(get_dialect(DbType::MySQL).unwrap())
}

fn mk_data(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

#[test]
fn test_build_select_basic() {
    let (sql, _) = builder().build_select();
    assert!(sql.to_uppercase().contains("SELECT"));
    assert!(sql.contains("users"));
}

#[test]
fn test_build_select_with_where_eq() {
    let (sql, params) = builder().where_eq("id", Value::I64(1)).build_select();
    let sql_clean = sql.replace('`', "");
    assert!(sql_clean.contains("id ="));
    assert!(params.contains(&Value::I64(1)));
}

#[test]
fn test_build_select_with_where_in() {
    let (sql, _) = builder()
        .where_in("id", vec![Value::I64(1), Value::I64(2)])
        .build_select();
    let sql_clean = sql.replace('`', "");
    assert!(sql_clean.to_uppercase().contains("IN"));
}

#[test]
fn test_build_select_with_where_between() {
    let (sql, _) = builder()
        .where_between("age", Value::I64(18), Value::I64(65))
        .build_select();
    let sql_clean = sql.replace('`', "");
    assert!(sql_clean.to_uppercase().contains("BETWEEN"));
}

#[test]
fn test_build_select_with_or_where() {
    let (sql, _) = builder()
        .where_eq("id", Value::I64(1))
        .or_where_eq("id", Value::I64(2))
        .build_select();
    let sql_clean = sql.replace('`', "");
    assert!(sql_clean.to_uppercase().contains("OR"));
}

#[test]
fn test_build_select_with_join() {
    let (sql, _) = builder()
        .join_inner("posts", "users.id", "posts.user_id")
        .build_select();
    assert!(sql.to_uppercase().contains("INNER JOIN"));
}

#[test]
fn test_build_select_with_order_by() {
    let (sql, _) = builder().order_by("id").build_select();
    assert!(sql.to_uppercase().contains("ORDER BY"));
}

#[test]
fn test_build_select_with_limit() {
    let (sql, _) = builder().limit(10).build_select();
    assert!(sql.contains("LIMIT 10"));
}

#[test]
fn test_build_select_with_limit_offset() {
    let (sql, _) = builder().limit(10).offset(20).build_select();
    assert!(sql.contains("LIMIT 10"));
    assert!(sql.contains("OFFSET 20") || sql.contains("20"));
}

#[test]
fn test_build_select_with_group_by() {
    let (sql, _) = builder().group_by("id").build_select();
    assert!(sql.to_uppercase().contains("GROUP BY"));
}

#[test]
fn test_build_insert_basic() {
    let data = mk_data(&[("id", Value::I64(1)), ("name", Value::String("a".to_string()))]);
    let (sql, params) = builder().build_insert(&data);
    assert!(sql.to_uppercase().contains("INSERT INTO"));
    assert!(sql.contains("users"));
    assert!(!params.is_empty());
}

#[test]
fn test_build_insert_with_null() {
    let data = mk_data(&[("id", Value::I64(1)), ("name", Value::Null)]);
    let (sql, _) = builder().build_insert(&data);
    assert!(sql.to_uppercase().contains("INSERT INTO"));
}

#[test]
fn test_build_insert_multiple_fields() {
    let data = mk_data(&[
        ("id", Value::I64(1)),
        ("name", Value::String("test".to_string())),
        ("age", Value::I64(30)),
        ("active", Value::Bool(true)),
    ]);
    let (sql, params) = builder().build_insert(&data);
    assert!(sql.to_uppercase().contains("INSERT INTO"));
    assert_eq!(params.len(), 4);
}

#[test]
fn test_build_update_basic() {
    let data = mk_data(&[("name", Value::String("updated".to_string()))]);
    let (sql, params) = builder().where_eq("id", Value::I64(1)).build_update(&data);
    assert!(sql.to_uppercase().contains("UPDATE"));
    assert!(sql.contains("users"));
    assert!(!params.is_empty());
}

#[test]
fn test_build_update_multiple_fields() {
    let data = mk_data(&[
        ("name", Value::String("new".to_string())),
        ("age", Value::I64(25)),
    ]);
    let (sql, _) = builder().where_eq("id", Value::I64(1)).build_update(&data);
    assert!(sql.to_uppercase().contains("UPDATE"));
}

#[test]
fn test_build_update_with_where() {
    let data = mk_data(&[("name", Value::String("x".to_string()))]);
    let (sql, params) = builder()
        .where_eq("id", Value::I64(42))
        .build_update(&data);
    let sql_clean = sql.replace('`', "");
    assert!(sql_clean.contains("id ="));
    assert!(params.contains(&Value::I64(42)));
}

#[test]
fn test_build_delete_basic() {
    let (sql, _) = builder().build_delete();
    assert!(sql.to_uppercase().contains("DELETE FROM"));
    assert!(sql.contains("users"));
}

#[test]
fn test_build_delete_with_where() {
    let (sql, params) = builder().where_eq("id", Value::I64(1)).build_delete();
    let sql_clean = sql.replace('`', "");
    assert!(sql_clean.contains("id ="));
    assert!(params.contains(&Value::I64(1)));
}

#[test]
fn test_build_delete_with_complex_where() {
    let (sql, _) = builder()
        .where_eq("id", Value::I64(1))
        .or_where_eq("id", Value::I64(2))
        .build_delete();
    let sql_clean = sql.replace('`', "");
    assert!(sql_clean.to_uppercase().contains("OR"));
}

#[test]
fn test_build_select_with_where_like() {
    let (sql, _) = builder()
        .where_like("name", Value::String("%test%".to_string()))
        .build_select();
    let sql_clean = sql.replace('`', "");
    assert!(sql_clean.to_uppercase().contains("LIKE"));
}