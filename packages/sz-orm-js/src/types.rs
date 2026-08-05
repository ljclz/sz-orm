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
