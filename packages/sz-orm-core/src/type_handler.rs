//! TypeHandler SPI — 自定义类型处理器注册
//!
//! 对应文档 6.8 节改进项 31（TypeHandler SPI）。
//!
//! # 核心概念
//!
//! - **`TypeHandler<T>`**：类型处理器 trait，负责 Rust 类型 T 与 ORM `Value` 之间的双向转换
//! - **TypeHandlerRegistry**：类型处理器注册中心，按名称注册 + 按字段名绑定
//! - **内置处理器**：`DateTimeHandler`、`UuidHandler`、`JsonHandler`、`DecimalHandler`
//!
//! # 设计灵感
//!
//! - MyBatis `TypeHandler<T>` 接口
//! - Hibernate `AttributeConverter<X, Y>` / `@Converter`
//! - Doctrine `Types::getTypeRegistry()` 自定义类型
//! - SQLAlchemy `TypeDecorator`
//!
//! # 使用示例
//!
//! ```
//! use sz_orm_core::type_handler::{
//!     TypeHandler, TypeHandlerRegistry, DateTimeHandler,
//! };
//! use sz_orm_core::Value;
//!
//! let mut registry = TypeHandlerRegistry::new();
//!
//! // 1. 注册内置处理器
//! registry.register("datetime", Box::new(DateTimeHandler));
//!
//! // 2. 将字段绑定到处理器
//! registry.bind("created_at", "datetime");
//!
//! // 3. 读取时：Value -> Rust 类型（这里用 String 模拟 DateTime）
//! let stored = Value::String("2026-07-19T10:00:00Z".to_string());
//! let parsed: String = registry.handle("created_at", &stored).unwrap();
//! assert_eq!(parsed, "2026-07-19T10:00:00Z");
//!
//! // 4. 写入时：Rust 类型 -> Value
//! let value = registry.to_value("created_at", &String::from("2026-07-19T10:00:00Z")).unwrap();
//! assert!(matches!(value, Value::DateTime(_)));
//! ```

use crate::Value;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::RwLock;

// ============================================================================
// TypeHandlerError — 类型处理器错误
// ============================================================================

/// 类型处理器错误
#[derive(Debug)]
pub enum TypeHandlerError {
    /// 处理器不存在（按名称查找失败）
    HandlerNotFound {
        /// 查找的处理器名称
        name: String,
    },
    /// 字段未绑定任何处理器
    FieldNotBound {
        /// 字段名
        field: String,
    },
    /// 类型不匹配（TypeId 不一致）
    TypeMismatch {
        /// 期望的 Rust 类型 ID
        expected: TypeId,
        /// 实际的 Rust 类型 ID
        actual: TypeId,
        /// 实际类型名（用于错误信息）
        actual_type_name: String,
    },
    /// 值转换失败（如格式错误、解析错误）
    ConversionFailed {
        /// 错误描述
        reason: String,
    },
}

impl std::fmt::Display for TypeHandlerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeHandlerError::HandlerNotFound { name } => {
                write!(f, "TypeHandler `{}` not registered", name)
            }
            TypeHandlerError::FieldNotBound { field } => {
                write!(f, "Field `{}` is not bound to any TypeHandler", field)
            }
            TypeHandlerError::TypeMismatch {
                expected,
                actual,
                actual_type_name,
            } => {
                write!(
                    f,
                    "Type mismatch: expected {:?}, got {:?} ({})",
                    expected, actual, actual_type_name
                )
            }
            TypeHandlerError::ConversionFailed { reason } => {
                write!(f, "Conversion failed: {}", reason)
            }
        }
    }
}

impl std::error::Error for TypeHandlerError {}

/// TypeHandler 结果类型
pub type TypeHandlerResult<T> = Result<T, TypeHandlerError>;

// ============================================================================
// ErasedTypeHandler — 类型擦除的 TypeHandler（用于注册中心存储）
// ============================================================================

/// 类型擦除的 TypeHandler trait（用于注册中心存储 `Box<dyn ErasedTypeHandler>`）
///
/// 提供获取 `TypeId` 和类型名的能力，使注册中心能在运行时
/// 正确报告 `TypeMismatch` 错误（包含实际的 TypeId 而非硬编码 `()`）。
pub trait ErasedTypeHandler: Send + Sync {
    /// 返回注册时的 T 的 TypeId
    fn erased_type_id(&self) -> TypeId;

    /// 返回注册时的 T 的类型名（用于错误信息）
    fn erased_type_name(&self) -> &'static str;

    /// 提供到 `Any` 的向下转换入口
    fn as_any(&self) -> &dyn Any;
}

impl<T: 'static + Send + Sync> ErasedTypeHandler for Box<dyn TypeHandler<T>> {
    fn erased_type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn erased_type_name(&self) -> &'static str {
        std::any::type_name::<T>()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ============================================================================
// TypeHandler trait — 类型处理器接口
// ============================================================================

