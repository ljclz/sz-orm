//! v9.2.0 M2 T13: query.rs 批量操作测试

use std::collections::HashMap;
use sz_orm_core::{dialect::get_dialect, DbType, Model, ModelExt, QueryBuilder, Value};

#[derive(Clone, Debug)]
struct User {
    id: i64,
}
impl Model for User {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "users"
    }
    fn pk(&self) -> i64 {
        self.id
    }
    fn set_pk(&mut self, pk: i64) {
        self.id = pk;
    }
}
impl ModelExt for User {
    fn columns() -> Vec<&'static str> {
        vec!["id"]
    }
    fn fillable() -> Vec<&'static str> {
        vec![]
    }
    fn guarded() -> Vec<&'static str> {
        vec!["id"]
    }
    fn hidden() -> Vec<&'static str> {
        vec![]
    }
    fn relations() -> std::collections::HashMap<&'static str, sz_orm_core::Relation> {
        Default::default()
    }
    fn fill(&mut self, _: std::collections::HashMap<String, Value>) {}
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({})
    }
}

fn builder() -> QueryBuilder<User> {
    QueryBuilder::<User>::new(get_dialect(DbType::MySQL).unwrap())
}

fn mk_row(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[test]
fn test_build_batch_insert_with_params_multiple_rows() {
    let rows = vec![
        mk_row(&[
            ("id", Value::I64(1)),
            ("name", Value::String("a".to_string())),
        ]),
        mk_row(&[
            ("id", Value::I64(2)),
            ("name", Value::String("b".to_string())),
        ]),
    ];
    let (sql, params) = builder().build_batch_insert_with_params(&rows);
    assert!(sql.to_uppercase().contains("INSERT INTO"));
    assert_eq!(params.len(), 4);
}

#[test]
fn test_build_batch_insert_with_params_empty_rows() {
    let rows: Vec<HashMap<String, Value>> = vec![];
    let (sql, params) = builder().build_batch_insert_with_params(&rows);
    assert!(params.is_empty());
    assert!(sql.is_empty() || sql.to_uppercase().contains("INSERT"));
}

#[test]
fn test_build_batch_upsert_with_params() {
    let rows = vec![mk_row(&[
        ("id", Value::I64(1)),
        ("name", Value::String("a".to_string())),
    ])];
    let result = builder().build_batch_upsert_with_params(&rows, &["id"], &["name"]);
    assert!(result.is_ok());
    let (sql, params) = result.unwrap();
    assert!(sql.to_uppercase().contains("INSERT INTO"));
    assert!(!params.is_empty());
}
