use sz_orm_core::{FromQueryResult, Value};

#[test]
fn test_from_value_i64() {
    assert_eq!(i64::from_value(&Value::I64(42)).unwrap(), 42);
}

#[test]
fn test_from_value_i32() {
    assert_eq!(i32::from_value(&Value::I32(42)).unwrap(), 42);
}

#[test]
fn test_from_value_i16() {
    assert_eq!(i16::from_value(&Value::I16(42)).unwrap(), 42);
}

#[test]
fn test_from_value_i8() {
    assert_eq!(i8::from_value(&Value::I8(42)).unwrap(), 42);
}

#[test]
fn test_from_value_u64() {
    assert_eq!(u64::from_value(&Value::U64(42)).unwrap(), 42);
}

#[test]
fn test_from_value_u32() {
    assert_eq!(u32::from_value(&Value::U32(42)).unwrap(), 42);
}

#[test]
fn test_from_value_u16_and_u8() {
    assert_eq!(u16::from_value(&Value::U16(42)).unwrap(), 42);
    assert_eq!(u8::from_value(&Value::U8(42)).unwrap(), 42);
}

#[test]
fn test_from_value_f64() {
    assert_eq!(f64::from_value(&Value::F64(3.5)).unwrap(), 3.5);
}

#[test]
fn test_from_value_f32() {
    assert_eq!(f32::from_value(&Value::F32(3.5)).unwrap(), 3.5);
}

#[test]
fn test_from_value_bool() {
    assert!(bool::from_value(&Value::Bool(true)).unwrap());
    assert!(!bool::from_value(&Value::Bool(false)).unwrap());
    assert!(bool::from_value(&Value::I64(1)).unwrap());
    assert!(!bool::from_value(&Value::I64(0)).unwrap());
}

#[test]
fn test_from_value_string() {
    assert_eq!(
        String::from_value(&Value::String("hi".to_string())).unwrap(),
        "hi"
    );
    assert_eq!(
        String::from_value(&Value::Uuid("abc".to_string())).unwrap(),
        "abc"
    );
    assert_eq!(
        String::from_value(&Value::DateTime("2024-01-01".to_string())).unwrap(),
        "2024-01-01"
    );
}

#[test]
fn test_from_value_mismatch_err() {
    assert!(i64::from_value(&Value::String("x".to_string())).is_err());
    assert!(i64::from_value(&Value::Null).is_err());
    assert!(bool::from_value(&Value::Null).is_err());
    assert!(String::from_value(&Value::I64(1)).is_err());
    assert!(f64::from_value(&Value::Bool(true)).is_err());
}
