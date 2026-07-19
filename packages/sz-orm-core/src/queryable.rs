//! derive(Queryable) — 从 SELECT 结果自动派生结构体（Diesel 风格）
//!
//! Diesel 通过 `#[derive(Queryable)]` 让结构体自动从 SQL 行反序列化。
//! SZ-ORM 在 [`crate::value::Value`] 之上提供类似的 trait + 派生辅助。
//!
//! 由于 proc-macro derive 需要在 `sz-orm-macros` 包中实现，
//! 此模块提供 trait 定义和运行时反序列化逻辑；
//! 派生宏 `#[derive(Queryable)]` 在 `sz-orm-macros` 中实现。
//!
//! # 设计
//!
//! - [`Queryable`] trait：从 `Vec<Value>` 按列顺序填充结构体字段
//! - [`FromRow`] trait：从 `HashMap<String, Value>` 按列名填充（更鲁棒）
//! - [`RowDesc`]：行描述（列名 + 列数），用于反序列化校验
//!
//! # 用法
//!
//! ```ignore
//! use sz_orm_core::value::Value;
//! use sz_orm_core::queryable::{Queryable, FromRow, RowDesc};
//!
//! #[derive(Debug, Default, PartialEq)]
//! struct UserRow {
//!     id: i64,
//!     name: String,
//! }
//!
//! impl Queryable for UserRow {
//!     fn from_values(values: Vec<Value>) -> Result<Self, QueryError> {
//!         if values.len() != 2 {
//!             return Err(QueryError::ColumnCountMismatch {
//!                 expected: 2,
//!                 actual: values.len(),
//!             });
//!         }
//!         let id = values[0].as_i64().ok_or(QueryError::TypeMismatch {
//!             column: 0,
//!             expected: "i64",
//!         })?;
//!         let name = values[1].as_str().ok_or(QueryError::TypeMismatch {
//!             column: 1,
//!             expected: "String",
//!         })?.to_string();
//!         Ok(UserRow { id, name })
//!     }
//! }
//!
//! let row = UserRow::from_values(vec![Value::I64(42), Value::String("Alice".into())]).unwrap();
//! assert_eq!(row.id, 42);
//! assert_eq!(row.name, "Alice");
//! ```

use crate::value::Value;
use std::collections::HashMap;

/// 反序列化错误
#[derive(Debug, Clone, PartialEq)]
pub enum QueryError {
    /// 列数不匹配
    ColumnCountMismatch {
        /// 期望的列数
        expected: usize,
        /// 实际的列数
        actual: usize,
    },
    /// 类型不匹配
    TypeMismatch {
        /// 列索引（按位置反序列化时）或列名（按名反序列化时）
        column: std::borrow::Cow<'static, str>,
        /// 期望的 Rust 类型名
        expected: &'static str,
    },
    /// 缺少列
    MissingColumn {
        /// 缺失的列名
        column: &'static str,
    },
    /// 自定义错误
    Custom(String),
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryError::ColumnCountMismatch { expected, actual } => {
                write!(f, "列数不匹配: 期望 {}, 实际 {}", expected, actual)
            }
            QueryError::TypeMismatch { column, expected } => {
                write!(f, "列 {:?} 类型不匹配, 期望 {}", column, expected)
            }
            QueryError::MissingColumn { column } => {
                write!(f, "缺少列: {}", column)
            }
            QueryError::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for QueryError {}

/// 行描述：列名 + 列数
///
/// 用于在按位置反序列化（[`Queryable`]）时提供列名信息，
/// 或在按名反序列化（[`FromRow`]）时校验列存在性。
#[derive(Debug, Clone)]
pub struct RowDesc {
    /// 列名列表（按 SELECT 顺序）
    pub columns: Vec<String>,
}

impl RowDesc {
    /// 创建行描述
    pub fn new(columns: Vec<String>) -> Self {
        Self { columns }
    }

