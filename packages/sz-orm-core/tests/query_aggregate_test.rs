//! v9.2.0 M2 T12: query.rs 聚合查询测试

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

#[test]
fn test_build_count_generates_count_star() {
    let sql = builder().build_count().to_uppercase();
    assert!(sql.contains("COUNT(*)"));
}

#[test]
fn test_build_exists_generates_exists() {
    let sql = builder().build_exists().to_uppercase();
    assert!(sql.contains("EXISTS"));
}

#[test]
fn test_build_max_generates_max() {
    let sql = builder().build_max("score").to_uppercase();
    assert!(sql.contains("MAX"));
}

#[test]
fn test_build_min_generates_min() {
    let sql = builder().build_min("score").to_uppercase();
    assert!(sql.contains("MIN"));
}

#[test]
fn test_build_sum_generates_sum() {
    let sql = builder().build_sum("score").to_uppercase();
    assert!(sql.contains("SUM"));
}

#[test]
fn test_build_avg_generates_avg() {
    let sql = builder().build_avg("score").to_uppercase();
    assert!(sql.contains("AVG"));
}
