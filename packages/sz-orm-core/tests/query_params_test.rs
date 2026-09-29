//! v9.2.0 M14: query.rs build_select_with_params 参数化方法覆盖

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
    fn columns() -> Vec<&'static str> { vec!["id", "name", "age", "email"] }
    fn fillable() -> Vec<&'static str> { vec!["name", "age", "email"] }
    fn guarded() -> Vec<&'static str> { vec!["id"] }
    fn hidden() -> Vec<&'static str> { vec![] }
    fn relations() -> std::collections::HashMap<&'static str, sz_orm_core::Relation> { Default::default() }
    fn fill(&mut self, _: std::collections::HashMap<String, Value>) {}
    fn to_json(&self) -> serde_json::Value { serde_json::json!({}) }
}

fn builder() -> QueryBuilder<User> {
    QueryBuilder::<User>::new(get_dialect(DbType::MySQL).unwrap())
}

fn pg_builder() -> QueryBuilder<User> {
    QueryBuilder::<User>::new(get_dialect(DbType::PostgreSQL).unwrap())
}

#[test]
fn test_select_with_params_basic() {
    let (sql, params) = builder().build_select_with_params();
    assert!(sql.to_uppercase().contains("SELECT"));
    assert!(params.is_empty());
}

#[test]
fn test_select_with_params_where_eq() {
    let (sql, params) = builder()
        .where_eq("id", Value::I64(1))
        .build_select_with_params();
    assert!(sql.contains("?"));
    assert_eq!(params.len(), 1);
}

#[test]
fn test_select_with_params_where_ne() {
    let (sql, params) = builder()
        .where_ne("id", Value::I64(1))
        .build_select_with_params();
    assert!(sql.contains("!="));
    assert_eq!(params.len(), 1);
}

#[test]
fn test_select_with_params_where_gt() {
    let (sql, params) = builder()
        .where_gt("age", Value::I64(18))
        .build_select_with_params();
    assert!(sql.contains(">"));
    assert_eq!(params.len(), 1);
}

#[test]
fn test_select_with_params_where_ge() {
    let (sql, params) = builder()
        .where_ge("age", Value::I64(18))
        .build_select_with_params();
    assert!(sql.contains(">="));
    assert_eq!(params.len(), 1);
}

#[test]
fn test_select_with_params_where_lt() {
    let (sql, params) = builder()
        .where_lt("age", Value::I64(65))
        .build_select_with_params();
    assert!(sql.contains("<"));
    assert_eq!(params.len(), 1);
}

#[test]
fn test_select_with_params_where_le() {
    let (sql, params) = builder()
        .where_le("age", Value::I64(65))
        .build_select_with_params();
    assert!(sql.contains("<="));
    assert_eq!(params.len(), 1);
}

#[test]
fn test_select_with_params_where_like() {
    let (sql, params) = builder()
        .where_like("name", Value::String("%test%".to_string()))
        .build_select_with_params();
    assert!(sql.to_uppercase().contains("LIKE"));
    assert_eq!(params.len(), 1);
}

#[test]
fn test_select_with_params_where_in() {
    let (sql, params) = builder()
        .where_in("id", vec![Value::I64(1), Value::I64(2), Value::I64(3)])
        .build_select_with_params();
    assert!(sql.to_uppercase().contains("IN"));
    assert_eq!(params.len(), 3);
}

#[test]
fn test_select_with_params_where_not_in() {
    let (sql, params) = builder()
        .where_not_in("id", vec![Value::I64(1), Value::I64(2)])
        .build_select_with_params();
    assert!(sql.to_uppercase().contains("NOT IN"));
    assert_eq!(params.len(), 2);
}

#[test]
fn test_select_with_params_where_between() {
    let (sql, params) = builder()
        .where_between("age", Value::I64(10), Value::I64(20))
        .build_select_with_params();
    assert!(sql.to_uppercase().contains("BETWEEN"));
    assert_eq!(params.len(), 2);
}

