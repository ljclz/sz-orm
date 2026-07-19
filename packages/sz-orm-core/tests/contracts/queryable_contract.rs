//! Queryable 模块契约测试 — 对应 `docs/api-contracts.md` §17
//!
//! 锁定 Queryable / FromRow trait 契约、QueryError 变体、列数/类型/缺失列错误。

use std::collections::HashMap;
use sz_orm_core::queryable::{FromRow, QueryError, Queryable, RowDesc};
use sz_orm_core::Value;

// ===== §17.1 Queryable for Value 契约 =====

#[test]
fn test_queryable_for_value_single_column_contract() {
    let v = Value::from_values(vec![Value::I64(42)]).unwrap();
    assert_eq!(v.as_i64(), Some(42));
}

#[test]
fn test_queryable_for_value_rejects_zero_columns_contract() {
    let result = Value::from_values(vec![]);
    assert!(result.is_err());
    match result.unwrap_err() {
        QueryError::ColumnCountMismatch {
            expected: 1,
            actual: 0,
        } => {}
        other => panic!("期望 ColumnCountMismatch {{1, 0}}，实际: {:?}", other),
    }
}

#[test]
fn test_queryable_for_value_rejects_two_columns_contract() {
    let result = Value::from_values(vec![Value::I64(1), Value::I64(2)]);
    assert!(result.is_err());
    match result.unwrap_err() {
        QueryError::ColumnCountMismatch {
            expected: 1,
            actual: 2,
        } => {}
        other => panic!("期望 ColumnCountMismatch {{1, 2}}，实际: {:?}", other),
    }
}

// ===== §17.1 Queryable for (Value, Value) 契约 =====

#[test]
fn test_queryable_for_pair_two_columns_contract() {
    let (a, b) =
        <(Value, Value)>::from_values(vec![Value::I64(1), Value::String("x".into())]).unwrap();
    assert_eq!(a.as_i64(), Some(1));
    assert_eq!(b.as_str(), Some("x"));
}

#[test]
fn test_queryable_for_pair_rejects_wrong_column_count_contract() {
    let result = <(Value, Value)>::from_values(vec![Value::I64(1)]);
    assert!(result.is_err());
    match result.unwrap_err() {
        QueryError::ColumnCountMismatch {
            expected: 2,
            actual: 1,
        } => {}
        other => panic!("期望 ColumnCountMismatch {{2, 1}}，实际: {:?}", other),
    }
}

// ===== §17.1 Queryable for (Value, Value, Value) 契约 =====

#[test]
fn test_queryable_for_triple_three_columns_contract() {
    let (a, b, c) =
        <(Value, Value, Value)>::from_values(vec![Value::I64(1), Value::I64(2), Value::I64(3)])
            .unwrap();
    assert_eq!(a.as_i64(), Some(1));
    assert_eq!(b.as_i64(), Some(2));
    assert_eq!(c.as_i64(), Some(3));
}

#[test]
fn test_queryable_for_triple_rejects_wrong_column_count_contract() {
    let result = <(Value, Value, Value)>::from_values(vec![Value::I64(1), Value::I64(2)]);
    assert!(result.is_err());
    match result.unwrap_err() {
        QueryError::ColumnCountMismatch {
            expected: 3,
            actual: 2,
        } => {}
        other => panic!("期望 ColumnCountMismatch {{3, 2}}，实际: {:?}", other),
    }
}

// ===== §17.1 from_values_with_desc 契约 =====

#[test]
fn test_queryable_from_values_with_desc_matches_contract() {
    let desc = RowDesc::new(vec!["id".to_string(), "name".to_string()]);
    let result = <(Value, Value)>::from_values_with_desc(
        vec![Value::I64(1), Value::String("x".into())],
        &desc,
    );
    assert!(result.is_ok());
}

