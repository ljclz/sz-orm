//! M4-T2.2: Schema derive 列名常量测试
//!
//! 验证启用 `type-safe-columns` feature 后，`#[derive(Schema)]` 为每个字段
//! 生成 `pub const FIELD_NAME: &'static str = "field_name"` 常量。

#![cfg(feature = "type-safe-columns")]

use sz_orm_macros::Schema;

#[derive(Schema, Debug, PartialEq, Clone)]
#[table(name = "users")]
struct User {
    #[column(primary_key)]
    id: i64,
    name: String,
    #[column(name = "user_email")]
    email: String,
    age: i32,
}

impl sz_orm_core::Model for User {
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
fn test_column_constants_exist() {
    assert_eq!(User::ID, "id");
    assert_eq!(User::NAME, "name");
    assert_eq!(User::EMAIL, "user_email");
    assert_eq!(User::AGE, "age");
}

#[test]
fn test_table_name_constant() {
    assert_eq!(User::SZ_ORM_TABLE_NAME, "users");
}

#[test]
fn test_column_constants_match_columns_method() {
    let columns = User::sz_orm_columns();
    let const_names = [User::ID, User::NAME, User::EMAIL, User::AGE];
    for (i, &const_name) in const_names.iter().enumerate() {
        assert_eq!(columns[i].0, const_name);
    }
}

#[derive(Schema, Debug, PartialEq, Clone)]
#[table(name = "orders")]
struct Order {
    #[column(primary_key)]
    order_id: i64,
    total: f64,
    #[column(skip)]
    internal_cache: String,
}

impl sz_orm_core::Model for Order {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "orders"
    }
    fn pk_name() -> &'static str {
        "order_id"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.order_id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.order_id = pk;
    }
}

#[test]
fn test_skip_field_not_generated() {
    assert_eq!(Order::ORDER_ID, "order_id");
    assert_eq!(Order::TOTAL, "total");
    assert_eq!(Order::sz_orm_column_count(), 2);
}

#[derive(Schema, Debug, PartialEq, Clone)]
#[table(name = "products")]
struct Product {
    id: i64,
    name: String,
    data: Vec<u8>,
}

impl sz_orm_core::Model for Product {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "products"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
}

#[test]
fn test_vec_u8_field() {
    assert_eq!(Product::ID, "id");
    assert_eq!(Product::NAME, "name");
    assert_eq!(Product::DATA, "data");
}

// M4-T3: Column<T> 类型安全列引用测试

use sz_orm_core::column::{Column, Schema};
use sz_orm_core::{DbType, QueryBuilder, Value};

#[test]
fn test_column_creation() {
    let col = Column::<User>::new("id");
    assert_eq!(col.name(), "id");
    assert_eq!(&*col, "id");
    assert_eq!(col.to_string(), "id");
}

#[test]
fn test_column_table_name() {
    assert_eq!(Column::<User>::table_name(), "users");
    assert_eq!(Column::<Order>::table_name(), "orders");
}

#[test]
fn test_column_deref_as_str() {
    let col = Column::<User>::new("name");
    let s: &str = &col;
    assert_eq!(s, "name");
    assert_eq!(col.as_ref(), "name");
}

#[test]
fn test_column_from_str() {
    let col: Column<User> = "email".into();
    assert_eq!(col.name(), "email");
}

#[test]
fn test_column_copy_clone() {
    let col1 = Column::<User>::new("id");
    let col2 = col1;
    let col3 = col1;
    assert_eq!(col1, col2);
    assert_eq!(col2, col3);
}

#[test]
fn test_column_with_schema_constants() {
    let id_col = Column::<User>::new(User::ID);
    let name_col = Column::<User>::new(User::NAME);
    assert_eq!(id_col.name(), "id");
    assert_eq!(name_col.name(), "name");
}

#[test]
fn test_where_eq_col_with_query_builder() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
    let col = Column::<User>::new("id");
    let q = QueryBuilder::<User>::new(dialect)
        .where_eq_col(col, Value::I64(42))
        .limit(10);
    let sql = q.build_select();
    assert!(sql.contains("id"));
    assert!(sql.contains("LIMIT 10"));
}

