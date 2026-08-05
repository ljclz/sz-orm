//! FromQueryResult derive 宏端到端集成测试
//!
//! 验证 `#[derive(FromQueryResult)]` 可正确将查询结果行映射到结构体，
//! 覆盖：基本类型、列名覆盖、Option  nullable、缺失列报错、额外列忽略。

use std::collections::HashMap;
use sz_orm_core::{FromQueryResult, Value};
use sz_orm_macros::FromQueryResult as FromQueryResultDerive;

/// 基本类型映射
#[derive(Debug, PartialEq, FromQueryResultDerive)]
struct BasicRow {
    id: i64,
    name: String,
    score: f64,
    active: bool,
}

/// 列名覆盖（#[column(name = "...")]）
#[derive(Debug, PartialEq, FromQueryResultDerive)]
struct RenamedRow {
    #[column(name = "user_id")]
    id: i64,
    #[column(name = "user_name")]
    name: String,
}

/// Option 字段（nullable 列）
#[derive(Debug, PartialEq, FromQueryResultDerive)]
struct NullableRow {
    id: i64,
    email: Option<String>,
    phone: Option<String>,
}

#[test]
fn from_query_result_basic_types() {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(42));
    row.insert("name".to_string(), Value::String("Alice".to_string()));
    row.insert("score".to_string(), Value::F64(95.5));
    row.insert("active".to_string(), Value::Bool(true));

    let r = BasicRow::from_query_result(&row).unwrap();
    assert_eq!(
        r,
        BasicRow {
            id: 42,
            name: "Alice".to_string(),
            score: 95.5,
            active: true,
        }
    );
}

#[test]
fn from_query_result_column_rename() {
    let mut row = HashMap::new();
    row.insert("user_id".to_string(), Value::I64(7));
    row.insert("user_name".to_string(), Value::String("Bob".to_string()));

    let r = RenamedRow::from_query_result(&row).unwrap();
    assert_eq!(r.id, 7);
    assert_eq!(r.name, "Bob");
}

#[test]
fn from_query_result_option_null_is_none() {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(1));
    row.insert("email".to_string(), Value::Null);
    row.insert("phone".to_string(), Value::String("123456".to_string()));

    let r = NullableRow::from_query_result(&row).unwrap();
    assert_eq!(r.id, 1);
    assert_eq!(r.email, None);
    assert_eq!(r.phone, Some("123456".to_string()));
}

#[test]
fn from_query_result_option_missing_key_is_none() {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(2));
    // email 和 phone 均缺失
    let r = NullableRow::from_query_result(&row).unwrap();
    assert_eq!(r.email, None);
    assert_eq!(r.phone, None);
}

#[test]
fn from_query_result_extra_columns_ignored() {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(99));
    row.insert("name".to_string(), Value::String("Charlie".to_string()));
    row.insert("score".to_string(), Value::F64(88.0));
    row.insert("active".to_string(), Value::Bool(false));
    row.insert(
        "extra_col".to_string(),
        Value::String("ignored".to_string()),
    );
    row.insert("another".to_string(), Value::I64(123));

    let r = BasicRow::from_query_result(&row).unwrap();
    assert_eq!(r.id, 99);
    assert_eq!(r.name, "Charlie");
}

#[test]
fn from_query_result_missing_required_column_errors() {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(1));
    // name 缺失 → 非 Option 字段应报错
    let result = BasicRow::from_query_result(&row);
    assert!(result.is_err(), "缺失非 Option 列应返回 Err");
}

#[test]
fn from_query_result_type_mismatch_errors() {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::String("not_an_int".to_string()));
    row.insert("name".to_string(), Value::String("Alice".to_string()));
    row.insert("score".to_string(), Value::F64(1.0));
    row.insert("active".to_string(), Value::Bool(true));

    let result = BasicRow::from_query_result(&row);
    assert!(result.is_err(), "类型不匹配应返回 Err");
    let err = result.unwrap_err();
    assert!(err.contains("id"), "错误信息应包含列名: {}", err);
}

#[test]
fn rows_to_helper() {
    use sz_orm_core::rows_to;
    use sz_orm_core::QueryRows;

    let mut row1 = HashMap::new();
    row1.insert("id".to_string(), Value::I64(1));
    row1.insert("name".to_string(), Value::String("Alice".to_string()));
    row1.insert("score".to_string(), Value::F64(90.0));
    row1.insert("active".to_string(), Value::Bool(true));

    let mut row2 = HashMap::new();
    row2.insert("id".to_string(), Value::I64(2));
    row2.insert("name".to_string(), Value::String("Bob".to_string()));
    row2.insert("score".to_string(), Value::F64(85.0));
    row2.insert("active".to_string(), Value::Bool(false));

    // QueryRows is Vec<HashMap<String, Value>>
    let rows: QueryRows = vec![row1, row2];
    let users: Vec<BasicRow> = rows_to::<BasicRow>(&rows).unwrap();
    assert_eq!(users.len(), 2);
    assert_eq!(users[0].name, "Alice");
    assert_eq!(users[1].id, 2);
}
