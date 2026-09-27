//! v9.2.0 M2 T8: query.rs HAVING 聚合过滤测试

use sz_orm_core::{AggExpr, DbType, HavingOp, Model, ModelExt, QueryBuilder, Value, dialect::get_dialect};

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

#[test]
fn test_having_generates_having_clause() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let q = QueryBuilder::<User>::new(dialect)
        .having(AggExpr::CountStar, HavingOp::Gt, Value::I64(5))
        .unwrap();
    let sql = q.sql().to_uppercase();
    assert!(sql.contains("HAVING"));
    assert!(sql.contains("COUNT(*)"));
}