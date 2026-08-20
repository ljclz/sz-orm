//! Oracle 数据类型映射扩展
//!
//! 提供 [`TypeMapping`] 在 sz-orm-core 的 `Value` 与 Oracle 数据类型之间
//! 双向映射，支持精度/标度、字符集、约束等元信息。

use std::collections::HashMap;
use std::fmt;

use sz_orm_core::Value;

/// Oracle 列类型描述（含精度/标度/长度等元信息）
#[derive(Debug, Clone, PartialEq)]
pub struct OracleColumnMeta {
    /// 列名
    pub name: String,
    /// 数据类型
    pub data_type: OracleTypeKind,
    /// 长度（VARCHAR2/CHAR/RAW）
    pub length: Option<u32>,
    /// 精度（NUMBER）
    pub precision: Option<u8>,
    /// 标度（NUMBER）
    pub scale: Option<i8>,
    /// 是否可空
    pub nullable: bool,
    /// 默认值
    pub default_value: Option<String>,
    /// 字符集
    pub charset: Option<String>,
    /// 是否主键
    pub is_primary_key: bool,
    /// 是否唯一
    pub is_unique: bool,
}

impl OracleColumnMeta {
    /// 创建新的列元信息
    #[must_use]
    pub fn new(name: &str, data_type: OracleTypeKind) -> Self {
        Self {
            name: name.to_string(),
            data_type,
            length: None,
            precision: None,
            scale: None,
            nullable: true,
            default_value: None,
            charset: None,
            is_primary_key: false,
            is_unique: false,
        }
    }

    /// 设置长度
    #[must_use]
    pub fn with_length(mut self, length: u32) -> Self {
        self.length = Some(length);
        self
    }

    /// 设置精度与标度
    #[must_use]
    pub fn with_precision(mut self, precision: u8, scale: i8) -> Self {
        self.precision = Some(precision);
        self.scale = Some(scale);
        self
    }

    /// 设置是否可空
    #[must_use]
    pub fn with_nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }

    /// 设置默认值
    #[must_use]
    pub fn with_default(mut self, value: &str) -> Self {
        self.default_value = Some(value.to_string());
        self
    }

    /// 设置字符集
    #[must_use]
    pub fn with_charset(mut self, charset: &str) -> Self {
        self.charset = Some(charset.to_string());
        self
    }

    /// 标记为主键
    #[must_use]
    pub fn primary_key(mut self) -> Self {
        self.is_primary_key = true;
        self.nullable = false;
        self
    }

    /// 标记为唯一
    #[must_use]
    pub fn unique(mut self) -> Self {
        self.is_unique = true;
        self
    }

    /// 生成 DDL 片段（不含列名）
    #[must_use]
    pub fn to_ddl(&self) -> String {
        let mut parts = Vec::new();
        parts.push(
            self.data_type
                .to_ddl(self.length, self.precision, self.scale),
        );
        if !self.nullable {
            parts.push("NOT NULL".to_string());
        }
        if let Some(ref default) = self.default_value {
            parts.push(format!("DEFAULT {default}"));
        }
        parts.join(" ")
    }

    /// 生成完整列定义（列名 + 类型 + 约束）
    #[must_use]
    pub fn to_column_ddl(&self) -> String {
        format!("{} {}", self.name, self.to_ddl())
    }
}

impl fmt::Display for OracleColumnMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_column_ddl())
    }
}

/// Oracle 类型种类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OracleTypeKind {
    Number,
    Varchar2,
    Nvarchar2,
    Char,
    Nchar,
    Clob,
    Nclob,
    Blob,
    Raw,
    Long,
    LongRaw,
    Date,
    Timestamp,
    TimestampTz,
    TimestampLtz,
    BinaryFloat,
    BinaryDouble,
    Rowid,
    Urowid,
    Xmltype,
    Json,
    Boolean,
}

