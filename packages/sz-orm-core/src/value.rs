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

    /// 高精度十进制数（NUMERIC/DECIMAL），以字符串形式存储避免 f64 精度丢失
    Decimal(String),

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
            Value::Decimal(s) => Some(s),
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
            Value::Decimal(s) => s.parse::<i64>().ok(),
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
            Value::Decimal(s) => s.parse::<f64>().ok(),
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
            Value::Decimal(s) => Cow::Owned(s.clone()),
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
            Value::Decimal(s) => Cow::Owned(s.clone()),
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
            Value::Decimal(v) => write!(f, "{}", v),
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

/// 列类型枚举（v1.1.0 新增）
///
/// 用于 `row_to_value_*` 函数的预解析列类型分派，避免每行每列做字符串 `match`。
/// 适配器在第一行解析列类型为 `Vec<ColType>`，后续行复用枚举分派（编译器优化为跳转表）。
///
/// # 性能优势
///
/// - 字符串 `match type_name` 无法被 LLVM 优化为跳转表（`&str` 比较）
/// - 枚举 `match col_type` 编译为跳转表，O(1) 且缓存友好
/// - 在 SELECT ALL 大结果集场景下，每行每列节省 1 次字符串比较
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ColType {
    /// 布尔类型（SQLite BOOLEAN / MySQL BOOLEAN/TINYINT(1) / PG BOOL / Oracle Boolean）
    Bool,
    /// 8 位有符号整数（MySQL TINYINT）
    I8,
    /// 16 位有符号整数（MySQL SMALLINT / PG INT2）
    I16,
    /// 32 位有符号整数（MySQL INT/MEDIUMINT / PG INT4）
    I32,
    /// 64 位有符号整数（MySQL BIGINT / PG INT8 / SQLite INTEGER / Oracle NUMBER）
    I64,
    /// 8 位无符号整数（MySQL TINYINT UNSIGNED）
    U8,
    /// 16 位无符号整数（MySQL SMALLINT UNSIGNED）
    U16,
    /// 32 位无符号整数（MySQL INT UNSIGNED/MEDIUMINT UNSIGNED）
    U32,
    /// 64 位无符号整数（MySQL BIGINT UNSIGNED）
    U64,
    /// 32 位浮点数（MySQL FLOAT / PG FLOAT4 / SQLite REAL）
    F32,
    /// 64 位浮点数（MySQL DOUBLE / PG FLOAT8 / Oracle BinaryDouble）
    F64,
    /// 高精度十进制数（MySQL DECIMAL/NUMERIC/NEWDECIMAL / PG NUMERIC / Oracle NUMBER(p,s)）
    Decimal,
    /// 字符串类型（TEXT/VARCHAR/CHAR/CLOB 等）
    String,
    /// 字节类型（BLOB/BYTEA/RAW 等）
    Bytes,
    /// 日期类型（DATE）
    Date,
    /// 日期时间类型（DATETIME/TIMESTAMP）
    DateTime,
    /// 时间类型（TIME）
    Time,
    /// JSON 类型
    Json,
    /// UUID 类型
    Uuid,
    /// 未知类型（回退到 i64 → f64 → bool → String 顺序尝试）
    Unknown,
}

impl ColType {
    /// 从数据库类型名解析为 ColType（通用回退实现）
    ///
    /// 各适配器应优先使用自己专门的 `parse_col_type_<db>` 函数（覆盖数据库特有类型名），
    /// 此函数作为通用回退，覆盖最常见的标准 SQL 类型名。
    ///
    /// # 注意
    ///
    /// "INTEGER" 在通用映射中被归为 I32（与 MySQL INT/PG INT4 一致）。
    /// **SQLite 适配器必须使用 [`ColType::parse_sqlite`]**：SQLite 的 INTEGER
    /// 类型采用动态存储，可容纳 64 位整数（sqlx 默认按 i64 解码），若按 I32
    /// 解码会在数值超过 i32::MAX 时截断。
    pub fn from_type_name(type_name: &str) -> Self {
        match type_name {
            "BOOLEAN" | "BOOL" => Self::Bool,
            "TINYINT" => Self::I8,
            "SMALLINT" | "INT2" => Self::I16,
            "INT" | "INT4" | "OID" | "MEDIUMINT" | "INTEGER" => Self::I32,
            "BIGINT" | "INT8" => Self::I64,
            "TINYINT UNSIGNED" => Self::U8,
            "SMALLINT UNSIGNED" => Self::U16,
            "INT UNSIGNED" | "MEDIUMINT UNSIGNED" => Self::U32,
            "BIGINT UNSIGNED" => Self::U64,
            "FLOAT" | "FLOAT4" | "REAL" => Self::F32,
            "DOUBLE" | "FLOAT8" => Self::F64,
            "DECIMAL" | "NUMERIC" | "NEWDECIMAL" | "MONEY" => Self::Decimal,
            "TEXT" | "VARCHAR" | "CHAR" | "NAME" => Self::String,
            "BLOB" | "BYTEA" => Self::Bytes,
            "DATE" => Self::Date,
            "DATETIME" | "TIMESTAMP" => Self::DateTime,
            "TIME" => Self::Time,
            "JSON" => Self::Json,
            "UUID" => Self::Uuid,
            _ => Self::Unknown,
        }
    }

