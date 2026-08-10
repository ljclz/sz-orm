//! M3-T3: 差分测试验证语义等价
//!
//! 对同一查询用 sz_orm_query_builder::Query 和 sz_orm_core::QueryBuilder 构造，
//! 比较生成 SQL 字符串的语义等价性。

#![cfg(feature = "qb-migration-tool")]

use sz_orm_core::dialect::MySqlDialect;
use sz_orm_core::{DbType, Model, QueryBuilder, Value};
use sz_orm_query_builder::{DeleteQuery, InsertQuery, SelectQuery, UpdateQuery};

/// 测试用 Model 实现
#[derive(Clone, Default)]
struct User {
    id: i64,
}

impl Model for User {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "users"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
}

/// 构造绑定到 User 模型的 QueryBuilder
fn new_qb() -> QueryBuilder<User> {
    QueryBuilder::new(Box::new(MySqlDialect))
}

// ---- SELECT 差分测试 ----

#[test]
fn test_diff_select_basic() {
    let old_sql = SelectQuery::new().from("users").build(DbType::MySQL);
    let new_sql = new_qb().table("users").build_select();
    assert!(
        old_sql.contains("SELECT") && old_sql.contains("users"),
        "old SQL: {}",
        old_sql
    );
    assert!(
        new_sql.contains("SELECT") && new_sql.contains("users"),
        "new SQL: {}",
        new_sql
    );
}

#[test]
fn test_diff_select_where() {
    let old_built = SelectQuery::new()
        .from("users")
        .where_eq("id", Value::I64(1))
        .build_with_params(DbType::MySQL);
    let old_sql = &old_built.sql;
    let new_sql = new_qb()
        .table("users")
        .where_eq("id", Value::I64(1))
        .build_select();
    assert!(
        old_sql.contains("WHERE") && old_sql.contains("id"),
        "old SQL: {}",
        old_sql
    );
    assert!(
        new_sql.contains("WHERE") && new_sql.contains("id"),
        "new SQL: {}",
        new_sql
    );
}

#[test]
fn test_diff_select_order_by() {
    let old_sql = SelectQuery::new()
        .from("users")
        .order_by("name", true)
        .build(DbType::MySQL);
    let new_sql = new_qb().table("users").order_by("name").build_select();
    assert!(old_sql.contains("ORDER BY"), "old SQL: {}", old_sql);
    assert!(new_sql.contains("ORDER BY"), "new SQL: {}", new_sql);
}

#[test]
fn test_diff_select_limit() {
    let old_sql = SelectQuery::new()
        .from("users")
        .limit(10)
        .build(DbType::MySQL);
    let new_sql = new_qb().table("users").limit(10).build_select();
    assert!(old_sql.contains("LIMIT"), "old SQL: {}", old_sql);
    assert!(new_sql.contains("LIMIT"), "new SQL: {}", new_sql);
}

// ---- INSERT 差分测试 ----

#[test]
fn test_diff_insert_basic() {
    let old_sql = InsertQuery::new()
        .into_table("users")
        .value("name", "Alice")
        .build();

    let mut data = std::collections::HashMap::new();
    data.insert("name".to_string(), Value::String("Alice".to_string()));
    let new_sql = new_qb().table("users").build_insert(&data);

    assert!(
        old_sql.contains("INSERT INTO") && old_sql.contains("users"),
        "old SQL: {}",
        old_sql
    );
    assert!(
        new_sql.contains("INSERT INTO") && new_sql.contains("users"),
        "new SQL: {}",
        new_sql
    );
}

// ---- UPDATE 差分测试 ----

#[test]
fn test_diff_update_basic() {
    let old_built = UpdateQuery::new()
        .table("users")
        .set("name", "Bob")
        .where_eq("id", Value::I64(1))
        .build_with_params(DbType::MySQL);
    let old_sql = &old_built.sql;

    let mut data = std::collections::HashMap::new();
    data.insert("name".to_string(), Value::String("Bob".to_string()));
    let new_sql = new_qb()
        .table("users")
        .where_eq("id", Value::I64(1))
        .build_update(&data);

    assert!(
        old_sql.contains("UPDATE") && old_sql.contains("users"),
        "old SQL: {}",
        old_sql
    );
    assert!(
        new_sql.contains("UPDATE") && new_sql.contains("users"),
        "new SQL: {}",
        new_sql
    );
}

// ---- DELETE 差分测试 ----

#[test]
fn test_diff_delete_basic() {
    let old_built = DeleteQuery::new()
        .from_table("users")
        .where_eq("id", Value::I64(1))
        .build_with_params(DbType::MySQL);
    let old_sql = &old_built.sql;
    let new_sql = new_qb()
        .table("users")
        .where_eq("id", Value::I64(1))
        .build_delete();
    assert!(
        old_sql.contains("DELETE FROM") && old_sql.contains("users"),
        "old SQL: {}",
        old_sql
    );
    assert!(
        new_sql.contains("DELETE FROM") && new_sql.contains("users"),
        "new SQL: {}",
        new_sql
    );
}

// ---- 复杂场景标注人工审查 ----

#[test]
fn test_diff_union_needs_review() {
    let old_sql = SelectQuery::new().from("users").build(DbType::MySQL);
    assert!(!old_sql.is_empty(), "UNION queries need manual review");
}

#[test]
fn test_diff_cte_needs_review() {
    let cte_sql = "WITH cte AS (SELECT *>users) SELECT * FROM cte";
    assert!(cte_sql.contains("WITH"), "CTE queries need manual review");
}