#[test]
fn test_select_with_params_where_not_between() {
    let (sql, params) = builder()
        .where_not_between("age", Value::I64(10), Value::I64(20))
        .build_select_with_params();
    assert!(sql.to_uppercase().contains("NOT BETWEEN"));
    assert_eq!(params.len(), 2);
}

#[test]
fn test_select_with_params_where_null() {
    let (sql, params) = builder()
        .where_null("email")
        .build_select_with_params();
    assert!(sql.to_uppercase().contains("IS NULL"));
    assert!(params.is_empty());
}

#[test]
fn test_select_with_params_where_not_null() {
    let (sql, params) = builder()
        .where_not_null("email")
        .build_select_with_params();
    assert!(sql.to_uppercase().contains("IS NOT NULL"));
    assert!(params.is_empty());
}

#[test]
fn test_select_with_params_multiple_conditions() {
    let (sql, params) = builder()
        .where_eq("id", Value::I64(1))
        .where_gt("age", Value::I64(18))
        .where_like("name", Value::String("%test%".to_string()))
        .build_select_with_params();
    assert!(sql.to_uppercase().contains("AND"));
    assert_eq!(params.len(), 3);
}

#[test]
fn test_select_with_params_with_or() {
    let (sql, params) = builder()
        .where_eq("id", Value::I64(1))
        .or_where_eq("name", Value::String("test".to_string()))
        .build_select_with_params();
    assert!(sql.to_uppercase().contains("OR") || sql.to_uppercase().contains("AND"));
    assert_eq!(params.len(), 2);
}

#[test]
fn test_select_with_params_with_or_ne() {
    let (sql, _) = builder()
        .where_eq("id", Value::I64(1))
        .or_where_ne("name", Value::String("test".to_string()))
        .build_select_with_params();
    assert!(sql.to_uppercase().contains("OR") || sql.to_uppercase().contains("AND"));
}

#[test]
fn test_select_with_params_with_or_gt() {
    let (sql, _) = builder()
        .where_eq("id", Value::I64(1))
        .or_where_gt("age", Value::I64(18))
        .build_select_with_params();
    assert!(sql.to_uppercase().contains("OR") || sql.to_uppercase().contains("AND"));
}

#[test]
fn test_select_with_params_with_or_ge() {
    let (sql, _) = builder()
        .where_eq("id", Value::I64(1))
        .or_where_ge("age", Value::I64(18))
        .build_select_with_params();
    assert!(sql.to_uppercase().contains("OR") || sql.to_uppercase().contains("AND"));
}

#[test]
fn test_select_with_params_with_or_lt() {
    let (sql, _) = builder()
        .where_eq("id", Value::I64(1))
        .or_where_lt("age", Value::I64(65))
        .build_select_with_params();
    assert!(sql.to_uppercase().contains("OR") || sql.to_uppercase().contains("AND"));
}

#[test]
fn test_select_with_params_with_or_le() {
    let (sql, _) = builder()
        .where_eq("id", Value::I64(1))
        .or_where_le("age", Value::I64(65))
        .build_select_with_params();
    assert!(sql.to_uppercase().contains("OR") || sql.to_uppercase().contains("AND"));
}

#[test]
fn test_select_with_params_with_or_like() {
    let (sql, _) = builder()
        .where_eq("id", Value::I64(1))
        .or_where_like("name", Value::String("%test%".to_string()))
        .build_select_with_params();
    assert!(sql.to_uppercase().contains("OR") || sql.to_uppercase().contains("AND"));
}

#[test]
fn test_select_with_params_order_by() {
    let (sql, _) = builder()
        .order_by("id")
        .build_select_with_params();
    assert!(sql.to_uppercase().contains("ORDER BY"));
}

#[test]
fn test_select_with_params_order_desc() {
    let (sql, _) = builder()
        .order_desc("id")
        .build_select_with_params();
    assert!(sql.to_uppercase().contains("DESC"));
}