impl OracleTypeKind {
    /// 从类型名解析
    #[must_use]
    pub fn parse_name(name: &str) -> Self {
        let upper = name.to_uppercase();
        if upper.starts_with("TIMESTAMP") {
            if upper.contains("LOCAL") {
                return OracleTypeKind::TimestampLtz;
            }
            if upper.contains("TIME ZONE") || upper.contains("TZ") {
                return OracleTypeKind::TimestampTz;
            }
            return OracleTypeKind::Timestamp;
        }
        match upper.as_str() {
            "NUMBER" | "NUMERIC" | "DECIMAL" | "DEC" | "INTEGER" | "INT" | "FLOAT" | "REAL" => {
                OracleTypeKind::Number
            }
            "VARCHAR2" | "VARCHAR" => OracleTypeKind::Varchar2,
            "NVARCHAR2" => OracleTypeKind::Nvarchar2,
            "CHAR" => OracleTypeKind::Char,
            "NCHAR" => OracleTypeKind::Nchar,
            "CLOB" => OracleTypeKind::Clob,
            "NCLOB" => OracleTypeKind::Nclob,
            "BLOB" => OracleTypeKind::Blob,
            "RAW" => OracleTypeKind::Raw,
            "LONG" => OracleTypeKind::Long,
            "LONG RAW" => OracleTypeKind::LongRaw,
            "DATE" => OracleTypeKind::Date,
            "BINARY_FLOAT" => OracleTypeKind::BinaryFloat,
            "BINARY_DOUBLE" => OracleTypeKind::BinaryDouble,
            "ROWID" => OracleTypeKind::Rowid,
            "UROWID" => OracleTypeKind::Urowid,
            "XMLTYPE" => OracleTypeKind::Xmltype,
            "JSON" => OracleTypeKind::Json,
            "BOOLEAN" => OracleTypeKind::Boolean,
            _ => OracleTypeKind::Varchar2,
        }
    }

    /// 生成 DDL 类型片段
    #[must_use]
    pub fn to_ddl(&self, length: Option<u32>, precision: Option<u8>, scale: Option<i8>) -> String {
        match self {
            OracleTypeKind::Number => match (precision, scale) {
                (Some(p), Some(s)) => format!("NUMBER({p}, {s})"),
                (Some(p), None) => format!("NUMBER({p})"),
                _ => "NUMBER".to_string(),
            },
            OracleTypeKind::Varchar2 => {
                format!("VARCHAR2({})", length.unwrap_or(4000))
            }
            OracleTypeKind::Nvarchar2 => {
                format!("NVARCHAR2({})", length.unwrap_or(2000))
            }
            OracleTypeKind::Char => format!("CHAR({})", length.unwrap_or(1)),
            OracleTypeKind::Nchar => format!("NCHAR({})", length.unwrap_or(1)),
            OracleTypeKind::Raw => format!("RAW({})", length.unwrap_or(2000)),
            OracleTypeKind::Timestamp => "TIMESTAMP".to_string(),
            OracleTypeKind::TimestampTz => "TIMESTAMP WITH TIME ZONE".to_string(),
            OracleTypeKind::TimestampLtz => "TIMESTAMP WITH LOCAL TIME ZONE".to_string(),
            OracleTypeKind::BinaryFloat => "BINARY_FLOAT".to_string(),
            OracleTypeKind::BinaryDouble => "BINARY_DOUBLE".to_string(),
            OracleTypeKind::Date => "DATE".to_string(),
            OracleTypeKind::Clob => "CLOB".to_string(),
            OracleTypeKind::Nclob => "NCLOB".to_string(),
            OracleTypeKind::Blob => "BLOB".to_string(),
            OracleTypeKind::Long => "LONG".to_string(),
            OracleTypeKind::LongRaw => "LONG RAW".to_string(),
            OracleTypeKind::Rowid => "ROWID".to_string(),
            OracleTypeKind::Urowid => "UROWID".to_string(),
            OracleTypeKind::Xmltype => "XMLTYPE".to_string(),
            OracleTypeKind::Json => "JSON".to_string(),
            OracleTypeKind::Boolean => "BOOLEAN".to_string(),
        }
    }

