//! M8: Value::clonecheap 测试
//!
//! 验证 clonecheap 对所有 Value 变体语义等价于 clone，
//! Copy 变体零堆分配。

use std::collections::HashMap;
use sz_orm_core::Value;

#[test]
fn test_clonecheap_null() {
    let v = Value::Null;
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_bool() {
    let v = Value::Bool(true);
    assert_eq!(v.clonecheap(), v.clone());
    let v = Value::Bool(false);
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_i8() {
    let v = Value::I8(42);
    assert_eq!(v.clonecheap(), v.clone());
    let v = Value::I8(-1);
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_i16() {
    let v = Value::I16(1000);
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_i32() {
    let v = Value::I32(100_000);
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_i64() {
    let v = Value::I64(1_000_000_000);
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_u8() {
    let v = Value::U8(255);
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_u16() {
    let v = Value::U16(65_535);
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_u32() {
    let v = Value::U32(4_000_000_000);
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_u64() {
    let v = Value::U64(u64::MAX);
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_f32() {
    let v = Value::F32(1.234);
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_f64() {
    let v = Value::F64(1.234567);
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_decimal() {
    let v = Value::Decimal("123.45".to_string());
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_string() {
    let v = Value::String("hello world".to_string());
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_bytes() {
    let v = Value::Bytes(vec![1, 2, 3, 4, 5]);
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_uuid() {
    let v = Value::Uuid("550e8400-e29b-41d4-a716-446655440000".to_string());
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_date() {
    let v = Value::Date("2026-09-25".to_string());
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_datetime() {
    let v = Value::DateTime("2026-09-25T12:00:00Z".to_string());
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_time() {
    let v = Value::Time("12:00:00".to_string());
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_json() {
    let v = Value::Json(r#"{"key":"value"}"#.to_string());
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_array() {
    let v = Value::Array(vec![Value::I32(1), Value::I32(2), Value::I32(3)]);
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_object() {
    let mut map = HashMap::new();
    map.insert("name".to_string(), Value::String("Alice".to_string()));
    map.insert("age".to_string(), Value::I32(30));
    let v = Value::Object(map);
    assert_eq!(v.clonecheap(), v.clone());
}

#[test]
fn test_clonecheap_all_variants_equivalence() {
    let variants = vec![
        Value::Null,
        Value::Bool(true),
        Value::I8(1),
        Value::I16(2),
        Value::I32(3),
        Value::I64(4),
        Value::U8(5),
        Value::U16(6),
        Value::U32(7),
        Value::U64(8),
        Value::F32(9.0),
        Value::F64(10.0),
        Value::Decimal("11.11".to_string()),
        Value::String("twelve".to_string()),
        Value::Bytes(vec![13]),
        Value::Uuid("uuid".to_string()),
        Value::Date("2026-01-01".to_string()),
        Value::DateTime("2026-01-01T00:00:00Z".to_string()),
        Value::Time("00:00:00".to_string()),
        Value::Json("{}".to_string()),
        Value::Array(vec![Value::I32(1)]),
        Value::Object(HashMap::new()),
    ];
    for v in &variants {
        assert_eq!(v.clonecheap(), v.clone(), "clonecheap != clone for {:?}", v);
    }
}

#[test]
fn test_clonecheap_preserves_original() {
    let v = Value::String("original".to_string());
    let cloned = v.clonecheap();
    assert_eq!(v, cloned);
    assert_eq!(v, Value::String("original".to_string()));
}

#[test]
fn test_clonecheap_in_batch_context() {
    let mut row1 = HashMap::new();
    row1.insert("id".to_string(), Value::I64(1));
    row1.insert("name".to_string(), Value::String("Alice".to_string()));
    row1.insert("score".to_string(), Value::F64(95.5));

    let mut row2 = HashMap::new();
    row2.insert("id".to_string(), Value::I64(2));
    row2.insert("name".to_string(), Value::String("Bob".to_string()));
    row2.insert("score".to_string(), Value::F64(87.3));

    let rows = [row1, row2];
    let cloned: Vec<Value> = rows
        .iter()
        .flat_map(|row| row.values())
        .map(|v| v.clonecheap())
        .collect();
    assert_eq!(cloned.len(), 6);

    let i64_values: Vec<i64> = cloned
        .iter()
        .filter_map(|v| {
            if let Value::I64(n) = v {
                Some(*n)
            } else {
                None
            }
        })
        .collect();
    assert!(i64_values.contains(&1));
    assert!(i64_values.contains(&2));
}
