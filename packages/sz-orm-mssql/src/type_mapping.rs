//! SQL Server 数据类型映射
//!
//! 提供 [`MssqlTypeMapping`] 在 sz-orm-core 的 `Value` 与 SQL Server
//! 数据类型之间双向映射，支持精度/标度、字符集、稀疏列等。

use std::collections::HashMap;
use std::fmt;

use sz_orm_core::Value;

/// SQL Server 列类型描述
#[derive(Debug, Clone, PartialEq)]
pub struct MssqlColumnMeta {
    /// 列名
    pub name: String,
    /// 数据类型
    pub data_type: MssqlTypeKind,
    /// 最大长度
    pub max_length: Option<i32>,
    /// 精度
    pub precision: Option<u8>,
    /// 标度
    pub scale: Option<u8>,
    /// 是否可空
    pub nullable: bool,
    /// 默认值
    pub default_value: Option<String>,
    /// 是否标识列（IDENTITY）
    pub is_identity: bool,
    /// 是否计算列
    pub is_computed: bool,
    /// 计算列表达式
    pub computed_expression: Option<String>,
    /// 是否稀疏列
    pub is_sparse: bool,
    /// 是否主键
    pub is_primary_key: bool,
    /// 是否唯一
    pub is_unique: bool,
}

impl MssqlColumnMeta {
    /// 创建新的列元信息
    #[must_use]
    pub fn new(name: &str, data_type: MssqlTypeKind) -> Self {
        Self {
            name: name.to_string(),
            data_type,
            max_length: None,
            precision: None,
            scale: None,
            nullable: true,
            default_value: None,
            is_identity: false,
            is_computed: false,
            computed_expression: None,
            is_sparse: false,
            is_primary_key: false,
            is_unique: false,
        }
    }

    /// 设置最大长度
    #[must_use]
    pub fn with_max_length(mut self, length: i32) -> Self {
        self.max_length = Some(length);
        self
    }

    /// 设置精度与标度
    #[must_use]
    pub fn with_precision(mut self, precision: u8, scale: u8) -> Self {
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

    /// 标记为标识列
    #[must_use]
    pub fn identity(mut self) -> Self {
        self.is_identity = true;
        self.nullable = false;
        self
    }

    /// 标记为计算列
    #[must_use]
    pub fn computed(mut self, expression: &str) -> Self {
        self.is_computed = true;
        self.computed_expression = Some(expression.to_string());
        self
    }

    /// 标记为稀疏列
    #[must_use]
    pub fn sparse(mut self) -> Self {
        self.is_sparse = true;
        self.nullable = true;
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

    /// 生成 DDL 片段
    #[must_use]
    pub fn to_ddl(&self) -> String {
        if self.is_computed {
            let expr = self.computed_expression.as_deref().unwrap_or("''");
            return format!("AS ({expr})");
        }
        let mut parts = Vec::new();
        parts.push(
            self.data_type
                .to_ddl(self.max_length, self.precision, self.scale),
        );
        if self.is_identity {
            parts.push("IDENTITY(1,1)".to_string());
        }
        if !self.nullable {
            parts.push("NOT NULL".to_string());
        } else {
            parts.push("NULL".to_string());
        }
        if let Some(ref default) = self.default_value {
            parts.push(format!("DEFAULT {default}"));
        }
        if self.is_sparse {
            parts.push("SPARSE".to_string());
        }
        parts.join(" ")
    }

    /// 生成完整列定义
    #[must_use]
    pub fn to_column_ddl(&self) -> String {
        format!("{} {}", self.name, self.to_ddl())
    }
}

impl fmt::Display for MssqlColumnMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_column_ddl())
    }
}

/// SQL Server 类型种类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MssqlTypeKind {
    BigInt,
    Binary,
    Bit,
    Char,
    Date,
    Datetime,
    Datetime2,
    Datetimeoffset,
    Decimal,
    Float,
    Image,
    Int,
    Money,
    Nchar,
    Ntext,
    Numeric,
    Nvarchar,
    Real,
    Smalldatetime,
    Smallint,
    Smallmoney,
    SqlVariant,
    Text,
    Time,
    Tinyint,
    Uniqueidentifier,
    Varbinary,
    Varchar,
    Xml,
    Json,
    Geography,
    Geometry,
    Hierarchyid,
}

