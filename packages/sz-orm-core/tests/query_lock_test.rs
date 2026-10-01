//! v9.2.0 M2 T11: query.rs FOR UPDATE / SHARED 锁测试

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

#[test]
fn test_lock_for_update_generates_for_update() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let q = QueryBuilder::<User>::new(dialect)
        .lock_for_update()
        .unwrap();
    let (sql, _) = q.build_select();
    assert!(sql.to_uppercase().contains("FOR UPDATE"));
}

#[test]
fn test_lock_shared_generates_share_lock() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let q = QueryBuilder::<User>::new(dialect).lock_shared().unwrap();
    let (sql, _) = q.build_select();
    let sql_upper = sql.to_uppercase();
    assert!(sql_upper.contains("FOR SHARE") || sql_upper.contains("LOCK IN SHARE MODE"));
}