    /// 是否为数值类型
    #[must_use]
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            OracleTypeKind::Number | OracleTypeKind::BinaryFloat | OracleTypeKind::BinaryDouble
        )
    }

    /// 是否为字符串类型
    #[must_use]
    pub fn is_string(&self) -> bool {
        matches!(
            self,
            OracleTypeKind::Varchar2
                | OracleTypeKind::Nvarchar2
                | OracleTypeKind::Char
                | OracleTypeKind::Nchar
                | OracleTypeKind::Long
                | OracleTypeKind::Xmltype
                | OracleTypeKind::Clob
                | OracleTypeKind::Nclob
        )
    }

    /// 是否为二进制类型
    #[must_use]
    pub fn is_binary(&self) -> bool {
        matches!(
            self,
            OracleTypeKind::Raw | OracleTypeKind::LongRaw | OracleTypeKind::Blob
        )
    }

    /// 是否为时间类型
    #[must_use]
    pub fn is_temporal(&self) -> bool {
        matches!(
            self,
            OracleTypeKind::Date
                | OracleTypeKind::Timestamp
                | OracleTypeKind::TimestampTz
                | OracleTypeKind::TimestampLtz
        )
    }

    /// 是否为 LOB 类型
    #[must_use]
    pub fn is_lob(&self) -> bool {
        matches!(
            self,
            OracleTypeKind::Clob | OracleTypeKind::Nclob | OracleTypeKind::Blob
        )
    }

    /// 推断 sz-orm Value 类型
    #[must_use]
    pub fn value_kind(&self) -> ValueKind {
        if self.is_numeric() {
            ValueKind::Number
        } else if self.is_string() {
            ValueKind::String
        } else if self.is_binary() {
            ValueKind::Bytes
        } else if self.is_temporal() {
            ValueKind::DateTime
        } else {
            match self {
                OracleTypeKind::Boolean => ValueKind::Bool,
                OracleTypeKind::Json => ValueKind::Json,
                OracleTypeKind::Rowid | OracleTypeKind::Urowid => ValueKind::String,
                _ => ValueKind::Other,
            }
        }
    }
}

/// sz-orm Value 类型分类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Number,
    String,
    Bytes,
    DateTime,
    Bool,
    Json,
    Other,
}

/// 类型映射器
///
/// 在 OracleTypeKind 与 sz-orm Value 之间双向映射。
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct TypeMapping {
    /// 自定义类型映射（Oracle 类型名 -> ValueKind）
    custom_mappings: HashMap<String, ValueKind>,
}


impl TypeMapping {
    /// 创建新的类型映射器
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册自定义类型映射
    #[must_use]
    pub fn register(mut self, oracle_type: &str, kind: ValueKind) -> Self {
        self.custom_mappings
            .insert(oracle_type.to_uppercase(), kind);
        self
    }

    /// 从 Oracle 类型名推断 ValueKind
    #[must_use]
    pub fn to_value_kind(&self, oracle_type: &str) -> ValueKind {
        let upper = oracle_type.to_uppercase();
        if let Some(&kind) = self.custom_mappings.get(&upper) {
            return kind;
        }
        OracleTypeKind::parse_name(&upper).value_kind()
    }

    /// 从 sz-orm Value 推断 OracleTypeKind
    #[must_use]
    pub fn from_value(value: &Value) -> OracleTypeKind {
        match value {
            Value::Null => OracleTypeKind::Varchar2,
            Value::Bool(_) => OracleTypeKind::Boolean,
            Value::I8(_) | Value::I16(_) | Value::I32(_) | Value::U8(_) | Value::U16(_) => {
                OracleTypeKind::Number
            }
            Value::I64(_) | Value::U32(_) | Value::U64(_) => OracleTypeKind::Number,
            Value::F32(_) | Value::F64(_) => OracleTypeKind::BinaryDouble,
            Value::String(_) => OracleTypeKind::Varchar2,
            Value::Decimal(_) => OracleTypeKind::Number,
            Value::Bytes(_) => OracleTypeKind::Raw,
            Value::Date(_) => OracleTypeKind::Date,
            Value::DateTime(_) => OracleTypeKind::Timestamp,
            Value::Time(_) => OracleTypeKind::Timestamp,
            Value::Uuid(_) => OracleTypeKind::Varchar2,
            Value::Json(_) => OracleTypeKind::Json,
            Value::Array(_) | Value::Object(_) => OracleTypeKind::Json,
            _ => OracleTypeKind::Varchar2,
        }
    }

    /// 生成类型转换 SQL（Oracle CAST 表达式）
    #[must_use]
    pub fn cast_sql(&self, expr: &str, target: OracleTypeKind) -> String {
        let type_ddl = target.to_ddl(None, None, None);
        format!("CAST({expr} AS {type_ddl})")
    }

