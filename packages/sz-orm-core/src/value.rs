//! Value 类型定义
//!
//! 数据库操作的统一值表示

use std::borrow::Cow;
use std::fmt;

use serde::{Deserialize, Serialize};

/// 数据库值类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub enum Value {
    /// Null 值
    #[default]
    Null,

    /// 布尔值
    Bool(bool),

    /// 8 位有符号整数
    I8(i8),

    /// 16 位有符号整数
    I16(i16),

    /// 32 位有符号整数
    I32(i32),

    /// 64 位有符号整数
    I64(i64),

    /// 8 位无符号整数
    U8(u8),

    /// 16 位无符号整数
    U16(u16),

    /// 32 位无符号整数
    U32(u32),

    /// 64 位无符号整数
    U64(u64),

    /// 32 位浮点数
    F32(f32),

    /// 64 位浮点数
    F64(f64),

    /// 字符串值
    String(String),

    /// 字节值
    Bytes(Vec<u8>),

    /// UUID 值（以字符串形式存储）
    Uuid(String),

    /// 日期值（ISO 8601 格式）
    Date(String),

    /// 日期时间值（ISO 8601 格式）
    DateTime(String),

    /// 时间值
    Time(String),

    /// JSON 值
    Json(String),

    /// 值数组
    Array(Vec<Value>),

    /// 基于 HashMap 的对象值，用于存储关系数据
    Object(std::collections::HashMap<String, Value>),
}

impl Value {
    /// 判断是否为 null
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// 判断是否为布尔值
    pub fn is_bool(&self) -> bool {
        matches!(self, Value::Bool(_))
    }

    /// 判断是否为整数
    pub fn is_i64(&self) -> bool {
        matches!(self, Value::I64(_))
    }

    /// 判断是否为浮点数
    pub fn is_f64(&self) -> bool {
        matches!(self, Value::F64(_))
    }

