//! FromRow derive 宏端到端集成测试
//!
//! 验证 `#[derive(FromRow)]` 可正确将查询结果行映射到结构体，
//! 错误类型为 `QueryError`（含列索引/类型信息），适合底层精确错误定位。

use std::collections::HashMap;
use sz_orm_core::queryable::{FromRow, QueryError};
use sz_orm_core::Value;
use sz_orm_macros::FromRow as FromRowDerive;

/// 基本类型映射
#[derive(Debug, PartialEq, FromRowDerive)]
struct BasicFromRow {
    id: i64,
    name: String,
    score: f64,
    active: bool,
}

/// 列名覆盖（#[column(name = "...")]）
#[derive(Debug, PartialEq, FromRowDerive)]
struct RenamedFromRow {
    #[column(name = "user_id")]
    id: i64,
    #[column(name = "user_name")]
    name: String,
}

/// Option 字段（nullable 列）
#[derive(Debug, PartialEq, FromRowDerive)]
struct NullableFromRow {
    id: i64,
    email: Option<String>,
    phone: Option<String>,
}

#[test]
fn from_row_basic_types() {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(42));
    row.insert("name".to_string(), Value::String("Alice".to_string()));
    row.insert("score".to_string(), Value::F64(95.5));
    row.insert("active".to_string(), Value::Bool(true));

    let r = BasicFromRow::from_row(row).unwrap();
    assert_eq!(
        r,
        BasicFromRow {
            id: 42,
            name: "Alice".to_string(),
            score: 95.5,
            active: true,
        }
    );
}

#[test]
fn from_row_column_rename() {
    let mut row = HashMap::new();
    row.insert("user_id".to_string(), Value::I64(7));
    row.insert("user_name".to_string(), Value::String("Bob".to_string()));

    let r = RenamedFromRow::from_row(row).unwrap();
    assert_eq!(r.id, 7);
    assert_eq!(r.name, "Bob");
}

#[test]
fn from_row_option_null_is_none() {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(1));
    row.insert("email".to_string(), Value::Null);
    row.insert("phone".to_string(), Value::String("123456".to_string()));

    let r = NullableFromRow::from_row(row).unwrap();
    assert_eq!(r.id, 1);
    assert_eq!(r.email, None);
    assert_eq!(r.phone, Some("123456".to_string()));
}

#[test]
fn from_row_option_missing_key_is_none() {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(2));
    // email 和 phone 均缺失
    let r = NullableFromRow::from_row(row).unwrap();
    assert_eq!(r.email, None);
    assert_eq!(r.phone, None);
}

#[test]
fn from_row_extra_columns_ignored() {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(99));
    row.insert("name".to_string(), Value::String("Charlie".to_string()));
    row.insert("score".to_string(), Value::F64(88.0));
    row.insert("active".to_string(), Value::Bool(false));
    row.insert(
        "extra_col".to_string(),
        Value::String("ignored".to_string()),
    );

    let r = BasicFromRow::from_row(row).unwrap();
    assert_eq!(r.id, 99);
    assert_eq!(r.name, "Charlie");
}

#[test]
fn from_row_missing_required_column_errors() {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(1));
    // name 缺失 → 非 Option 字段应返回 MissingColumn
    let result = BasicFromRow::from_row(row);
    assert!(
        matches!(result, Err(QueryError::MissingColumn { column: "name" })),
        "缺失非 Option 列应返回 MissingColumn: {:?}",
        result
    );
}

#[test]
fn from_row_type_mismatch_errors() {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::String("not_an_int".to_string()));
    row.insert("name".to_string(), Value::String("Alice".to_string()));
    row.insert("score".to_string(), Value::F64(1.0));
    row.insert("active".to_string(), Value::Bool(true));

    let result = BasicFromRow::from_row(row);
    assert!(
        matches!(result, Err(QueryError::TypeMismatch { .. })),
        "类型不匹配应返回 TypeMismatch: {:?}",
        result
    );
}

#[test]
fn from_row_error_display() {
    let mut row = HashMap::new();
    row.insert("name".to_string(), Value::String("NoId".to_string()));
    let result = BasicFromRow::from_row(row);
    let err = result.unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("id"), "错误消息应包含列名: {}", msg);
}
