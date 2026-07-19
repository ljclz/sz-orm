//! Value type definitions
//!
//! Unified value representation for database operations

use std::borrow::Cow;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Represents a database value
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub enum Value {
    /// Null value
    #[default]
    Null,

    /// Boolean value
    Bool(bool),

    /// 8-bit signed integer
    I8(i8),

    /// 16-bit signed integer
    I16(i16),

    /// 32-bit signed integer
    I32(i32),

    /// 64-bit signed integer
    I64(i64),

    /// 8-bit unsigned integer
    U8(u8),

    /// 16-bit unsigned integer
    U16(u16),

    /// 32-bit unsigned integer
    U32(u32),

    /// 64-bit unsigned integer
    U64(u64),

    /// 32-bit floating point
    F32(f32),

    /// 64-bit floating point
    F64(f64),

    /// String value
    String(String),

    /// Bytes value
    Bytes(Vec<u8>),

    /// UUID value (stored as string)
    Uuid(String),

    /// Date value (ISO 8601 format)
    Date(String),

    /// DateTime value (ISO 8601 format)
    DateTime(String),

    /// Time value
    Time(String),

    /// JSON value
    Json(String),

    /// Array of values
    Array(Vec<Value>),

    /// HashMap-based object value for storing relation data
    Object(std::collections::HashMap<String, Value>),
}

impl Value {
    /// Check if the value is null
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Check if the value is a boolean
    pub fn is_bool(&self) -> bool {
        matches!(self, Value::Bool(_))
    }

    /// Check if the value is an integer
    pub fn is_i64(&self) -> bool {
        matches!(self, Value::I64(_))
    }

    /// Check if the value is a float
    pub fn is_f64(&self) -> bool {
        matches!(self, Value::F64(_))
    }

    /// Check if the value is a string
    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }

    /// Check if the value is bytes
    pub fn is_bytes(&self) -> bool {
        matches!(self, Value::Bytes(_))
    }

    /// Check if this value is an Object
    pub fn is_object(&self) -> bool {
        matches!(self, Value::Object(_))
    }

    /// Create a Value from a HashMap
    pub fn from_map(map: std::collections::HashMap<String, Value>) -> Self {
        Value::Object(map)
    }

    /// Get the value as &str if possible
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Get the value as i64 if possible
    /// 支持 F32/F64 → i64 的有损转换（数据库 SUM/AVG 等聚合函数常返回浮点类型）
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::I8(v) => Some(*v as i64),
            Value::I16(v) => Some(*v as i64),
            Value::I32(v) => Some(*v as i64),
            Value::I64(v) => Some(*v),
            Value::U8(v) => Some(*v as i64),
            Value::U16(v) => Some(*v as i64),
            Value::U32(v) => Some(*v as i64),
            Value::U64(v) => Some(*v as i64),
            Value::F32(v) => Some(*v as i64),
            Value::F64(v) => Some(*v as i64),
            Value::Bool(v) => Some(if *v { 1 } else { 0 }),
            Value::String(s) => s.parse::<i64>().ok(),
            _ => None,
        }
    }

    /// Get the value as f64 if possible
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::F32(v) => Some(*v as f64),
            Value::F64(v) => Some(*v),
            Value::I8(v) => Some(*v as f64),
            Value::I16(v) => Some(*v as f64),
            Value::I32(v) => Some(*v as f64),
            Value::I64(v) => Some(*v as f64),
            Value::U8(v) => Some(*v as f64),
            Value::U16(v) => Some(*v as f64),
            Value::U32(v) => Some(*v as f64),
            Value::U64(v) => Some(*v as f64),
            Value::Bool(v) => Some(if *v { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    /// Get the value as bool if possible
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(v) => Some(*v),
            Value::I8(v) => Some(*v != 0),
            Value::I16(v) => Some(*v != 0),
            Value::I32(v) => Some(*v != 0),
            Value::I64(v) => Some(*v != 0),
            Value::U8(v) => Some(*v != 0),
            Value::U16(v) => Some(*v != 0),
            Value::U32(v) => Some(*v != 0),
            Value::U64(v) => Some(*v != 0),
            Value::F32(v) => Some(*v != 0.0),
            Value::F64(v) => Some(*v != 0.0),
            Value::String(s) => match s.to_lowercase().as_str() {
                "1" | "true" | "yes" | "on" => Some(true),
                "0" | "false" | "no" | "off" => Some(false),
                _ => None,
            },
            Value::Null => Some(false),
            _ => None,
        }
    }

    /// Get the value as bytes if possible
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Bytes(v) => Some(v),
            Value::String(s) => Some(s.as_bytes()),
            _ => None,
        }
    }

    /// Convert to SQL parameter string
    pub fn to_param(&self) -> Cow<'_, str> {
        match self {
            Value::Null => Cow::Borrowed("NULL"),
            Value::Bool(b) => Cow::Owned(if *b { "TRUE" } else { "FALSE" }.to_string()),
            Value::I8(v) => Cow::Owned(v.to_string()),
            Value::I16(v) => Cow::Owned(v.to_string()),
            Value::I32(v) => Cow::Owned(v.to_string()),
            Value::I64(v) => Cow::Owned(v.to_string()),
            Value::U8(v) => Cow::Owned(v.to_string()),
            Value::U16(v) => Cow::Owned(v.to_string()),
            Value::U32(v) => Cow::Owned(v.to_string()),
            Value::U64(v) => Cow::Owned(v.to_string()),
            Value::F32(v) => Cow::Owned(v.to_string()),
            Value::F64(v) => Cow::Owned(v.to_string()),
            Value::String(s) => Cow::Owned(format!("'{}'", escape_string(s))),
            Value::Bytes(b) => Cow::Owned(format!("X'{}'", hex_encode(b))),
            Value::Uuid(s) => Cow::Owned(format!("'{}'", escape_string(s))),
            Value::Date(s) => Cow::Owned(format!("'{}'", escape_string(s))),
            Value::DateTime(s) => Cow::Owned(format!("'{}'", escape_string(s))),
            Value::Time(s) => Cow::Owned(format!("'{}'", escape_string(s))),
            Value::Json(s) => Cow::Owned(format!("'{}'", escape_string(s))),
            Value::Array(arr) => {
                let params: Vec<String> = arr.iter().map(|v| v.to_param().into_owned()).collect();
                Cow::Owned(format!("({})", params.join(", ")))
            }
            Value::Object(_) => Cow::Borrowed("NULL"),
        }
    }

    /// Create from any type that implements Into<Value>
    pub fn from<T: Into<Value>>(v: T) -> Self {
        v.into()
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "NULL"),
            Value::Bool(b) => write!(f, "{}", b),
            Value::I8(v) => write!(f, "{}", v),
            Value::I16(v) => write!(f, "{}", v),
            Value::I32(v) => write!(f, "{}", v),
            Value::I64(v) => write!(f, "{}", v),
            Value::U8(v) => write!(f, "{}", v),
            Value::U16(v) => write!(f, "{}", v),
            Value::U32(v) => write!(f, "{}", v),
            Value::U64(v) => write!(f, "{}", v),
            Value::F32(v) => write!(f, "{}", v),
            Value::F64(v) => write!(f, "{}", v),
            Value::String(v) => write!(f, "'{}'", v),
            Value::Bytes(v) => write!(f, "X'{}'", hex_encode(v)),
            Value::Uuid(v) => write!(f, "'{}'", v),
            Value::Date(v) => write!(f, "'{}'", v),
            Value::DateTime(v) => write!(f, "'{}'", v),
            Value::Time(v) => write!(f, "'{}'", v),
            Value::Json(v) => write!(f, "'{}'", v),
            Value::Array(v) => {
                let items: Vec<String> = v.iter().map(|i| format!("{}", i)).collect();
                write!(f, "({})", items.join(", "))
            }
            Value::Object(map) => {
                let items: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v))
                    .collect();
                write!(f, "{{{}}}", items.join(", "))
            }
        }
    }
}