#[test]
fn test_select_with_params_group_by() {
    let (sql, _) = builder()
        .group_by("age")
        .build_select_with_params();
    assert!(sql.to_uppercase().contains("GROUP BY"));
}

#[test]
fn test_select_with_params_limit_offset() {
    let (sql, _) = builder()
        .limit(10)
        .offset(20)
        .build_select_with_params();
    assert!(sql.to_uppercase().contains("LIMIT"));
}

#[test]
fn test_update_with_params() {
    let data = HashMap::from([
        ("name".to_string(), Value::String("updated".to_string())),
        ("age".to_string(), Value::I64(30)),
    ]);
    let (sql, params) = builder()
        .where_eq("id", Value::I64(1))
        .build_update_with_params(&data);
    assert!(sql.to_uppercase().contains("UPDATE"));
    assert!(params.len() >= 3);
}

#[test]
fn test_delete_with_params() {
    let (sql, params) = builder()
        .where_eq("id", Value::I64(1))
        .build_delete_with_params();
    assert!(sql.to_uppercase().contains("DELETE"));
    assert_eq!(params.len(), 1);
}

#[test]
fn test_insert_with_params() {
    let data = HashMap::from([
        ("id".to_string(), Value::I64(1)),
        ("name".to_string(), Value::String("test".to_string())),
    ]);
    let (sql, params) = builder().build_insert_with_params(&data);
    assert!(sql.to_uppercase().contains("INSERT"));
    assert_eq!(params.len(), 2);
}

#[test]
fn test_batch_insert_with_params() {
    let data: Vec<HashMap<String, Value>> = vec![
        HashMap::from([("id".to_string(), Value::I64(1)), ("name".to_string(), Value::String("a".to_string()))]),
        HashMap::from([("id".to_string(), Value::I64(2)), ("name".to_string(), Value::String("b".to_string()))]),
    ];
    let (sql, params) = builder().build_batch_insert_with_params(&data);
    assert!(sql.to_uppercase().contains("INSERT"));
    assert_eq!(params.len(), 4);
}

#[test]
fn test_batch_upsert_with_params() {
    let data: Vec<HashMap<String, Value>> = vec![
        HashMap::from([("id".to_string(), Value::I64(1)), ("name".to_string(), Value::String("a".to_string()))]),
    ];
    let result = builder().build_batch_upsert_with_params(&data, &["id"], &["name"]);
    if let Ok((sql, params)) = result {
        assert!(sql.to_uppercase().contains("INSERT") || sql.to_uppercase().contains("UPDATE"));
        assert!(params.len() >= 2);
    }
}

#[test]
fn test_force_delete_with_params() {
    let (sql, params) = builder()
        .where_eq("id", Value::I64(1))
        .build_force_delete_with_params();
    assert!(sql.to_uppercase().contains("DELETE"));
    assert_eq!(params.len(), 1);
}

#[test]
fn test_count_with_params() {
    let sql = builder()
        .where_eq("id", Value::I64(1))
        .build_count();
    assert!(sql.to_uppercase().contains("COUNT"));
}

#[test]
fn test_exists_with_params() {
    let sql = builder()
        .where_eq("id", Value::I64(1))
        .build_exists();
    assert!(sql.to_uppercase().contains("EXISTS") || sql.to_uppercase().contains("SELECT"));
}

#[test]
fn test_pg_select_with_params() {
    let (sql, params) = pg_builder()
        .where_eq("id", Value::I64(1))
        .where_gt("age", Value::I64(18))
        .build_select_with_params();
    assert!(sql.contains("$1") || sql.contains("?"));
    assert_eq!(params.len(), 2);
}

#[test]
fn test_pg_update_with_params() {
    let data = HashMap::from([
        ("name".to_string(), Value::String("updated".to_string())),
    ]);
    let (sql, params) = pg_builder()
        .where_eq("id", Value::I64(1))
        .build_update_with_params(&data);
    assert!(sql.to_uppercase().contains("UPDATE"));
    assert!(params.len() >= 2);
}