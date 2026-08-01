//! Accessors / Mutators + Attribute Casting
//!
//! 对应文档 6.8 节改进项 22（Accessors/Mutators）+ 23（Attribute Casting）。
//!
//! # 核心概念
//!
//! - **Accessor**：字段读取器（getter），从存储值转换为展示值
//! - **Mutator**：字段设置器（setter），从输入值转换为存储值
//! - **AttributeCaster**：字段类型转换器（数据库 <-> Rust 类型）
//! - **AccessorRegistry**：Accessor/Mutator 注册中心
//!
//! # 设计灵感
//!
//! - Laravel Eloquent `getCasts()` / `mutators` / `accessors`
//! - Doctrine `@Column(type="...")` 类型转换
//! - Rails ActiveRecord `serialize` / `attr_accessor`
//!
//! # 使用示例
//!
//! ```no_run
//! use sz_orm_core::accessors::{
//!     AccessorRegistry, AttributeCaster, CastType,
//! };
//! use sz_orm_core::Value;
//!
//! let mut registry = AccessorRegistry::new();
//!
//! // 注册 is_admin 字段：数据库存 SMALLINT，读出时转为 bool
//! registry.register_cast("is_admin", CastType::Boolean);
//!
//! // 注册 settings 字段：数据库存 TEXT，读出时解析为 JSON
//! registry.register_cast("settings", CastType::Json);
//!
//! // 应用 casting（从数据库读出）
//! let stored = Value::I64(1);
//! let casted = registry.cast_read("is_admin", stored);
//! assert_eq!(casted, Value::Bool(true));
//! ```

use crate::value::Value;
use std::collections::HashMap;

// ============================================================================
// CastType — 字段类型转换枚举
// ============================================================================

/// 字段类型转换枚举
///
/// 定义字段在数据库存储与 Rust 类型之间的转换方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CastType {
    /// 转 i64（适用于 INTEGER/BIGINT → i64）
    Integer,
    /// 转 f64（适用于 FLOAT/DOUBLE → f64）
    Float,
    /// 转布尔（适用于 SMALLINT(0/1)/CHAR('Y'/'N') → bool）
    Boolean,
    /// 转字符串（适用于 TEXT/VARCHAR → String）
    String,
    /// 转 JSON（适用于 TEXT → JSON 反序列化）
    Json,
    /// 转 DateTime（适用于 TIMESTAMP → ISO8601 字符串）
    DateTime,
    /// 转 Date（适用于 DATE → YYYY-MM-DD 字符串）
    Date,
    /// 转 Time（适用于 TIME → HH:MM:SS 字符串）
    Time,
    /// 转 Bytes（适用于 BLOB → `Vec<u8>`)
    Bytes,
    /// 转 Array（适用于 JSON 数组 → `Vec<Value>`)
    Array,
}

impl CastType {
    /// 类型名称（用于错误信息）
    pub fn name(&self) -> &'static str {
        match self {
            CastType::Integer => "integer",
            CastType::Float => "float",
            CastType::Boolean => "boolean",
            CastType::String => "string",
            CastType::Json => "json",
            CastType::DateTime => "datetime",
            CastType::Date => "date",
            CastType::Time => "time",
            CastType::Bytes => "bytes",
            CastType::Array => "array",
        }
    }
}

// ============================================================================
// Accessor / Mutator trait — 自定义字段读写器
// ============================================================================

/// 自定义字段读取器（Accessor / Getter）
///
/// 在从数据库读取字段值后调用，将存储值转换为展示值。
pub trait Accessor: Send + Sync {
    /// 字段名
    fn field(&self) -> &str;

    /// 读取转换：将存储值转换为展示值
    fn read(&self, value: Value) -> Value;
}

/// 自定义字段设置器（Mutator / Setter）
///
/// 在写入数据库前调用，将输入值转换为存储值。
pub trait Mutator: Send + Sync {
    /// 字段名
    fn field(&self) -> &str;

    /// 写入转换：将输入值转换为存储值
    fn write(&self, value: Value) -> Value;
}

// ============================================================================
// 闭包风格的 Accessor / Mutator
// ============================================================================

/// 闭包风格 Accessor
pub struct ClosureAccessor {
    /// 字段名
    pub field_name: String,
    /// 读取转换闭包
    pub reader: Box<dyn Fn(Value) -> Value + Send + Sync>,
}

impl ClosureAccessor {
    /// 创建闭包 Accessor
    pub fn new(
        field: impl Into<String>,
        reader: impl Fn(Value) -> Value + Send + Sync + 'static,
    ) -> Self {
        Self {
            field_name: field.into(),
            reader: Box::new(reader),
        }
    }
}

impl Accessor for ClosureAccessor {
    fn field(&self) -> &str {
        &self.field_name
    }

    fn read(&self, value: Value) -> Value {
        (self.reader)(value)
    }
}

/// 闭包风格 Mutator
pub struct ClosureMutator {
    /// 字段名
    pub field_name: String,
    /// 写入转换闭包
    pub writer: Box<dyn Fn(Value) -> Value + Send + Sync>,
}