    /// 判断是否为字符串
    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }

    /// 判断是否为字节
    pub fn is_bytes(&self) -> bool {
        matches!(self, Value::Bytes(_))
    }

    /// 判断是否为对象
    pub fn is_object(&self) -> bool {
        matches!(self, Value::Object(_))
    }

    /// 从 HashMap 构造 Value
    pub fn from_map(map: std::collections::HashMap<String, Value>) -> Self {
        Value::Object(map)
    }

    /// 若可能，返回 &str 形式的值
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// 若可能，返回 i64 形式的值
    /// 支持 F32/F64 → i64 的有损转换（数据库 SUM/AVG 等聚合函数常返回浮点类型）
    /// U64 → i64 使用 `try_from`，超过 `i64::MAX` 时返回 `None`（避免静默截断为负数）
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::I8(v) => Some(*v as i64),
            Value::I16(v) => Some(*v as i64),
            Value::I32(v) => Some(*v as i64),
            Value::I64(v) => Some(*v),
            Value::U8(v) => Some(*v as i64),
            Value::U16(v) => Some(*v as i64),
            Value::U32(v) => Some(*v as i64),
            Value::U64(v) => i64::try_from(*v).ok(),
            Value::F32(v) => Some(*v as i64),
            Value::F64(v) => Some(*v as i64),
            Value::Bool(v) => Some(if *v { 1 } else { 0 }),
            Value::String(s) => s.parse::<i64>().ok(),
            _ => None,
        }
    }

    /// 若可能，返回 f64 形式的值
    /// 支持整数类型 → f64 的转换
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

    /// 若可能，返回 bool 形式的值
    /// 支持整数（非 0 即真）、浮点（非 0.0 即真）、字符串（"1"/"true"/"yes"/"on" 为真）的转换
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

    /// 若可能，返回字节切片形式（&[u8]）的值
    /// 字符串类型会返回其 UTF-8 字节
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Bytes(v) => Some(v),
            Value::String(s) => Some(s.as_bytes()),
            _ => None,
        }
    }

    /// 转换为 SQL 参数字符串（用于直接拼接 SQL 语句）
    /// 字符串类型会进行转义并加引号；字节类型转换为 X'..' 形式
    ///
    /// # 安全性警告
    ///
    /// 本方法使用简单的 `'` → `''` 转义，对 PostgreSQL/SQLite 默认配置安全，
    /// 但对 MySQL 默认配置（backslash 是转义字符）不安全：含 `\` 的字符串
    /// 可能被 MySQL 误解。**生产环境请使用 [`Value::to_param_with_dialect`]**
    /// 以获得方言感知的转义。
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

    /// v0.2.2 修复 H-1：方言感知的 SQL 参数转换
    ///
    /// 与 [`to_param`](Self::to_param) 的区别：字符串类型使用 `dialect.escape_string()`
    /// 而非简单的 `'` → `''` 转义，确保在所有方言下都安全：
    ///
    /// - **MySQL**：转义 `\`、`'`、`\0`、`\n`、`\r`、`\t`、`\x1a`
    /// - **PostgreSQL**：仅转义 `'`（依赖 `standard_conforming_strings=on` 默认配置）
    /// - **SQLite**：仅转义 `'`
    ///
    /// # 推荐用法
    ///
    /// ```ignore
    /// use sz_orm_core::{DbType, get_dialect};
    /// let dialect = get_dialect(DbType::MySQL)?;
    /// let v = Value::String("hello\\nworld".to_string());
    /// let param = v.to_param_with_dialect(&**dialect);
    /// ```
    pub fn to_param_with_dialect(&self, dialect: &dyn crate::dialect::Dialect) -> Cow<'_, str> {
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
            Value::String(s) => Cow::Owned(format!("'{}'", dialect.escape_string(s))),
            Value::Bytes(b) => Cow::Owned(format!("X'{}'", hex_encode(b))),
            Value::Uuid(s) => Cow::Owned(format!("'{}'", dialect.escape_string(s))),
            Value::Date(s) => Cow::Owned(format!("'{}'", dialect.escape_string(s))),
            Value::DateTime(s) => Cow::Owned(format!("'{}'", dialect.escape_string(s))),
            Value::Time(s) => Cow::Owned(format!("'{}'", dialect.escape_string(s))),
            Value::Json(s) => Cow::Owned(format!("'{}'", dialect.escape_string(s))),
            Value::Array(arr) => {
                let params: Vec<String> = arr
                    .iter()
                    .map(|v| v.to_param_with_dialect(dialect).into_owned())
                    .collect();
                Cow::Owned(format!("({})", params.join(", ")))
            }
            Value::Object(_) => Cow::Borrowed("NULL"),
        }
    }

    /// 从任何实现了 `Into<Value>` 的类型构造 Value
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
                let items: Vec<String> = map.iter().map(|(k, v)| format!("{}: {}", k, v)).collect();
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

/// 字符串字面量转义（v0.2.1 修复 Critical D-1）
///
/// # 旧实现的问题
///
/// 旧实现同时使用 `'` → `''`（标准 SQL）和 `\` → `\\`（MySQL 风格）转义，
/// 导致在 PostgreSQL/SQLite 等不把 `\` 作为转义字符的方言下数据完整性受损
/// （写入 `\\n` 字面量而非 `\n`）。
///
/// # 新实现
///
/// 只使用标准 SQL 转义：`'` → `''`。
///
/// - **SQL 注入防御**：`'` 被转义为 `''`，攻击者无法突破字符串字面量
/// - **数据完整性**：在所有方言（MySQL/PG/SQLite/Oracle）下数据保持原样
/// - **MySQL 兼容性**：MySQL 默认把 `\` 作为转义字符，但我们不主动转义 `\`，
///   所以写入的 `\` 会被 MySQL 解析为字面 `\`（与 PG/SQLite 一致）
///
/// # 注意
///
/// 对于需要方言感知转义的场景（如 MySQL 的 `NO_BACKSLASH_ESCAPES` 模式），
/// 应使用 `Dialect::escape_string()` 方法。
fn escape_string(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len() + s.chars().filter(|&c| c == '\'').count());
    for c in s.chars() {
        if c == '\'' {
            escaped.push_str("''");
        } else {
            escaped.push(c);
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
