use sz_orm_core::Value;

#[test]
fn test_value_as_str_match() {
    assert_eq!(Value::String("hi".to_string()).as_str(), Some("hi"));
    assert_eq!(Value::Decimal("3.14".to_string()).as_str(), Some("3.14"));
}

#[test]
fn test_value_as_str_no_match() {
    assert!(Value::I64(1).as_str().is_none());
    assert!(Value::Null.as_str().is_none());
    assert!(Value::Bytes(vec![]).as_str().is_none());
}

#[test]
fn test_value_as_i64_match() {
    assert_eq!(Value::I64(42).as_i64(), Some(42));
    assert_eq!(Value::I32(42).as_i64(), Some(42));
    assert_eq!(Value::Bool(true).as_i64(), Some(1));
    assert_eq!(Value::Bool(false).as_i64(), Some(0));
    assert_eq!(Value::String("99".to_string()).as_i64(), Some(99));
}

#[test]
fn test_value_as_i64_no_match() {
    assert!(Value::Bytes(vec![]).as_i64().is_none());
    assert!(Value::String("abc".to_string()).as_i64().is_none());
}

#[test]
fn test_value_as_f64_match() {
    assert_eq!(Value::F64(2.5).as_f64(), Some(2.5));
    assert_eq!(Value::F32(1.0).as_f64(), Some(1.0));
    assert_eq!(Value::I64(42).as_f64(), Some(42.0));
    assert_eq!(Value::Decimal("3.14".to_string()).as_f64(), Some(3.14));
}

#[test]
fn test_value_as_f64_no_match() {
    assert!(Value::Bytes(vec![]).as_f64().is_none());
    assert!(Value::String("abc".to_string()).as_f64().is_none());
}

#[test]
fn test_value_as_bool_match() {
    assert_eq!(Value::Bool(true).as_bool(), Some(true));
    assert_eq!(Value::Bool(false).as_bool(), Some(false));
    assert_eq!(Value::I64(1).as_bool(), Some(true));
    assert_eq!(Value::I64(0).as_bool(), Some(false));
    assert_eq!(Value::String("true".to_string()).as_bool(), Some(true));
    assert_eq!(Value::String("no".to_string()).as_bool(), Some(false));
}

#[test]
fn test_value_as_bool_null_returns_false() {
    assert_eq!(Value::Null.as_bool(), Some(false));
}

#[test]
fn test_value_as_bytes_match() {
    assert_eq!(Value::Bytes(vec![1, 2]).as_bytes(), Some(&[1u8, 2][..]));
    assert_eq!(
        Value::String("ab".to_string()).as_bytes(),
        Some(&[b'a', b'b'][..])
    );
}

#[test]
fn test_value_as_bytes_no_match() {
    assert!(Value::I64(1).as_bytes().is_none());
    assert!(Value::Null.as_bytes().is_none());
}