impl ClosureMutator {
    /// 创建闭包 Mutator
    pub fn new(
        field: impl Into<String>,
        writer: impl Fn(Value) -> Value + Send + Sync + 'static,
    ) -> Self {
        Self {
            field_name: field.into(),
            writer: Box::new(writer),
        }
    }
}

impl Mutator for ClosureMutator {
    fn field(&self) -> &str {
        &self.field_name
    }

    fn write(&self, value: Value) -> Value {
        (self.writer)(value)
    }
}

// ============================================================================
// AttributeCaster — 类型转换器（数据库 <-> Rust 类型）
// ============================================================================

/// 类型转换器
///
/// 根据 `CastType` 将 Value 在数据库存储类型与 Rust 业务类型之间转换。
pub struct AttributeCaster;

impl AttributeCaster {
    /// 从数据库读出时的类型转换（db → rust）
    pub fn cast_read(value: Value, target: CastType) -> Value {
        match target {
            CastType::Integer => Self::to_integer(value),
            CastType::Float => Self::to_float(value),
            CastType::Boolean => Self::to_boolean(value),
            CastType::String => Self::to_string_value(value),
            CastType::Json => Self::to_json(value),
            CastType::DateTime => Self::to_datetime(value),
            CastType::Date => Self::to_date(value),
            CastType::Time => Self::to_time(value),
            CastType::Bytes => Self::to_bytes(value),
            CastType::Array => Self::to_array(value),
        }
    }

    /// 写入数据库时的类型转换（rust → db）
    pub fn cast_write(value: Value, target: CastType) -> Value {
        match target {
            CastType::Integer => Self::to_integer(value),
            CastType::Float => Self::to_float(value),
            CastType::Boolean => Self::to_boolean_storage(value),
            CastType::String => Self::to_string_value(value),
            CastType::Json => Self::to_json_storage(value),
            CastType::DateTime => Self::to_datetime_storage(value),
            CastType::Date => Self::to_date_storage(value),
            CastType::Time => Self::to_time_storage(value),
            CastType::Bytes => Self::to_bytes(value),
            CastType::Array => Self::to_array_storage(value),
        }
    }

    // ===== 转换函数 =====

    fn to_integer(value: Value) -> Value {
        match value {
            Value::I64(_) | Value::I32(_) | Value::I8(_) | Value::I16(_) => value,
            Value::U32(v) => Value::I64(v as i64),
            Value::U64(v) => Value::I64(v as i64),
            Value::U8(v) => Value::I64(v as i64),
            Value::U16(v) => Value::I64(v as i64),
            Value::F32(v) => Value::I64(v as i64),
            Value::F64(v) => Value::I64(v as i64),
            Value::Bool(b) => Value::I64(if b { 1 } else { 0 }),
            Value::String(s) => {
                if let Ok(n) = s.trim().parse::<i64>() {
                    Value::I64(n)
                } else {
                    Value::Null
                }
            }
            Value::Null => Value::Null,
            _ => Value::Null,
        }
    }

    fn to_float(value: Value) -> Value {
        match value {
            Value::F32(_) | Value::F64(_) => value,
            Value::I64(v) => Value::F64(v as f64),
            Value::I32(v) => Value::F64(v as f64),
            Value::I8(v) => Value::F64(v as f64),
            Value::I16(v) => Value::F64(v as f64),
            Value::U32(v) => Value::F64(v as f64),
            Value::U64(v) => Value::F64(v as f64),
            Value::U8(v) => Value::F64(v as f64),
            Value::U16(v) => Value::F64(v as f64),
            Value::Bool(b) => Value::F64(if b { 1.0 } else { 0.0 }),
            Value::String(s) => {
                if let Ok(n) = s.trim().parse::<f64>() {
                    Value::F64(n)
                } else {
                    Value::Null
                }
            }
            Value::Null => Value::Null,
            _ => Value::Null,
        }
    }

    fn to_boolean(value: Value) -> Value {
        match value {
            Value::Bool(_) => value,
            Value::I64(v) => Value::Bool(v != 0),
            Value::I32(v) => Value::Bool(v != 0),
            Value::I8(v) => Value::Bool(v != 0),
            Value::I16(v) => Value::Bool(v != 0),
            Value::U32(v) => Value::Bool(v != 0),
            Value::U64(v) => Value::Bool(v != 0),
            Value::U8(v) => Value::Bool(v != 0),
            Value::U16(v) => Value::Bool(v != 0),
            Value::F32(v) => Value::Bool(v != 0.0),
            Value::F64(v) => Value::Bool(v != 0.0),
            Value::String(s) => {
                let lower = s.trim().to_lowercase();
                Value::Bool(matches!(
                    lower.as_str(),
                    "1" | "true" | "yes" | "on" | "y" | "t"
                ))
            }
            Value::Null => Value::Null,
            _ => Value::Null,
        }
    }