#[test]
fn test_where_eq_col_with_schema_constants() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
    let q = QueryBuilder::<User>::new(dialect)
        .where_eq_col(Column::<User>::new(User::ID), Value::I64(1))
        .where_eq_col(
            Column::<User>::new(User::NAME),
            Value::String("Alice".to_string()),
        );
    let sql = q.build_select();
    assert!(sql.contains("id"));
    assert!(sql.contains("name"));
}

#[test]
fn test_schema_trait_impl() {
    assert_eq!(User::schema_table_name(), "users");
    assert_eq!(Order::schema_table_name(), "orders");
    assert_eq!(Product::schema_table_name(), "products");
}

#[test]
fn test_column_type_safety_different_tables() {
    let user_col = Column::<User>::new("id");
    let order_col = Column::<Order>::new("order_id");
    assert_eq!(user_col.name(), "id");
    assert_eq!(order_col.name(), "order_id");
    assert_ne!(user_col, Column::<User>::new("name"));
}

// M4-T4.4: DSL 与 QueryBuilder 差分测试

use sz_orm_core::typed::{TypedColumn, TypedTable};
use sz_orm_core::typed_ast::{BoolExpressionExt, TypedColumnExt, TypedExpression, Untyped};

struct UsersTbl;
impl TypedTable for UsersTbl {
    const NAME: &'static str = "users";
}

struct UsersId;
impl TypedColumn for UsersId {
    const NAME: &'static str = "id";
    type Table = UsersTbl;
    type RustType = i64;
    type SqlType = Untyped;
}

struct UsersName;
impl TypedColumn for UsersName {
    const NAME: &'static str = "name";
    type Table = UsersTbl;
    type RustType = String;
    type SqlType = Untyped;
}

struct UsersAge;
impl TypedColumn for UsersAge {
    const NAME: &'static str = "age";
    type Table = UsersTbl;
    type RustType = i32;
    type SqlType = Untyped;
}

#[test]
fn test_dsl_eq_vs_query_builder() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
    let expr = UsersId.eq(42i64);
    let (dsl_sql, dsl_params) = expr.to_sql(&*dialect);
    assert!(dsl_sql.contains("id"));
    assert!(dsl_sql.contains("?"));
    assert_eq!(dsl_params, vec!["42".to_string()]);
}

#[test]
fn test_dsl_gt_vs_query_builder() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
    let expr = UsersAge.gt(18i32);
    let (dsl_sql, dsl_params) = expr.to_sql(&*dialect);
    assert!(dsl_sql.contains("age"));
    assert!(dsl_sql.contains(">"));
    assert_eq!(dsl_params, vec!["18".to_string()]);
}

#[test]
fn test_dsl_lt_vs_query_builder() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
    let expr = UsersAge.lt(100i32);
    let (dsl_sql, _) = expr.to_sql(&*dialect);
    assert!(dsl_sql.contains("age"));
    assert!(dsl_sql.contains("<"));
}

#[test]
fn test_dsl_like() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
    let expr = UsersName.like("%Alice%");
    let (dsl_sql, dsl_params) = expr.to_sql(&*dialect);
    assert!(dsl_sql.contains("LIKE"));
    assert_eq!(dsl_params, vec!["%Alice%".to_string()]);
}

#[test]
fn test_dsl_in() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
    let expr = UsersId.in_(vec![1i64, 2, 3]);
    let (dsl_sql, dsl_params) = expr.to_sql(&*dialect);
    assert!(dsl_sql.contains("IN"));
    assert!(dsl_sql.contains("?, ?, ?"));
    assert_eq!(dsl_params.len(), 3);
}

#[test]
fn test_dsl_and_combination() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
    let expr = UsersId.eq(1i64).and(UsersName.like("%test%"));
    let (dsl_sql, params) = expr.to_sql(&*dialect);
    assert!(dsl_sql.contains("AND"));
    assert_eq!(params.len(), 2);
}

#[test]
fn test_dsl_or_combination() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
    let expr = UsersId.eq(1i64).or(UsersId.eq(2i64));
    let (dsl_sql, params) = expr.to_sql(&*dialect);
    assert!(dsl_sql.contains("OR"));
    assert_eq!(params.len(), 2);
}

#[test]
fn test_dsl_not() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
    let expr = UsersId.eq(1i64).not();
    let (dsl_sql, _) = expr.to_sql(&*dialect);
    assert!(dsl_sql.contains("NOT"));
}