#[test]
fn test_queryable_from_values_with_desc_mismatch_contract() {
    let desc = RowDesc::new(vec![
        "id".to_string(),
        "name".to_string(),
        "email".to_string(),
    ]);
    let result = <(Value, Value)>::from_values_with_desc(
        vec![Value::I64(1), Value::String("x".into())],
        &desc,
    );
    // desc.len()==3, values.len()==2，应返回 ColumnCountMismatch
    assert!(result.is_err());
    match result.unwrap_err() {
        QueryError::ColumnCountMismatch {
            expected: 3,
            actual: 2,
        } => {}
        other => panic!("期望 ColumnCountMismatch {{3, 2}}，实际: {:?}", other),
    }
}

// ===== §17.2 FromRow trait 契约 =====

#[derive(Debug, PartialEq)]
struct UserRow {
    id: i64,
    name: String,
}

impl FromRow for UserRow {
    fn from_row(row: HashMap<String, Value>) -> Result<Self, QueryError> {
        let id = row
            .get("id")
            .and_then(|v| v.as_i64())
            .ok_or(QueryError::MissingColumn { column: "id" })?;
        let name = row
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or(QueryError::MissingColumn { column: "name" })?
            .to_string();
        Ok(UserRow { id, name })
    }
}

#[test]
fn test_from_row_basic_contract() {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(42));
    row.insert("name".to_string(), Value::String("Alice".into()));

    let user = UserRow::from_row(row).unwrap();
    assert_eq!(
        user,
        UserRow {
            id: 42,
            name: "Alice".to_string()
        }
    );
}

#[test]
fn test_from_row_missing_column_returns_error_contract() {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(42));
    // 缺少 name 列

    let err = UserRow::from_row(row).unwrap_err();
    match err {
        QueryError::MissingColumn { column } => assert_eq!(column, "name"),
        other => panic!("期望 MissingColumn {{name}}，实际: {:?}", other),
    }
}

// ===== §17.3 QueryError 变体契约 =====

#[test]
fn test_query_error_column_count_mismatch_contract() {
    let e = QueryError::ColumnCountMismatch {
        expected: 3,
        actual: 2,
    };
    let msg = format!("{}", e);
    assert!(msg.contains("3"));
    assert!(msg.contains("2"));
}

#[test]
fn test_query_error_type_mismatch_contract() {
    let e = QueryError::TypeMismatch {
        column: "id".into(),
        expected: "i64",
    };
    let msg = format!("{}", e);
    assert!(msg.contains("id"));
    assert!(msg.contains("i64"));
}

#[test]
fn test_query_error_missing_column_contract() {
    let e = QueryError::MissingColumn { column: "email" };
    let msg = format!("{}", e);
    assert!(msg.contains("email"));
}

#[test]
fn test_query_error_custom_contract() {
    let e = QueryError::Custom("something wrong".to_string());
    let msg = format!("{}", e);
    assert!(msg.contains("something wrong"));
}

// ===== §17 RowDesc 契约 =====

#[test]
fn test_row_desc_new_contract() {
    let desc = RowDesc::new(vec!["id".to_string(), "name".to_string()]);
    assert_eq!(desc.len(), 2);
    assert!(!desc.is_empty());
}

#[test]
fn test_row_desc_empty_contract() {
    let desc = RowDesc::new(vec![]);
    assert_eq!(desc.len(), 0);
    assert!(desc.is_empty());
}

#[test]
fn test_row_desc_index_of_contract() {
    let desc = RowDesc::new(vec![
        "id".to_string(),
        "name".to_string(),
        "email".to_string(),
    ]);
    assert_eq!(desc.index_of("id"), Some(0));
    assert_eq!(desc.index_of("name"), Some(1));
    assert_eq!(desc.index_of("email"), Some(2));
    assert_eq!(desc.index_of("missing"), None);
}

// ===== §17.2 FromRow 列名大小写敏感契约 =====

#[test]
fn test_from_row_column_name_case_sensitive_contract() {
    let mut row = HashMap::new();
    // 列名 ID（大写），from_row 查找 "id"（小写）应返回 MissingColumn
    row.insert("ID".to_string(), Value::I64(42));

    let err = UserRow::from_row(row).unwrap_err();
    assert!(
        matches!(err, QueryError::MissingColumn { column: "id" }),
        "列名匹配应大小写敏感"
    );
}
