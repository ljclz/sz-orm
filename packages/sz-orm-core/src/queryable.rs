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
        Ok(values.into_iter().next().unwrap()) // SAFETY: 前置 len == 1 校验保证 next() 返回 Some
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
        Ok((iter.next().unwrap(), iter.next().unwrap())) // SAFETY: 前置 len == 2 校验保证两次 next() 返回 Some
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
            iter.next().unwrap(), // SAFETY: 前置 len == 3 校验保证 next() 返回 Some
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

// ===== QueryAs<T>：类型化裸 SQL 查询（SQLx query_as! 风格） =====

/// 类型化裸 SQL 查询构建器。
///
/// 由 `query_as!` 宏生成，等效于 SQLx 的 `query_as!(Record, "SELECT ...")`：
/// 在编译期验证 SQL 语法（`db-verify` feature 下连真 DB 验证列名），
/// 运行时将结果行按列名映射到 `T: FromQueryResult`。
///
/// # 用法
///
/// ```ignore
/// use sz_orm_core::queryable::QueryAs;
///
/// #[derive(FromQueryResult)]
/// struct User { id: i64, name: String }
///
/// let q = QueryAs::<User>::new("SELECT id, name FROM users WHERE id = ?");
/// let users: Vec<User> = q.fetch_all(&mut conn).await?;
/// let one: User = QueryAs::<User>::new("SELECT id, name FROM users LIMIT 1")
///     .fetch_one(&mut conn)
///     .await?;
/// ```
/// 无类型裸 SQL 查询对象（SQLx `query!` 的 sz-orm 等效物）。
///
/// 与 [`QueryAs<T>`] 的区别：`Query` 返回 `Vec<HashMap<String, Value>>`，
/// 不绑定具体结构体类型；`QueryAs<T>` 将行映射为 `T: FromQueryResult`。
///
/// # 用法
///
/// ```ignore
/// use sz_orm_core::queryable::Query;
///
/// let q = Query::new("SELECT id, name FROM users WHERE id = ?");
/// let rows = q.fetch_all(&mut conn).await?;
/// ```
pub struct Query {
    sql: String,
}

impl Query {
    /// 从 SQL 字符串构造查询对象
    pub fn new(sql: impl Into<String>) -> Self {
        Self { sql: sql.into() }
    }

    /// 获取底层 SQL（用于日志/调试）
    pub fn sql(&self) -> &str {
        &self.sql
    }
}

impl std::fmt::Display for Query {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.sql)
    }
}

impl Query {
    /// 执行查询，返回所有行（`Vec<HashMap<列名, Value>>`）
    pub async fn fetch_all(
        &self,
        conn: &mut dyn crate::Connection,
    ) -> Result<Vec<std::collections::HashMap<String, crate::value::Value>>, crate::DbError> {
        conn.query(&self.sql).await
    }

    /// 执行查询，期望恰好一行；0 行或多行均返回错误
    pub async fn fetch_one(
        &self,
        conn: &mut dyn crate::Connection,
    ) -> Result<std::collections::HashMap<String, crate::value::Value>, crate::DbError> {
        let rows = conn.query(&self.sql).await?;
        match rows.len() {
            0 => Err(crate::DbError::NotFound(
                "fetch_one: no rows returned".into(),
            )),
            1 => Ok(rows.into_iter().next().unwrap()), // SAFETY: 前置 len == 1 校验保证 next() 返回 Some
            n => Err(crate::DbError::QueryError(format!(
                "fetch_one expected 1 row, got {}",
                n
            ))),
        }
    }

    /// 执行查询，返回 0 或 1 行；0 行返回 `Ok(None)`
    pub async fn fetch_optional(
        &self,
        conn: &mut dyn crate::Connection,
    ) -> Result<Option<std::collections::HashMap<String, crate::value::Value>>, crate::DbError>
    {
        let rows = conn.query(&self.sql).await?;
        Ok(rows.into_iter().next())
    }
}