    fn to_boolean_storage(value: Value) -> Value {
        match value {
            Value::Bool(b) => Value::I64(if b { 1 } else { 0 }),
            Value::I64(_) | Value::I32(_) | Value::I8(_) | Value::I16(_) => value,
            Value::U32(v) => Value::I64(if v != 0 { 1 } else { 0 }),
            Value::U64(v) => Value::I64(if v != 0 { 1 } else { 0 }),
            Value::U8(v) => Value::I64(if v != 0 { 1 } else { 0 }),
            Value::U16(v) => Value::I64(if v != 0 { 1 } else { 0 }),
            Value::F32(v) => Value::I64(if v != 0.0 { 1 } else { 0 }),
            Value::F64(v) => Value::I64(if v != 0.0 { 1 } else { 0 }),
            Value::String(s) => {
                let lower = s.trim().to_lowercase();
                Value::I64(
                    if matches!(lower.as_str(), "1" | "true" | "yes" | "on" | "y" | "t") {
                        1
                    } else {
                        0
                    },
                )
            }
            Value::Null => Value::Null,
            _ => Value::Null,
        }
    }

    fn to_string_value(value: Value) -> Value {
        match value {
            Value::String(_) => value,
            Value::I64(v) => Value::String(v.to_string()),
            Value::I32(v) => Value::String(v.to_string()),
            Value::I8(v) => Value::String(v.to_string()),
            Value::I16(v) => Value::String(v.to_string()),
            Value::U32(v) => Value::String(v.to_string()),
            Value::U64(v) => Value::String(v.to_string()),
            Value::U8(v) => Value::String(v.to_string()),
            Value::U16(v) => Value::String(v.to_string()),
            Value::F32(v) => Value::String(v.to_string()),
            Value::F64(v) => Value::String(v.to_string()),
            Value::Bool(b) => Value::String(b.to_string()),
            Value::Null => Value::Null,
            other => Value::String(format!("{:?}", other)),
        }
    }

    fn to_json(value: Value) -> Value {
        match value {
            Value::String(s) => {
                // 校验是否为合法 JSON；合法则包装为 Value::Json，否则保留为 String
                if serde_json::from_str::<serde_json::Value>(&s).is_ok() {
                    Value::Json(s)
                } else {
                    Value::String(s)
                }
            }
            Value::Json(s) => Value::Json(s),
            other => Value::Json(value_to_json_string(&other)),
        }
    }

    fn to_json_storage(value: Value) -> Value {
        match value {
            Value::Json(s) => Value::Json(s),
            Value::String(s) => Value::Json(s),
            other => Value::Json(value_to_json_string(&other)),
        }
    }

    fn to_datetime(value: Value) -> Value {
        match value {
            Value::DateTime(s) => Value::DateTime(s),
            Value::String(s) => Value::DateTime(s),
            Value::Null => Value::Null,
            other => Value::DateTime(format!("{:?}", other)),
        }
    }

    fn to_datetime_storage(value: Value) -> Value {
        match value {
            Value::DateTime(s) => Value::DateTime(s),
            Value::String(s) => Value::DateTime(s),
            Value::Null => Value::Null,
            other => Value::DateTime(format!("{:?}", other)),
        }
    }

    fn to_date(value: Value) -> Value {
        match value {
            Value::Date(s) => Value::Date(s),
            Value::String(s) => Value::Date(s),
            Value::Null => Value::Null,
            other => Value::Date(format!("{:?}", other)),
        }
    }

    fn to_date_storage(value: Value) -> Value {
        match value {
            Value::Date(s) => Value::Date(s),
            Value::String(s) => Value::Date(s),
            Value::Null => Value::Null,
            other => Value::Date(format!("{:?}", other)),
        }
    }

    fn to_time(value: Value) -> Value {
        match value {
            Value::Time(s) => Value::Time(s),
            Value::String(s) => Value::Time(s),
            Value::Null => Value::Null,
            other => Value::Time(format!("{:?}", other)),
        }
    }

    fn to_time_storage(value: Value) -> Value {
        match value {
            Value::Time(s) => Value::Time(s),
            Value::String(s) => Value::Time(s),
            Value::Null => Value::Null,
            other => Value::Time(format!("{:?}", other)),
        }
    }

    fn to_bytes(value: Value) -> Value {
        match value {
            Value::Bytes(_) => value,
            Value::String(s) => Value::Bytes(s.into_bytes()),
            Value::Null => Value::Null,
            _ => Value::Null,
        }
    }

