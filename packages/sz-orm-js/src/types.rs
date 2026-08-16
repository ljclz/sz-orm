//! Value ↔ JS 类型双向映射
//!
//! 使用 serde_json::Value 作为中间格式，避免 napi 类型复杂性。
//! JS 侧用 JSON.parse 还原。

use serde_json::Value as JsonValue;
use sz_orm_core::Value;

pub fn value_to_json(v: &Value) -> JsonValue {
    match v {
        Value::Null => JsonValue::Null,
        Value::Bool(b) => JsonValue::Bool(*b),
        Value::I8(n) => JsonValue::from(*n as i64),
        Value::I16(n) => JsonValue::from(*n as i64),
        Value::I32(n) => JsonValue::from(*n as i64),
        Value::I64(n) => JsonValue::from(*n),
        Value::U8(n) => JsonValue::from(*n as u64),
        Value::U16(n) => JsonValue::from(*n as u64),
        Value::U32(n) => JsonValue::from(*n as u64),
        Value::U64(n) => JsonValue::from(*n),
        Value::F32(f) => JsonValue::from(*f as f64),
        Value::F64(f) => JsonValue::from(*f),
        Value::Decimal(s)
        | Value::String(s)
        | Value::Uuid(s)
        | Value::Date(s)
        | Value::DateTime(s)
        | Value::Time(s)
        | Value::Json(s) => JsonValue::String(s.clone()),
        Value::Bytes(b) => serde_json::to_value(b).unwrap_or(JsonValue::Null),
        Value::Array(arr) => JsonValue::Array(arr.iter().map(value_to_json).collect()),
        Value::Object(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(k.clone(), value_to_json(v));
            }
            JsonValue::Object(obj)
        }
        _ => JsonValue::Null,
    }
}

