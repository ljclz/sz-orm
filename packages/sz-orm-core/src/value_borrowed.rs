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
    /// NULL 值
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
    /// 十进制数（字符串形式）
    Decimal(Cow<'a, str>),
    /// 字符串
    String(Cow<'a, str>),
    /// 字节数组
    Bytes(Cow<'a, [u8]>),
    /// UUID（字符串形式）
    Uuid(Cow<'a, str>),
    /// 日期（字符串形式）
    Date(Cow<'a, str>),
    /// 日期时间（字符串形式）
    DateTime(Cow<'a, str>),
    /// 时间（字符串形式）
    Time(Cow<'a, str>),
    /// JSON（字符串形式）
    Json(Cow<'a, str>),
    /// Decimal 原始字节表示（v7.4.0 新增，零拷贝避免 Cow wrapper）
    DecimalBytes(&'a [u8]),
    /// JSON 原始字节表示（v7.4.0 新增）
    JsonBytes(&'a [u8]),
    /// Bytes 直接引用（v7.4.0 新增，避免 Cow wrapper 开销）
    BytesRef(&'a [u8]),
    /// DateTime 整数表示（v7.4.0 新增，Unix timestamp）
    DateTimeInt(i64),
    /// 数组
    Array(Vec<BorrowedValue<'a>>),
    /// 对象（键值对）
    Object(HashMap<String, BorrowedValue<'a>>),
    /// v7.6.0：数组借用引用（零拷贝，避免 Vec 堆分配）
    ArrayRef(&'a [BorrowedValue<'a>]),
    /// v7.6.0：对象借用引用（零拷贝，避免 HashMap 堆分配）
    ObjectRef(&'a [(&'a str, BorrowedValue<'a>)]),
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
            BorrowedValue::DecimalBytes(v) => {
                Value::Decimal(String::from_utf8_lossy(v).to_string())
            }
            BorrowedValue::JsonBytes(v) => Value::Json(String::from_utf8_lossy(v).to_string()),
            BorrowedValue::BytesRef(v) => Value::Bytes(v.to_vec()),
            BorrowedValue::DateTimeInt(v) => Value::DateTime(v.to_string()),
            BorrowedValue::Array(v) => Value::Array(v.iter().map(|b| b.to_owned_value()).collect()),
            BorrowedValue::Object(v) => Value::Object(
                v.iter()
                    .map(|(k, b)| (k.clone(), b.to_owned_value()))
                    .collect(),
            ),
            BorrowedValue::ArrayRef(v) => {
                Value::Array(v.iter().map(|b| b.to_owned_value()).collect())
            }
            BorrowedValue::ObjectRef(v) => Value::Object(
                v.iter()
                    .map(|(k, b)| (k.to_string(), b.to_owned_value()))
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
            #[cfg(feature = "perf-box-str")]
            Value::BoxedStr(v) => BorrowedValue::String(Cow::Borrowed(&**v)),
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

    /// 返回字节引用（如果是 Bytes 类变体）
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            BorrowedValue::Bytes(v) => Some(v.as_ref()),
            BorrowedValue::DecimalBytes(v) => Some(v),
            BorrowedValue::JsonBytes(v) => Some(v),
            BorrowedValue::BytesRef(v) => Some(v),
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
            BorrowedValue::DecimalBytes(v) => write!(f, "{:?}", v),
            BorrowedValue::JsonBytes(v) => write!(f, "{:?}", v),
            BorrowedValue::BytesRef(v) => write!(f, "{:?}", v),
            BorrowedValue::DateTimeInt(v) => write!(f, "{}", v),
            BorrowedValue::Array(v) => write!(f, "{:?}", v),
            BorrowedValue::Object(v) => write!(f, "{:?}", v),
            BorrowedValue::ArrayRef(v) => write!(f, "{:?}", v),
            BorrowedValue::ObjectRef(v) => write!(f, "{:?}", v),
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

    // ========================================================================
    // v7.6.0 任务 1.3：ArrayRef / ObjectRef 零拷贝测试
    // ========================================================================

    #[test]
    fn test_borrowed_value_array_ref() {
        let elements = vec![
            BorrowedValue::I32(1),
            BorrowedValue::I32(2),
            BorrowedValue::I32(3),
        ];
        let borrowed = BorrowedValue::ArrayRef(&elements);
        let owned = borrowed.to_owned_value();
        assert_eq!(
            owned,
            Value::Array(vec![Value::I32(1), Value::I32(2), Value::I32(3)])
        );
    }

    #[test]
    fn test_borrowed_value_object_ref() {
        let entries: Vec<(&str, BorrowedValue)> = vec![
            ("a", BorrowedValue::I32(1)),
            ("b", BorrowedValue::String(Cow::Borrowed("hello"))),
        ];
        let borrowed = BorrowedValue::ObjectRef(&entries);
        let owned = borrowed.to_owned_value();
        let expected = {
            let mut m = HashMap::new();
            m.insert("a".to_string(), Value::I32(1));
            m.insert("b".to_string(), Value::String("hello".into()));
            Value::Object(m)
        };
        assert_eq!(owned, expected);
    }

    #[test]
    fn test_borrowed_value_array_ref_zero_copy() {
        let elements = vec![BorrowedValue::I64(42), BorrowedValue::I64(99)];
        let borrowed = BorrowedValue::ArrayRef(&elements);
        if let BorrowedValue::ArrayRef(slice) = &borrowed {
            assert_eq!(slice.len(), 2);
            assert_eq!(slice[0], BorrowedValue::I64(42));
        } else {
            panic!("应为 ArrayRef");
        }
    }

    #[test]
    fn test_borrowed_value_object_ref_zero_copy() {
        let entries: Vec<(&str, BorrowedValue)> = vec![("key", BorrowedValue::I32(100))];
        let borrowed = BorrowedValue::ObjectRef(&entries);
        if let BorrowedValue::ObjectRef(slice) = &borrowed {
            assert_eq!(slice.len(), 1);
            assert_eq!(slice[0].0, "key");
            assert_eq!(slice[0].1, BorrowedValue::I32(100));
        } else {
            panic!("应为 ObjectRef");
        }
    }

    #[test]
    fn test_borrowed_value_array_ref_empty() {
        let elements: Vec<BorrowedValue> = vec![];
        let borrowed = BorrowedValue::ArrayRef(&elements);
        let owned = borrowed.to_owned_value();
        assert_eq!(owned, Value::Array(vec![]));
    }

    #[test]
    fn test_borrowed_value_object_ref_empty() {
        let entries: Vec<(&str, BorrowedValue)> = vec![];
        let borrowed = BorrowedValue::ObjectRef(&entries);
        let owned = borrowed.to_owned_value();
        assert_eq!(owned, Value::Object(HashMap::new()));
    }

    #[test]
    fn test_borrowed_value_array_ref_nested() {
        let inner = vec![BorrowedValue::I32(10), BorrowedValue::I32(20)];
        let outer = vec![BorrowedValue::ArrayRef(&inner), BorrowedValue::I32(30)];
        let borrowed = BorrowedValue::ArrayRef(&outer);
        let owned = borrowed.to_owned_value();
        assert_eq!(
            owned,
            Value::Array(vec![
                Value::Array(vec![Value::I32(10), Value::I32(20)]),
                Value::I32(30),
            ])
        );
    }

    #[test]
    fn test_borrowed_value_array_ref_display() {
        let elements = vec![BorrowedValue::I32(1), BorrowedValue::I32(2)];
        let borrowed = BorrowedValue::ArrayRef(&elements);
        let s = format!("{}", borrowed);
        assert!(s.contains("1"));
        assert!(s.contains("2"));
    }

    #[test]
    fn t26vb_decimal_bytes_to_owned() {
        let bytes = b"123.45";
        let borrowed = BorrowedValue::DecimalBytes(bytes);
        let owned = borrowed.to_owned_value();
        assert_eq!(owned, Value::Decimal("123.45".into()));
    }

    #[test]
    fn t26vb_json_bytes_to_owned() {
        let bytes = br#"{"k":"v"}"#;
        let borrowed = BorrowedValue::JsonBytes(bytes);
        let owned = borrowed.to_owned_value();
        assert_eq!(owned, Value::Json(r#"{"k":"v"}"#.into()));
    }

    #[test]
    fn t26vb_bytes_ref_to_owned() {
        let bytes = [1u8, 2, 3];
        let borrowed = BorrowedValue::BytesRef(&bytes);
        let owned = borrowed.to_owned_value();
        assert_eq!(owned, Value::Bytes(vec![1, 2, 3]));
    }

    #[test]
    fn t26vb_datetime_int_to_owned() {
        let borrowed = BorrowedValue::DateTimeInt(1700000000);
        let owned = borrowed.to_owned_value();
        assert_eq!(owned, Value::DateTime("1700000000".into()));
    }

    #[test]
    fn t26vb_decimal_bytes_as_bytes() {
        let bytes = b"123.45";
        let borrowed = BorrowedValue::DecimalBytes(bytes);
        assert_eq!(borrowed.as_bytes(), Some(&b"123.45"[..]));
    }

    #[test]
    fn t26vb_json_bytes_as_bytes() {
        let bytes = br#"{"k":1}"#;
        let borrowed = BorrowedValue::JsonBytes(bytes);
        assert_eq!(borrowed.as_bytes(), Some(&br#"{"k":1}"#[..]));
    }

    #[test]
    fn t26vb_bytes_ref_as_bytes() {
        let bytes = [1u8, 2, 3];
        let borrowed = BorrowedValue::BytesRef(&bytes);
        assert_eq!(borrowed.as_bytes(), Some(&[1u8, 2, 3][..]));
    }

    #[test]
    fn t26vb_as_str_all_string_variants() {
        let dec = BorrowedValue::Decimal(Cow::Borrowed("3.14"));
        assert_eq!(dec.as_str(), Some("3.14"));
        let uid = BorrowedValue::Uuid(Cow::Borrowed("550e8400"));
        assert_eq!(uid.as_str(), Some("550e8400"));
        let dt = BorrowedValue::Date(Cow::Borrowed("2026-01-01"));
        assert_eq!(dt.as_str(), Some("2026-01-01"));
        let dti = BorrowedValue::DateTime(Cow::Borrowed("2026-01-01T00:00:00"));
        assert_eq!(dti.as_str(), Some("2026-01-01T00:00:00"));
        let tm = BorrowedValue::Time(Cow::Borrowed("12:00:00"));
        assert_eq!(tm.as_str(), Some("12:00:00"));
        let js = BorrowedValue::Json(Cow::Borrowed("{}"));
        assert_eq!(js.as_str(), Some("{}"));
    }

    #[test]
    fn t26vb_display_all_numeric_variants() {
        assert_eq!(format!("{}", BorrowedValue::Null), "NULL");
        assert_eq!(format!("{}", BorrowedValue::Bool(true)), "true");
        assert_eq!(format!("{}", BorrowedValue::I8(1)), "1");
        assert_eq!(format!("{}", BorrowedValue::I16(2)), "2");
        assert_eq!(format!("{}", BorrowedValue::I32(3)), "3");
        assert_eq!(format!("{}", BorrowedValue::I64(4)), "4");
        assert_eq!(format!("{}", BorrowedValue::U8(5)), "5");
        assert_eq!(format!("{}", BorrowedValue::U16(6)), "6");
        assert_eq!(format!("{}", BorrowedValue::U32(7)), "7");
        assert_eq!(format!("{}", BorrowedValue::U64(8)), "8");
        assert_eq!(format!("{}", BorrowedValue::F32(1.5)), "1.5");
        assert_eq!(format!("{}", BorrowedValue::F64(2.5)), "2.5");
    }

    #[test]
    fn t26vb_display_string_variants() {
        assert_eq!(
            format!("{}", BorrowedValue::Decimal(Cow::Borrowed("3.14"))),
            "3.14"
        );
        assert_eq!(
            format!("{}", BorrowedValue::String(Cow::Borrowed("hi"))),
            "hi"
        );
        assert_eq!(
            format!("{}", BorrowedValue::Uuid(Cow::Borrowed("uid"))),
            "uid"
        );
        assert_eq!(format!("{}", BorrowedValue::Date(Cow::Borrowed("d"))), "d");
        assert_eq!(
            format!("{}", BorrowedValue::DateTime(Cow::Borrowed("dt"))),
            "dt"
        );
        assert_eq!(format!("{}", BorrowedValue::Time(Cow::Borrowed("t"))), "t");
        assert_eq!(format!("{}", BorrowedValue::Json(Cow::Borrowed("j"))), "j");
    }

    #[test]
    fn t26vb_display_bytes_variants() {
        let s = format!("{}", BorrowedValue::Bytes(Cow::Borrowed(&[1u8, 2][..])));
        assert!(s.contains("1"));
        let s = format!("{}", BorrowedValue::DecimalBytes(b"x"));
        assert!(s.contains("120")); // 'x' = 120 in decimal
        let s = format!("{}", BorrowedValue::JsonBytes(b"j"));
        assert!(s.contains("106")); // 'j' = 106 in decimal
        let s = format!("{}", BorrowedValue::BytesRef(&[1u8]));
        assert!(s.contains("1"));
    }

    #[test]
    fn t26vb_display_datetime_int() {
        assert_eq!(format!("{}", BorrowedValue::DateTimeInt(42)), "42");
    }

    #[test]
    fn t26vb_display_array_and_object() {
        let arr = vec![BorrowedValue::I32(1)];
        let s = format!("{}", BorrowedValue::Array(arr));
        assert!(s.contains("1"));
        let mut m = HashMap::new();
        m.insert("k".to_string(), BorrowedValue::I32(1));
        let s = format!("{}", BorrowedValue::Object(m));
        assert!(s.contains("k"));
    }

    #[test]
    fn t26vb_display_object_ref() {
        let entries: Vec<(&str, BorrowedValue)> = vec![("k", BorrowedValue::I32(1))];
        let s = format!("{}", BorrowedValue::ObjectRef(&entries));
        assert!(s.contains("k"));
    }

    #[test]
    fn t26vb_eq_value_decimal_bytes() {
        let borrowed = BorrowedValue::DecimalBytes(b"3.14");
        assert!(borrowed.eq_value(&Value::Decimal("3.14".into())));
    }

    #[test]
    fn t26vb_eq_value_json_bytes() {
        let borrowed = BorrowedValue::JsonBytes(br#"{"k":1}"#);
        assert!(borrowed.eq_value(&Value::Json(r#"{"k":1}"#.into())));
    }

    #[test]
    fn t26vb_eq_value_bytes_ref() {
        let borrowed = BorrowedValue::BytesRef(&[1u8, 2, 3]);
        assert!(borrowed.eq_value(&Value::Bytes(vec![1, 2, 3])));
    }

    #[test]
    fn t26vb_eq_value_datetime_int() {
        let borrowed = BorrowedValue::DateTimeInt(42);
        assert!(borrowed.eq_value(&Value::DateTime("42".into())));
    }

    #[test]
    fn t26vb_borrowed_row_data_default() {
        let row = BorrowedRowData::default();
        assert!(row.is_empty());
    }

    #[test]
    fn t26vb_borrowed_row_data_is_not_null() {
        let mut row = BorrowedRowData::new();
        row.set("a", BorrowedValue::I32(1));
        row.set("b", BorrowedValue::Null);
        assert!(row.is_not_null("a"));
        assert!(!row.is_not_null("b"));
        assert!(!row.is_not_null("missing"));
    }

    #[test]
    fn t26vb_borrowed_row_data_len_and_empty() {
        let mut row = BorrowedRowData::new();
        assert!(row.is_empty());
        row.set("a", BorrowedValue::I32(1));
        assert!(!row.is_empty());
        assert_eq!(row.len(), 1);
    }

    #[test]
    fn t26vb_borrowed_value_clone() {
        let bv = BorrowedValue::I32(42);
        let bv2 = bv.clone();
        assert_eq!(bv, bv2);
    }

    #[test]
    fn t26vb_borrowed_value_partial_eq() {
        assert_eq!(BorrowedValue::I32(1), BorrowedValue::I32(1));
        assert_ne!(BorrowedValue::I32(1), BorrowedValue::I32(2));
        assert_ne!(BorrowedValue::I32(1), BorrowedValue::Null);
    }
}
