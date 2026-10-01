//! v9.2.0 M2 T7: query.rs JOIN 子句生成测试

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
fn test_join_inner_generates_inner_join_clause() {
    let q = builder().join_inner("posts", "users.id", "posts.user_id");
    let sql = q.sql().replace('`', "");
    assert!(sql.to_uppercase().contains("INNER JOIN"));
    assert!(sql.contains("posts"));
    assert!(sql.contains("users.id"));
    assert!(sql.contains("posts.user_id"));
}

#[test]
fn test_join_left_generates_left_join_clause() {
    let q = builder().join_left("orders", "users.id", "orders.user_id");
    let sql = q.sql().replace('`', "");
    assert!(sql.to_uppercase().contains("LEFT JOIN"));
    assert!(sql.contains("orders"));
}

#[test]
fn test_join_right_generates_right_join_clause() {
    let q = builder().join_right("profiles", "users.id", "profiles.user_id");
    let sql = q.sql().replace('`', "");
    assert!(sql.to_uppercase().contains("RIGHT JOIN"));
    assert!(sql.contains("profiles"));
}
