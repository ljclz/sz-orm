//! v3.2.0 零拷贝序列化 — 借用型值类型
//!
//! `BorrowedValue<'a>` 与 `Value` 变体一一对应，但字符串类变体使用 `Cow<'a, str>`
//! 替代 `String`，字节变体使用 `Cow<'a, [u8]>` 替代 `Vec<u8>`。
//! 生命周期 `'a` 绑定原始行缓冲区，实现零拷贝反序列化。

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;

use crate::result_map::RowData;
use crate::value::Value;

/// 借用型值枚举（与 `Value` 一一对应，字符串/字节使用 `Cow` 借用）
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub enum BorrowedValue<'a> {
    #[default]
    Null,
    Bool(bool),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    F32(f32),
    F64(f64),
    Decimal(Cow<'a, str>),
    String(Cow<'a, str>),
    Bytes(Cow<'a, [u8]>),
    Uuid(Cow<'a, str>),
    Date(Cow<'a, str>),
    DateTime(Cow<'a, str>),
    Time(Cow<'a, str>),
    Json(Cow<'a, str>),
    Array(Vec<BorrowedValue<'a>>),
    Object(HashMap<String, BorrowedValue<'a>>),
}

impl<'a> BorrowedValue<'a> {
    /// 转换为 owned `Value`
    pub fn to_owned_value(&self) -> Value {
        match self {
            BorrowedValue::Null => Value::Null,
            BorrowedValue::Bool(v) => Value::Bool(*v),
            BorrowedValue::I8(v) => Value::I8(*v),
            BorrowedValue::I16(v) => Value::I16(*v),
            BorrowedValue::I32(v) => Value::I32(*v),
            BorrowedValue::I64(v) => Value::I64(*v),
            BorrowedValue::U8(v) => Value::U8(*v),
            BorrowedValue::U16(v) => Value::U16(*v),
            BorrowedValue::U32(v) => Value::U32(*v),
            BorrowedValue::U64(v) => Value::U64(*v),
            BorrowedValue::F32(v) => Value::F32(*v),
            BorrowedValue::F64(v) => Value::F64(*v),
            BorrowedValue::Decimal(v) => Value::Decimal(v.to_string()),
            BorrowedValue::String(v) => Value::String(v.to_string()),
            BorrowedValue::Bytes(v) => Value::Bytes(v.to_vec()),
            BorrowedValue::Uuid(v) => Value::Uuid(v.to_string()),
            BorrowedValue::Date(v) => Value::Date(v.to_string()),
            BorrowedValue::DateTime(v) => Value::DateTime(v.to_string()),
            BorrowedValue::Time(v) => Value::Time(v.to_string()),
            BorrowedValue::Json(v) => Value::Json(v.to_string()),
            BorrowedValue::Array(v) => Value::Array(v.iter().map(|b| b.to_owned_value()).collect()),
            BorrowedValue::Object(v) => Value::Object(
                v.iter()
                    .map(|(k, b)| (k.clone(), b.to_owned_value()))
                    .collect(),
            ),
        }
    }

    /// 从 `&Value` 构造 `BorrowedValue`（零拷贝借用）
    pub fn from_value(value: &'a Value) -> Self {
        match value {
            Value::Null => BorrowedValue::Null,
            Value::Bool(v) => BorrowedValue::Bool(*v),
            Value::I8(v) => BorrowedValue::I8(*v),
            Value::I16(v) => BorrowedValue::I16(*v),
            Value::I32(v) => BorrowedValue::I32(*v),
            Value::I64(v) => BorrowedValue::I64(*v),
            Value::U8(v) => BorrowedValue::U8(*v),
            Value::U16(v) => BorrowedValue::U16(*v),
            Value::U32(v) => BorrowedValue::U32(*v),
            Value::U64(v) => BorrowedValue::U64(*v),
            Value::F32(v) => BorrowedValue::F32(*v),
            Value::F64(v) => BorrowedValue::F64(*v),
            Value::Decimal(v) => BorrowedValue::Decimal(Cow::Borrowed(v.as_str())),
            Value::String(v) => BorrowedValue::String(Cow::Borrowed(v.as_str())),
            Value::Bytes(v) => BorrowedValue::Bytes(Cow::Borrowed(v.as_slice())),
            Value::Uuid(v) => BorrowedValue::Uuid(Cow::Borrowed(v.as_str())),
            Value::Date(v) => BorrowedValue::Date(Cow::Borrowed(v.as_str())),
            Value::DateTime(v) => BorrowedValue::DateTime(Cow::Borrowed(v.as_str())),
            Value::Time(v) => BorrowedValue::Time(Cow::Borrowed(v.as_str())),
            Value::Json(v) => BorrowedValue::Json(Cow::Borrowed(v.as_str())),
            Value::Array(v) => {
                BorrowedValue::Array(v.iter().map(BorrowedValue::from_value).collect())
            }
            Value::Object(v) => BorrowedValue::Object(
                v.iter()
                    .map(|(k, val)| (k.clone(), BorrowedValue::from_value(val)))
                    .collect(),
            ),
        }
    }

    /// 返回字符串引用（如果是字符串类变体）
    pub fn as_str(&self) -> Option<&str> {
        match self {
            BorrowedValue::Decimal(v) => Some(v.as_ref()),
            BorrowedValue::String(v) => Some(v.as_ref()),
            BorrowedValue::Uuid(v) => Some(v.as_ref()),
            BorrowedValue::Date(v) => Some(v.as_ref()),
            BorrowedValue::DateTime(v) => Some(v.as_ref()),
            BorrowedValue::Time(v) => Some(v.as_ref()),
            BorrowedValue::Json(v) => Some(v.as_ref()),
            _ => None,
        }
    }

    /// 返回字节引用（如果是 Bytes 变体）
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            BorrowedValue::Bytes(v) => Some(v.as_ref()),
            _ => None,
        }
    }

    /// 与 `Value` 等价比较
    pub fn eq_value(&self, other: &Value) -> bool {
        &self.to_owned_value() == other
    }
}

impl<'a> fmt::Display for BorrowedValue<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BorrowedValue::Null => write!(f, "NULL"),
            BorrowedValue::Bool(v) => write!(f, "{}", v),
            BorrowedValue::I8(v) => write!(f, "{}", v),
            BorrowedValue::I16(v) => write!(f, "{}", v),
            BorrowedValue::I32(v) => write!(f, "{}", v),
            BorrowedValue::I64(v) => write!(f, "{}", v),
            BorrowedValue::U8(v) => write!(f, "{}", v),
            BorrowedValue::U16(v) => write!(f, "{}", v),
            BorrowedValue::U32(v) => write!(f, "{}", v),
            BorrowedValue::U64(v) => write!(f, "{}", v),
            BorrowedValue::F32(v) => write!(f, "{}", v),
            BorrowedValue::F64(v) => write!(f, "{}", v),
            BorrowedValue::Decimal(v) => write!(f, "{}", v),
            BorrowedValue::String(v) => write!(f, "{}", v),
            BorrowedValue::Bytes(v) => write!(f, "{:?}", v.as_ref()),
            BorrowedValue::Uuid(v) => write!(f, "{}", v),
            BorrowedValue::Date(v) => write!(f, "{}", v),
            BorrowedValue::DateTime(v) => write!(f, "{}", v),
            BorrowedValue::Time(v) => write!(f, "{}", v),
            BorrowedValue::Json(v) => write!(f, "{}", v),
            BorrowedValue::Array(v) => write!(f, "{:?}", v),
            BorrowedValue::Object(v) => write!(f, "{:?}", v),
        }
    }
}

// ─── BorrowedRowData ─────────────────────────────────────────────

/// 借用型行数据（列名 -> BorrowedValue）
pub struct BorrowedRowData<'a> {
    columns: HashMap<String, BorrowedValue<'a>>,
}

impl<'a> BorrowedRowData<'a> {
    /// 创建空行
    pub fn new() -> Self {
        Self {
            columns: HashMap::new(),
        }
    }

    /// 从 schema 列名列表创建空行
    pub fn with_schema(schema: &[&str]) -> Self {
        let mut columns = HashMap::new();
        for name in schema {
            columns.insert((*name).to_string(), BorrowedValue::Null);
        }
        Self { columns }
    }

    /// 插入/更新列
    pub fn set(&mut self, col: impl Into<String>, value: BorrowedValue<'a>) {
        self.columns.insert(col.into(), value);
    }

    /// 获取列值
    pub fn get(&self, col: &str) -> Option<&BorrowedValue<'a>> {
        self.columns.get(col)
    }

    /// 迭代所有列
    pub fn iter(&self) -> impl Iterator<Item = (&String, &BorrowedValue<'a>)> {
        self.columns.iter()
    }

    /// 列数
    pub fn len(&self) -> usize {
        self.columns.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    /// 判断列是否存在且非 NULL
    pub fn is_not_null(&self, column: &str) -> bool {
        match self.columns.get(column) {
            Some(BorrowedValue::Null) | None => false,
            Some(_) => true,
        }
    }

    /// 转换为 owned `RowData`
    pub fn to_owned_row(&self) -> RowData {
        let owned: HashMap<String, Value> = self
            .columns
            .iter()
            .map(|(k, v)| (k.clone(), v.to_owned_value()))
            .collect();
        RowData::new(owned)
    }
}

impl<'a> Default for BorrowedRowData<'a> {
    fn default() -> Self {
        Self::new()
    }
}

// ─── 单元测试 ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_borrowed_value_variants_match_value() {
        let values = vec![
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
            Value::F32(1.5),
            Value::F64(2.5),
            Value::Decimal("3.14".into()),
            Value::String("hello".into()),
            Value::Bytes(vec![1, 2, 3]),
            Value::Uuid("550e8400-e29b-41d4-a716-446655440000".into()),
            Value::Date("2026-08-08".into()),
            Value::DateTime("2026-08-08T12:00:00".into()),
            Value::Time("12:00:00".into()),
            Value::Json("{\"key\":\"value\"}".into()),
        ];

        for v in &values {
            let borrowed = BorrowedValue::from_value(v);
            assert_eq!(borrowed.to_owned_value(), *v, "往返转换应一致");
        }
    }

    #[test]
    fn test_borrowed_value_roundtrip() {
        let value = Value::String("hello world".into());
        let borrowed = BorrowedValue::from_value(&value);
        assert_eq!(borrowed.to_owned_value(), value);
    }

    #[test]
    fn test_borrowed_value_as_str() {
        let value = Value::String("hello".into());
        let borrowed = BorrowedValue::from_value(&value);
        assert_eq!(borrowed.as_str(), Some("hello"));
    }

    #[test]
    fn test_borrowed_value_as_str_non_string() {
        let value = Value::I32(42);
        let borrowed = BorrowedValue::from_value(&value);
        assert_eq!(borrowed.as_str(), None);
    }

    #[test]
    fn test_borrowed_value_as_bytes() {
        let value = Value::Bytes(vec![1, 2, 3]);
        let borrowed = BorrowedValue::from_value(&value);
        assert_eq!(borrowed.as_bytes(), Some(&[1u8, 2, 3][..]));
    }

    #[test]
    fn test_borrowed_value_as_bytes_non_bytes() {
        let value = Value::I32(42);
        let borrowed = BorrowedValue::from_value(&value);
        assert_eq!(borrowed.as_bytes(), None);
    }

    #[test]
    fn test_borrowed_value_eq_value() {
        let v1 = Value::String("hello".into());
        let v2 = Value::String("hello".into());
        let v3 = Value::String("world".into());

        let borrowed = BorrowedValue::from_value(&v1);
        assert!(borrowed.eq_value(&v2), "相同值应相等");
        assert!(!borrowed.eq_value(&v3), "不同值应不等");
    }

    #[test]
    fn test_borrowed_value_cow_borrowed_zero_copy() {
        let s = String::from("test string");
        let value = Value::String(s);
        let borrowed = BorrowedValue::from_value(&value);

        if let BorrowedValue::String(Cow::Borrowed(_)) = &borrowed {
        } else {
            panic!("应为 Cow::Borrowed");
        }
    }

    #[test]
    fn test_borrowed_value_array() {
        let value = Value::Array(vec![Value::I32(1), Value::I32(2), Value::I32(3)]);
        let borrowed = BorrowedValue::from_value(&value);
        assert_eq!(borrowed.to_owned_value(), value);
    }

    #[test]
    fn test_borrowed_value_object() {
        let mut map = HashMap::new();
        map.insert("key".to_string(), Value::I32(42));
        let value = Value::Object(map);
        let borrowed = BorrowedValue::from_value(&value);
        assert_eq!(borrowed.to_owned_value(), value);
    }

    #[test]
    fn test_borrowed_value_default() {
        let borrowed = BorrowedValue::default();
        assert_eq!(borrowed.to_owned_value(), Value::Null);
    }

    #[test]
    fn test_borrowed_row_data_new() {
        let row = BorrowedRowData::new();
        assert!(row.is_empty());
        assert_eq!(row.len(), 0);
    }

    #[test]
    fn test_borrowed_row_data_with_schema() {
        let row = BorrowedRowData::with_schema(&["id", "name", "email"]);
        assert_eq!(row.len(), 3);
        assert!(row.get("id").is_some());
        assert!(row.get("name").is_some());
        assert!(row.get("email").is_some());
        assert!(row.get("nonexistent").is_none());
    }

    #[test]
    fn test_borrowed_row_data_set_get() {
        let mut row = BorrowedRowData::new();
        row.set("id", BorrowedValue::I64(42));
        row.set("name", BorrowedValue::String(Cow::Borrowed("Alice")));

        assert_eq!(row.get("id"), Some(&BorrowedValue::I64(42)));
        assert!(row.get("name").is_some());
    }

    #[test]
    fn test_borrowed_row_data_to_owned() {
        let mut row = BorrowedRowData::new();
        row.set("id", BorrowedValue::I64(42));
        row.set("name", BorrowedValue::String(Cow::Borrowed("Alice")));

        let owned = row.to_owned_row();
        assert_eq!(owned.get("id").cloned(), Some(Value::I64(42)));
    }

    #[test]
    fn test_borrowed_row_data_iter() {
        let mut row = BorrowedRowData::new();
        row.set("a", BorrowedValue::I32(1));
        row.set("b", BorrowedValue::I32(2));

        let entries: Vec<_> = row.iter().collect();
        assert_eq!(entries.len(), 2);
    }
}