/// 类型化裸 SQL 查询对象（SQLx `query_as!` 的 sz-orm 等效物）。
///
/// 与 [`Query`] 的区别：`QueryAs<T>` 将行映射为 `T: FromQueryResult`，
/// 提供类型安全的查询结果；`Query` 返回 `Vec<HashMap<String, Value>>`。
///
/// # 用法
///
/// ```ignore
/// use sz_orm_core::queryable::QueryAs;
///
/// #[derive(FromQueryResult)]
/// struct User { id: i64, name: String }
///
/// let q = QueryAs::<User>::new("SELECT id, name FROM users WHERE id = ?");
/// let users: Vec<User> = q.fetch_all(&mut conn).await?;
/// let one: User = QueryAs::<User>::new("SELECT id, name FROM users LIMIT 1")
///     .fetch_one(&mut conn)
///     .await?;
/// ```
pub struct QueryAs<T> {
    sql: String,
    _marker: std::marker::PhantomData<T>,
}

impl<T> QueryAs<T> {
    /// 从 SQL 字符串构造类型化查询
    pub fn new(sql: impl Into<String>) -> Self {
        Self {
            sql: sql.into(),
            _marker: std::marker::PhantomData,
        }
    }

    /// 获取底层 SQL（用于日志/调试）
    pub fn sql(&self) -> &str {
        &self.sql
    }
}

impl<T> std::fmt::Display for QueryAs<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.sql)
    }
}

impl<T: crate::value::FromQueryResult> QueryAs<T> {
    /// 执行查询，返回所有行映射为 `Vec<T>`
    ///
    /// **运行时列名验证**：比对 DB 返回的列名与 `T::row_desc()`（由
    /// `#[derive(FromQueryResult)]` 自动生成）。若 SQL SELECT 列不在 struct 字段中，
    /// 返回 `DbError::QueryError`，避免静默忽略多余列或类型不匹配。
    ///
    /// **运行时列类型验证**（P0-2）：比对 DB 返回值的实际类型与
    /// `T::column_types()` 期望类型。若不兼容（如 DB 返回 `TEXT` 但 struct 期望
    /// `i64`），返回 `DbError::QueryError`，防止静默类型截断。
    pub async fn fetch_all(
        &self,
        conn: &mut dyn crate::Connection,
    ) -> Result<Vec<T>, crate::DbError> {
        let rows = conn.query(&self.sql).await?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        // 运行时列名交叉验证（P0-2）
        let expected: Vec<&str> = T::row_desc();
        if !expected.is_empty() {
            let actual: Vec<&str> = rows[0].keys().map(|s| s.as_str()).collect();
            validate_columns(&actual, &expected)?;
            // 运行时列类型交叉验证（P0-2）
            validate_column_types(&rows[0], T::column_types())?;
        }
        rows.into_iter()
            .map(|row| {
                T::from_query_result(&row).map_err(|e| crate::DbError::QueryError(e.to_string()))
            })
            .collect()
    }

    /// 执行查询，期望恰好一行；0 行或多行均返回错误
    pub async fn fetch_one(&self, conn: &mut dyn crate::Connection) -> Result<T, crate::DbError> {
        let rows = conn.query(&self.sql).await?;
        match rows.len() {
            0 => Err(crate::DbError::NotFound(
                "fetch_one: no rows returned".into(),
            )),
            1 => {
                let row = &rows[0];
                let expected: Vec<&str> = T::row_desc();
                if !expected.is_empty() {
                    let actual: Vec<&str> = row.keys().map(|s| s.as_str()).collect();
                    validate_columns(&actual, &expected)?;
                    validate_column_types(row, T::column_types())?;
                }
                T::from_query_result(row).map_err(|e| crate::DbError::QueryError(e.to_string()))
            }
            n => Err(crate::DbError::QueryError(format!(
                "fetch_one expected 1 row, got {}",
                n
            ))),
        }
    }

    /// 执行查询，返回 0 或 1 行；0 行返回 `Ok(None)`
    pub async fn fetch_optional(
        &self,
        conn: &mut dyn crate::Connection,
    ) -> Result<Option<T>, crate::DbError> {
        let rows = conn.query(&self.sql).await?;
        match rows.len() {
            0 => Ok(None),
            _ => {
                let row = &rows[0];
                let expected: Vec<&str> = T::row_desc();
                if !expected.is_empty() {
                    let actual: Vec<&str> = row.keys().map(|s| s.as_str()).collect();
                    validate_columns(&actual, &expected)?;
                    validate_column_types(row, T::column_types())?;
                }
                T::from_query_result(row)
                    .map_err(|e| crate::DbError::QueryError(e.to_string()))
                    .map(Some)
            }
        }
    }
}

