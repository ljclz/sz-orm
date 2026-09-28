use sz_orm_core::{get_dialect, DbType, Value};

#[test]
fn test_to_param_null() {
    assert_eq!(Value::Null.to_param(), "NULL");
}

#[test]
fn test_to_param_bool() {
    assert_eq!(Value::Bool(true).to_param(), "TRUE");
    assert_eq!(Value::Bool(false).to_param(), "FALSE");
}

#[test]
fn test_to_param_int() {
    assert_eq!(Value::I64(42).to_param(), "42");
    assert_eq!(Value::I32(-5).to_param(), "-5");
    assert_eq!(Value::U64(100).to_param(), "100");
}

#[test]
fn test_to_param_float() {
    assert_eq!(Value::F64(3.5).to_param(), "3.5");
    assert_eq!(Value::F32(1.0).to_param(), "1");
}

#[test]
fn test_to_param_string_and_bytes() {
    assert_eq!(Value::String("hi".to_string()).to_param(), "'hi'");
    assert_eq!(Value::String("it's".to_string()).to_param(), "'it''s'");
    assert_eq!(Value::Bytes(vec![0xAB, 0xCD]).to_param(), "X'abcd'");
}

#[test]
fn test_to_param_with_dialect() {
    let mysql = get_dialect(DbType::MySQL).unwrap();
    assert_eq!(
        Value::String("hi".to_string()).to_param_with_dialect(&*mysql),
        "'hi'"
    );
    let pg = get_dialect(DbType::PostgreSQL).unwrap();
    assert_eq!(
        Value::String("hi".to_string()).to_param_with_dialect(&*pg),
        "'hi'"
    );
    assert_eq!(Value::Null.to_param_with_dialect(&*mysql), "NULL");
    assert_eq!(Value::I64(42).to_param_with_dialect(&*pg), "42");
}