    fn to_array(value: Value) -> Value {
        match value {
            Value::Array(_) => value,
            Value::Json(s) => {
                // 尝试解析 JSON 数组；解析失败则包装为单元素数组
                match serde_json::from_str::<Vec<serde_json::Value>>(&s) {
                    Ok(json_arr) => {
                        let items: Vec<Value> = json_arr
                            .into_iter()
                            .map(|jv| json_to_value(jv))
                            .collect();
                        Value::Array(items)
                    }
                    Err(_) => Value::Array(vec![Value::Json(s)]),
                }
            }
            Value::String(s) => {
                // 尝试解析字符串为 JSON 数组；失败则包装为单元素数组
                match serde_json::from_str::<Vec<serde_json::Value>>(&s) {
                    Ok(json_arr) => {
                        let items: Vec<Value> = json_arr
                            .into_iter()
                            .map(|jv| json_to_value(jv))
                            .collect();
                        Value::Array(items)
                    }
                    Err(_) => Value::Array(vec![Value::String(s)]),
                }
            }
            Value::Null => Value::Null,
            other => Value::Array(vec![other]),
        }
    }

    fn to_array_storage(value: Value) -> Value {
        match value {
            Value::Array(items) => {
                // 序列化为合法 JSON 数组字符串存储
                let json_arr: Vec<serde_json::Value> =
                    items.iter().map(|v| value_to_json(&v)).collect();
                Value::Json(serde_json::to_string(&json_arr).unwrap_or_else(|_| "[]".to_string()))
            }
            other => Value::Json(value_to_json_string(&other)),
        }
    }
}

/// 将 `Value` 转换为 `serde_json::Value`
///
/// 用于 `to_array_storage` / `to_json_storage` 等场景，确保产生合法 JSON。
fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::I8(v) => serde_json::Value::Number((*v).into()),
        Value::I16(v) => serde_json::Value::Number((*v).into()),
        Value::I32(v) => serde_json::Value::Number((*v).into()),
        Value::I64(v) => serde_json::Value::Number((*v).into()),
        Value::U8(v) => serde_json::Value::Number((*v).into()),
        Value::U16(v) => serde_json::Value::Number((*v).into()),
        Value::U32(v) => serde_json::Value::Number((*v).into()),
        Value::U64(v) => serde_json::Value::Number((*v).into()),
        Value::F32(v) => {
            serde_json::Number::from_f64(*v as f64).map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        }
        Value::F64(v) => {
            serde_json::Number::from_f64(*v).map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        }
        Value::Decimal(s) => {
            // 高精度十进制数：尝试作为数字，否则作为字符串
            serde_json::from_str(s).unwrap_or_else(|_| serde_json::Value::String(s.clone()))
        }
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Bytes(b) => {
            // 字节值：以 base64 编码字符串形式表示
            use std::fmt::Write;
            let mut s = String::with_capacity(b.len() * 2);
            for byte in b {
                let _ = write!(&mut s, "{:02x}", byte);
            }
            serde_json::Value::String(s)
        }
        Value::Uuid(s) => serde_json::Value::String(s.clone()),
        Value::Date(s) => serde_json::Value::String(s.clone()),
        Value::DateTime(s) => serde_json::Value::String(s.clone()),
        Value::Time(s) => serde_json::Value::String(s.clone()),
        Value::Json(s) => {
            serde_json::from_str(s).unwrap_or(serde_json::Value::String(s.clone()))
        }
        Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect())
        }
        Value::Object(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(k.clone(), value_to_json(v));
            }
            serde_json::Value::Object(obj)
        }
    }
}

/// 将 `Value` 转换为 JSON 字符串
fn value_to_json_string(value: &Value) -> String {
    serde_json::to_string(&value_to_json(value)).unwrap_or_else(|_| "null".to_string())
}

/// 将 `serde_json::Value` 转换为内部 `Value`
///
/// 用于 `to_array` 等场景，将解析出的 JSON 数组元素转换为内部 Value。
fn json_to_value(jv: serde_json::Value) -> Value {
    match jv {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::I64(i)
            } else if let Some(u) = n.as_u64() {
                Value::U64(u)
            } else if let Some(f) = n.as_f64() {
                Value::F64(f)
            } else {
                Value::Null
            }
        }
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(arr) => {
            Value::Array(arr.into_iter().map(json_to_value).collect())
        }
        serde_json::Value::Object(obj) => {
            let mut map = std::collections::HashMap::new();
            for (k, v) in obj {
                map.insert(k, json_to_value(v));
            }
            Value::Object(map)
        }
    }
}

// ============================================================================
// AccessorRegistry — 注册中心
// ============================================================================

/// Accessor / Mutator / Cast 注册中心
///
/// 管理字段级别的读取器、设置器、类型转换器。
pub struct AccessorRegistry {
    /// 字段读取器
    accessors: HashMap<String, Box<dyn Accessor>>,
    /// 字段设置器
    mutators: HashMap<String, Box<dyn Mutator>>,
    /// 字段类型转换
    casts: HashMap<String, CastType>,
}

impl Default for AccessorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AccessorRegistry {
    /// 创建空注册中心
    pub fn new() -> Self {
        Self {
            accessors: HashMap::new(),
            mutators: HashMap::new(),
            casts: HashMap::new(),
        }
    }

    /// 注册 Accessor
    pub fn register_accessor(&mut self, accessor: Box<dyn Accessor>) {
        let field = accessor.field().to_string();
        self.accessors.insert(field, accessor);
    }