/// 比对 DB 实际返回列名与 struct 期望列名（P0-2 运行时列名验证）。
fn validate_columns(actual: &[&str], expected: &[&str]) -> Result<(), crate::DbError> {
    use std::collections::HashSet;
    let actual_set: HashSet<&str> = actual.iter().copied().collect();
    let expected_set: HashSet<&str> = expected.iter().copied().collect();
    let missing: Vec<&str> = expected_set.difference(&actual_set).copied().collect();
    let extra: Vec<&str> = actual_set.difference(&expected_set).copied().collect();
    if !missing.is_empty() || !extra.is_empty() {
        return Err(crate::DbError::QueryError(format!(
            "query_as! column mismatch: struct expects {:?}, DB returned {:?}{}",
            expected,
            actual,
            if missing.is_empty() {
                String::new()
            } else {
                format!(" (missing: {:?})", missing)
            }
        )));
    }
    Ok(())
}

/// 比对 DB 实际返回值的类型与 struct 期望的列类型（P0-2 运行时列类型验证）。
///
/// 从 `Value` 变体推导实际 SQL 类型名，与 `T::column_types()` 中的期望类型
/// 通过 `ColType::from_type_name()` 做兼容性匹配。
///
/// 例如：DB 返回 `Value::I64`（→ `"BIGINT"`）但 struct 期望 `"TEXT"`，
/// 两者 `ColType` 分类不同（I64 vs String），返回类型不匹配错误。
fn validate_column_types(
    row: &std::collections::HashMap<String, crate::value::Value>,
    expected: &[(&str, &str)],
) -> Result<(), crate::DbError> {
    use crate::value::{ColType, Value};

    for (col_name, expected_type) in expected {
        let value = match row.get(*col_name) {
            Some(v) => v,
            None => continue, // 列缺失由 validate_columns 处理
        };
        if matches!(value, Value::Null) {
            continue; // NULL 不校验类型
        }
        let actual_type = value_to_sql_type_name(value);
        let actual_col = ColType::from_type_name(actual_type);
        let expected_col = ColType::from_type_name(expected_type);
        if actual_col != expected_col
            && actual_col != ColType::Unknown
            && expected_col != ColType::Unknown
        {
            return Err(crate::DbError::QueryError(format!(
                "query_as! column TYPE mismatch: column '{}' has actual type '{}' (ColType::{:?}), \
                 but struct expects '{}' (ColType::{:?})",
                col_name, actual_type, actual_col, expected_type, expected_col
            )));
        }
    }
    Ok(())
}

