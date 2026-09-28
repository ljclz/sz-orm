//! v9.2.0 M7-T42：zero_copy to_borrowed/from_borrowed/serialize/deserialize（3 tests）
//!
//! 需要 `zero-copy` feature（默认启用）。

#![cfg(feature = "zero-copy")]

use sz_orm_core::l2_cache::zero_copy::{from_borrowed, serialize_zero_copy, to_borrowed};
use sz_orm_core::value_borrowed::BorrowedValue;
use sz_orm_core::Value;
use std::borrow::Cow;

#[test]
fn test_zerocopy_to_borrowed_preserves_value_semantics() {
    let values = vec![
        Value::Null,
        Value::Bool(true),
        Value::I64(42),
        Value::F64(3.14),
        Value::String("hello".to_string()),
        Value::I32(-7),
    ];
    for v in &values {
        let borrowed = to_borrowed(v);
        let restored = from_borrowed(&borrowed);
        assert_eq!(*v, restored, "to_borrowed/from_borrowed 应保持往返一致");
    }
    let s = Value::String("borrowed".to_string());
    let borrowed = to_borrowed(&s);
    match borrowed {
        BorrowedValue::String(Cow::Borrowed(_)) => {}
        ref other => panic!("字符串应零拷贝借用，实际: {:?}", other),
    }
}

#[test]
fn test_zerocopy_from_borrowed_reconstructs_value() {
    let original = Value::I64(123);
    let borrowed = to_borrowed(&original);
    let reconstructed = from_borrowed(&borrowed);
    assert_eq!(original, reconstructed);

    let arr = Value::Array(vec![Value::I64(1), Value::I64(2), Value::I64(3)]);
    let borrowed_arr = to_borrowed(&arr);
    let reconstructed_arr = from_borrowed(&borrowed_arr);
    assert_eq!(arr, reconstructed_arr);
}

#[test]
fn test_zerocopy_serialize_deserialize_roundtrip() {
    let v = Value::String("test-data".to_string());
    let data = serialize_zero_copy(&v);
    assert!(!data.is_empty(), "序列化结果不应为空");
    let restored = sz_orm_core::l2_cache::zero_copy::deserialize_zero_copy(&data).unwrap();
    assert!(
        matches!(restored, Value::String(ref s) if !s.is_empty()),
        "反序列化应返回非空字符串"
    );
}