pub fn value_to_json_string(v: &Value) -> String {
    serde_json::to_string(&value_to_json(v)).unwrap_or_else(|_| "null".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_to_json_null() {
        assert_eq!(value_to_json(&Value::Null), JsonValue::Null);
    }

    #[test]
    fn test_value_to_json_bool() {
        assert_eq!(value_to_json(&Value::Bool(true)), JsonValue::Bool(true));
        assert_eq!(value_to_json(&Value::Bool(false)), JsonValue::Bool(false));
    }

    #[test]
    fn test_value_to_json_integers() {
        assert_eq!(value_to_json(&Value::I8(1)), JsonValue::from(1i64));
        assert_eq!(value_to_json(&Value::I16(2)), JsonValue::from(2i64));
        assert_eq!(value_to_json(&Value::I32(3)), JsonValue::from(3i64));
        assert_eq!(value_to_json(&Value::I64(4)), JsonValue::from(4i64));
        assert_eq!(value_to_json(&Value::U8(5)), JsonValue::from(5u64));
        assert_eq!(value_to_json(&Value::U16(6)), JsonValue::from(6u64));
        assert_eq!(value_to_json(&Value::U32(7)), JsonValue::from(7u64));
        assert_eq!(value_to_json(&Value::U64(8)), JsonValue::from(8u64));
    }

    #[test]
    fn test_value_to_json_floats() {
        assert_eq!(value_to_json(&Value::F32(1.5)), JsonValue::from(1.5f64));
        assert_eq!(value_to_json(&Value::F64(2.5)), JsonValue::from(2.5f64));
    }

    #[test]
    fn test_value_to_json_string() {
        assert_eq!(
            value_to_json(&Value::String("hello".into())),
            JsonValue::String("hello".into())
        );
    }

    #[test]
    fn test_value_to_json_decimal() {
        assert_eq!(
            value_to_json(&Value::Decimal("123.45".into())),
            JsonValue::String("123.45".into())
        );
    }

    #[test]
    fn test_value_to_json_uuid() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        assert_eq!(
            value_to_json(&Value::Uuid(uuid.into())),
            JsonValue::String(uuid.into())
        );
    }

    #[test]
    fn test_value_to_json_dates() {
        assert_eq!(
            value_to_json(&Value::Date("2024-01-01".into())),
            JsonValue::String("2024-01-01".into())
        );
        assert_eq!(
            value_to_json(&Value::DateTime("2024-01-01T00:00:00".into())),
            JsonValue::String("2024-01-01T00:00:00".into())
        );
        assert_eq!(
            value_to_json(&Value::Time("12:30:00".into())),
            JsonValue::String("12:30:00".into())
        );
    }

    #[test]
    fn test_value_to_json_bytes() {
        let result = value_to_json(&Value::Bytes(vec![1, 2, 3]));
        assert!(result.is_array());
    }

    #[test]
    fn test_value_to_json_array() {
        let arr = Value::Array(vec![Value::I64(1), Value::I64(2), Value::I64(3)]);
        let result = value_to_json(&arr);
        assert!(result.is_array());
        assert_eq!(result.as_array().unwrap().len(), 3);
    }

    #[test]
    fn test_value_to_json_object() {
        let mut map = std::collections::HashMap::new();
        map.insert("key".to_string(), Value::String("value".into()));
        let obj = Value::Object(map);
        let result = value_to_json(&obj);
        assert!(result.is_object());
        assert_eq!(result["key"], JsonValue::String("value".into()));
    }

    #[test]
    fn test_value_to_json_string_fn() {
        assert_eq!(value_to_json_string(&Value::I64(42)), "42");
        assert_eq!(value_to_json_string(&Value::String("hi".into())), "\"hi\"");
        assert_eq!(value_to_json_string(&Value::Bool(true)), "true");
        assert_eq!(value_to_json_string(&Value::Null), "null");
    }

    #[test]
    fn test_value_to_json_bytes_empty() {
        let result = value_to_json(&Value::Bytes(vec![]));
        assert!(result.is_array());
        assert_eq!(result.as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_value_to_json_array_empty() {
        let arr = Value::Array(vec![]);
        let result = value_to_json(&arr);
        assert!(result.is_array());
        assert_eq!(result.as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_value_to_json_object_empty() {
        let map = std::collections::HashMap::new();
        let obj = Value::Object(map);
        let result = value_to_json(&obj);
        assert!(result.is_object());
        assert_eq!(result.as_object().unwrap().len(), 0);
    }

    #[test]
    fn test_value_to_json_string_fn_array() {
        let arr = Value::Array(vec![Value::I64(1), Value::I64(2)]);
        assert_eq!(value_to_json_string(&arr), "[1,2]");
    }

    #[test]
    fn test_value_to_json_string_fn_object() {
        let mut map = std::collections::HashMap::new();
        map.insert("k".to_string(), Value::I64(1));
        let obj = Value::Object(map);
        let json = value_to_json_string(&obj);
        assert!(json.contains("\"k\":1"));
    }

    #[test]
    fn test_value_to_json_negative_integers() {
        assert_eq!(value_to_json(&Value::I8(-1)), JsonValue::from(-1i64));
        assert_eq!(value_to_json(&Value::I16(-100)), JsonValue::from(-100i64));
        assert_eq!(value_to_json(&Value::I32(-1000)), JsonValue::from(-1000i64));
        assert_eq!(
            value_to_json(&Value::I64(-10000)),
            JsonValue::from(-10000i64)
        );
    }

    #[test]
    fn test_value_to_json_nested_array() {
        let inner = Value::Array(vec![Value::I64(1), Value::I64(2)]);
        let outer = Value::Array(vec![inner, Value::I64(3)]);
        let result = value_to_json(&outer);
        assert!(result.is_array());
        assert_eq!(result.as_array().unwrap().len(), 2);
        assert!(result[0].is_array());
    }

    #[test]
    fn test_value_to_json_nested_object() {
        let mut inner = std::collections::HashMap::new();
        inner.insert("nested".to_string(), Value::I64(42));
        let mut outer = std::collections::HashMap::new();
        outer.insert("inner".to_string(), Value::Object(inner));
        let obj = Value::Object(outer);
        let result = value_to_json(&obj);
        assert!(result.is_object());
        assert!(result["inner"].is_object());
        assert_eq!(result["inner"]["nested"], JsonValue::from(42i64));
    }
}