/// 类型处理器 trait — 负责 Rust 类型 T 与 ORM `Value` 之间的双向转换
///
/// # 设计要点
///
/// - **to_value**：Rust 类型 T → `Value`（写入数据库时调用）
/// - **from_value**：`Value` → Rust 类型 T（读取数据库时调用）
/// - **type_id**：返回 T 的 `TypeId`，用于 `Registry::handle` 的类型断言
/// - **type_name**：返回 T 的类型名（用于错误信息）
///
/// # 示例
///
/// ```
/// use sz_orm_core::type_handler::{TypeHandler, TypeHandlerResult, TypeHandlerError};
/// use sz_orm_core::Value;
/// use std::any::TypeId;
///
/// struct MyMoney(i64);
///
/// struct MoneyHandler;
///
/// impl TypeHandler<MyMoney> for MoneyHandler {
///     fn to_value(&self, value: &MyMoney) -> Value {
///         Value::I64(value.0)
///     }
///     fn from_value(&self, value: &Value) -> TypeHandlerResult<MyMoney> {
///         match value {
///             Value::I64(v) => Ok(MyMoney(*v)),
///             _ => Err(TypeHandlerError::ConversionFailed {
///                 reason: format!("Expected I64, got {:?}", value),
///             }),
///         }
///     }
/// }
/// ```
pub trait TypeHandler<T: 'static>: Send + Sync {
    /// Rust 类型 T → Value
    fn to_value(&self, value: &T) -> Value;

    /// Value → Rust 类型 T
    ///
    /// 注：按 Rust 约定 `from_*` 通常为关联函数，但本 trait 需要访问 handler 实例状态，
    /// 故保留 `&self`；此处显式 allow 以保留向后兼容接口。
    #[allow(clippy::wrong_self_convention)]
    fn from_value(&self, value: &Value) -> TypeHandlerResult<T>;

    /// 返回 T 的 TypeId（默认实现）
    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    /// 返回 T 的类型名（默认实现）
    fn type_name(&self) -> &'static str {
        std::any::type_name::<T>()
    }
}

// ============================================================================
// TypeHandlerRegistry — 类型处理器注册中心
// ============================================================================

/// 类型处理器注册中心
///
/// 维护两层映射：
/// 1. **handler 名称 → TypeHandler 实例**（`register("datetime", handler)`）
/// 2. **字段名 → handler 名称**（`bind("created_at", "datetime")`）
///
/// 通过两层映射，多个字段可复用同一 handler 实例，且 handler 可动态替换。
///
/// # 线程安全
///
/// 内部使用 `RwLock<HashMap<...>>`，支持多线程并发读取。
pub struct TypeHandlerRegistry {
    /// handler 名称 → 类型擦除的 handler 实例
    handlers: RwLock<HashMap<String, Box<dyn ErasedTypeHandler>>>,
    /// 字段名 → handler 名称
    field_bindings: RwLock<HashMap<String, String>>,
}