impl MssqlTypeKind {
    /// 从类型名解析
    #[must_use]
    pub fn parse_name(name: &str) -> Self {
        let upper = name.to_uppercase();
        match upper.as_str() {
            "BIGINT" => MssqlTypeKind::BigInt,
            "BINARY" => MssqlTypeKind::Binary,
            "BIT" => MssqlTypeKind::Bit,
            "CHAR" | "CHARACTER" => MssqlTypeKind::Char,
            "DATE" => MssqlTypeKind::Date,
            "DATETIME" => MssqlTypeKind::Datetime,
            "DATETIME2" => MssqlTypeKind::Datetime2,
            "DATETIMEOFFSET" => MssqlTypeKind::Datetimeoffset,
            "DECIMAL" => MssqlTypeKind::Decimal,
            "FLOAT" | "DOUBLE PRECISION" => MssqlTypeKind::Float,
            "IMAGE" => MssqlTypeKind::Image,
            "INT" | "INTEGER" => MssqlTypeKind::Int,
            "MONEY" => MssqlTypeKind::Money,
            "NCHAR" => MssqlTypeKind::Nchar,
            "NTEXT" => MssqlTypeKind::Ntext,
            "NUMERIC" => MssqlTypeKind::Numeric,
            "NVARCHAR" => MssqlTypeKind::Nvarchar,
            "REAL" => MssqlTypeKind::Real,
            "SMALLDATETIME" => MssqlTypeKind::Smalldatetime,
            "SMALLINT" => MssqlTypeKind::Smallint,
            "SMALLMONEY" => MssqlTypeKind::Smallmoney,
            "SQL_VARIANT" => MssqlTypeKind::SqlVariant,
            "TEXT" => MssqlTypeKind::Text,
            "TIME" => MssqlTypeKind::Time,
            "TINYINT" => MssqlTypeKind::Tinyint,
            "UNIQUEIDENTIFIER" => MssqlTypeKind::Uniqueidentifier,
            "VARBINARY" => MssqlTypeKind::Varbinary,
            "VARCHAR" => MssqlTypeKind::Varchar,
            "XML" => MssqlTypeKind::Xml,
            "JSON" => MssqlTypeKind::Json,
            "GEOGRAPHY" => MssqlTypeKind::Geography,
            "GEOMETRY" => MssqlTypeKind::Geometry,
            "HIERARCHYID" => MssqlTypeKind::Hierarchyid,
            _ => MssqlTypeKind::Varchar,
        }
    }

    /// 生成 DDL 类型片段
    #[must_use]
    pub fn to_ddl(
        &self,
        max_length: Option<i32>,
        precision: Option<u8>,
        scale: Option<u8>,
    ) -> String {
        match self {
            MssqlTypeKind::Decimal | MssqlTypeKind::Numeric => match (precision, scale) {
                (Some(p), Some(s)) => format!("{}({p}, {s})", self.as_sql_name()),
                (Some(p), None) => format!("{}({p})", self.as_sql_name()),
                _ => self.as_sql_name().to_string(),
            },
            MssqlTypeKind::Varchar
            | MssqlTypeKind::Nvarchar
            | MssqlTypeKind::Char
            | MssqlTypeKind::Nchar
            | MssqlTypeKind::Binary
            | MssqlTypeKind::Varbinary => match max_length {
                Some(-1) => format!("{}(MAX)", self.as_sql_name()),
                Some(n) => format!("{}({n})", self.as_sql_name()),
                None => self.as_sql_name().to_string(),
            },
            MssqlTypeKind::Datetime2 | MssqlTypeKind::Datetimeoffset | MssqlTypeKind::Time => {
                match precision {
                    Some(p) => format!("{}({p})", self.as_sql_name()),
                    None => self.as_sql_name().to_string(),
                }
            }
            MssqlTypeKind::Float => match max_length {
                Some(n) => format!("FLOAT({n})"),
                None => "FLOAT".to_string(),
            },
            _ => self.as_sql_name().to_string(),
        }
    }

