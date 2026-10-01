//! v9.2.0 M18: typed_ast Like/In/Not/BoolExpressionExt 覆盖（从内联测试迁出）

use sz_orm_core::dialect::MySqlDialect;
use sz_orm_core::typed::{TypedColumn, TypedTable};
use sz_orm_core::typed_ast::{
    And, BigInt, Bool, BoolExpressionExt, Double, Eq, In, Like, Not, Or, Text, TypedColumnExt,
    TypedExpression,
};

struct UsersTable;
impl TypedTable for UsersTable {
    const NAME: &'static str = "users";
}

struct ColId;
impl TypedColumn for ColId {
    const NAME: &'static str = "id";
    type Table = UsersTable;
    type RustType = i64;
    type SqlType = BigInt;
}

struct ColName;
impl TypedColumn for ColName {
    const NAME: &'static str = "name";
    type Table = UsersTable;
    type RustType = String;
    type SqlType = Text;
}

struct ColAge;
impl TypedColumn for ColAge {
    const NAME: &'static str = "age";
    type Table = UsersTable;
    type RustType = i64;
    type SqlType = BigInt;
}

#[test]
fn test_like_expression_sql() {
    let dialect = MySqlDialect;
    let expr = ColName.like("%abc%");
    let (sql, params) = expr.to_sql(&dialect);
    assert_eq!(sql, "`name` LIKE ?");
    assert_eq!(params, vec!["%abc%"]);
}

#[test]
fn test_like_new_direct() {
    let dialect = MySqlDialect;
    let expr = Like::new(ColName, "test");
    let (sql, params) = expr.to_sql(&dialect);
    assert_eq!(sql, "`name` LIKE ?");
    assert_eq!(params, vec!["test"]);
}

#[test]
fn test_in_expression_sql() {
    let dialect = MySqlDialect;
    let expr = ColId.in_(vec![1i64, 2, 3]);
    let (sql, params) = expr.to_sql(&dialect);
    assert_eq!(sql, "`id` IN (?, ?, ?)");
    assert_eq!(params, vec!["1", "2", "3"]);
}

#[test]
fn test_in_expression_empty() {
    let dialect = MySqlDialect;
    let expr = ColId.in_(Vec::<i64>::new());
    let (sql, params) = expr.to_sql(&dialect);
    assert_eq!(sql, "`id` IN ()");
    assert_eq!(params, Vec::<String>::new());
}

#[test]
fn test_in_new_direct() {
    let dialect = MySqlDialect;
    let expr = In::new(ColAge, vec![10i64, 20]);
    let (sql, params) = expr.to_sql(&dialect);
    assert_eq!(sql, "`age` IN (?, ?)");
    assert_eq!(params, vec!["10", "20"]);
}

#[test]
fn test_not_expression_sql() {
    let dialect = MySqlDialect;
    let expr = ColId.eq(1i64).not();
    let (sql, params) = expr.to_sql(&dialect);
    assert_eq!(sql, "NOT `id` = ?");
    assert_eq!(params, vec!["1"]);
}

#[test]
fn test_not_new_direct() {
    let dialect = MySqlDialect;
    let expr = Not::new(ColName.like("%test%"));
    let (sql, params) = expr.to_sql(&dialect);
    assert_eq!(sql, "NOT `name` LIKE ?");
    assert_eq!(params, vec!["%test%"]);
}

#[test]
fn test_bool_ext_and() {
    let dialect = MySqlDialect;
    let expr = ColId.eq(1i64).and(ColAge.gt(18i64));
    let (sql, params) = expr.to_sql(&dialect);
    assert_eq!(sql, "(`id` = ? AND `age` > ?)");
    assert_eq!(params, vec!["1", "18"]);
}

#[test]
fn test_bool_ext_or() {
    let dialect = MySqlDialect;
    let expr = ColId.eq(1i64).or(ColName.like("%admin%"));
    let (sql, params) = expr.to_sql(&dialect);
    assert_eq!(sql, "(`id` = ? OR `name` LIKE ?)");
    assert_eq!(params, vec!["1", "%admin%"]);
}

#[test]
fn test_bool_ext_not() {
    let dialect = MySqlDialect;
    let expr = ColId.eq(1i64).not();
    let (sql, _) = expr.to_sql(&dialect);
    assert_eq!(sql, "NOT `id` = ?");
}

#[test]
fn test_bool_ext_chain_and_or_not() {
    let dialect = MySqlDialect;
    let expr = ColId.eq(1i64).and(ColAge.gt(18i64)).or(ColName.like("%vip%"));
    let (sql, params) = expr.to_sql(&dialect);
    assert!(sql.contains("OR"));
    assert_eq!(params.len(), 3);
}

#[test]
fn test_like_with_integer_value() {
    let dialect = MySqlDialect;
    let expr = ColAge.like(100i64);
    let (sql, params) = expr.to_sql(&dialect);
    assert_eq!(sql, "`age` LIKE ?");
    assert_eq!(params, vec!["100"]);
}