    /// 生成类型兼容性检查 SQL
    #[must_use]
    pub fn compatibility_check_sql(&self, col: &str, target: OracleTypeKind) -> String {
        let type_ddl = target.to_ddl(None, None, None);
        format!("CASE WHEN CAST({col} AS {type_ddl}) IS NULL THEN 0 ELSE 1 END")
    }

    /// 自定义映射数量
    #[must_use]
    pub fn custom_mapping_count(&self) -> usize {
        self.custom_mappings.len()
    }
}

impl fmt::Display for TypeMapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TypeMapping(custom={})", self.custom_mappings.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oracle_type_kind_parse_name() {
        assert_eq!(OracleTypeKind::parse_name("NUMBER"), OracleTypeKind::Number);
        assert_eq!(
            OracleTypeKind::parse_name("varchar2"),
            OracleTypeKind::Varchar2
        );
        assert_eq!(
            OracleTypeKind::parse_name("TIMESTAMP WITH TIME ZONE"),
            OracleTypeKind::TimestampTz
        );
        assert_eq!(OracleTypeKind::parse_name("JSON"), OracleTypeKind::Json);
    }

    #[test]
    fn test_oracle_type_kind_to_ddl_number() {
        let ddl = OracleTypeKind::Number.to_ddl(None, Some(10), Some(2));
        assert_eq!(ddl, "NUMBER(10, 2)");
    }

    #[test]
    fn test_oracle_type_kind_to_ddl_varchar() {
        let ddl = OracleTypeKind::Varchar2.to_ddl(Some(255), None, None);
        assert_eq!(ddl, "VARCHAR2(255)");
    }

    #[test]
    fn test_oracle_type_kind_to_ddl_default_length() {
        let ddl = OracleTypeKind::Varchar2.to_ddl(None, None, None);
        assert_eq!(ddl, "VARCHAR2(4000)");
    }

    #[test]
    fn test_oracle_type_kind_is_numeric() {
        assert!(OracleTypeKind::Number.is_numeric());
        assert!(OracleTypeKind::BinaryFloat.is_numeric());
        assert!(!OracleTypeKind::Varchar2.is_numeric());
    }

    #[test]
    fn test_oracle_type_kind_is_string() {
        assert!(OracleTypeKind::Varchar2.is_string());
        assert!(OracleTypeKind::Clob.is_string());
        assert!(!OracleTypeKind::Number.is_string());
    }

    #[test]
    fn test_oracle_type_kind_is_binary() {
        assert!(OracleTypeKind::Raw.is_binary());
        assert!(OracleTypeKind::Blob.is_binary());
        assert!(!OracleTypeKind::Number.is_binary());
    }

    #[test]
    fn test_oracle_type_kind_is_temporal() {
        assert!(OracleTypeKind::Date.is_temporal());
        assert!(OracleTypeKind::Timestamp.is_temporal());
        assert!(!OracleTypeKind::Number.is_temporal());
    }

    #[test]
    fn test_oracle_type_kind_is_lob() {
        assert!(OracleTypeKind::Clob.is_lob());
        assert!(OracleTypeKind::Blob.is_lob());
        assert!(!OracleTypeKind::Varchar2.is_lob());
    }

    #[test]
    fn test_oracle_type_kind_value_kind() {
        assert_eq!(OracleTypeKind::Number.value_kind(), ValueKind::Number);
        assert_eq!(OracleTypeKind::Varchar2.value_kind(), ValueKind::String);
        assert_eq!(OracleTypeKind::Date.value_kind(), ValueKind::DateTime);
        assert_eq!(OracleTypeKind::Boolean.value_kind(), ValueKind::Bool);
        assert_eq!(OracleTypeKind::Json.value_kind(), ValueKind::Json);
    }

    #[test]
    fn test_oracle_column_meta_new() {
        let col = OracleColumnMeta::new("id", OracleTypeKind::Number);
        assert_eq!(col.name, "id");
        assert!(col.nullable);
    }

    #[test]
    fn test_oracle_column_meta_builder() {
        let col = OracleColumnMeta::new("name", OracleTypeKind::Varchar2)
            .with_length(100)
            .with_nullable(false)
            .with_default("'unknown'");
        assert_eq!(col.length, Some(100));
        assert!(!col.nullable);
        assert_eq!(col.default_value.as_deref(), Some("'unknown'"));
    }

    #[test]
    fn test_oracle_column_meta_primary_key() {
        let col = OracleColumnMeta::new("id", OracleTypeKind::Number).primary_key();
        assert!(col.is_primary_key);
        assert!(!col.nullable);
    }

    #[test]
    fn test_oracle_column_meta_to_ddl() {
        let col = OracleColumnMeta::new("id", OracleTypeKind::Number)
            .with_precision(10, 0)
            .with_nullable(false);
        let ddl = col.to_ddl();
        assert!(ddl.contains("NUMBER(10, 0)"));
        assert!(ddl.contains("NOT NULL"));
    }

    #[test]
    fn test_oracle_column_meta_to_column_ddl() {
        let col = OracleColumnMeta::new("name", OracleTypeKind::Varchar2)
            .with_length(50)
            .with_default("'test'");
        let ddl = col.to_column_ddl();
        assert!(ddl.contains("name VARCHAR2(50)"));
        assert!(ddl.contains("DEFAULT 'test'"));
    }

    #[test]
    fn test_oracle_column_meta_display() {
        let col = OracleColumnMeta::new("id", OracleTypeKind::Number);
        let s = format!("{}", col);
        assert!(s.contains("id NUMBER"));
    }

    #[test]
    fn test_type_mapping_default() {
        let tm = TypeMapping::default();
        assert_eq!(tm.custom_mapping_count(), 0);
    }

    #[test]
    fn test_type_mapping_register() {
        let tm = TypeMapping::new()
            .register("MY_TYPE", ValueKind::String)
            .register("MY_NUM", ValueKind::Number);
        assert_eq!(tm.custom_mapping_count(), 2);
    }

    #[test]
    fn test_type_mapping_to_value_kind_custom() {
        let tm = TypeMapping::new().register("MY_TYPE", ValueKind::Bytes);
        assert_eq!(tm.to_value_kind("MY_TYPE"), ValueKind::Bytes);
    }

    #[test]
    fn test_type_mapping_to_value_kind_builtin() {
        let tm = TypeMapping::new();
        assert_eq!(tm.to_value_kind("NUMBER"), ValueKind::Number);
        assert_eq!(tm.to_value_kind("VARCHAR2"), ValueKind::String);
    }

    #[test]
    fn test_type_mapping_from_value_int() {
        let kind = TypeMapping::from_value(&Value::I64(42));
        assert_eq!(kind, OracleTypeKind::Number);
    }

    #[test]
    fn test_type_mapping_from_value_string() {
        let kind = TypeMapping::from_value(&Value::String("hello".to_string()));
        assert_eq!(kind, OracleTypeKind::Varchar2);
    }

    #[test]
    fn test_type_mapping_from_value_bool() {
        let kind = TypeMapping::from_value(&Value::Bool(true));
        assert_eq!(kind, OracleTypeKind::Boolean);
    }

    #[test]
    fn test_type_mapping_from_value_bytes() {
        let kind = TypeMapping::from_value(&Value::Bytes(vec![1, 2, 3]));
        assert_eq!(kind, OracleTypeKind::Raw);
    }

    #[test]
    fn test_type_mapping_from_value_json() {
        let kind = TypeMapping::from_value(&Value::Json("{}".to_string()));
        assert_eq!(kind, OracleTypeKind::Json);
    }

    #[test]
    fn test_type_mapping_cast_sql() {
        let tm = TypeMapping::new();
        let sql = tm.cast_sql("col1", OracleTypeKind::Number);
        assert_eq!(sql, "CAST(col1 AS NUMBER)");
    }

    #[test]
    fn test_type_mapping_compatibility_check_sql() {
        let tm = TypeMapping::new();
        let sql = tm.compatibility_check_sql("col1", OracleTypeKind::Number);
        assert!(sql.contains("CAST(col1 AS NUMBER)"));
        assert!(sql.contains("CASE WHEN"));
    }

    #[test]
    fn test_type_mapping_display() {
        let tm = TypeMapping::new().register("X", ValueKind::Number);
        let s = format!("{}", tm);
        assert!(s.contains("custom=1"));
    }
}
