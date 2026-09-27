//! v9.2.0 M2 T9: query.rs keyset 游标分页测试

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

#[test]
fn test_keyset_after_generates_forward_cursor() {
    let q = builder().keyset_after("id", Value::I64(100), 20);
    let (sql, params) = q.build_select();
    let sql_clean = sql.replace('`', "");
    assert!(sql_clean.contains("id >"));
    assert!(sql.to_uppercase().contains("ORDER BY"));
    assert!(sql.to_uppercase().contains("ASC"));
    assert!(sql.contains("LIMIT 20"));
    assert!(params.contains(&Value::I64(100)));
}

#[test]
fn test_keyset_before_generates_backward_cursor() {
    let q = builder().keyset_before("id", Value::I64(100), 20);
    let (sql, params) = q.build_select();
    let sql_clean = sql.replace('`', "");
    assert!(sql_clean.contains("id <"));
    assert!(sql.to_uppercase().contains("ORDER BY"));
    assert!(sql.to_uppercase().contains("DESC"));
    assert!(sql.contains("LIMIT 20"));
    assert!(params.contains(&Value::I64(100)));
}

#[test]
fn test_keyset_after_zero_value_boundary() {
    let q = builder().keyset_after("id", Value::I64(0), 10);
    let (sql, params) = q.build_select();
    assert!(sql.contains("LIMIT 10"));
    assert!(params.contains(&Value::I64(0)));
}