impl TypeHandlerRegistry {
    /// 创建空注册中心
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
            field_bindings: RwLock::new(HashMap::new()),
        }
    }

    /// 注册类型处理器
    ///
    /// # 参数
    /// - `name`：处理器名称（如 "datetime"、"uuid"、"json"）
    /// - `handler`：实现 `TypeHandler<T>` 的实例
    pub fn register<T: 'static + Send + Sync>(
        &self,
        name: impl Into<String>,
        handler: Box<dyn TypeHandler<T>>,
    ) {
        let mut handlers = self.handlers.write().unwrap();
        handlers.insert(name.into(), Box::new(handler));
    }

    /// 将字段绑定到已注册的处理器
    ///
    /// # 参数
    /// - `field`：字段名
    /// - `handler_name`：处理器名称
    pub fn bind(&self, field: impl Into<String>, handler_name: impl Into<String>) {
        let mut bindings = self.field_bindings.write().unwrap();
        bindings.insert(field.into(), handler_name.into());
    }

    /// 解除字段绑定
    pub fn unbind(&self, field: &str) {
        let mut bindings = self.field_bindings.write().unwrap();
        bindings.remove(field);
    }

    /// 注销处理器（同时解除所有使用该 handler 的字段绑定）
    pub fn unregister(&self, name: &str) {
        let mut handlers = self.handlers.write().unwrap();
        handlers.remove(name);
        let mut bindings = self.field_bindings.write().unwrap();
        bindings.retain(|_, v| v != name);
    }

    /// 判断 handler 是否已注册
    pub fn has_handler(&self, name: &str) -> bool {
        self.handlers.read().unwrap().contains_key(name)
    }

    /// 判断字段是否已绑定
    pub fn is_bound(&self, field: &str) -> bool {
        self.field_bindings.read().unwrap().contains_key(field)
    }

    /// 获取字段绑定的 handler 名称
    pub fn handler_name_of(&self, field: &str) -> Option<String> {
        self.field_bindings.read().unwrap().get(field).cloned()
    }

    /// 处理字段读取：`Value` → `T`
    ///
    /// 自动按字段名查找绑定的 handler，再调用 `TypeHandler::from_value`。
    ///
    /// # 类型安全
    ///
    /// 内部通过 `TypeId` 断言确保调用方传入的 T 与 handler 注册时的 T 一致。
    /// 若类型不匹配，返回 `TypeMismatch` 错误，包含**实际的** TypeId 和类型名
    /// （而非硬编码 `()`），便于调试。
    pub fn handle<T: 'static>(&self, field: &str, value: &Value) -> TypeHandlerResult<T> {
        let handler_name = {
            let bindings = self.field_bindings.read().unwrap();
            bindings
                .get(field)
                .cloned()
                .ok_or_else(|| TypeHandlerError::FieldNotBound {
                    field: field.to_string(),
                })?
        };

        let handlers = self.handlers.read().unwrap();
        let erased =
            handlers
                .get(&handler_name)
                .ok_or_else(|| TypeHandlerError::HandlerNotFound {
                    name: handler_name.clone(),
                })?;

        // 通过 erased_type_id 比较 TypeId，提前识别类型不匹配
        let actual_id = erased.erased_type_id();
        if actual_id != TypeId::of::<T>() {
            return Err(TypeHandlerError::TypeMismatch {
                expected: TypeId::of::<T>(),
                actual: actual_id,
                actual_type_name: erased.erased_type_name().to_string(),
            });
        }

        // 类型匹配，downcast 到具体的 Box<dyn TypeHandler<T>>
        let handler = erased
            .as_any()
            .downcast_ref::<Box<dyn TypeHandler<T>>>()
            .expect("TypeId 已校验匹配，downcast 必然成功");
        handler.from_value(value)
    }

    /// 处理字段写入：`T` → `Value`
    pub fn to_value<T: 'static>(&self, field: &str, value: &T) -> TypeHandlerResult<Value> {
        let handler_name = {
            let bindings = self.field_bindings.read().unwrap();
            bindings
                .get(field)
                .cloned()
                .ok_or_else(|| TypeHandlerError::FieldNotBound {
                    field: field.to_string(),
                })?
        };

        let handlers = self.handlers.read().unwrap();
        let erased =
            handlers
                .get(&handler_name)
                .ok_or_else(|| TypeHandlerError::HandlerNotFound {
                    name: handler_name.clone(),
                })?;

        // 通过 erased_type_id 比较 TypeId，提前识别类型不匹配
        let actual_id = erased.erased_type_id();
        if actual_id != TypeId::of::<T>() {
            return Err(TypeHandlerError::TypeMismatch {
                expected: TypeId::of::<T>(),
                actual: actual_id,
                actual_type_name: erased.erased_type_name().to_string(),
            });
        }

        let handler = erased
            .as_any()
            .downcast_ref::<Box<dyn TypeHandler<T>>>()
            .expect("TypeId 已校验匹配，downcast 必然成功");
        Ok(handler.to_value(value))
    }

    /// 列出所有已注册的 handler 名称
    pub fn list_handlers(&self) -> Vec<String> {
        let mut names: Vec<String> = self.handlers.read().unwrap().keys().cloned().collect();
        names.sort();
        names
    }

    /// 列出所有已绑定的字段名
    pub fn list_bound_fields(&self) -> Vec<String> {
        let mut fields: Vec<String> = self
            .field_bindings
            .read()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        fields.sort();
        fields
    }

    /// 清空所有注册与绑定
    pub fn clear(&self) {
        self.handlers.write().unwrap().clear();
        self.field_bindings.write().unwrap().clear();
    }
}

impl Default for TypeHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 内置 TypeHandler 实现
// ============================================================================

// -------------------- DateTimeHandler --------------------

/// DateTime 类型处理器
///
/// Rust 端使用 `String` 表示 ISO8601 格式的时间字符串，
/// ORM 端使用 `Value::DateTime` 存储。
///
/// # 转换规则
///
/// - **to_value**：`String` → `Value::DateTime(s)`
/// - **from_value**：`Value::DateTime(s)` / `Value::String(s)` → `String`
pub struct DateTimeHandler;

impl TypeHandler<String> for DateTimeHandler {
    fn to_value(&self, value: &String) -> Value {
        Value::DateTime(value.clone())
    }

    fn from_value(&self, value: &Value) -> TypeHandlerResult<String> {
        match value {
            Value::DateTime(s) => Ok(s.clone()),
            Value::String(s) => Ok(s.clone()),
            Value::Null => Ok(String::new()),
            other => Err(TypeHandlerError::ConversionFailed {
                reason: format!("Expected DateTime/String, got {:?}", other),
            }),
        }
    }
}

// -------------------- UuidHandler --------------------

/// UUID 类型处理器
///
/// Rust 端使用 `String` 表示 UUID 字符串，
/// ORM 端使用 `Value::Uuid` 存储。
pub struct UuidHandler;

impl TypeHandler<String> for UuidHandler {
    fn to_value(&self, value: &String) -> Value {
        Value::Uuid(value.clone())
    }

    fn from_value(&self, value: &Value) -> TypeHandlerResult<String> {
        match value {
            Value::Uuid(s) => Ok(s.clone()),
            Value::String(s) => Ok(s.clone()),
            Value::Null => Ok(String::new()),
            other => Err(TypeHandlerError::ConversionFailed {
                reason: format!("Expected Uuid/String, got {:?}", other),
            }),
        }
    }
}

