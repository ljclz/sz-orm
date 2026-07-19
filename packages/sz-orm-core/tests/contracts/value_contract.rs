//! Value 模块契约测试 — 对应 `docs/api-contracts.md` §1
//!
//! 锁定 `Value` 枚举的 20 种变体、类型转换契约、From 实现、参数化契约。

use sz_orm_core::Value;

// ===== §1.1 Value 枚举变体契约 =====

#[test]
fn test_value_null_variant_contract() {
    let v = Value::Null;
    assert!(v.is_null());
    assert_eq!(v.to_param(), "NULL");
    assert_eq!(v.as_i64(), None);
    assert_eq!(v.as_str(), None);
    // 注：实际实现中 Null::as_bool() 返回 Some(false)（SQL 风格：NULL 视为 false）
    assert_eq!(v.as_bool(), Some(false));
    assert_eq!(v.as_f64(), None);
    assert_eq!(v.as_bytes(), None);
}

#[test]
fn test_value_bool_variant_contract() {
    assert!(Value::Bool(true).as_bool() == Some(true));
    assert!(Value::Bool(false).as_bool() == Some(false));
    // Bool → i64 转换契约：true→1, false→0
    assert_eq!(Value::Bool(true).as_i64(), Some(1));
    assert_eq!(Value::Bool(false).as_i64(), Some(0));
}

#[test]
fn test_value_i64_variant_contract() {
    let v = Value::I64(42);
    assert_eq!(v.as_i64(), Some(42));
    assert_eq!(v.as_f64(), Some(42.0));
    assert!(v.is_i64());
    assert_eq!(v.to_param(), "42");
}

#[test]
fn test_value_string_variant_contract() {
    let v = Value::String("hello".to_string());
    assert_eq!(v.as_str(), Some("hello"));
    assert!(v.is_string());
    assert_eq!(v.to_param(), "'hello'");
}

#[test]
fn test_value_string_escaping_contract() {
    // 单引号转义为 ''
    let v = Value::String("it's a test".to_string());
    assert_eq!(v.to_param(), "'it''s a test'");
}

#[test]
fn test_value_bytes_variant_contract() {
    let v = Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF]);
    assert!(v.is_bytes());
    assert_eq!(v.as_bytes(), Some(&[0xDE, 0xAD, 0xBE, 0xEF][..]));
    assert_eq!(v.to_param(), "X'deadbeef'");
}

#[test]
fn test_value_array_variant_contract() {
    let arr = Value::Array(vec![Value::I64(1), Value::I64(2), Value::I64(3)]);
    assert_eq!(arr.to_param(), "(1, 2, 3)");
}

// ===== §1.1 类型转换契约（已知陷阱） =====

#[test]
fn test_as_i64_truncates_f64_contract() {
    // 陷阱：F64(3.15) 截断为 Some(3)，**不是** None
    assert_eq!(Value::F64(3.15).as_i64(), Some(3));
    assert_eq!(Value::F32(2.7).as_i64(), Some(2));
}

#[test]
fn test_as_str_returns_none_for_non_string_contract() {
    // 陷阱：as_str 对 I64 返回 None，不自动转字符串
    assert_eq!(Value::I64(42).as_str(), None);
    assert_eq!(Value::Bool(true).as_str(), None);
    assert_eq!(Value::F64(3.15).as_str(), None);
}

#[test]
fn test_as_bool_string_parsing_contract() {
    // 字符串解析为 bool：支持 "true"/"1"/"yes"/"on"（大小写不敏感）
    assert_eq!(Value::String("true".to_string()).as_bool(), Some(true));
    assert_eq!(Value::String("TRUE".to_string()).as_bool(), Some(true));
    assert_eq!(Value::String("1".to_string()).as_bool(), Some(true));
    assert_eq!(Value::String("yes".to_string()).as_bool(), Some(true));
    assert_eq!(Value::String("on".to_string()).as_bool(), Some(true));
    assert_eq!(Value::String("false".to_string()).as_bool(), Some(false));
    assert_eq!(Value::String("0".to_string()).as_bool(), Some(false));
    assert_eq!(Value::String("no".to_string()).as_bool(), Some(false));
    assert_eq!(Value::String("off".to_string()).as_bool(), Some(false));
}

#[test]
fn test_as_bool_i64_contract() {
    // I64 → bool：0→false，非0→true
    assert_eq!(Value::I64(0).as_bool(), Some(false));
    assert_eq!(Value::I64(1).as_bool(), Some(true));
    assert_eq!(Value::I64(-1).as_bool(), Some(true));
    assert_eq!(Value::I64(42).as_bool(), Some(true));
}

// ===== §1.2 From 实现契约 =====

#[test]
fn test_from_unit_for_value_contract() {
    let v: Value = ().into();
    assert!(v.is_null());
}

#[test]
fn test_from_bool_for_value_contract() {
    let v: Value = true.into();
    assert!(v.is_bool());
    assert_eq!(v.as_bool(), Some(true));
}

#[test]
fn test_from_i32_for_value_contract() {
    // i32 → Value（应转为 I64 变体）
    let v: Value = 42i32.into();
    assert_eq!(v.as_i64(), Some(42));
}

#[test]
fn test_from_f64_for_value_contract() {
    let v: Value = 2.5f64.into();
    assert_eq!(v.as_f64(), Some(2.5));
}

#[test]
fn test_from_str_for_value_contract() {
    let v: Value = "hello".into();
    assert_eq!(v.as_str(), Some("hello"));
}

#[test]
fn test_from_string_for_value_contract() {
    let v: Value = String::from("world").into();
    assert_eq!(v.as_str(), Some("world"));
}

#[test]
fn test_from_vec_u8_for_value_contract() {
    // 陷阱：Vec<u8> 转为 Value::Bytes，不是 Value::Array
    let v: Value = vec![1u8, 2u8].into();
    assert!(v.is_bytes());
    assert_eq!(v.as_bytes(), Some(&[1u8, 2u8][..]));
}

#[test]
fn test_from_vec_value_for_value_contract() {
    // Vec<Value> → Value::Array
    let arr: Vec<Value> = vec![Value::I64(1), Value::I64(2), Value::I64(3)];
    let v: Value = arr.into();
    match v {
        Value::Array(items) => assert_eq!(items.len(), 3),
        _ => panic!("Expected Value::Array"),
    }
}

#[test]
fn test_value_default_is_null_contract() {
    let v: Value = Default::default();
    assert!(v.is_null());
}