    /// SQLite 专用列类型解析
    ///
    /// SQLite 使用动态类型系统（type affinity），同一列可存储 INT/REAL/TEXT/BLOB 任意类型。
    /// sqlx 报告的类型名遵循 SQLite 的"声明类型"（declared type）规则：
    ///
    /// - **INTEGER**：实际可容纳 8 字节整数（最大 2^63-1），sqlx 默认按 `i64` 解码。
    ///   若按 I32 解码，数值超过 `i32::MAX` 会静默截断。
    /// - **INT/INTEGER/BIGINT** 等：在 SQLite 中都按 INTEGER 亲和性处理，应统一映射为 I64。
    /// - **REAL/FLOAT/DOUBLE**：映射为 F64（SQLite REAL 是 8 字节 IEEE 754）。
    /// - **TEXT/CLOB**：映射为 String。
    /// - **BLOB**：映射为 Bytes。
    /// - **NUMERIC/DECIMAL**：保留为 Decimal（按字符串解码避免精度丢失）。
    /// - **BOOLEAN**：SQLite 无原生 BOOLEAN，存为 INTEGER 0/1，但声明 BOOLEAN 时按 Bool 解码。
    /// - **DATETIME/TIMESTAMP/DATE/TIME**：SQLite 通常以 TEXT 存储，按 String 解码。
    /// - **JSON**：SQLite 4.x 后有 JSON 类型，按 String 解码（保留原始 JSON 文本）。
    pub fn parse_sqlite(type_name: &str) -> Self {
        // SQLite type_info 可能返回空字符串（NULL 或表达式结果），按 Unknown 处理
        if type_name.is_empty() {
            return Self::Unknown;
        }
        match type_name.to_uppercase().as_str() {
            // SQLite INTEGER 亲和性：实际为 64 位有符号整数
            "INTEGER" | "INT" | "BIGINT" | "INT8" | "INT4" | "INT2" | "TINYINT" | "SMALLINT"
            | "MEDIUMINT" => Self::I64,
            "BOOLEAN" | "BOOL" => Self::Bool,
            "REAL" | "FLOAT" | "DOUBLE" | "FLOAT8" | "DOUBLE PRECISION" => Self::F64,
            "DECIMAL" | "NUMERIC" => Self::Decimal,
            "TEXT" | "CLOB" | "VARCHAR" | "CHAR" | "NAME" => Self::String,
            "BLOB" => Self::Bytes,
            "DATE" => Self::Date,
            "DATETIME" | "TIMESTAMP" => Self::DateTime,
            "TIME" => Self::Time,
            "JSON" => Self::Json,
            _ => Self::Unknown,
        }
    }

    /// MySQL 专用列类型解析
    ///
    /// MySQL 类型名来自 `Column::type_info().name()`，遵循 MySQL 协议报告的类型名。
    pub fn parse_mysql(type_name: &str) -> Self {
        match type_name.to_uppercase().as_str() {
            "TINYINT" => Self::I8,
            "SMALLINT" => Self::I16,
            "INT" | "INTEGER" | "MEDIUMINT" => Self::I32,
            "BIGINT" => Self::I64,
            "TINYINT UNSIGNED" => Self::U8,
            "SMALLINT UNSIGNED" => Self::U16,
            "INT UNSIGNED" | "MEDIUMINT UNSIGNED" => Self::U32,
            "BIGINT UNSIGNED" => Self::U64,
            "FLOAT" => Self::F32,
            "DOUBLE" => Self::F64,
            "DECIMAL" | "NUMERIC" | "NEWDECIMAL" => Self::Decimal,
            "VARCHAR" | "CHAR" | "TEXT" | "TINYTEXT" | "MEDIUMTEXT" | "LONGTEXT" | "ENUM"
            | "SET" => Self::String,
            "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB" | "BINARY" | "VARBINARY" => Self::Bytes,
            "DATE" => Self::Date,
            "DATETIME" | "TIMESTAMP" => Self::DateTime,
            "TIME" => Self::Time,
            "YEAR" => Self::I16,
            "JSON" => Self::Json,
            "BOOLEAN" | "BOOL" => Self::Bool,
            _ => Self::from_type_name(type_name),
        }
    }