    /// 列数
    pub fn len(&self) -> usize {
        self.columns.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    /// 查找列索引（按名）
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }
}

/// 从 `Vec<Value>` 按列顺序反序列化（Diesel 风格）
///
/// 适用于 `SELECT id, name FROM users` 这种列顺序已知的查询。
/// 列顺序由 SQL 决定，结构体字段顺序需与之对应。
pub trait Queryable: Sized {
    /// 从按 SELECT 顺序排列的值列表构造实例
    fn from_values(values: Vec<Value>) -> Result<Self, QueryError>;

    /// 从带行描述的值列表构造（默认实现忽略描述）
    fn from_values_with_desc(values: Vec<Value>, desc: &RowDesc) -> Result<Self, QueryError> {
        if values.len() != desc.len() {
            return Err(QueryError::ColumnCountMismatch {
                expected: desc.len(),
                actual: values.len(),
            });
        }
        Self::from_values(values)
    }
}

/// 从 `HashMap<String, Value>` 按列名反序列化（更鲁棒）
///
/// 适用于列顺序不固定或查询使用 `*` 的场景。
/// 按列名查找，不受 SQL 列顺序影响。
pub trait FromRow: Sized {
    /// 从列名到值的映射构造实例
    fn from_row(row: HashMap<String, Value>) -> Result<Self, QueryError>;
}

// ---- 基础类型的 Queryable 实现 ----

/// 单列查询结果（如 `SELECT COUNT(*)`）
impl Queryable for Value {
    fn from_values(values: Vec<Value>) -> Result<Self, QueryError> {
        if values.len() != 1 {
            return Err(QueryError::ColumnCountMismatch {
                expected: 1,
                actual: values.len(),
            });
        }
        Ok(values.into_iter().next().unwrap())
    }
}

/// 双列查询结果
impl Queryable for (Value, Value) {
    fn from_values(values: Vec<Value>) -> Result<Self, QueryError> {
        if values.len() != 2 {
            return Err(QueryError::ColumnCountMismatch {
                expected: 2,
                actual: values.len(),
            });
        }
        let mut iter = values.into_iter();
        Ok((iter.next().unwrap(), iter.next().unwrap()))
    }
}

/// 三列查询结果
impl Queryable for (Value, Value, Value) {
    fn from_values(values: Vec<Value>) -> Result<Self, QueryError> {
        if values.len() != 3 {
            return Err(QueryError::ColumnCountMismatch {
                expected: 3,
                actual: values.len(),
            });
        }
        let mut iter = values.into_iter();
        Ok((
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
        ))
    }
}

// ---- 辅助函数：从 Value 提取 Rust 类型 ----

/// 提取 i64（支持 I8/I16/I32/I64/U8/U16/U32/U64/F32/F64/Bool/String 转换）
pub fn value_as_i64(v: &Value) -> Option<i64> {
    v.as_i64()
}

/// 提取 f64
pub fn value_as_f64(v: &Value) -> Option<f64> {
    v.as_f64()
}

/// 提取 String
pub fn value_as_string(v: &Value) -> Option<String> {
    v.as_str().map(|s| s.to_string())
}

/// 提取 bool
pub fn value_as_bool(v: &Value) -> Option<bool> {
    v.as_bool()
}

/// 提取 `Option<i64>`（Null 返回 None）
pub fn value_as_nullable_i64(v: &Value) -> Option<i64> {
    if v.is_null() {
        None
    } else {
        v.as_i64()
    }
}

/// 提取 `Option<String>`（Null 返回 None）
pub fn value_as_nullable_string(v: &Value) -> Option<String> {
    if v.is_null() {
        None
    } else {
        v.as_str().map(|s| s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- 测试用结构体 ----

    #[derive(Debug, Default, PartialEq)]
    struct UserRow {
        id: i64,
        name: String,
    }

    impl Queryable for UserRow {
        fn from_values(values: Vec<Value>) -> Result<Self, QueryError> {
            if values.len() != 2 {
                return Err(QueryError::ColumnCountMismatch {
                    expected: 2,
                    actual: values.len(),
                });
            }
            let id = values[0].as_i64().ok_or(QueryError::TypeMismatch {
                column: "0".into(),
                expected: "i64",
            })?;
            let name = values[1]
                .as_str()
                .ok_or(QueryError::TypeMismatch {
                    column: "1".into(),
                    expected: "String",
                })?
                .to_string();
            Ok(UserRow { id, name })
        }
    }

    impl FromRow for UserRow {
        fn from_row(row: HashMap<String, Value>) -> Result<Self, QueryError> {
            let id = row
                .get("id")
                .ok_or(QueryError::MissingColumn { column: "id" })?
                .as_i64()
                .ok_or(QueryError::TypeMismatch {
                    column: "id".into(),
                    expected: "i64",
                })?;
            let name = row
                .get("name")
                .ok_or(QueryError::MissingColumn { column: "name" })?
                .as_str()
                .ok_or(QueryError::TypeMismatch {
                    column: "name".into(),
                    expected: "String",
                })?
                .to_string();
            Ok(UserRow { id, name })
        }
    }

    // ---- QueryError 测试 ----

    #[test]
    fn test_query_error_display() {
        let e = QueryError::ColumnCountMismatch {
            expected: 3,
            actual: 2,
        };
        assert!(format!("{}", e).contains("3"));
        assert!(format!("{}", e).contains("2"));

        let e = QueryError::TypeMismatch {
            column: "age".into(),
            expected: "i64",
        };
        assert!(format!("{}", e).contains("age"));

        let e = QueryError::MissingColumn { column: "id" };
        assert!(format!("{}", e).contains("id"));

        let e = QueryError::Custom("custom".into());
        assert_eq!(format!("{}", e), "custom");
    }

    // ---- RowDesc 测试 ----

    #[test]
    fn test_row_desc_basic() {
        let desc = RowDesc::new(vec!["id".into(), "name".into(), "age".into()]);
        assert_eq!(desc.len(), 3);
        assert!(!desc.is_empty());
        assert_eq!(desc.index_of("name"), Some(1));
        assert_eq!(desc.index_of("missing"), None);
    }

    #[test]
    fn test_row_desc_empty() {
        let desc = RowDesc::new(vec![]);
        assert!(desc.is_empty());
        assert_eq!(desc.len(), 0);
    }

    // ---- Queryable for UserRow 测试 ----

    #[test]
    fn test_user_row_from_values_success() {
        let row =
            UserRow::from_values(vec![Value::I64(42), Value::String("Alice".into())]).unwrap();
        assert_eq!(row.id, 42);
        assert_eq!(row.name, "Alice");
    }

    #[test]
    fn test_user_row_from_values_count_mismatch() {
        let result = UserRow::from_values(vec![Value::I64(42)]);
        assert!(matches!(
            result,
            Err(QueryError::ColumnCountMismatch {
                expected: 2,
                actual: 1
            })
        ));
    }

    #[test]
    fn test_user_row_from_values_type_mismatch() {
        let result = UserRow::from_values(vec![
            Value::String("not_an_int".into()),
            Value::String("Alice".into()),
        ]);
        assert!(matches!(result, Err(QueryError::TypeMismatch { .. })));
    }

    #[test]
    fn test_user_row_from_values_with_desc() {
        let desc = RowDesc::new(vec!["id".into(), "name".into()]);
        let row =
            UserRow::from_values_with_desc(vec![Value::I64(1), Value::String("Bob".into())], &desc)
                .unwrap();
        assert_eq!(row.id, 1);
        assert_eq!(row.name, "Bob");
    }

    #[test]
    fn test_user_row_from_values_with_desc_mismatch() {
        let desc = RowDesc::new(vec!["id".into(), "name".into(), "age".into()]);
        let result =
            UserRow::from_values_with_desc(vec![Value::I64(1), Value::String("Bob".into())], &desc);
        assert!(matches!(
            result,
            Err(QueryError::ColumnCountMismatch { .. })
        ));
    }

    // ---- FromRow for UserRow 测试 ----

    #[test]
    fn test_user_row_from_row_success() {
        let mut map = HashMap::new();
        map.insert("id".into(), Value::I64(99));
        map.insert("name".into(), Value::String("Charlie".into()));
        let row = UserRow::from_row(map).unwrap();
        assert_eq!(row.id, 99);
        assert_eq!(row.name, "Charlie");
    }

    #[test]
    fn test_user_row_from_row_missing_column() {
        let mut map = HashMap::new();
        map.insert("id".into(), Value::I64(99));
        // 缺少 name
        let result = UserRow::from_row(map);
        assert!(matches!(
            result,
            Err(QueryError::MissingColumn { column: "name" })
        ));
    }

    #[test]
    fn test_user_row_from_row_extra_columns_ignored() {
        let mut map = HashMap::new();
        map.insert("id".into(), Value::I64(1));
        map.insert("name".into(), Value::String("X".into()));
        map.insert("extra".into(), Value::String("ignored".into()));
        let row = UserRow::from_row(map).unwrap();
        assert_eq!(row.id, 1);
    }

    // ---- 基础类型 Queryable 实现 ----

    #[test]
    fn test_value_queryable_single() {
        let v = Value::from_values(vec![Value::I64(42)]).unwrap();
        assert_eq!(v.as_i64(), Some(42));
    }

    #[test]
    fn test_value_queryable_count_mismatch() {
        let result = Value::from_values(vec![Value::I64(1), Value::I64(2)]);
        assert!(matches!(
            result,
            Err(QueryError::ColumnCountMismatch { .. })
        ));
    }

    #[test]
    fn test_tuple_2_queryable() {
        let (a, b) =
            <(Value, Value)>::from_values(vec![Value::I64(1), Value::String("hello".into())])
                .unwrap();
        assert_eq!(a.as_i64(), Some(1));
        assert_eq!(b.as_str(), Some("hello"));
    }

    #[test]
    fn test_tuple_3_queryable() {
        let (a, b, c) = <(Value, Value, Value)>::from_values(vec![
            Value::I64(1),
            Value::String("two".into()),
            Value::F64(3.5),
        ])
        .unwrap();
        assert_eq!(a.as_i64(), Some(1));
        assert_eq!(b.as_str(), Some("two"));
        assert_eq!(c.as_f64(), Some(3.5));
    }

    // ---- 辅助函数测试 ----

    #[test]
    fn test_value_helpers() {
        assert_eq!(value_as_i64(&Value::I64(42)), Some(42));
        assert_eq!(value_as_i64(&Value::String("42".into())), Some(42));
        assert_eq!(value_as_f64(&Value::F64(3.5)), Some(3.5));
        assert_eq!(
            value_as_string(&Value::String("hi".into())),
            Some("hi".into())
        );
        assert_eq!(value_as_bool(&Value::Bool(true)), Some(true));
    }

    #[test]
    fn test_nullable_helpers() {
        assert_eq!(value_as_nullable_i64(&Value::Null), None);
        assert_eq!(value_as_nullable_i64(&Value::I64(42)), Some(42));
        assert_eq!(value_as_nullable_string(&Value::Null), None);
        assert_eq!(
            value_as_nullable_string(&Value::String("hi".into())),
            Some("hi".into())
        );
    }

    // ---- 完整流程测试 ----

    #[test]
    fn test_full_flow_queryable() {
        // 模拟 SELECT id, name FROM users
        let values = vec![Value::I64(1), Value::String("Alice".into())];
        let row = UserRow::from_values(values).unwrap();
        assert_eq!(
            row,
            UserRow {
                id: 1,
                name: "Alice".into()
            }
        );
    }

    #[test]
    fn test_full_flow_from_row_with_extra_data() {
        // 模拟 SELECT * FROM users（带额外列）
        let mut map = HashMap::new();
        map.insert("id".into(), Value::I64(7));
        map.insert("name".into(), Value::String("Bob".into()));
        map.insert("email".into(), Value::String("bob@example.com".into()));
        map.insert("created_at".into(), Value::String("2026-01-01".into()));

        let row = UserRow::from_row(map).unwrap();
        assert_eq!(row.id, 7);
        assert_eq!(row.name, "Bob");
    }
}