// -------------------- JsonHandler --------------------

/// JSON 类型处理器
///
/// Rust 端使用 `String` 表示 JSON 字符串，
/// ORM 端使用 `Value::Json` 存储。
pub struct JsonHandler;

impl TypeHandler<String> for JsonHandler {
    fn to_value(&self, value: &String) -> Value {
        Value::Json(value.clone())
    }

    fn from_value(&self, value: &Value) -> TypeHandlerResult<String> {
        match value {
            Value::Json(s) => Ok(s.clone()),
            Value::String(s) => Ok(s.clone()),
            Value::Null => Ok(String::from("null")),
            other => Err(TypeHandlerError::ConversionFailed {
                reason: format!("Expected Json/String, got {:?}", other),
            }),
        }
    }
}

// -------------------- DecimalHandler --------------------

/// 十进制数类型处理器（定点小数，以分为单位存储为 i64）
///
/// 适合货币场景：Rust 端用 i64 表示「以分为单位」的金额，
/// ORM 端用 `Value::I64` 存储。
///
/// # 转换规则
///
/// - **to_value**：`i64` → `Value::I64(v)`
/// - **from_value**：`Value::I64` / `Value::I32` / `Value::F64` → `i64`
pub struct DecimalHandler;

impl TypeHandler<i64> for DecimalHandler {
    fn to_value(&self, value: &i64) -> Value {
        Value::I64(*value)
    }

    fn from_value(&self, value: &Value) -> TypeHandlerResult<i64> {
        match value {
            Value::I64(v) => Ok(*v),
            Value::I32(v) => Ok(*v as i64),
            Value::I16(v) => Ok(*v as i64),
            Value::I8(v) => Ok(*v as i64),
            Value::U64(v) => Ok(*v as i64),
            Value::U32(v) => Ok(*v as i64),
            Value::U16(v) => Ok(*v as i64),
            Value::U8(v) => Ok(*v as i64),
            Value::F64(v) => Ok(*v as i64),
            Value::F32(v) => Ok(*v as i64),
            Value::String(s) => {
                // 优先按 i64 解析，若失败（如 "123.45"）则按 f64 解析后截断
                if let Ok(v) = s.parse::<i64>() {
                    return Ok(v);
                }
                s.parse::<f64>()
                    .map(|v| v as i64)
                    .map_err(|e| TypeHandlerError::ConversionFailed {
                        reason: format!("Failed to parse `{}` as number: {}", s, e),
                    })
            }
            Value::Null => Ok(0),
            other => Err(TypeHandlerError::ConversionFailed {
                reason: format!("Expected numeric, got {:?}", other),
            }),
        }
    }
}

// -------------------- BoolHandler --------------------

/// 布尔类型处理器
///
/// Rust 端使用 `bool`，ORM 端用 `Value::Bool` 或 `Value::I64(0/1)` 存储。
pub struct BoolHandler;

impl TypeHandler<bool> for BoolHandler {
    fn to_value(&self, value: &bool) -> Value {
        Value::Bool(*value)
    }

