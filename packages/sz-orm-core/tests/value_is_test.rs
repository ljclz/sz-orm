use std::collections::HashMap;

use sz_orm_core::Value;

#[test]
fn test_value_is_null() {
    assert!(Value::Null.is_null());
    assert!(!Value::I64(0).is_null());
    assert!(!Value::Bool(false).is_null());
}

#[test]
fn test_value_is_bool() {
    assert!(Value::Bool(true).is_bool());
    assert!(Value::Bool(false).is_bool());
    assert!(!Value::Null.is_bool());
    assert!(!Value::I64(1).is_bool());
}

#[test]
fn test_value_is_i64() {
    assert!(Value::I64(42).is_i64());
    assert!(!Value::I32(42).is_i64());
    assert!(!Value::Null.is_i64());
}

#[test]
fn test_value_is_f64() {
    assert!(Value::F64(1.0).is_f64());
    assert!(!Value::F32(1.0).is_f64());
    assert!(!Value::I64(1).is_f64());
}

#[test]
fn test_value_is_string() {
    assert!(Value::String("x".to_string()).is_string());
    assert!(!Value::Null.is_string());
    assert!(!Value::Bytes(vec![]).is_string());
}

#[test]
fn test_value_is_bytes() {
    assert!(Value::Bytes(vec![]).is_bytes());
    assert!(Value::Bytes(vec![1, 2, 3]).is_bytes());
    assert!(!Value::Null.is_bytes());
}

#[test]
fn test_value_is_object() {
    assert!(Value::Object(HashMap::new()).is_object());
    assert!(!Value::Null.is_object());
    assert!(!Value::Array(vec![]).is_object());
}