    /// 返回 SQL 类型名
    #[must_use]
    pub fn as_sql_name(&self) -> &'static str {
        match self {
            MssqlTypeKind::BigInt => "BIGINT",
            MssqlTypeKind::Binary => "BINARY",
            MssqlTypeKind::Bit => "BIT",
            MssqlTypeKind::Char => "CHAR",
            MssqlTypeKind::Date => "DATE",
            MssqlTypeKind::Datetime => "DATETIME",
            MssqlTypeKind::Datetime2 => "DATETIME2",
            MssqlTypeKind::Datetimeoffset => "DATETIMEOFFSET",
            MssqlTypeKind::Decimal => "DECIMAL",
            MssqlTypeKind::Float => "FLOAT",
            MssqlTypeKind::Image => "IMAGE",
            MssqlTypeKind::Int => "INT",
            MssqlTypeKind::Money => "MONEY",
            MssqlTypeKind::Nchar => "NCHAR",
            MssqlTypeKind::Ntext => "NTEXT",
            MssqlTypeKind::Numeric => "NUMERIC",
            MssqlTypeKind::Nvarchar => "NVARCHAR",
            MssqlTypeKind::Real => "REAL",
            MssqlTypeKind::Smalldatetime => "SMALLDATETIME",
            MssqlTypeKind::Smallint => "SMALLINT",
            MssqlTypeKind::Smallmoney => "SMALLMONEY",
            MssqlTypeKind::SqlVariant => "SQL_VARIANT",
            MssqlTypeKind::Text => "TEXT",
            MssqlTypeKind::Time => "TIME",
            MssqlTypeKind::Tinyint => "TINYINT",
            MssqlTypeKind::Uniqueidentifier => "UNIQUEIDENTIFIER",
            MssqlTypeKind::Varbinary => "VARBINARY",
            MssqlTypeKind::Varchar => "VARCHAR",
            MssqlTypeKind::Xml => "XML",
            MssqlTypeKind::Json => "JSON",
            MssqlTypeKind::Geography => "GEOGRAPHY",
            MssqlTypeKind::Geometry => "GEOMETRY",
            MssqlTypeKind::Hierarchyid => "HIERARCHYID",
        }
    }

    /// 是否为数值类型
    #[must_use]
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            MssqlTypeKind::BigInt
                | MssqlTypeKind::Bit
                | MssqlTypeKind::Decimal
                | MssqlTypeKind::Float
                | MssqlTypeKind::Int
                | MssqlTypeKind::Money
                | MssqlTypeKind::Numeric
                | MssqlTypeKind::Real
                | MssqlTypeKind::Smallint
                | MssqlTypeKind::Smallmoney
                | MssqlTypeKind::Tinyint
        )
    }

    /// 是否为字符串类型
    #[must_use]
    pub fn is_string(&self) -> bool {
        matches!(
            self,
            MssqlTypeKind::Char
                | MssqlTypeKind::Nchar
                | MssqlTypeKind::Ntext
                | MssqlTypeKind::Nvarchar
                | MssqlTypeKind::Text
                | MssqlTypeKind::Varchar
                | MssqlTypeKind::Xml
        )
    }

    /// 是否为二进制类型
    #[must_use]
    pub fn is_binary(&self) -> bool {
        matches!(
            self,
            MssqlTypeKind::Binary | MssqlTypeKind::Image | MssqlTypeKind::Varbinary
        )
    }

    /// 是否为时间类型
    #[must_use]
    pub fn is_temporal(&self) -> bool {
        matches!(
            self,
            MssqlTypeKind::Date
                | MssqlTypeKind::Datetime
                | MssqlTypeKind::Datetime2
                | MssqlTypeKind::Datetimeoffset
                | MssqlTypeKind::Smalldatetime
                | MssqlTypeKind::Time
        )
    }

    /// 是否为 LOB 类型
    #[must_use]
    pub fn is_lob(&self) -> bool {
        matches!(
            self,
            MssqlTypeKind::Image | MssqlTypeKind::Ntext | MssqlTypeKind::Text | MssqlTypeKind::Xml
        )
    }

    /// 是否需要长度参数
    #[must_use]
    pub fn requires_length(&self) -> bool {
        matches!(
            self,
            MssqlTypeKind::Binary
                | MssqlTypeKind::Char
                | MssqlTypeKind::Nchar
                | MssqlTypeKind::Nvarchar
                | MssqlTypeKind::Varbinary
                | MssqlTypeKind::Varchar
        )
    }

    /// 是否需要精度参数
    #[must_use]
    pub fn requires_precision(&self) -> bool {
        matches!(
            self,
            MssqlTypeKind::Decimal
                | MssqlTypeKind::Numeric
                | MssqlTypeKind::Datetime2
                | MssqlTypeKind::Datetimeoffset
                | MssqlTypeKind::Time
        )
    }

    /// 推断 ValueKind
    #[must_use]
    pub fn value_kind(&self) -> ValueKind {
        match self {
            MssqlTypeKind::Bit => ValueKind::Bool,
            MssqlTypeKind::Uniqueidentifier => ValueKind::Uuid,
            MssqlTypeKind::Json => ValueKind::Json,
            _ if self.is_numeric() => ValueKind::Number,
            _ if self.is_string() => ValueKind::String,
            _ if self.is_binary() => ValueKind::Bytes,
            _ if self.is_temporal() => ValueKind::DateTime,
            _ => ValueKind::Other,
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
    Uuid,
    Json,
    Other,
}

/// 类型映射器
#[derive(Debug, Clone)]
pub struct MssqlTypeMapping {
    custom_mappings: HashMap<String, ValueKind>,
}

impl Default for MssqlTypeMapping {
    fn default() -> Self {
        Self {
            custom_mappings: HashMap::new(),
        }
    }
}

impl MssqlTypeMapping {
    /// 创建新的映射器
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册自定义映射
    #[must_use]
    pub fn register(mut self, type_name: &str, kind: ValueKind) -> Self {
        self.custom_mappings.insert(type_name.to_uppercase(), kind);
        self
    }

    /// 从类型名推断 ValueKind
    #[must_use]
    pub fn to_value_kind(&self, type_name: &str) -> ValueKind {
        let upper = type_name.to_uppercase();
        if let Some(&kind) = self.custom_mappings.get(&upper) {
            return kind;
        }
        MssqlTypeKind::parse_name(&upper).value_kind()
    }

    /// 从 Value 推断 MssqlTypeKind
    #[must_use]
    pub fn from_value(value: &Value) -> MssqlTypeKind {
        match value {
            Value::Null => MssqlTypeKind::SqlVariant,
            Value::Bool(_) => MssqlTypeKind::Bit,
            Value::I8(_) | Value::U8(_) => MssqlTypeKind::Tinyint,
            Value::I16(_) | Value::U16(_) => MssqlTypeKind::Smallint,
            Value::I32(_) | Value::U32(_) => MssqlTypeKind::Int,
            Value::I64(_) | Value::U64(_) => MssqlTypeKind::BigInt,
            Value::F32(_) => MssqlTypeKind::Real,
            Value::F64(_) => MssqlTypeKind::Float,
            Value::String(_) => MssqlTypeKind::Nvarchar,
            Value::Decimal(_) => MssqlTypeKind::Decimal,
            Value::Bytes(_) => MssqlTypeKind::Varbinary,
            Value::Date(_) => MssqlTypeKind::Date,
            Value::DateTime(_) => MssqlTypeKind::Datetime2,
            Value::Time(_) => MssqlTypeKind::Time,
            Value::Uuid(_) => MssqlTypeKind::Uniqueidentifier,
            Value::Json(_) => MssqlTypeKind::Json,
            Value::Array(_) | Value::Object(_) => MssqlTypeKind::Json,
            _ => MssqlTypeKind::Nvarchar,
        }
    }

    /// 生成 CAST 表达式
    #[must_use]
    pub fn cast_sql(&self, expr: &str, target: MssqlTypeKind) -> String {
        let type_ddl = target.to_ddl(None, None, None);
        format!("CAST({expr} AS {type_ddl})")
    }

    /// 生成 TRY_CAST 表达式
    #[must_use]
    pub fn try_cast_sql(&self, expr: &str, target: MssqlTypeKind) -> String {
        let type_ddl = target.to_ddl(None, None, None);
        format!("TRY_CAST({expr} AS {type_ddl})")
    }

    /// 生成 CONVERT 表达式
    #[must_use]
    pub fn convert_sql(&self, target: MssqlTypeKind, expr: &str, style: Option<u16>) -> String {
        let type_ddl = target.to_ddl(None, None, None);
        match style {
            Some(s) => format!("CONVERT({type_ddl}, {expr}, {s})"),
            None => format!("CONVERT({type_ddl}, {expr})"),
        }
    }

    /// 自定义映射数量
    #[must_use]
    pub fn custom_mapping_count(&self) -> usize {
        self.custom_mappings.len()
    }
}

impl fmt::Display for MssqlTypeMapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MssqlTypeMapping(custom={})", self.custom_mappings.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_name_int() {
        assert_eq!(MssqlTypeKind::parse_name("INT"), MssqlTypeKind::Int);
        assert_eq!(MssqlTypeKind::parse_name("INTEGER"), MssqlTypeKind::Int);
    }

    #[test]
    fn test_parse_name_varchar() {
        assert_eq!(MssqlTypeKind::parse_name("varchar"), MssqlTypeKind::Varchar);
    }

    #[test]
    fn test_parse_name_unknown() {
        assert_eq!(MssqlTypeKind::parse_name("unknown"), MssqlTypeKind::Varchar);
    }

    #[test]
    fn test_to_ddl_decimal() {
        let ddl = MssqlTypeKind::Decimal.to_ddl(None, Some(10), Some(2));
        assert_eq!(ddl, "DECIMAL(10, 2)");
    }

    #[test]
    fn test_to_ddl_varchar_max() {
        let ddl = MssqlTypeKind::Varchar.to_ddl(Some(-1), None, None);
        assert_eq!(ddl, "VARCHAR(MAX)");
    }

    #[test]
    fn test_to_ddl_nvarchar_length() {
        let ddl = MssqlTypeKind::Nvarchar.to_ddl(Some(100), None, None);
        assert_eq!(ddl, "NVARCHAR(100)");
    }

    #[test]
    fn test_to_ddl_datetime2_precision() {
        let ddl = MssqlTypeKind::Datetime2.to_ddl(None, Some(7), None);
        assert_eq!(ddl, "DATETIME2(7)");
    }

    #[test]
    fn test_is_numeric() {
        assert!(MssqlTypeKind::Int.is_numeric());
        assert!(MssqlTypeKind::Decimal.is_numeric());
        assert!(!MssqlTypeKind::Varchar.is_numeric());
    }

    #[test]
    fn test_is_string() {
        assert!(MssqlTypeKind::Varchar.is_string());
        assert!(MssqlTypeKind::Xml.is_string());
        assert!(!MssqlTypeKind::Int.is_string());
    }

    #[test]
    fn test_is_binary() {
        assert!(MssqlTypeKind::Varbinary.is_binary());
        assert!(!MssqlTypeKind::Int.is_binary());
    }

    #[test]
    fn test_is_temporal() {
        assert!(MssqlTypeKind::Date.is_temporal());
        assert!(MssqlTypeKind::Datetime2.is_temporal());
        assert!(!MssqlTypeKind::Int.is_temporal());
    }

    #[test]
    fn test_is_lob() {
        assert!(MssqlTypeKind::Text.is_lob());
        assert!(MssqlTypeKind::Xml.is_lob());
        assert!(!MssqlTypeKind::Varchar.is_lob());
    }

    #[test]
    fn test_requires_length() {
        assert!(MssqlTypeKind::Varchar.requires_length());
        assert!(!MssqlTypeKind::Int.requires_length());
    }

    #[test]
    fn test_requires_precision() {
        assert!(MssqlTypeKind::Decimal.requires_precision());
        assert!(!MssqlTypeKind::Int.requires_precision());
    }

    #[test]
    fn test_value_kind() {
        assert_eq!(MssqlTypeKind::Int.value_kind(), ValueKind::Number);
        assert_eq!(MssqlTypeKind::Varchar.value_kind(), ValueKind::String);
        assert_eq!(MssqlTypeKind::Bit.value_kind(), ValueKind::Bool);
        assert_eq!(
            MssqlTypeKind::Uniqueidentifier.value_kind(),
            ValueKind::Uuid
        );
    }

    #[test]
    fn test_column_meta_new() {
        let col = MssqlColumnMeta::new("id", MssqlTypeKind::Int);
        assert_eq!(col.name, "id");
        assert!(col.nullable);
    }

    #[test]
    fn test_column_meta_identity() {
        let col = MssqlColumnMeta::new("id", MssqlTypeKind::Int).identity();
        assert!(col.is_identity);
        assert!(!col.nullable);
    }

    #[test]
    fn test_column_meta_computed() {
        let col =
            MssqlColumnMeta::new("total", MssqlTypeKind::Decimal).computed("price * quantity");
        let ddl = col.to_ddl();
        assert!(ddl.contains("AS (price * quantity)"));
    }

    #[test]
    fn test_column_meta_sparse() {
        let col = MssqlColumnMeta::new("optional", MssqlTypeKind::Nvarchar).sparse();
        let ddl = col.to_ddl();
        assert!(ddl.contains("SPARSE"));
    }

    #[test]
    fn test_column_meta_to_ddl() {
        let col = MssqlColumnMeta::new("id", MssqlTypeKind::Int)
            .identity()
            .with_nullable(false);
        let ddl = col.to_ddl();
        assert!(ddl.contains("IDENTITY(1,1)"));
        assert!(ddl.contains("NOT NULL"));
    }

    #[test]
    fn test_column_meta_to_column_ddl() {
        let col = MssqlColumnMeta::new("name", MssqlTypeKind::Nvarchar)
            .with_max_length(100)
            .with_nullable(false);
        let ddl = col.to_column_ddl();
        assert!(ddl.contains("name NVARCHAR(100)"));
    }

    #[test]
    fn test_column_meta_primary_key() {
        let col = MssqlColumnMeta::new("id", MssqlTypeKind::Int).primary_key();
        assert!(col.is_primary_key);
        assert!(!col.nullable);
    }

    #[test]
    fn test_column_meta_display() {
        let col = MssqlColumnMeta::new("id", MssqlTypeKind::Int);
        let s = format!("{}", col);
        assert!(s.contains("id INT"));
    }

    #[test]
    fn test_type_mapping_default() {
        let tm = MssqlTypeMapping::default();
        assert_eq!(tm.custom_mapping_count(), 0);
    }

    #[test]
    fn test_type_mapping_register() {
        let tm = MssqlTypeMapping::new().register("MY_TYPE", ValueKind::Bytes);
        assert_eq!(tm.custom_mapping_count(), 1);
    }

    #[test]
    fn test_type_mapping_to_value_kind_custom() {
        let tm = MssqlTypeMapping::new().register("MY_TYPE", ValueKind::Json);
        assert_eq!(tm.to_value_kind("MY_TYPE"), ValueKind::Json);
    }

    #[test]
    fn test_type_mapping_to_value_kind_builtin() {
        let tm = MssqlTypeMapping::new();
        assert_eq!(tm.to_value_kind("INT"), ValueKind::Number);
    }

    #[test]
    fn test_type_mapping_from_value_int() {
        let kind = MssqlTypeMapping::from_value(&Value::I64(42));
        assert_eq!(kind, MssqlTypeKind::BigInt);
    }

    #[test]
    fn test_type_mapping_from_value_string() {
        let kind = MssqlTypeMapping::from_value(&Value::String("hello".to_string()));
        assert_eq!(kind, MssqlTypeKind::Nvarchar);
    }

    #[test]
    fn test_type_mapping_from_value_bool() {
        let kind = MssqlTypeMapping::from_value(&Value::Bool(true));
        assert_eq!(kind, MssqlTypeKind::Bit);
    }

    #[test]
    fn test_type_mapping_from_value_uuid() {
        let kind = MssqlTypeMapping::from_value(&Value::Uuid("...".to_string()));
        assert_eq!(kind, MssqlTypeKind::Uniqueidentifier);
    }

    #[test]
    fn test_type_mapping_cast_sql() {
        let tm = MssqlTypeMapping::new();
        let sql = tm.cast_sql("col1", MssqlTypeKind::Int);
        assert_eq!(sql, "CAST(col1 AS INT)");
    }

    #[test]
    fn test_type_mapping_try_cast_sql() {
        let tm = MssqlTypeMapping::new();
        let sql = tm.try_cast_sql("col1", MssqlTypeKind::Int);
        assert_eq!(sql, "TRY_CAST(col1 AS INT)");
    }

    #[test]
    fn test_type_mapping_convert_sql() {
        let tm = MssqlTypeMapping::new();
        let sql = tm.convert_sql(MssqlTypeKind::Varchar, "GETDATE()", Some(120));
        assert!(sql.contains("CONVERT(VARCHAR, GETDATE(), 120)"));
    }

    #[test]
    fn test_type_mapping_convert_sql_no_style() {
        let tm = MssqlTypeMapping::new();
        let sql = tm.convert_sql(MssqlTypeKind::Int, "col1", None);
        assert_eq!(sql, "CONVERT(INT, col1)");
    }

    #[test]
    fn test_type_mapping_display() {
        let tm = MssqlTypeMapping::new().register("X", ValueKind::Number);
        let s = format!("{}", tm);
        assert!(s.contains("custom=1"));
    }
}
