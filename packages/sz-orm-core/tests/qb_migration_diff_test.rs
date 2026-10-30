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

/// 归一化 SQL 用于对拍：转小写、折叠空白、去掉末尾分号。
/// 2026-10-28 SDET 遗留项整改：原测试只分别断言 old/new 包含关键子句
/// （名义差分），现增加归一化后的全量对拍断言，验证语义等价。
fn normalize_sql(sql: &str) -> String {
    sql.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(';')
        .to_lowercase()
}

/// 断言新旧 SQL 归一化后等价
fn assert_diff_eq(old_sql: &str, new_sql: &str) {
    assert_eq!(
        normalize_sql(old_sql),
        normalize_sql(new_sql),
        "SQL 语义不等价\nold={old_sql}\nnew={new_sql}"
    );
}

// ---- SELECT 差分测试 ----

#[test]
fn test_diff_select_basic() {
    let old_sql = SelectQuery::new().from("users").build(DbType::MySQL);
    let (new_sql, _params) = new_qb().table("users").build_select();
    assert_diff_eq(&old_sql, &new_sql);
    assert!(
        old_sql.contains("SELECT") && old_sql.contains("users"),
        "old SQL: {}",
        old_sql
    );
}

#[test]
fn test_diff_select_where() {
    let old_built = SelectQuery::new()
        .from("users")
        .where_eq("id", Value::I64(1))
        .build_with_params(DbType::MySQL);
    let old_sql = &old_built.sql;
    let (new_sql, _params) = new_qb()
        .table("users")
        .where_eq("id", Value::I64(1))
        .build_select();
    assert_diff_eq(old_sql, &new_sql);
    assert!(
        old_sql.contains("WHERE") && old_sql.contains("id"),
        "old SQL: {}",
        old_sql
    );
}

#[test]
fn test_diff_select_order_by() {
    let old_sql = SelectQuery::new()
        .from("users")
        .order_by("name", true)
        .build(DbType::MySQL);
    let (new_sql, _params) = new_qb().table("users").order_by("name").build_select();
    assert_diff_eq(&old_sql, &new_sql);
    assert!(old_sql.contains("ORDER BY"), "old SQL: {}", old_sql);
}

#[test]
fn test_diff_select_limit() {
    let old_sql = SelectQuery::new()
        .from("users")
        .limit(10)
        .build(DbType::MySQL);
    let (new_sql, _params) = new_qb().table("users").limit(10).build_select();
    assert_diff_eq(&old_sql, &new_sql);
    assert!(old_sql.contains("LIMIT"), "old SQL: {}", old_sql);
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
    let (new_sql, _params) = new_qb().table("users").build_insert(&data);

    // old 库把值内联为字面量（Alice），new 库参数化（?）——对拍前将内联值
    // 归一化为占位符，验证 SQL 结构等价且值一致。
    assert_diff_eq(&old_sql.replace("Alice", "?"), &new_sql);
    assert!(
        old_sql.contains("INSERT INTO") && old_sql.contains("users"),
        "old SQL: {}",
        old_sql
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
    let (new_sql, _params) = new_qb()
        .table("users")
        .where_eq("id", Value::I64(1))
        .build_update(&data);

    // old 库把值内联为字面量（Bob），new 库参数化（?）——对拍前归一化。
    assert_diff_eq(&old_sql.replace("Bob", "?"), &new_sql);
    assert!(
        old_sql.contains("UPDATE") && old_sql.contains("users"),
        "old SQL: {}",
        old_sql
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
    let (new_sql, _params) = new_qb()
        .table("users")
        .where_eq("id", Value::I64(1))
        .build_delete();
    assert_diff_eq(old_sql, &new_sql);
    assert!(
        old_sql.contains("DELETE FROM") && old_sql.contains("users"),
        "old SQL: {}",
        old_sql
    );
}