#[test]
fn test_where_expr_with_query_builder() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
    let expr = UsersId.eq(42i64).and(UsersAge.gt(18i32));
    let q = QueryBuilder::<User>::new(dialect)
        .where_expr(expr)
        .limit(10);
    let sql = q.build_select();
    assert!(sql.contains("id"));
    assert!(sql.contains("age"));
    assert!(sql.contains("AND"));
    assert!(sql.contains("LIMIT 10"));
}

#[test]
fn test_dsl_complex_expression() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
    let expr = UsersId
        .eq(1i64)
        .and(UsersName.like("%Alice%"))
        .or(UsersAge.gt(30i32));
    let (dsl_sql, params) = expr.to_sql(&*dialect);
    assert!(dsl_sql.contains("AND"));
    assert!(dsl_sql.contains("OR"));
    assert_eq!(params.len(), 3);
}

// M4-T5.2: SQL 注入安全验证

#[test]
fn test_sql_injection_safety_where_eq_col() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
    let malicious = "'; DROP TABLE users; --";
    let q = QueryBuilder::<User>::new(dialect).where_eq_col(
        Column::<User>::new("name"),
        Value::String(malicious.to_string()),
    );
    let (sql, params) = q.build_select_with_params();
    assert!(sql.contains("?"), "SQL must use parameterized placeholders");
    assert!(
        !sql.contains("DROP TABLE"),
        "SQL must not contain injected content"
    );
    assert!(!sql.contains(malicious), "Value must not be inlined in SQL");
    assert!(params
        .iter()
        .any(|v| matches!(v, Value::String(s) if s == malicious)));
}

#[test]
fn test_sql_injection_safety_where_expr() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
    let malicious = "' OR '1'='1";
    let expr = UsersName.eq(malicious.to_string());
    let (dsl_sql, dsl_params) = expr.to_sql(&*dialect);
    assert!(
        dsl_sql.contains("?"),
        "DSL SQL must use parameterized placeholders"
    );
    assert!(
        !dsl_sql.contains("OR"),
        "DSL SQL must not contain injected content"
    );
    assert!(
        !dsl_sql.contains(malicious),
        "Value must not be inlined in DSL SQL"
    );
    assert!(dsl_params.iter().any(|p| p == malicious));
}

#[test]
fn test_sql_injection_safety_typed_in() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
    let malicious_values = vec!["1; DROP TABLE--".to_string(), "2' OR '1'='1".to_string()];
    let expr = UsersName.in_(malicious_values.clone());
    let (dsl_sql, dsl_params) = expr.to_sql(&*dialect);
    assert!(dsl_sql.contains("?"), "IN expression must use placeholders");
    assert!(
        !dsl_sql.contains("DROP TABLE"),
        "IN expression must not contain injected content"
    );
    assert!(
        !dsl_sql.contains("OR"),
        "IN expression must not contain injected content"
    );
    assert_eq!(dsl_params.len(), 2);
    assert!(dsl_params.iter().all(|p| malicious_values.contains(p)));
}

#[test]
fn test_sql_injection_safety_typed_like() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
    let malicious = "%'; DROP TABLE users; --";
    let expr = UsersName.like(malicious.to_string());
    let (dsl_sql, dsl_params) = expr.to_sql(&*dialect);
    assert!(
        dsl_sql.contains("?"),
        "LIKE expression must use placeholders"
    );
    assert!(
        !dsl_sql.contains("DROP TABLE"),
        "LIKE expression must not contain injected content"
    );
    assert_eq!(dsl_params, vec![malicious.to_string()]);
}

#[test]
fn test_parameterized_where_expr_with_query_builder() {
    let dialect = sz_orm_core::get_dialect(DbType::MySQL).unwrap();
    let expr = UsersId.eq(42i64).and(UsersName.like("%test%"));
    let q = QueryBuilder::<User>::new(dialect).where_expr(expr);
    let (sql, params) = q.build_select_with_params();
    assert!(
        sql.contains("?"),
        "Query must use parameterized placeholders"
    );
    assert!(!sql.contains("42"), "Integer value must not be inlined");
    assert!(!sql.contains("%test%"), "String value must not be inlined");
    assert!(params.len() >= 2);
}
