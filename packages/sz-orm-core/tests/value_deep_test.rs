//! value.rs 深度路径测试 — 覆盖 Value 类型各种方法分支

use sz_orm_core::Value;
use sz_orm_core::get_dialect;
use sz_orm_core::DbType;

#[test]
fn test_value_is_methods() {
    assert!(Value::Null.is_null());
    assert!(Value::Bool(true).is_bool());
    assert!(Value::I64(1).is_i64());
    assert!(Value::F64(1.0).is_f64());
    assert!(Value::String("a".into()).is_string());
    assert!(Value::Bytes(vec![1]).is_bytes());
}

#[test]
fn test_value_as_str() {
    assert_eq!(Value::String("hello".into()).as_str(), Some("hello"));
    assert_eq!(Value::Null.as_str(), None);
    assert_eq!(Value::I64(1).as_str(), None);
}

#[test]
fn test_value_as_i64() {
    assert_eq!(Value::I64(42).as_i64(), Some(42));
    assert_eq!(Value::I32(42).as_i64(), Some(42));
    assert_eq!(Value::I8(42).as_i64(), Some(42));
    assert_eq!(Value::U8(42).as_i64(), Some(42));
    assert_eq!(Value::U32(42).as_i64(), Some(42));
    assert_eq!(Value::Null.as_i64(), None);
    assert_eq!(Value::String("a".into()).as_i64(), None);
}

#[test]
fn test_value_as_f64() {
    assert_eq!(Value::F64(3.15).as_f64(), Some(3.15));
    assert_eq!(Value::F32(3.15).as_f64(), Some(3.15_f32 as f64));
    assert_eq!(Value::I64(42).as_f64(), Some(42.0));
    assert_eq!(Value::Null.as_f64(), None);
}

#[test]
fn test_value_as_bool() {
    assert!(Value::Bool(true).as_bool().is_some());
    assert!(Value::Bool(false).as_bool().is_some());
    assert_eq!(Value::I64(1).as_bool(), Some(true));
    assert_eq!(Value::I64(0).as_bool(), Some(false));
    assert_eq!(Value::String("true".into()).as_bool(), Some(true));
    assert_eq!(Value::String("false".into()).as_bool(), Some(false));
    assert_eq!(Value::Null.as_bool(), Some(false));
}

#[test]
fn test_value_as_bytes() {
    assert_eq!(Value::Bytes(vec![1, 2, 3]).as_bytes(), Some(&[1u8, 2, 3][..]));
    assert_eq!(Value::Null.as_bytes(), None);
}

#[test]
fn test_value_display() {
    assert_eq!(Value::Null.to_string(), "NULL");
    assert_eq!(Value::Bool(true).to_string(), "true");
    assert_eq!(Value::Bool(false).to_string(), "false");
    assert_eq!(Value::I64(42).to_string(), "42");
    let s = Value::String("hello".into()).to_string();
    assert!(s.contains("hello"));
}

#[test]
fn test_value_from_bool() {
    let v: Value = true.into();
    assert_eq!(v, Value::Bool(true));
}

#[test]
fn test_value_from_integers() {
    let v: Value = 42i8.into();
    assert_eq!(v, Value::I8(42));
    let v: Value = 42i16.into();
    assert_eq!(v, Value::I16(42));
    let v: Value = 42i32.into();
    assert_eq!(v, Value::I32(42));
    let v: Value = 42i64.into();
    assert_eq!(v, Value::I64(42));
    let v: Value = 42u8.into();
    assert_eq!(v, Value::U8(42));
    let v: Value = 42u16.into();
    assert_eq!(v, Value::U16(42));
    let v: Value = 42u32.into();
    assert_eq!(v, Value::U32(42));
}

#[test]
fn test_value_from_floats() {
    let v: Value = 3.15f32.into();
    assert_eq!(v, Value::F32(3.15));
    let v: Value = 3.15f64.into();
    assert_eq!(v, Value::F64(3.15));
}

#[test]
fn test_value_from_string() {
    let v: Value = "hello".to_string().into();
    assert_eq!(v, Value::String("hello".into()));
    let v: Value = "world".into();
    assert_eq!(v, Value::String("world".into()));
}

#[test]
fn test_value_from_unit() {
    let v: Value = ().into();
    assert_eq!(v, Value::Null);
}

#[test]
fn test_value_clonecheap() {
    let v = Value::String("hello".into());
    let cloned = v.clonecheap();
    assert_eq!(cloned, v);
}

#[test]
fn test_value_from_map() {
    let mut map = std::collections::HashMap::new();
    map.insert("key".to_string(), Value::I64(42));
    let v = Value::from_map(map);
    assert!(v.is_object());
}

#[test]
fn test_value_to_param() {
    let v = Value::I64(42);
    assert!(v.to_param().contains("42"));
    let v = Value::String("hello".into());
    assert!(v.to_param().contains("hello"));
    let v = Value::Null;
    assert!(v.to_param().contains("NULL"));
}

#[test]
fn test_value_to_param_with_dialect() {
    let d = get_dialect(DbType::MySQL).unwrap();
    let v = Value::I64(42);
    assert!(v.to_param_with_dialect(&*d).contains("42"));
    let v = Value::String("hello".into());
    assert!(!v.to_param_with_dialect(&*d).is_empty());
}

#[test]
fn test_value_boxed_str() {
    let v = Value::boxed_str("hello");
    assert!(!v.is_null());
}

#[test]
fn test_value_from_generic() {
    let v = Value::from(42i64);
    assert_eq!(v, Value::I64(42));
}

#[test]
fn test_value_default() {
    let v = Value::default();
    assert!(v.is_null());
}

#[test]
fn test_value_decimal_display() {
    let v = Value::Decimal("123.45".into());
    assert!(v.to_string().contains("123.45"));
}

#[test]
fn test_value_bytes_display() {
    let v = Value::Bytes(vec![0x41, 0x42]);
    assert!(!v.to_string().is_empty());
}