    /// 注册 Mutator
    pub fn register_mutator(&mut self, mutator: Box<dyn Mutator>) {
        let field = mutator.field().to_string();
        self.mutators.insert(field, mutator);
    }

    /// 注册类型转换
    pub fn register_cast(&mut self, field: impl Into<String>, cast: CastType) {
        self.casts.insert(field.into(), cast);
    }

    /// 应用读取流程：cast_read → accessor.read
    pub fn read(&self, field: &str, value: Value) -> Value {
        let v1 = if let Some(cast) = self.casts.get(field) {
            AttributeCaster::cast_read(value, *cast)
        } else {
            value
        };
        if let Some(accessor) = self.accessors.get(field) {
            accessor.read(v1)
        } else {
            v1
        }
    }

    /// 应用写入流程：mutator.write → cast_write
    pub fn write(&self, field: &str, value: Value) -> Value {
        let v1 = if let Some(mutator) = self.mutators.get(field) {
            mutator.write(value)
        } else {
            value
        };
        if let Some(cast) = self.casts.get(field) {
            AttributeCaster::cast_write(v1, *cast)
        } else {
            v1
        }
    }

    /// 仅应用类型转换（读取方向）
    pub fn cast_read(&self, field: &str, value: Value) -> Value {
        if let Some(cast) = self.casts.get(field) {
            AttributeCaster::cast_read(value, *cast)
        } else {
            value
        }
    }

    /// 仅应用类型转换（写入方向）
    pub fn cast_write(&self, field: &str, value: Value) -> Value {
        if let Some(cast) = self.casts.get(field) {
            AttributeCaster::cast_write(value, *cast)
        } else {
            value
        }
    }

    /// 检查字段是否已注册 Accessor
    pub fn has_accessor(&self, field: &str) -> bool {
        self.accessors.contains_key(field)
    }

    /// 检查字段是否已注册 Mutator
    pub fn has_mutator(&self, field: &str) -> bool {
        self.mutators.contains_key(field)
    }

    /// 检查字段是否已注册 Cast
    pub fn has_cast(&self, field: &str) -> bool {
        self.casts.contains_key(field)
    }

    /// 获取字段已注册的 CastType
    pub fn get_cast(&self, field: &str) -> Option<CastType> {
        self.casts.get(field).copied()
    }

    /// 已注册 Accessor 数量
    pub fn accessor_count(&self) -> usize {
        self.accessors.len()
    }

    /// 已注册 Mutator 数量
    pub fn mutator_count(&self) -> usize {
        self.mutators.len()
    }

