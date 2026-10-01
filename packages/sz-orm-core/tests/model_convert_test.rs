use std::collections::HashMap;

use sz_orm_core::{rows_to_values, value_to_json, Value};

#[test]
fn test_rows_to_values_empty() {
    let result = rows_to_values(vec![]);
    assert_eq!(result, Value::Array(vec![]));
}

#[test]
fn test_rows_to_values_single_row() {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(1));
    row.insert("name".to_string(), Value::String("Alice".to_string()));
    let result = rows_to_values(vec![row]);
    match result {
        Value::Array(items) => {
            assert_eq!(items.len(), 1);
            assert!(matches!(items[0], Value::Object(_)));
        }
        _ => panic!("expected Array"),
    }
}

#[test]
fn test_rows_to_values_multi_rows() {
    let row1 = HashMap::from([("id".to_string(), Value::I64(1))]);
    let row2 = HashMap::from([("id".to_string(), Value::I64(2))]);
    let result = rows_to_values(vec![row1, row2]);
    match result {
        Value::Array(items) => assert_eq!(items.len(), 2),
        _ => panic!("expected Array"),
    }
}

#[test]
fn test_value_to_json_null_and_bool() {
    assert_eq!(value_to_json(Value::Null), serde_json::Value::Null);
    assert_eq!(
        value_to_json(Value::Bool(true)),
        serde_json::Value::Bool(true)
    );
}

#[test]
fn test_value_to_json_int_and_float() {
    assert_eq!(value_to_json(Value::I64(42)), serde_json::json!(42));
    assert_eq!(value_to_json(Value::U64(99)), serde_json::json!(99));
    assert_eq!(value_to_json(Value::F64(3.25)), serde_json::json!(3.25));
}

#[test]
fn test_value_to_json_string() {
    assert_eq!(
        value_to_json(Value::String("hello".to_string())),
        serde_json::json!("hello")
    );
}

#[test]
fn test_value_to_json_bytes() {
    let result = value_to_json(Value::Bytes(vec![0x48, 0x49]));
    assert_eq!(result, serde_json::json!("4849"));
}

#[test]
fn test_value_to_json_array() {
    let arr = Value::Array(vec![Value::I64(1), Value::I64(2)]);
    assert_eq!(value_to_json(arr), serde_json::json!([1, 2]));
}

#[test]
fn test_value_to_json_object() {
    let map = HashMap::from([("key".to_string(), Value::I64(42))]);
    let obj = Value::Object(map);
    assert_eq!(value_to_json(obj), serde_json::json!({"key": 42}));
}
