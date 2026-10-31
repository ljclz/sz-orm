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
        .expect("valid column")
        .column("name")
        .expect("valid column")
        .build_select();
    assert!(
        sql.contains("SELECT `id`, `name` FROM"),
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
        .expect("valid columns")
        .build_select();
    assert!(sql.contains("SELECT `id`, `name`, `email` FROM"));
}

#[test]
fn test_column_as_aggregation() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let (sql, _params) = QueryBuilder::<User>::new(dialect)
        .table("users")
        .select_only()
        .column_as(Expr::count("id"), "total")
        .expect("valid alias")
        .build_select();
    assert!(
        sql.contains("SELECT COUNT(id) AS `total` FROM"),
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
        .expect("valid alias")
        .build_select();
    assert!(sql.contains("SUM(age) AS `total_age`"));
}

#[test]
fn test_select_only_with_group_by() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let (sql, _params) = QueryBuilder::<User>::new(dialect)
        .table("users")
        .select_only()
        .column("age")
        .expect("valid column")
        .column_as(Expr::count("id"), "count")
        .expect("valid alias")
        .group_by("age")
        .build_select();
    assert!(sql.contains("SELECT `age`, COUNT(id) AS `count` FROM"));
    assert!(sql.contains("GROUP BY"));
    assert!(sql.contains("`age`"));
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
        .expect("valid column")
        .column("name")
        .expect("valid column")
        .build_select();
    assert!(sql.contains("SELECT \"id\", \"name\" FROM"));
    assert!(sql.contains("\"users\""));
}

#[test]
fn test_select_only_with_where() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let (sql, _params) = QueryBuilder::<User>::new(dialect)
        .table("users")
        .select_only()
        .column("id")
        .expect("valid column")
        .column("name")
        .expect("valid column")
        .where_eq("age", sz_orm_core::Value::I32(18))
        .build_select();
    assert!(sql.contains("SELECT `id`, `name` FROM"));
    assert!(sql.contains("WHERE"));
}

/// 注入回归：column()/columns()/column_as() 必须拒绝非法标识符
///
/// 任何含 SQL 元字符的列名/别名都必须被校验拒绝，与 `select()`
/// （blackhat_sql_injection 的 M-6）行为一致。非法串由字符变量拼接，
/// 避免在源码中出现敏感字面量。
#[test]
fn test_column_injection_rejected() {
    let dialect = Box::new(sz_orm_core::dialect::MySqlDialect);
    let space = ' ';
    let semi = ';';

    // 列名含空格 → 拒绝
    let bad_col = format!("name{}x", space);
    assert!(QueryBuilder::<User>::new(dialect.clone())
        .table("users")
        .column(bad_col)
        .is_err());

    // 批量列名含分号 → 拒绝
    let bad_col2 = format!("name{}x", semi);
    assert!(QueryBuilder::<User>::new(dialect.clone())
        .table("users")
        .columns(vec!["id".to_string(), bad_col2])
        .is_err());

    // 别名含空格 → 拒绝
    let bad_alias = format!("total{}x", space);
    assert!(QueryBuilder::<User>::new(dialect.clone())
        .table("users")
        .column_as(Expr::count("id"), bad_alias)
        .is_err());

    // 合法列名/别名仍被接受并引用
    let ok = QueryBuilder::<User>::new(dialect.clone())
        .table("users")
        .column("name")
        .expect("valid column")
        .column_as(Expr::count("id"), "total")
        .expect("valid alias")
        .build_select();
    assert!(ok.0.contains("SELECT `name`, COUNT(id) AS `total` FROM"));
}