    /// 已注册 Cast 数量
    pub fn cast_count(&self) -> usize {
        self.casts.len()
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ===== CastType 测试 =====

    #[test]
    fn test_cast_type_name() {
        assert_eq!(CastType::Integer.name(), "integer");
        assert_eq!(CastType::Boolean.name(), "boolean");
        assert_eq!(CastType::Json.name(), "json");
        assert_eq!(CastType::DateTime.name(), "datetime");
    }

    // ===== AttributeCaster - Integer =====

    #[test]
    fn test_cast_to_integer_from_string() {
        let v = AttributeCaster::cast_read(Value::String("42".to_string()), CastType::Integer);
        assert_eq!(v, Value::I64(42));
    }

    #[test]
    fn test_cast_to_integer_from_invalid_string() {
        let v = AttributeCaster::cast_read(Value::String("abc".to_string()), CastType::Integer);
        assert_eq!(v, Value::Null);
    }

    #[test]
    fn test_cast_to_integer_from_bool() {
        let v = AttributeCaster::cast_read(Value::Bool(true), CastType::Integer);
        assert_eq!(v, Value::I64(1));
    }

    #[test]
    fn test_cast_to_integer_from_float() {
        let v = AttributeCaster::cast_read(Value::F64(3.7), CastType::Integer);
        assert_eq!(v, Value::I64(3));
    }

    #[test]
    fn test_cast_to_integer_preserves_i64() {
        let v = AttributeCaster::cast_read(Value::I64(100), CastType::Integer);
        assert_eq!(v, Value::I64(100));
    }

    // ===== AttributeCaster - Float =====

    #[test]
    fn test_cast_to_float_from_string() {
        let v = AttributeCaster::cast_read(Value::String("3.15".to_string()), CastType::Float);
        assert_eq!(v, Value::F64(3.15));
    }

    #[test]
    fn test_cast_to_float_from_i64() {
        let v = AttributeCaster::cast_read(Value::I64(42), CastType::Float);
        assert_eq!(v, Value::F64(42.0));
    }

    // ===== AttributeCaster - Boolean =====

    #[test]
    fn test_cast_to_boolean_from_i64_one() {
        let v = AttributeCaster::cast_read(Value::I64(1), CastType::Boolean);
        assert_eq!(v, Value::Bool(true));
    }

    #[test]
    fn test_cast_to_boolean_from_i64_zero() {
        let v = AttributeCaster::cast_read(Value::I64(0), CastType::Boolean);
        assert_eq!(v, Value::Bool(false));
    }

    #[test]
    fn test_cast_to_boolean_from_string_true() {
        let v = AttributeCaster::cast_read(Value::String("true".to_string()), CastType::Boolean);
        assert_eq!(v, Value::Bool(true));
    }

    #[test]
    fn test_cast_to_boolean_from_string_yes() {
        let v = AttributeCaster::cast_read(Value::String("yes".to_string()), CastType::Boolean);
        assert_eq!(v, Value::Bool(true));
    }

    #[test]
    fn test_cast_to_boolean_from_string_on() {
        let v = AttributeCaster::cast_read(Value::String("on".to_string()), CastType::Boolean);
        assert_eq!(v, Value::Bool(true));
    }

    #[test]
    fn test_cast_to_boolean_from_string_random() {
        let v = AttributeCaster::cast_read(Value::String("random".to_string()), CastType::Boolean);
        assert_eq!(v, Value::Bool(false));
    }

    #[test]
    fn test_cast_to_boolean_preserves_bool() {
        let v = AttributeCaster::cast_read(Value::Bool(true), CastType::Boolean);
        assert_eq!(v, Value::Bool(true));
    }

    // ===== AttributeCaster - Boolean Storage（写入方向）=====

    #[test]
    fn test_cast_to_boolean_storage_from_bool() {
        let v = AttributeCaster::cast_write(Value::Bool(true), CastType::Boolean);
        assert_eq!(v, Value::I64(1));
    }

    #[test]
    fn test_cast_to_boolean_storage_from_string() {
        let v = AttributeCaster::cast_write(Value::String("yes".to_string()), CastType::Boolean);
        assert_eq!(v, Value::I64(1));
    }

    // ===== AttributeCaster - String =====

    #[test]
    fn test_cast_to_string_from_i64() {
        let v = AttributeCaster::cast_read(Value::I64(42), CastType::String);
        assert_eq!(v, Value::String("42".to_string()));
    }

    #[test]
    fn test_cast_to_string_from_bool() {
        let v = AttributeCaster::cast_read(Value::Bool(true), CastType::String);
        assert_eq!(v, Value::String("true".to_string()));
    }

    #[test]
    fn test_cast_to_string_preserves_string() {
        let v = AttributeCaster::cast_read(Value::String("hello".to_string()), CastType::String);
        assert_eq!(v, Value::String("hello".to_string()));
    }

    // ===== AttributeCaster - Json =====

    #[test]
    fn test_cast_to_json_from_string() {
        let v = AttributeCaster::cast_read(
            Value::String(r#"{"key":"value"}"#.to_string()),
            CastType::Json,
        );
        // 合法 JSON 字符串应转换为 Value::Json
        assert!(matches!(v, Value::Json(_)));
        if let Value::Json(s) = v {
            // 验证 JSON 内容正确
            let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
            assert_eq!(parsed["key"], "value");
        }
    }

    #[test]
    fn test_cast_to_json_from_invalid_string() {
        let v = AttributeCaster::cast_read(
            Value::String("not a json".to_string()),
            CastType::Json,
        );
        // 非法 JSON 字符串应保留为 String
        assert!(matches!(v, Value::String(_)));
    }

    #[test]
    fn test_cast_to_json_from_other() {
        let v = AttributeCaster::cast_read(Value::I64(42), CastType::Json);
        assert!(matches!(v, Value::Json(_)));
        if let Value::Json(s) = v {
            // 验证产生的 JSON 是合法的
            let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
            assert_eq!(parsed, serde_json::Value::Number(42.into()));
        }
    }

    // ===== AttributeCaster - DateTime / Date / Time =====

    #[test]
    fn test_cast_to_datetime_from_string() {
        let v = AttributeCaster::cast_read(
            Value::String("2026-07-19T10:00:00Z".to_string()),
            CastType::DateTime,
        );
        assert_eq!(v, Value::DateTime("2026-07-19T10:00:00Z".to_string()));
    }

    #[test]
    fn test_cast_to_date_from_string() {
        let v = AttributeCaster::cast_read(Value::String("2026-07-19".to_string()), CastType::Date);
        assert_eq!(v, Value::Date("2026-07-19".to_string()));
    }

    #[test]
    fn test_cast_to_time_from_string() {
        let v = AttributeCaster::cast_read(Value::String("10:30:00".to_string()), CastType::Time);
        assert_eq!(v, Value::Time("10:30:00".to_string()));
    }

    // ===== AttributeCaster - Bytes =====

    #[test]
    fn test_cast_to_bytes_from_string() {
        let v = AttributeCaster::cast_read(Value::String("hello".to_string()), CastType::Bytes);
        assert_eq!(v, Value::Bytes(b"hello".to_vec()));
    }

    #[test]
    fn test_cast_to_bytes_preserves_bytes() {
        let v = AttributeCaster::cast_read(Value::Bytes(b"data".to_vec()), CastType::Bytes);
        assert_eq!(v, Value::Bytes(b"data".to_vec()));
    }

    // ===== AttributeCaster - Array =====

    #[test]
    fn test_cast_to_array_from_string() {
        let v = AttributeCaster::cast_read(Value::String("item".to_string()), CastType::Array);
        assert!(matches!(v, Value::Array(_)));
        if let Value::Array(arr) = v {
            assert_eq!(arr.len(), 1);
        }
    }

    #[test]
    fn test_cast_to_array_from_json_string() {
        // 合法 JSON 数组字符串应被正确解析
        let v = AttributeCaster::cast_read(
            Value::String("[1, 2, 3]".to_string()),
            CastType::Array,
        );
        assert!(matches!(v, Value::Array(_)));
        if let Value::Array(arr) = v {
            assert_eq!(arr.len(), 3);
            assert_eq!(arr[0], Value::I64(1));
            assert_eq!(arr[1], Value::I64(2));
            assert_eq!(arr[2], Value::I64(3));
        }
    }

    #[test]
    fn test_cast_to_array_from_json_value() {
        // Value::Json 中的合法 JSON 数组应被正确解析
        let v = AttributeCaster::cast_read(
            Value::Json(r#"["a", "b"]"#.to_string()),
            CastType::Array,
        );
        assert!(matches!(v, Value::Array(_)));
        if let Value::Array(arr) = v {
            assert_eq!(arr.len(), 2);
            assert_eq!(arr[0], Value::String("a".to_string()));
            assert_eq!(arr[1], Value::String("b".to_string()));
        }
    }

    #[test]
    fn test_cast_to_array_preserves_array() {
        let arr = vec![Value::I64(1), Value::I64(2)];
        let v = AttributeCaster::cast_read(Value::Array(arr.clone()), CastType::Array);
        assert_eq!(v, Value::Array(arr));
    }

    #[test]
    fn test_cast_to_array_storage_serializes_to_json() {
        let v = AttributeCaster::cast_write(
            Value::Array(vec![Value::I64(1), Value::I64(2)]),
            CastType::Array,
        );
        assert!(matches!(v, Value::Json(_)));
        if let Value::Json(s) = v {
            // 验证产生的 JSON 是合法的 JSON 数组
            let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
            assert!(parsed.is_array());
            assert_eq!(parsed[0], serde_json::Value::Number(1.into()));
            assert_eq!(parsed[1], serde_json::Value::Number(2.into()));
        }
    }

    #[test]
    fn test_cast_to_array_storage_not_debug_format() {
        // P2-5 回归测试：确保不再使用 Debug 格式（[I64(1), I64(2)]）
        let v = AttributeCaster::cast_write(
            Value::Array(vec![Value::I64(1), Value::I64(2)]),
            CastType::Array,
        );
        if let Value::Json(s) = v {
            // Debug 格式会包含 "I64" 字样，合法 JSON 不会
            assert!(!s.contains("I64"), "JSON 不应包含 Debug 格式 I64: {}", s);
            // 应为合法 JSON 数组
            let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
            assert!(parsed.is_array());
        }
    }

    #[test]
    fn test_cast_to_json_storage_from_other() {
        let v = AttributeCaster::cast_write(Value::I64(42), CastType::Json);
        assert!(matches!(v, Value::Json(_)));
        if let Value::Json(s) = v {
            // 验证产生的 JSON 是合法的
            let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
            assert_eq!(parsed, serde_json::Value::Number(42.into()));
        }
    }

    // ===== ClosureAccessor / ClosureMutator =====

    #[test]
    fn test_closure_accessor() {
        let accessor = ClosureAccessor::new("name", |v| match v {
            Value::String(s) => Value::String(s.to_uppercase()),
            other => other,
        });
        let v = accessor.read(Value::String("alice".to_string()));
        assert_eq!(v, Value::String("ALICE".to_string()));
        assert_eq!(accessor.field(), "name");
    }

    #[test]
    fn test_closure_mutator() {
        let mutator = ClosureMutator::new("email", |v| match v {
            Value::String(s) => Value::String(s.to_lowercase()),
            other => other,
        });
        let v = mutator.write(Value::String("ALICE@EXAMPLE.COM".to_string()));
        assert_eq!(v, Value::String("alice@example.com".to_string()));
        assert_eq!(mutator.field(), "email");
    }

    // ===== AccessorRegistry - 基本操作 =====

    #[test]
    fn test_registry_empty() {
        let r = AccessorRegistry::new();
        assert_eq!(r.accessor_count(), 0);
        assert_eq!(r.mutator_count(), 0);
        assert_eq!(r.cast_count(), 0);
    }

    #[test]
    fn test_registry_register_cast() {
        let mut r = AccessorRegistry::new();
        r.register_cast("is_admin", CastType::Boolean);
        assert!(r.has_cast("is_admin"));
        assert_eq!(r.get_cast("is_admin"), Some(CastType::Boolean));
        assert_eq!(r.cast_count(), 1);
    }

    #[test]
    fn test_registry_register_accessor() {
        let mut r = AccessorRegistry::new();
        r.register_accessor(Box::new(ClosureAccessor::new("name", |v| match v {
            Value::String(s) => Value::String(s.to_uppercase()),
            other => other,
        })));
        assert!(r.has_accessor("name"));
        assert_eq!(r.accessor_count(), 1);
    }

    #[test]
    fn test_registry_register_mutator() {
        let mut r = AccessorRegistry::new();
        r.register_mutator(Box::new(ClosureMutator::new("email", |v| match v {
            Value::String(s) => Value::String(s.to_lowercase()),
            other => other,
        })));
        assert!(r.has_mutator("email"));
        assert_eq!(r.mutator_count(), 1);
    }

    // ===== AccessorRegistry - read/write 流程 =====

    #[test]
    fn test_registry_read_applies_cast_then_accessor() {
        let mut r = AccessorRegistry::new();
        r.register_cast("is_admin", CastType::Boolean);
        r.register_accessor(Box::new(ClosureAccessor::new("is_admin", |v| {
            if v == Value::Bool(true) {
                Value::String("管理员".to_string())
            } else {
                Value::String("普通用户".to_string())
            }
        })));

        // 读取：I64(1) → cast(Boolean) → Bool(true) → accessor → String("管理员")
        let v = r.read("is_admin", Value::I64(1));
        assert_eq!(v, Value::String("管理员".to_string()));
    }

    #[test]
    fn test_registry_write_applies_mutator_then_cast() {
        let mut r = AccessorRegistry::new();
        r.register_cast("is_admin", CastType::Boolean);
        r.register_mutator(Box::new(ClosureMutator::new("is_admin", |v| match v {
            Value::String(s) => {
                let lower = s.to_lowercase();
                Value::Bool(lower == "admin" || lower == "true")
            }
            other => other,
        })));

        // 写入：String("admin") → mutator → Bool(true) → cast → I64(1)
        let v = r.write("is_admin", Value::String("admin".to_string()));
        assert_eq!(v, Value::I64(1));
    }

    #[test]
    fn test_registry_read_without_cast_or_accessor() {
        let r = AccessorRegistry::new();
        let v = r.read("any_field", Value::I64(42));
        assert_eq!(v, Value::I64(42));
    }

    #[test]
    fn test_registry_write_without_cast_or_mutator() {
        let r = AccessorRegistry::new();
        let v = r.write("any_field", Value::I64(42));
        assert_eq!(v, Value::I64(42));
    }

    #[test]
    fn test_registry_cast_read_only() {
        let mut r = AccessorRegistry::new();
        r.register_cast("is_admin", CastType::Boolean);

        let v = r.cast_read("is_admin", Value::I64(1));
        assert_eq!(v, Value::Bool(true));
    }

    #[test]
    fn test_registry_cast_write_only() {
        let mut r = AccessorRegistry::new();
        r.register_cast("is_admin", CastType::Boolean);

        let v = r.cast_write("is_admin", Value::Bool(true));
        assert_eq!(v, Value::I64(1));
    }

    // ===== 综合场景测试 =====

    #[test]
    fn test_complex_user_model_scenario() {
        let mut r = AccessorRegistry::new();

        // 1. is_admin: i64(0/1) ↔ bool
        r.register_cast("is_admin", CastType::Boolean);

        // 2. email: 自动转小写
        r.register_mutator(Box::new(ClosureMutator::new("email", |v| match v {
            Value::String(s) => Value::String(s.to_lowercase()),
            other => other,
        })));

        // 3. full_name: 拼接 first + last（演示 accessor）
        r.register_accessor(Box::new(ClosureAccessor::new(
            "full_name",
            |v| v, // 简化：直接返回
        )));

        // 4. settings: JSON 字段
        r.register_cast("settings", CastType::Json);

        // 5. created_at: DateTime
        r.register_cast("created_at", CastType::DateTime);

        // 读取 is_admin
        let v = r.read("is_admin", Value::I64(1));
        assert_eq!(v, Value::Bool(true));

        // 写入 email
        let v = r.write("email", Value::String("Alice@Example.COM".to_string()));
        assert_eq!(v, Value::String("alice@example.com".to_string()));

        // 读取 settings（合法 JSON 应转换为 Value::Json）
        let v = r.read("settings", Value::String(r#"{"theme":"dark"}"#.to_string()));
        assert!(matches!(v, Value::Json(_)));

        assert_eq!(r.accessor_count(), 1);
        assert_eq!(r.mutator_count(), 1);
        assert_eq!(r.cast_count(), 3);
    }

    // ===== Default 测试 =====

    #[test]
    fn test_default_is_empty() {
        let r = AccessorRegistry::default();
        assert_eq!(r.accessor_count(), 0);
        assert_eq!(r.mutator_count(), 0);
        assert_eq!(r.cast_count(), 0);
    }
}