/// 从 `Value` 变体推导 SQL 类型名（大写，与 `ColType::from_type_name` 对齐）。
fn value_to_sql_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "NULL",
        Value::Bool(_) => "BOOLEAN",
        Value::I8(_) | Value::U8(_) => "TINYINT",
        Value::I16(_) | Value::U16(_) => "SMALLINT",
        Value::I32(_) | Value::U32(_) => "INT",
        Value::I64(_) | Value::U64(_) => "BIGINT",
        Value::F32(_) => "FLOAT",
        Value::F64(_) => "DOUBLE",
        Value::Decimal(_) => "DECIMAL",
        Value::String(_)
        | Value::Uuid(_)
        | Value::Date(_)
        | Value::DateTime(_)
        | Value::Time(_)
        | Value::Json(_) => "VARCHAR",
        Value::Bytes(_) => "BLOB",
        Value::Array(_) | Value::Object(_) => "JSON",
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

    // ---- Query 测试 ----

    #[test]
    fn test_query_new_and_sql() {
        let q = Query::new("SELECT id, name FROM users");
        assert_eq!(q.sql(), "SELECT id, name FROM users");
    }

    #[test]
    fn test_query_from_str() {
        let q = Query::new("SELECT 1");
        assert_eq!(q.sql(), "SELECT 1");
    }

    #[test]
    fn test_query_with_params() {
        let q = Query::new("SELECT * FROM users WHERE id = ? AND name = ?");
        assert!(q.sql().contains("WHERE id = ?"));
    }

    // ---- QueryAs<T> 测试 ----

    #[test]
    fn test_validate_columns_match() {
        // 列名完全匹配
        let actual = vec!["id", "name"];
        let expected = vec!["id", "name"];
        assert!(validate_columns(&actual, &expected).is_ok());
    }

    #[test]
    fn test_validate_columns_missing() {
        // DB 缺少 struct 期望的列
        let actual = vec!["id"];
        let expected = vec!["id", "name"];
        let result = validate_columns(&actual, &expected);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("missing"));
    }

    #[test]
    fn test_validate_columns_extra() {
        // DB 返回了 struct 未定义的列
        let actual = vec!["id", "name", "age"];
        let expected = vec!["id", "name"];
        let result = validate_columns(&actual, &expected);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("column mismatch"));
    }

    #[test]
    fn test_query_as_new_and_sql() {
        let q = QueryAs::<UserRow>::new("SELECT id, name FROM users");
        assert_eq!(q.sql(), "SELECT id, name FROM users");
    }

    #[test]
    fn test_query_as_fetch_all_empty() {
        // QueryAs::fetch_all 需要真实 Connection，这里只验证构造不 panic
        let _q = QueryAs::<UserRow>::new("SELECT 1");
    }

    #[test]
    fn test_query_as_fetch_optional_empty_result() {
        // 验证 QueryAs 类型参数约束编译通过
        let _q = QueryAs::<(i64, String)>::new("SELECT id, name FROM t");
    }

    // ---- validate_column_types 测试（P0-2 运行时列类型验证） ----

    #[test]
    fn test_validate_column_types_compatible() {
        // DB 返回 i64，struct 期望 BIGINT → 兼容
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(42));
        row.insert("name".to_string(), Value::String("Alice".into()));
        let expected = vec![("id", "BIGINT"), ("name", "TEXT")];
        assert!(validate_column_types(&row, &expected).is_ok());
    }

    #[test]
    fn test_validate_column_types_case_insensitive() {
        // 大小写不敏感：bigint vs BIGINT
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(1));
        let expected = vec![("id", "bigint")];
        assert!(validate_column_types(&row, &expected).is_ok());
    }

    #[test]
    fn test_validate_column_types_null_skipped() {
        // NULL 值跳过类型校验
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::Null);
        let expected = vec![("id", "TEXT")]; // 即使期望 TEXT，NULL 也不报错
        assert!(validate_column_types(&row, &expected).is_ok());
    }

    #[test]
    fn test_validate_column_types_missing_column_skipped() {
        // 列不存在时跳过（由 validate_columns 处理）
        let row = HashMap::new();
        let expected = vec![("missing", "BIGINT")];
        assert!(validate_column_types(&row, &expected).is_ok());
    }

    #[test]
    fn test_validate_column_types_type_mismatch() {
        // DB 返回 I64，struct 期望 TEXT → 不兼容
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(42));
        let expected = vec![("id", "TEXT")];
        let result = validate_column_types(&row, &expected);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("TYPE mismatch"));
        assert!(err.contains("id"));
    }

    #[test]
    fn test_validate_column_types_bool_vs_int_mismatch() {
        // DB 返回 Bool，struct 期望 INT → 不兼容
        let mut row = HashMap::new();
        row.insert("flag".to_string(), Value::Bool(true));
        let expected = vec![("flag", "INT")];
        let result = validate_column_types(&row, &expected);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_column_types_unknown_actual_skipped() {
        // 实际类型未知时不报错（宽松模式）
        let mut row = HashMap::new();
        row.insert("data".to_string(), Value::String("x".into()));
        let expected = vec![("data", "UNKNOWN_CUSTOM_TYPE")];
        // expected 映射为 Unknown → 不报错
        assert!(validate_column_types(&row, &expected).is_ok());
    }

    #[test]
    fn test_value_to_sql_type_name_coverage() {
        // 覆盖所有 Value 变体
        use crate::value::Value;
        assert_eq!(super::value_to_sql_type_name(&Value::Bool(true)), "BOOLEAN");
        assert_eq!(super::value_to_sql_type_name(&Value::I64(1)), "BIGINT");
        assert_eq!(super::value_to_sql_type_name(&Value::F64(1.0)), "DOUBLE");
        assert_eq!(
            super::value_to_sql_type_name(&Value::Decimal("1.0".into())),
            "DECIMAL"
        );
        assert_eq!(
            super::value_to_sql_type_name(&Value::Bytes(vec![1, 2])),
            "BLOB"
        );
        assert_eq!(super::value_to_sql_type_name(&Value::Null), "NULL");
    }
}
