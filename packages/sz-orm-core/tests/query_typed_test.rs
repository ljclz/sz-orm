//! v9.2.0 M18: query.rs `*_typed` 系列方法覆盖（13 tests）

use sz_orm_core::{
    dialect::get_dialect, typed::TypedColumn, typed::TypedTable, DbType, QueryBuilder, Value,
};

// ---- mock table / column 类型 ----

struct MockUsersTable;
impl TypedTable for MockUsersTable {
    const NAME: &'static str = "users";
}

struct ColId;
impl TypedColumn for ColId {
    const NAME: &'static str = "id";
    type Table = MockUsersTable;
    type RustType = i64;
    type SqlType = sz_orm_core::typed_ast::Untyped;
}

struct ColName;
impl TypedColumn for ColName {
    const NAME: &'static str = "name";
    type Table = MockUsersTable;
    type RustType = String;
    type SqlType = sz_orm_core::typed_ast::Untyped;
}

struct ColAge;
impl TypedColumn for ColAge {
    const NAME: &'static str = "age";
    type Table = MockUsersTable;
    type RustType = i64;
    type SqlType = sz_orm_core::typed_ast::Untyped;
}

// ---- mock model ----

#[derive(Clone, Debug)]
#[allow(dead_code)] // 字段仅作为 Model schema 元数据，测试中不逐一读取
struct User {
    id: i64,
    name: String,
    age: i64,
}

impl sz_orm_core::Model for User {
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

fn qb() -> QueryBuilder<User> {
    QueryBuilder::<User>::new(get_dialect(DbType::MySQL).unwrap())
}

// ---- 13 个 *_typed 方法测试 ----

#[test]
fn test_where_eq_typed() {
    let q = qb().where_eq_typed::<ColId>(Value::from(1i64));
    let (sql, params) = q.build_select_with_params();
    assert!(sql.contains("WHERE"));
    assert!(sql.contains("id"));
    assert_eq!(params.len(), 1);
    assert_eq!(params[0], Value::from(1i64));
}

#[test]
fn test_where_ne_typed() {
    let q = qb().where_ne_typed::<ColId>(Value::from(5i64));
    let (sql, params) = q.build_select_with_params();
    assert!(sql.contains("WHERE"));
    assert!(sql.contains("!="));
    assert_eq!(params.len(), 1);
}

#[test]
fn test_where_gt_typed() {
    let q = qb().where_gt_typed::<ColAge>(Value::from(18i64));
    let (sql, params) = q.build_select_with_params();
    assert!(sql.contains("WHERE"));
    assert!(sql.contains(">"));
    assert_eq!(params.len(), 1);
}

#[test]
fn test_where_ge_typed() {
    let q = qb().where_ge_typed::<ColAge>(Value::from(21i64));
    let (sql, params) = q.build_select_with_params();
    assert!(sql.contains("WHERE"));
    assert!(sql.contains(">="));
    assert_eq!(params.len(), 1);
}

#[test]
fn test_where_lt_typed() {
    let q = qb().where_lt_typed::<ColAge>(Value::from(65i64));
    let (sql, params) = q.build_select_with_params();
    assert!(sql.contains("WHERE"));
    assert!(sql.contains("<"));
    assert_eq!(params.len(), 1);
}

#[test]
fn test_where_le_typed() {
    let q = qb().where_le_typed::<ColAge>(Value::from(30i64));
    let (sql, params) = q.build_select_with_params();
    assert!(sql.contains("WHERE"));
    assert!(sql.contains("<="));
    assert_eq!(params.len(), 1);
}

#[test]
fn test_where_null_typed() {
    let q = qb().where_null_typed::<ColName>();
    let (sql, params) = q.build_select_with_params();
    assert!(sql.contains("WHERE"));
    assert!(sql.contains("IS NULL"));
    assert_eq!(params.len(), 0);
}

#[test]
fn test_where_not_null_typed() {
    let q = qb().where_not_null_typed::<ColName>();
    let (sql, params) = q.build_select_with_params();
    assert!(sql.contains("WHERE"));
    assert!(sql.contains("IS NOT NULL"));
    assert_eq!(params.len(), 0);
}

#[test]
fn test_order_by_typed() {
    let q = qb().order_by_typed::<ColId>();
    let (sql, _) = q.build_select_with_params();
    assert!(sql.contains("ORDER BY"));
    assert!(sql.contains("id"));
    assert!(sql.contains("ASC"));
}

#[test]
fn test_order_desc_typed() {
    let q = qb().order_desc_typed::<ColId>();
    let (sql, _) = q.build_select_with_params();
    assert!(sql.contains("ORDER BY"));
    assert!(sql.contains("DESC"));
}

#[test]
fn test_group_by_typed() {
    let q = qb().group_by_typed::<ColAge>();
    let (sql, _) = q.build_select_with_params();
    assert!(sql.contains("GROUP BY"));
    assert!(sql.contains("age"));
}

#[test]
fn test_select_typed() {
    let q = qb().select_typed::<ColId>();
    let (sql, _) = q.build_select_with_params();
    assert!(sql.contains("SELECT"));
    assert!(sql.contains("id"));
}

#[test]
fn test_select_typed_cols() {
    let q = qb().select_typed_cols::<ColName, 1>();
    let (sql, _) = q.build_select_with_params();
    assert!(sql.contains("SELECT"));
    assert!(sql.contains("name"));
}

// ---- 链式组合测试 ----

#[test]
fn test_typed_chain_where_and_order() {
    let q = qb()
        .where_eq_typed::<ColId>(Value::from(42i64))
        .order_desc_typed::<ColAge>();
    let (sql, params) = q.build_select_with_params();
    assert!(sql.contains("WHERE"));
    assert!(sql.contains("ORDER BY"));
    assert_eq!(params.len(), 1);
}

#[test]
fn test_typed_chain_multi_where() {
    let q = qb()
        .where_gt_typed::<ColAge>(Value::from(18i64))
        .where_lt_typed::<ColAge>(Value::from(65i64));
    let (sql, params) = q.build_select_with_params();
    assert!(sql.contains("WHERE"));
    assert_eq!(params.len(), 2);
}

#[test]
fn test_typed_chain_select_multi() {
    let q = qb()
        .select_typed::<ColId>()
        .select_typed::<ColName>()
        .select_typed::<ColAge>();
    let (sql, _) = q.build_select_with_params();
    assert!(sql.contains("id"));
    assert!(sql.contains("name"));
    assert!(sql.contains("age"));
}

#[test]
fn test_typed_chain_group_and_order() {
    let q = qb().group_by_typed::<ColAge>().order_by_typed::<ColAge>();
    let (sql, _) = q.build_select_with_params();
    assert!(sql.contains("GROUP BY"));
    assert!(sql.contains("ORDER BY"));
}

#[test]
fn test_typed_null_and_not_null() {
    let q = qb()
        .where_null_typed::<ColName>()
        .where_not_null_typed::<ColId>();
    let (sql, params) = q.build_select_with_params();
    assert!(sql.contains("IS NULL"));
    assert!(sql.contains("IS NOT NULL"));
    assert_eq!(params.len(), 0);
}