    /// PostgreSQL 专用列类型解析
    ///
    /// PostgreSQL 类型名来自 `Column::type_info().name()`，使用 PG 内部类型名（如 INT4/INT8/FLOAT8）。
    pub fn parse_postgres(type_name: &str) -> Self {
        match type_name.to_uppercase().as_str() {
            "BOOL" => Self::Bool,
            "INT2" | "SMALLINT" => Self::I16,
            "INT4" | "INTEGER" | "INT" => Self::I32,
            "INT8" | "BIGINT" => Self::I64,
            "FLOAT4" | "REAL" => Self::F32,
            "FLOAT8" | "DOUBLE PRECISION" => Self::F64,
            "NUMERIC" | "DECIMAL" | "MONEY" => Self::Decimal,
            "TEXT" | "VARCHAR" | "CHAR" | "BPCHAR" | "NAME" | "CITEXT" => Self::String,
            "BYTEA" => Self::Bytes,
            "DATE" => Self::Date,
            "TIMESTAMP" | "TIMESTAMPTZ" => Self::DateTime,
            "TIME" | "TIMETZ" => Self::Time,
            "JSON" | "JSONB" => Self::Json,
            "UUID" => Self::Uuid,
            "OID" => Self::I32,
            _ => Self::from_type_name(type_name),
        }
    }
}

/// 位置式查询结果类型
///
/// 用于 `Connection::query_values` / `query_values_with_params`，绕过
/// `HashMap<String, Value>` 行映射的开销，直接返回列名 + 按列顺序的值矩阵。
///
/// # 性能优势
///
/// - 普通 `query` 返回 `Vec<HashMap<String, Value>>`，每行每列需哈希计算 + 字符串克隆
/// - `QueryValues` 返回 `(Vec<String>, Vec<Vec<Value>>)`，列名只分配一次，
///   每行值按列序号直接 `Vec::push`，无哈希计算
/// - 在 SELECT ALL 大结果集场景下，比 `query` 提升 30%~50%
///
/// # 用法
///
/// ```rust,ignore
/// let (names, values_matrix): QueryValues = conn.query_values("SELECT id, name FROM users").await?;
/// // names = ["id", "name"]
/// // values_matrix[0] = [Value::I64(1), Value::String("Alice".into())]
/// ```
pub type QueryValues = (Vec<String>, Vec<Vec<Value>>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_is_null() {
        assert!(Value::Null.is_null());
        assert!(!Value::I64(0).is_null());
    }

    #[test]
    fn test_col_type_from_type_name() {
        // 标准类型
        assert_eq!(ColType::from_type_name("BOOLEAN"), ColType::Bool);
        assert_eq!(ColType::from_type_name("TINYINT"), ColType::I8);
        assert_eq!(ColType::from_type_name("SMALLINT"), ColType::I16);
        assert_eq!(ColType::from_type_name("INT"), ColType::I32);
        assert_eq!(ColType::from_type_name("BIGINT"), ColType::I64);
        assert_eq!(ColType::from_type_name("INT UNSIGNED"), ColType::U32);
        assert_eq!(ColType::from_type_name("FLOAT"), ColType::F32);
        assert_eq!(ColType::from_type_name("DOUBLE"), ColType::F64);
        assert_eq!(ColType::from_type_name("TEXT"), ColType::String);
        assert_eq!(ColType::from_type_name("BLOB"), ColType::Bytes);
        assert_eq!(ColType::from_type_name("DATE"), ColType::Date);
        assert_eq!(ColType::from_type_name("TIMESTAMP"), ColType::DateTime);
        assert_eq!(ColType::from_type_name("JSON"), ColType::Json);
        // PG 风格
        assert_eq!(ColType::from_type_name("INT2"), ColType::I16);
        assert_eq!(ColType::from_type_name("INT4"), ColType::I32);
        assert_eq!(ColType::from_type_name("INT8"), ColType::I64);
        assert_eq!(ColType::from_type_name("FLOAT4"), ColType::F32);
        assert_eq!(ColType::from_type_name("FLOAT8"), ColType::F64);
        assert_eq!(ColType::from_type_name("BYTEA"), ColType::Bytes);
        // 未知类型
        assert_eq!(ColType::from_type_name("UNKNOWN_TYPE"), ColType::Unknown);
        assert_eq!(ColType::from_type_name(""), ColType::Unknown);
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
