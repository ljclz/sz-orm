//! M2 验证：Partial Models 部分字段选择
//!
//! 测试 select_only / column / columns / column_as 方法

use sz_orm_core::dialect::get_dialect;
use sz_orm_core::partial_model::{AggFunc, Expr, SelectMode};
use sz_orm_core::DbType;
use sz_orm_core::Model;
use sz_orm_core::QueryBuilder;

#[derive(Clone, Default)]
#[allow(dead_code)]
struct User {
    id: i64,
    name: String,
    email: String,
    age: i32,
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

#[test]
fn test_select_only_basic() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let (sql, _params) = QueryBuilder::<User>::new(dialect)
        .table("users")
        .select_only()
        .column("id")
        .column("name")
        .build_select();
    assert!(
        sql.contains("SELECT id, name FROM"),
        "expected SELECT id, name FROM, got: {}",
        sql
    );
    assert!(!sql.contains("*"), "should not contain SELECT *");
}

#[test]
fn test_select_only_with_columns_vec() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let (sql, _params) = QueryBuilder::<User>::new(dialect)
        .table("users")
        .select_only()
        .columns(vec!["id", "name", "email"])
        .build_select();
    assert!(sql.contains("SELECT id, name, email FROM"));
}

#[test]
fn test_column_as_aggregation() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let (sql, _params) = QueryBuilder::<User>::new(dialect)
        .table("users")
        .select_only()
        .column_as(Expr::count("id"), "total")
        .build_select();
    assert!(
        sql.contains("SELECT COUNT(id) AS total FROM"),
        "expected SELECT COUNT(id) AS total FROM, got: {}",
        sql
    );
}

#[test]
fn test_column_as_sum() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let (sql, _params) = QueryBuilder::<User>::new(dialect)
        .table("users")
        .select_only()
        .column_as(Expr::sum("age"), "total_age")
        .build_select();
    assert!(sql.contains("SUM(age) AS total_age"));
}

#[test]
fn test_select_only_with_group_by() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let (sql, _params) = QueryBuilder::<User>::new(dialect)
        .table("users")
        .select_only()
        .column("age")
        .column_as(Expr::count("id"), "count")
        .group_by("age")
        .build_select();
    assert!(sql.contains("SELECT age, COUNT(id) AS count FROM"));
    assert!(sql.contains("GROUP BY"));
    assert!(sql.contains("age"));
}

#[test]
fn test_select_only_no_column_falls_back_to_star() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let (sql, _params) = QueryBuilder::<User>::new(dialect)
        .table("users")
        .select_only()
        .build_select();
    assert!(
        sql.contains("SELECT * FROM"),
        "select_only without columns falls back to SELECT *, got: {}",
        sql
    );
}

#[test]
fn test_select_mode_default() {
    assert_eq!(SelectMode::default(), SelectMode::All);
}

#[test]
fn test_agg_func_rendering() {
    assert_eq!(AggFunc::Count.as_sql(), "COUNT");
    assert_eq!(AggFunc::Sum.as_sql(), "SUM");
    assert_eq!(AggFunc::Avg.as_sql(), "AVG");
    assert_eq!(AggFunc::Max.as_sql(), "MAX");
    assert_eq!(AggFunc::Min.as_sql(), "MIN");
}

#[test]
fn test_expr_render_as() {
    let expr = Expr::count("id");
    assert_eq!(expr.render_as("total"), "COUNT(id) AS total");

    let expr = Expr::max("price");
    assert_eq!(expr.render_as("max_price"), "MAX(price) AS max_price");
}

#[test]
fn test_select_only_postgresql() {
    let dialect = get_dialect(DbType::PostgreSQL).unwrap();
    let (sql, _params) = QueryBuilder::<User>::new(dialect)
        .table("users")
        .select_only()
        .column("id")
        .column("name")
        .build_select();
    assert!(sql.contains("SELECT id, name FROM"));
    assert!(sql.contains("\"users\""));
}

#[test]
fn test_select_only_with_where() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let (sql, _params) = QueryBuilder::<User>::new(dialect)
        .table("users")
        .select_only()
        .column("id")
        .column("name")
        .where_eq("age", sz_orm_core::Value::I32(18))
        .build_select();
    assert!(sql.contains("SELECT id, name FROM"));
    assert!(sql.contains("WHERE"));
}