    fn from_value(&self, value: &Value) -> TypeHandlerResult<bool> {
        match value {
            Value::Bool(b) => Ok(*b),
            Value::I64(v) => Ok(*v != 0),
            Value::I32(v) => Ok(*v != 0),
            Value::U64(v) => Ok(*v != 0),
            Value::U32(v) => Ok(*v != 0),
            Value::String(s) => match s.to_lowercase().as_str() {
                "true" | "1" | "yes" | "y" | "on" | "t" => Ok(true),
                "false" | "0" | "no" | "n" | "off" | "f" | "" => Ok(false),
                _ => Err(TypeHandlerError::ConversionFailed {
                    reason: format!("Cannot parse `{}` as bool", s),
                }),
            },
            Value::Null => Ok(false),
            other => Err(TypeHandlerError::ConversionFailed {
                reason: format!("Expected bool/int/string, got {:?}", other),
            }),
        }
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ===== TypeHandlerRegistry 基础测试 =====

    #[test]
    fn test_new_registry_is_empty() {
        let registry = TypeHandlerRegistry::new();
        assert!(registry.list_handlers().is_empty());
        assert!(registry.list_bound_fields().is_empty());
        assert!(!registry.has_handler("datetime"));
        assert!(!registry.is_bound("created_at"));
    }

    #[test]
    fn test_register_and_check_handler() {
        let registry = TypeHandlerRegistry::new();
        registry.register("datetime", Box::new(DateTimeHandler));
        assert!(registry.has_handler("datetime"));
        assert_eq!(registry.list_handlers(), vec!["datetime".to_string()]);
    }

    #[test]
    fn test_bind_and_check_field() {
        let registry = TypeHandlerRegistry::new();
        registry.register("datetime", Box::new(DateTimeHandler));
        registry.bind("created_at", "datetime");
        assert!(registry.is_bound("created_at"));
        assert_eq!(
            registry.handler_name_of("created_at"),
            Some("datetime".to_string())
        );
        assert_eq!(registry.list_bound_fields(), vec!["created_at".to_string()]);
    }

    #[test]
    fn test_unbind_field() {
        let registry = TypeHandlerRegistry::new();
        registry.register("datetime", Box::new(DateTimeHandler));
        registry.bind("created_at", "datetime");
        registry.unbind("created_at");
        assert!(!registry.is_bound("created_at"));
    }

    #[test]
    fn test_unregister_handler_clears_bindings() {
        let registry = TypeHandlerRegistry::new();
        registry.register("datetime", Box::new(DateTimeHandler));
        registry.bind("created_at", "datetime");
        registry.bind("updated_at", "datetime");

        registry.unregister("datetime");
        assert!(!registry.has_handler("datetime"));
        assert!(!registry.is_bound("created_at"));
        assert!(!registry.is_bound("updated_at"));
    }

    #[test]
    fn test_clear_registry() {
        let registry = TypeHandlerRegistry::new();
        registry.register("datetime", Box::new(DateTimeHandler));
        registry.bind("created_at", "datetime");
        registry.clear();
        assert!(registry.list_handlers().is_empty());
        assert!(registry.list_bound_fields().is_empty());
    }

    // ===== DateTimeHandler 测试 =====

    #[test]
    fn test_datetime_handler_to_value() {
        let registry = TypeHandlerRegistry::new();
        registry.register("datetime", Box::new(DateTimeHandler));
        registry.bind("created_at", "datetime");

        let value = registry
            .to_value("created_at", &String::from("2026-07-19T10:00:00Z"))
            .unwrap();
        assert!(matches!(value, Value::DateTime(_)));
        if let Value::DateTime(s) = value {
            assert_eq!(s, "2026-07-19T10:00:00Z");
        }
    }

    #[test]
    fn test_datetime_handler_from_value_datetime() {
        let registry = TypeHandlerRegistry::new();
        registry.register("datetime", Box::new(DateTimeHandler));
        registry.bind("created_at", "datetime");

        let stored = Value::DateTime("2026-07-19T10:00:00Z".to_string());
        let parsed: String = registry.handle("created_at", &stored).unwrap();
        assert_eq!(parsed, "2026-07-19T10:00:00Z");
    }

    #[test]
    fn test_datetime_handler_from_value_string() {
        let registry = TypeHandlerRegistry::new();
        registry.register("datetime", Box::new(DateTimeHandler));
        registry.bind("created_at", "datetime");

        let stored = Value::String("2026-07-19".to_string());
        let parsed: String = registry.handle("created_at", &stored).unwrap();
        assert_eq!(parsed, "2026-07-19");
    }

    #[test]
    fn test_datetime_handler_from_value_null() {
        let registry = TypeHandlerRegistry::new();
        registry.register("datetime", Box::new(DateTimeHandler));
        registry.bind("created_at", "datetime");

        let parsed: String = registry.handle("created_at", &Value::Null).unwrap();
        assert_eq!(parsed, "");
    }

    #[test]
    fn test_datetime_handler_from_value_invalid_type() {
        let registry = TypeHandlerRegistry::new();
        registry.register("datetime", Box::new(DateTimeHandler));
        registry.bind("created_at", "datetime");

        let result: TypeHandlerResult<String> = registry.handle("created_at", &Value::I64(42));
        assert!(matches!(
            result,
            Err(TypeHandlerError::ConversionFailed { .. })
        ));
    }

    // ===== UuidHandler 测试 =====

    #[test]
    fn test_uuid_handler_to_value() {
        let registry = TypeHandlerRegistry::new();
        registry.register("uuid", Box::new(UuidHandler));
        registry.bind("id", "uuid");

        let uuid = String::from("550e8400-e29b-41d4-a716-446655440000");
        let value = registry.to_value("id", &uuid).unwrap();
        assert!(matches!(value, Value::Uuid(_)));
    }

    #[test]
    fn test_uuid_handler_from_value_uuid() {
        let registry = TypeHandlerRegistry::new();
        registry.register("uuid", Box::new(UuidHandler));
        registry.bind("id", "uuid");

        let stored = Value::Uuid("550e8400-e29b-41d4-a716-446655440000".to_string());
        let parsed: String = registry.handle("id", &stored).unwrap();
        assert_eq!(parsed, "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_uuid_handler_from_value_string() {
        let registry = TypeHandlerRegistry::new();
        registry.register("uuid", Box::new(UuidHandler));
        registry.bind("id", "uuid");

        let stored = Value::String("550e8400-e29b-41d4-a716-446655440000".to_string());
        let parsed: String = registry.handle("id", &stored).unwrap();
        assert_eq!(parsed, "550e8400-e29b-41d4-a716-446655440000");
    }

    // ===== JsonHandler 测试 =====

    #[test]
    fn test_json_handler_to_value() {
        let registry = TypeHandlerRegistry::new();
        registry.register("json", Box::new(JsonHandler));
        registry.bind("settings", "json");

        let json = String::from(r#"{"theme":"dark","lang":"zh"}"#);
        let value = registry.to_value("settings", &json).unwrap();
        assert!(matches!(value, Value::Json(_)));
    }

    #[test]
    fn test_json_handler_from_value_json() {
        let registry = TypeHandlerRegistry::new();
        registry.register("json", Box::new(JsonHandler));
        registry.bind("settings", "json");

        let stored = Value::Json(r#"{"theme":"dark"}"#.to_string());
        let parsed: String = registry.handle("settings", &stored).unwrap();
        assert_eq!(parsed, r#"{"theme":"dark"}"#);
    }

    #[test]
    fn test_json_handler_from_value_null() {
        let registry = TypeHandlerRegistry::new();
        registry.register("json", Box::new(JsonHandler));
        registry.bind("settings", "json");

        let parsed: String = registry.handle("settings", &Value::Null).unwrap();
        assert_eq!(parsed, "null");
    }

    // ===== DecimalHandler 测试 =====

    #[test]
    fn test_decimal_handler_to_value() {
        let registry = TypeHandlerRegistry::new();
        registry.register("decimal", Box::new(DecimalHandler));
        registry.bind("price", "decimal");

        let value = registry.to_value("price", &12345i64).unwrap();
        assert_eq!(value, Value::I64(12345));
    }

    #[test]
    fn test_decimal_handler_from_value_i64() {
        let registry = TypeHandlerRegistry::new();
        registry.register("decimal", Box::new(DecimalHandler));
        registry.bind("price", "decimal");

        let stored = Value::I64(12345);
        let parsed: i64 = registry.handle("price", &stored).unwrap();
        assert_eq!(parsed, 12345);
    }

    #[test]
    fn test_decimal_handler_from_value_i32() {
        let registry = TypeHandlerRegistry::new();
        registry.register("decimal", Box::new(DecimalHandler));
        registry.bind("price", "decimal");

        let stored = Value::I32(99);
        let parsed: i64 = registry.handle("price", &stored).unwrap();
        assert_eq!(parsed, 99);
    }

    #[test]
    fn test_decimal_handler_from_value_string() {
        let registry = TypeHandlerRegistry::new();
        registry.register("decimal", Box::new(DecimalHandler));
        registry.bind("price", "decimal");

        let stored = Value::String("123.45".to_string());
        let parsed: i64 = registry.handle("price", &stored).unwrap();
        assert_eq!(parsed, 123);
    }

    #[test]
    fn test_decimal_handler_from_value_null() {
        let registry = TypeHandlerRegistry::new();
        registry.register("decimal", Box::new(DecimalHandler));
        registry.bind("price", "decimal");

        let parsed: i64 = registry.handle("price", &Value::Null).unwrap();
        assert_eq!(parsed, 0);
    }

    // ===== BoolHandler 测试 =====

    #[test]
    fn test_bool_handler_to_value() {
        let registry = TypeHandlerRegistry::new();
        registry.register("bool", Box::new(BoolHandler));
        registry.bind("active", "bool");

        let value = registry.to_value("active", &true).unwrap();
        assert_eq!(value, Value::Bool(true));
    }

    #[test]
    fn test_bool_handler_from_value_bool() {
        let registry = TypeHandlerRegistry::new();
        registry.register("bool", Box::new(BoolHandler));
        registry.bind("active", "bool");

        let parsed: bool = registry.handle("active", &Value::Bool(true)).unwrap();
        assert!(parsed);

        let parsed: bool = registry.handle("active", &Value::Bool(false)).unwrap();
        assert!(!parsed);
    }

    #[test]
    fn test_bool_handler_from_value_i64() {
        let registry = TypeHandlerRegistry::new();
        registry.register("bool", Box::new(BoolHandler));
        registry.bind("active", "bool");

        let parsed: bool = registry.handle("active", &Value::I64(1)).unwrap();
        assert!(parsed);

        let parsed: bool = registry.handle("active", &Value::I64(0)).unwrap();
        assert!(!parsed);
    }

    #[test]
    fn test_bool_handler_from_value_string_variants() {
        let registry = TypeHandlerRegistry::new();
        registry.register("bool", Box::new(BoolHandler));
        registry.bind("active", "bool");

        let truthy = ["true", "1", "yes", "y", "on", "t", "TRUE", "Yes"];
        for s in truthy {
            let parsed: bool = registry
                .handle("active", &Value::String(s.to_string()))
                .unwrap();
            assert!(parsed, "expected `{}` to be true", s);
        }

        let falsy = ["false", "0", "no", "n", "off", "f", "FALSE", "No"];
        for s in falsy {
            let parsed: bool = registry
                .handle("active", &Value::String(s.to_string()))
                .unwrap();
            assert!(!parsed, "expected `{}` to be false", s);
        }
    }

    #[test]
    fn test_bool_handler_from_value_invalid_string() {
        let registry = TypeHandlerRegistry::new();
        registry.register("bool", Box::new(BoolHandler));
        registry.bind("active", "bool");

        let result: TypeHandlerResult<bool> =
            registry.handle("active", &Value::String("maybe".to_string()));
        assert!(matches!(
            result,
            Err(TypeHandlerError::ConversionFailed { .. })
        ));
    }

    // ===== 错误场景测试 =====

    #[test]
    fn test_handle_field_not_bound() {
        let registry = TypeHandlerRegistry::new();
        let result: TypeHandlerResult<String> = registry.handle("foo", &Value::Null);
        assert!(matches!(
            result,
            Err(TypeHandlerError::FieldNotBound { .. })
        ));
    }

    #[test]
    fn test_to_value_field_not_bound() {
        let registry = TypeHandlerRegistry::new();
        let result: TypeHandlerResult<Value> = registry.to_value("foo", &String::from("x"));
        assert!(matches!(
            result,
            Err(TypeHandlerError::FieldNotBound { .. })
        ));
    }

    #[test]
    fn test_handle_handler_not_found_after_unbind() {
        let registry = TypeHandlerRegistry::new();
        registry.register("datetime", Box::new(DateTimeHandler));
        registry.bind("created_at", "datetime");
        registry.unregister("datetime");

        // unregister 会同时清除字段绑定，所以这里实际上是 FieldNotBound
        let result: TypeHandlerResult<String> = registry.handle("created_at", &Value::Null);
        assert!(matches!(
            result,
            Err(TypeHandlerError::FieldNotBound { .. })
        ));
    }

    #[test]
    fn test_type_mismatch_when_handler_type_differs() {
        let registry = TypeHandlerRegistry::new();
        // 注册的是 String 类型处理器
        registry.register("datetime", Box::new(DateTimeHandler));
        registry.bind("created_at", "datetime");

        // 调用方尝试用 i64 接收 — 应返回 TypeMismatch
        let result: TypeHandlerResult<i64> = registry.handle("created_at", &Value::Null);
        assert!(matches!(result, Err(TypeHandlerError::TypeMismatch { .. })));
    }

    #[test]
    fn test_type_mismatch_contains_actual_type_info() {
        // C10 修复回归测试：TypeMismatch 错误应包含**实际的** TypeId 和类型名
        // 而非硬编码的 TypeId::of::<()>()
        let registry = TypeHandlerRegistry::new();
        registry.register("datetime", Box::new(DateTimeHandler));
        registry.bind("created_at", "datetime");

        let result: TypeHandlerResult<i64> = registry.handle("created_at", &Value::Null);
        match result {
            Err(TypeHandlerError::TypeMismatch {
                expected,
                actual,
                actual_type_name,
            }) => {
                // expected 应该是 i64 的 TypeId
                assert_eq!(expected, TypeId::of::<i64>());
                // actual 应该是 String 的 TypeId（DateTimeHandler 注册的是 String）
                assert_eq!(actual, TypeId::of::<String>());
                // actual **不应该**是 () 的 TypeId（这是修复前的 bug）
                assert_ne!(actual, TypeId::of::<()>());
                // actual_type_name 应该包含 "String"
                assert!(actual_type_name.contains("String"));
            }
            other => panic!("expected TypeMismatch, got {:?}", other),
        }
    }

    #[test]
    fn test_type_mismatch_to_value_contains_actual_type_info() {
        // to_value 也应该返回包含实际类型信息的 TypeMismatch
        let registry = TypeHandlerRegistry::new();
        registry.register("decimal", Box::new(DecimalHandler));
        registry.bind("price", "decimal");

        // 调用方尝试用 String 写入，但 handler 注册的是 i64
        let result: TypeHandlerResult<Value> = registry.to_value("price", &String::from("99"));
        match result {
            Err(TypeHandlerError::TypeMismatch {
                expected,
                actual,
                actual_type_name,
            }) => {
                assert_eq!(expected, TypeId::of::<String>());
                assert_eq!(actual, TypeId::of::<i64>());
                assert_ne!(actual, TypeId::of::<()>());
                assert!(actual_type_name.contains("i64"));
            }
            other => panic!("expected TypeMismatch, got {:?}", other),
        }
    }

    // ===== 自定义 TypeHandler 测试 =====

    /// 自定义 Money 类型（用于验证 SPI 可扩展性）
    struct Money(i64);

    struct MoneyHandler;

    impl TypeHandler<Money> for MoneyHandler {
        fn to_value(&self, value: &Money) -> Value {
            Value::I64(value.0)
        }

        fn from_value(&self, value: &Value) -> TypeHandlerResult<Money> {
            match value {
                Value::I64(v) => Ok(Money(*v)),
                Value::String(s) => {
                    s.parse()
                        .map(Money)
                        .map_err(|e| TypeHandlerError::ConversionFailed {
                            reason: format!("Failed to parse `{}` as i64: {}", s, e),
                        })
                }
                Value::Null => Ok(Money(0)),
                other => Err(TypeHandlerError::ConversionFailed {
                    reason: format!("Expected I64, got {:?}", other),
                }),
            }
        }
    }

    #[test]
    fn test_custom_type_handler_to_value() {
        let registry = TypeHandlerRegistry::new();
        registry.register("money", Box::new(MoneyHandler));
        registry.bind("price", "money");

        let value = registry.to_value("price", &Money(12345)).unwrap();
        assert_eq!(value, Value::I64(12345));
    }

    #[test]
    fn test_custom_type_handler_from_value() {
        let registry = TypeHandlerRegistry::new();
        registry.register("money", Box::new(MoneyHandler));
        registry.bind("price", "money");

        let parsed: Money = registry.handle("price", &Value::I64(99)).unwrap();
        assert_eq!(parsed.0, 99);
    }

    #[test]
    fn test_custom_type_handler_from_value_string() {
        let registry = TypeHandlerRegistry::new();
        registry.register("money", Box::new(MoneyHandler));
        registry.bind("price", "money");

        let parsed: Money = registry
            .handle("price", &Value::String("8888".to_string()))
            .unwrap();
        assert_eq!(parsed.0, 8888);
    }

    // ===== 集成场景测试 =====

    #[test]
    fn test_multiple_handlers_coexist() {
        let registry = TypeHandlerRegistry::new();
        registry.register("datetime", Box::new(DateTimeHandler));
        registry.register("uuid", Box::new(UuidHandler));
        registry.register("json", Box::new(JsonHandler));
        registry.register("decimal", Box::new(DecimalHandler));
        registry.register("bool", Box::new(BoolHandler));

        registry.bind("created_at", "datetime");
        registry.bind("id", "uuid");
        registry.bind("settings", "json");
        registry.bind("price", "decimal");
        registry.bind("active", "bool");

        let handlers = registry.list_handlers();
        assert_eq!(handlers.len(), 5);
        assert!(handlers.contains(&"datetime".to_string()));
        assert!(handlers.contains(&"uuid".to_string()));
        assert!(handlers.contains(&"json".to_string()));
        assert!(handlers.contains(&"decimal".to_string()));
        assert!(handlers.contains(&"bool".to_string()));

        let fields = registry.list_bound_fields();
        assert_eq!(fields.len(), 5);
    }

    #[test]
    fn test_rebind_field_to_different_handler() {
        let registry = TypeHandlerRegistry::new();
        registry.register("datetime", Box::new(DateTimeHandler));
        registry.register("json", Box::new(JsonHandler));

        registry.bind("ts", "datetime");
        assert_eq!(registry.handler_name_of("ts"), Some("datetime".to_string()));

        // 重新绑定到 json
        registry.bind("ts", "json");
        assert_eq!(registry.handler_name_of("ts"), Some("json".to_string()));
    }

    #[test]
    fn test_multiple_fields_same_handler() {
        let registry = TypeHandlerRegistry::new();
        registry.register("datetime", Box::new(DateTimeHandler));
        registry.bind("created_at", "datetime");
        registry.bind("updated_at", "datetime");
        registry.bind("deleted_at", "datetime");

        // 同一 handler 可被多个字段复用
        let c: String = registry
            .handle("created_at", &Value::DateTime("2026-01-01".to_string()))
            .unwrap();
        let u: String = registry
            .handle("updated_at", &Value::DateTime("2026-02-01".to_string()))
            .unwrap();
        assert_eq!(c, "2026-01-01");
        assert_eq!(u, "2026-02-01");
    }

    #[test]
    fn test_workflow_read_write_user() {
        // 模拟一个完整的用户表读写流程
        let registry = TypeHandlerRegistry::new();
        registry.register("datetime", Box::new(DateTimeHandler));
        registry.register("uuid", Box::new(UuidHandler));
        registry.register("bool", Box::new(BoolHandler));
        registry.bind("id", "uuid");
        registry.bind("created_at", "datetime");
        registry.bind("active", "bool");

        // 写入：Rust -> Value
        let id_value = registry.to_value("id", &String::from("user-123")).unwrap();
        let ts_value = registry
            .to_value("created_at", &String::from("2026-07-19T10:00:00Z"))
            .unwrap();
        let active_value = registry.to_value("active", &true).unwrap();

        assert!(matches!(id_value, Value::Uuid(_)));
        assert!(matches!(ts_value, Value::DateTime(_)));
        assert!(matches!(active_value, Value::Bool(true)));

        // 读取：Value -> Rust
        let id: String = registry.handle("id", &id_value).unwrap();
        let ts: String = registry.handle("created_at", &ts_value).unwrap();
        let active: bool = registry.handle("active", &active_value).unwrap();

        assert_eq!(id, "user-123");
        assert_eq!(ts, "2026-07-19T10:00:00Z");
        assert!(active);
    }

    #[test]
    fn test_error_display_messages() {
        let err = TypeHandlerError::HandlerNotFound {
            name: "foo".to_string(),
        };
        assert!(err.to_string().contains("foo"));

        let err = TypeHandlerError::FieldNotBound {
            field: "bar".to_string(),
        };
        assert!(err.to_string().contains("bar"));

        let err = TypeHandlerError::ConversionFailed {
            reason: "test reason".to_string(),
        };
        assert!(err.to_string().contains("test reason"));
    }
}