impl From<()> for Value {
    fn from(_: ()) -> Self {
        Value::Null
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl From<i8> for Value {
    fn from(v: i8) -> Self {
        Value::I8(v)
    }
}

impl From<i16> for Value {
    fn from(v: i16) -> Self {
        Value::I16(v)
    }
}

impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::I32(v)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::I64(v)
    }
}

impl From<u8> for Value {
    fn from(v: u8) -> Self {
        Value::U8(v)
    }
}

impl From<u16> for Value {
    fn from(v: u16) -> Self {
        Value::U16(v)
    }
}

impl From<u32> for Value {
    fn from(v: u32) -> Self {
        Value::U32(v)
    }
}

impl From<u64> for Value {
    fn from(v: u64) -> Self {
        Value::U64(v)
    }
}

impl From<f32> for Value {
    fn from(v: f32) -> Self {
        Value::F32(v)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::F64(v)
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_string())
    }
}

impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Self {
        Value::Bytes(v)
    }
}

impl From<&[u8]> for Value {
    fn from(v: &[u8]) -> Self {
        Value::Bytes(v.to_vec())
    }
}

impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Self {
        Value::Array(v)
    }
}

fn escape_string(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\'' => escaped.push_str("''"),
            '\\' => escaped.push_str("\\\\"),
            '\0' => escaped.push_str("\\0"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(c),
        }
    }
    escaped
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_is_null() {
        assert!(Value::Null.is_null());
        assert!(!Value::I64(0).is_null());
    }

    #[test]
    fn test_value_as_i64() {
        assert_eq!(Value::I64(42).as_i64(), Some(42));
        assert_eq!(Value::I32(42).as_i64(), Some(42));
        assert_eq!(Value::Bool(true).as_i64(), Some(1));
        assert!(Value::String("test".to_string()).as_i64().is_none());
    }

    #[test]
    fn test_value_as_f64() {
        assert_eq!(Value::F64(2.5).as_f64(), Some(2.5));
        assert_eq!(Value::I64(42).as_f64(), Some(42.0));
    }

    #[test]
    fn test_value_as_str() {
        assert_eq!(Value::String("hello".to_string()).as_str(), Some("hello"));
    }

    #[test]
    fn test_value_to_param() {
        assert_eq!(Value::Null.to_param(), "NULL");
        assert_eq!(Value::Bool(true).to_param(), "TRUE");
        assert_eq!(Value::I64(42).to_param(), "42");
        assert_eq!(Value::String("test".to_string()).to_param(), "'test'");
        assert_eq!(Value::String("it's".to_string()).to_param(), "'it''s'");
    }

    #[test]
    fn test_value_into() {
        let v: Value = 42i64.into();
        assert_eq!(v, Value::I64(42));

        let v: Value = "hello".into();
        assert_eq!(v, Value::String("hello".to_string()));

        let arr: Vec<Value> = vec![Value::I64(1), Value::I64(2)];
        let v: Value = arr.into();
        assert_eq!(v, Value::Array(vec![Value::I64(1), Value::I64(2)]));
    }

    #[test]
    fn test_value_display() {
        assert_eq!(format!("{}", Value::Null), "NULL");
        assert_eq!(format!("{}", Value::Bool(true)), "true");
        assert_eq!(format!("{}", Value::I64(42)), "42");
        assert_eq!(format!("{}", Value::String("test".to_string())), "'test'");
    }
}
