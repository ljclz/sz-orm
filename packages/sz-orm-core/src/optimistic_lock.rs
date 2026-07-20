//! 乐观锁（Optimistic Locking）
//!
//! 对应文档 6.8 节改进项 20（乐观锁支持）。
//!
//! # 核心概念
//!
//! - **OptimisticLock**：Model trait，声明版本字段名
//! - **build_update_with_lock**：生成 `UPDATE ... SET version = version + 1, ... WHERE pk = ? AND version = ?`
//! - **LockError**：版本冲突错误
//! - **retry_on_conflict**：冲突重试机制
//!
//! # 设计灵感
//!
//! - Hibernate `@Version`
//! - Doctrine `@Version` / `LockMode::OPTIMISTIC`
//! - Yii2 `OptimisticLockBehavior`
//! - MyBatis-Plus `@Version`
//!
//! # 使用示例
//!
//! ```no_run
//! use sz_orm_core::optimistic_lock::{OptimisticLock, build_update_with_lock};
//! use sz_orm_core::{DbType, get_dialect, Value};
//! use std::collections::HashMap;
//!
//! // 1. 定义带 version 字段的 Model
//! struct Product {
//!     id: i64,
//!     name: String,
//!     version: i64,
//! }
//!
//! impl OptimisticLock for Product {
//!     fn version_field() -> &'static str { "version" }
//! }
//!
//! // 2. 生成乐观锁 UPDATE SQL
//! let dialect = get_dialect(DbType::MySQL).unwrap();
//! let mut data = HashMap::new();
//! data.insert("name".to_string(), Value::String("new-name".to_string()));
//! let sql = build_update_with_lock(&*dialect, "products", "id", "version", &Value::I64(1), &Value::I64(5), &data);
//! // UPDATE `products` SET `name` = 'new-name', `version` = `version` + 1 WHERE `id` = 1 AND `version` = 5
//! ```

use crate::dialect::Dialect;
use crate::error::DbError;
use crate::Value;
use std::collections::HashMap;

// ============================================================================
// OptimisticLock trait — Model 端声明
// ============================================================================

/// 乐观锁 trait — Model 实现此 trait 以声明版本字段名
///
/// 对应 Hibernate `@Version` / Doctrine `@Version` / MyBatis-Plus `@Version`。
///
/// # 示例
///
/// ```
/// use sz_orm_core::optimistic_lock::OptimisticLock;
///
/// struct Product {
///     id: i64,
///     name: String,
///     version: i64,
/// }
///
/// impl OptimisticLock for Product {
///     fn version_field() -> &'static str { "version" }
/// }
/// ```
pub trait OptimisticLock {
    /// 版本字段名（如 "version"、"lock_version"、"rev"）
    fn version_field() -> &'static str;
}

// ============================================================================
// LockError — 乐观锁错误类型
// ============================================================================

/// 乐观锁错误类型
#[derive(Debug)]
pub enum LockError {
    /// 版本冲突（受影响行数为 0）
    ///
    /// 携带 (entity, expected_version) 信息以便上层重试。
    Conflict {
        /// 实体描述（表名 + 主键，便于日志）
        entity: String,
        /// 期望的版本号
        expected_version: i64,
    },
    /// 未指定版本号（必填）
    MissingVersion {
        /// 字段名
        field: &'static str,
    },
    /// 版本号非法（负数或溢出）
    InvalidVersion {
        /// 字段名
        field: &'static str,
        /// 实际值
        value: i64,
    },
    /// 重试次数耗尽
    RetriesExhausted {
        /// 已重试次数
        attempts: u32,
    },
    /// 其他数据库错误
    Other(DbError),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::Conflict {
                entity,
                expected_version,
            } => write!(
                f,
                "Optimistic lock conflict on {} (expected version {})",
                entity, expected_version
            ),
            LockError::MissingVersion { field } => {
                write!(f, "Missing version value for field `{}`", field)
            }
            LockError::InvalidVersion { field, value } => {
                write!(f, "Invalid version value for field `{}`: {}", field, value)
            }
            LockError::RetriesExhausted { attempts } => {
                write!(f, "Retries exhausted after {} attempts", attempts)
            }
            LockError::Other(e) => write!(f, "Optimistic lock error: {}", e),
        }
    }
}

impl std::error::Error for LockError {}

impl From<DbError> for LockError {
    fn from(e: DbError) -> Self {
        LockError::Other(e)
    }
}

/// 乐观锁结果
pub type LockResult<T> = Result<T, LockError>;

// ============================================================================
// build_update_with_lock — 生成乐观锁 UPDATE SQL
// ============================================================================

/// 生成乐观锁 UPDATE SQL
///
/// 生成的 SQL 形如：
/// ```sql
/// UPDATE `table` SET col1 = ?, col2 = ?, `version` = `version` + 1
/// WHERE `pk` = ? AND `version` = ?
/// ```
///
/// # 参数
/// - `dialect`：数据库方言
/// - `table`：表名
/// - `pk_column`：主键列名
/// - `version_column`：版本列名
/// - `pk_value`：主键值
/// - `current_version`：当前版本号（从数据库读取）
/// - `data`：要更新的字段（不含 version 字段，会自动追加）
///
/// # 示例
///
/// ```
/// use sz_orm_core::optimistic_lock::build_update_with_lock;
/// use sz_orm_core::{DbType, get_dialect, Value};
/// use std::collections::HashMap;
///
/// let dialect = get_dialect(DbType::MySQL).unwrap();
/// let mut data = HashMap::new();
/// data.insert("name".to_string(), Value::String("alice".to_string()));
/// let sql = build_update_with_lock(
///     &*dialect, "users", "id", "version",
///     &Value::I64(1), &Value::I64(5), &data,
/// );
/// assert!(sql.contains("UPDATE"));
/// assert!(sql.contains("`users`"));
/// assert!(sql.contains("`version` = `version` + 1"));
/// assert!(sql.contains("`id` = 1"));
/// assert!(sql.contains("`version` = 5"));
/// ```
pub fn build_update_with_lock(
    dialect: &dyn Dialect,
    table: &str,
    pk_column: &str,
    version_column: &str,
    pk_value: &Value,
    current_version: &Value,
    data: &HashMap<String, Value>,
) -> String {
    let quoted_table = dialect.quote(table);
    let quoted_pk = dialect.quote(pk_column);
    let quoted_version = dialect.quote(version_column);

    let mut sets: Vec<String> = data
        .iter()
        .map(|(k, v)| {
            format!(
                "{} = {}",
                dialect.quote(k),
                v.to_param_with_dialect(dialect)
            )
        })
        .collect();
    // version 字段自增（与数据中是否含 version 无关，强制使用 version = version + 1）
    sets.push(format!("{} = {} + 1", quoted_version, quoted_version));

    let sets_sql = sets.join(", ");

    format!(
        "UPDATE {} SET {} WHERE {} = {} AND {} = {}",
        quoted_table,
        sets_sql,
        quoted_pk,
        pk_value.to_param_with_dialect(dialect),
        quoted_version,
        current_version.to_param_with_dialect(dialect),
    )
}

// ============================================================================
// build_delete_with_lock — 生成乐观锁 DELETE SQL
// ============================================================================

/// 生成乐观锁 DELETE SQL（删除前校验版本号）
///
/// 生成的 SQL 形如：
/// ```sql
/// DELETE FROM `table` WHERE `pk` = ? AND `version` = ?
/// ```
pub fn build_delete_with_lock(
    dialect: &dyn Dialect,
    table: &str,
    pk_column: &str,
    version_column: &str,
    pk_value: &Value,
    current_version: &Value,
) -> String {
    let quoted_table = dialect.quote(table);
    let quoted_pk = dialect.quote(pk_column);
    let quoted_version = dialect.quote(version_column);

    format!(
        "DELETE FROM {} WHERE {} = {} AND {} = {}",
        quoted_table,
        quoted_pk,
        pk_value.to_param_with_dialect(dialect),
        quoted_version,
        current_version.to_param_with_dialect(dialect),
    )
}

// ============================================================================
// check_affected_rows — 检查受影响行数判断是否冲突
// ============================================================================

/// 检查 UPDATE/DELETE 受影响行数，0 表示版本冲突
///
/// # 参数
/// - `affected`：受影响行数
/// - `entity`：实体描述（如 "products#id=1"）
/// - `expected_version`：期望的版本号
pub fn check_affected_rows(
    affected: u64,
    entity: impl Into<String>,
    expected_version: i64,
) -> LockResult<()> {
    if affected == 0 {
        Err(LockError::Conflict {
            entity: entity.into(),
            expected_version,
        })
    } else {
        Ok(())
    }
}

// ============================================================================
// extract_version — 从数据行中提取版本号
// ============================================================================

/// 从数据行（HashMap）中提取版本号
///
/// 若字段不存在或类型不匹配，返回 `LockError::MissingVersion` / `InvalidVersion`。
pub fn extract_version(
    row: &HashMap<String, Value>,
    version_field: &'static str,
) -> LockResult<i64> {
    match row.get(version_field) {
        None => Err(LockError::MissingVersion {
            field: version_field,
        }),
        Some(Value::I64(v)) => {
            if *v < 0 {
                Err(LockError::InvalidVersion {
                    field: version_field,
                    value: *v,
                })
            } else {
                Ok(*v)
            }
        }
        Some(Value::I32(v)) => {
            if *v < 0 {
                Err(LockError::InvalidVersion {
                    field: version_field,
                    value: *v as i64,
                })
            } else {
                Ok(*v as i64)
            }
        }
        Some(Value::U32(v)) => Ok(*v as i64),
        Some(Value::U64(v)) => {
            if *v > i64::MAX as u64 {
                Err(LockError::InvalidVersion {
                    field: version_field,
                    value: *v as i64, // 截断
                })
            } else {
                Ok(*v as i64)
            }
        }
        Some(other) => Err(LockError::InvalidVersion {
            field: version_field,
            value: other.as_i64().unwrap_or(-1),
        }),
    }
}

// ============================================================================
// retry_on_conflict — 冲突重试机制
// ============================================================================

/// 在乐观锁冲突时自动重试
///
/// 重复调用 `op`，直到成功或重试次数耗尽。
/// 每次 `op` 返回 `Err(LockError::Conflict)` 时，调用 `reload` 重新加载最新版本号后重试。
///
/// # 参数
/// - `max_retries`：最大重试次数（不含首次调用）
/// - `op`：执行更新操作，返回 `Result<u64, LockError>`（u64 = 受影响行数）
///
/// # 示例
///
/// ```no_run
/// use sz_orm_core::optimistic_lock::{retry_on_conflict, LockResult, LockError};
///
/// let result: LockResult<()> = retry_on_conflict(3, || {
///     // 模拟：第一次冲突，第二次成功
///     static mut CALLS: u32 = 0;
///     unsafe { CALLS += 1; }
///     if unsafe { CALLS } == 1 {
///         Err(LockError::Conflict { entity: "x".to_string(), expected_version: 1 })
///     } else {
///         Ok(1u64) // 1 行受影响
///     }
/// });
/// ```
pub fn retry_on_conflict<F>(max_retries: u32, mut op: F) -> LockResult<()>
where
    F: FnMut() -> LockResult<u64>,
{
    let mut attempts = 0u32;
    loop {
        attempts += 1;
        match op() {
            Ok(affected) => {
                if affected == 0 {
                    if attempts > max_retries {
                        return Err(LockError::RetriesExhausted { attempts });
                    }
                    continue;
                }
                return Ok(());
            }
            Err(LockError::Conflict { .. }) => {
                if attempts > max_retries {
                    return Err(LockError::RetriesExhausted { attempts });
                }
                // 继续重试
            }
            Err(e) => return Err(e),
        }
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::get_dialect;
    use crate::DbType;

    // ===== build_update_with_lock 测试 =====

    #[test]
    fn test_build_update_with_lock_mysql() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut data = HashMap::new();
        data.insert("name".to_string(), Value::String("alice".to_string()));
        data.insert("age".to_string(), Value::I64(30));

        let sql = build_update_with_lock(
            &*dialect,
            "users",
            "id",
            "version",
            &Value::I64(1),
            &Value::I64(5),
            &data,
        );

        // 应包含 UPDATE ... SET ... WHERE id = 1 AND version = 5
        assert!(sql.starts_with("UPDATE `users` SET"));
        assert!(sql.contains("`name` = 'alice'"));
        assert!(sql.contains("`age` = 30"));
        assert!(sql.contains("`version` = `version` + 1"));
        assert!(sql.contains("WHERE `id` = 1 AND `version` = 5"));
    }

    #[test]
    fn test_build_update_with_lock_postgres() {
        let dialect = get_dialect(DbType::PostgreSQL).unwrap();
        let mut data = HashMap::new();
        data.insert("name".to_string(), Value::String("bob".to_string()));

        let sql = build_update_with_lock(
            &*dialect,
            "products",
            "id",
            "version",
            &Value::I64(42),
            &Value::I64(3),
            &data,
        );

        // PostgreSQL 使用双引号
        assert!(sql.contains("\"products\""));
        assert!(sql.contains("\"name\" = 'bob'"));
        assert!(sql.contains("\"version\" = \"version\" + 1"));
        assert!(sql.contains("\"id\" = 42"));
        assert!(sql.contains("\"version\" = 3"));
    }

    #[test]
    fn test_build_update_with_lock_empty_data() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let data = HashMap::new();

        let sql = build_update_with_lock(
            &*dialect,
            "users",
            "id",
            "version",
            &Value::I64(1),
            &Value::I64(0),
            &data,
        );

        // 即使没有其他字段，也应包含 version 自增
        assert!(sql.contains("SET `version` = `version` + 1"));
        assert!(sql.contains("`version` = 0"));
    }

    #[test]
    fn test_build_update_with_lock_custom_version_field() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut data = HashMap::new();
        data.insert("name".to_string(), Value::String("test".to_string()));

        let sql = build_update_with_lock(
            &*dialect,
            "orders",
            "order_id",
            "lock_version",
            &Value::I64(100),
            &Value::I64(2),
            &data,
        );

        assert!(sql.contains("`lock_version` = `lock_version` + 1"));
        assert!(sql.contains("`order_id` = 100"));
        assert!(sql.contains("`lock_version` = 2"));
    }

    // ===== build_delete_with_lock 测试 =====

    #[test]
    fn test_build_delete_with_lock_mysql() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let sql = build_delete_with_lock(
            &*dialect,
            "users",
            "id",
            "version",
            &Value::I64(1),
            &Value::I64(5),
        );

        assert_eq!(sql, "DELETE FROM `users` WHERE `id` = 1 AND `version` = 5");
    }

    #[test]
    fn test_build_delete_with_lock_postgres() {
        let dialect = get_dialect(DbType::PostgreSQL).unwrap();
        let sql = build_delete_with_lock(
            &*dialect,
            "products",
            "id",
            "version",
            &Value::I64(42),
            &Value::I64(3),
        );

        assert_eq!(
            sql,
            "DELETE FROM \"products\" WHERE \"id\" = 42 AND \"version\" = 3"
        );
    }

    // ===== check_affected_rows 测试 =====

    #[test]
    fn test_check_affected_rows_success() {
        let result = check_affected_rows(1, "users#id=1", 5);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_affected_rows_conflict() {
        let result = check_affected_rows(0, "users#id=1", 5);
        assert!(matches!(
            result,
            Err(LockError::Conflict {
                entity,
                expected_version
            }) if entity == "users#id=1" && expected_version == 5
        ));
    }

    #[test]
    fn test_check_affected_rows_multi_rows_success() {
        // 受影响多行也视为成功（虽然乐观锁通常一次只更新一行）
        let result = check_affected_rows(5, "users#id=1", 5);
        assert!(result.is_ok());
    }

    // ===== extract_version 测试 =====

    #[test]
    fn test_extract_version_i64() {
        let mut row = HashMap::new();
        row.insert("version".to_string(), Value::I64(42));
        let v = extract_version(&row, "version").unwrap();
        assert_eq!(v, 42);
    }

    #[test]
    fn test_extract_version_i32() {
        let mut row = HashMap::new();
        row.insert("version".to_string(), Value::I32(7));
        let v = extract_version(&row, "version").unwrap();
        assert_eq!(v, 7);
    }

    #[test]
    fn test_extract_version_u32() {
        let mut row = HashMap::new();
        row.insert("version".to_string(), Value::U32(99));
        let v = extract_version(&row, "version").unwrap();
        assert_eq!(v, 99);
    }

    #[test]
    fn test_extract_version_missing() {
        let row = HashMap::new();
        let result = extract_version(&row, "version");
        assert!(matches!(result, Err(LockError::MissingVersion { field }) if field == "version"));
    }

    #[test]
    fn test_extract_version_negative_invalid() {
        let mut row = HashMap::new();
        row.insert("version".to_string(), Value::I64(-1));
        let result = extract_version(&row, "version");
        assert!(matches!(
            result,
            Err(LockError::InvalidVersion { field, value }) if field == "version" && value == -1
        ));
    }

    #[test]
    fn test_extract_version_wrong_type() {
        let mut row = HashMap::new();
        row.insert("version".to_string(), Value::String("abc".to_string()));
        let result = extract_version(&row, "version");
        assert!(matches!(result, Err(LockError::InvalidVersion { .. })));
    }

    // ===== retry_on_conflict 测试 =====

    #[test]
    fn test_retry_on_conflict_immediate_success() {
        let calls = std::cell::Cell::new(0u32);
        let result: LockResult<()> = retry_on_conflict(3, || {
            calls.set(calls.get() + 1);
            Ok(1u64)
        });
        assert!(result.is_ok());
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn test_retry_on_conflict_after_one_failure() {
        let calls = std::cell::Cell::new(0u32);
        let result: LockResult<()> = retry_on_conflict(3, || {
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                Err(LockError::Conflict {
                    entity: "x".to_string(),
                    expected_version: 1,
                })
            } else {
                Ok(1u64)
            }
        });
        assert!(result.is_ok());
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn test_retry_on_conflict_exhausted() {
        let calls = std::cell::Cell::new(0u32);
        let result: LockResult<()> = retry_on_conflict(2, || {
            calls.set(calls.get() + 1);
            Err(LockError::Conflict {
                entity: "x".to_string(),
                expected_version: 1,
            })
        });
        assert!(matches!(result, Err(LockError::RetriesExhausted { .. })));
        // 1 initial + 2 retries = 3 calls
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn test_retry_on_conflict_zero_affected_treated_as_conflict() {
        let calls = std::cell::Cell::new(0u32);
        let result: LockResult<()> = retry_on_conflict(2, || {
            calls.set(calls.get() + 1);
            if calls.get() <= 1 {
                Ok(0u64) // 0 行受影响 = 冲突
            } else {
                Ok(1u64) // 成功
            }
        });
        assert!(result.is_ok());
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn test_retry_on_conflict_propagates_non_conflict_error() {
        let calls = std::cell::Cell::new(0u32);
        let result: LockResult<()> = retry_on_conflict(3, || {
            calls.set(calls.get() + 1);
            Err(LockError::MissingVersion { field: "version" })
        });
        assert!(matches!(result, Err(LockError::MissingVersion { .. })));
        assert_eq!(calls.get(), 1); // 非 Conflict 错误立即返回，不重试
    }

    // ===== LockError Display 测试 =====

    #[test]
    fn test_lock_error_display_conflict() {
        let e = LockError::Conflict {
            entity: "users#id=1".to_string(),
            expected_version: 5,
        };
        let s = format!("{}", e);
        assert!(s.contains("Optimistic lock conflict"));
        assert!(s.contains("users#id=1"));
        assert!(s.contains("expected version 5"));
    }

    #[test]
    fn test_lock_error_display_missing_version() {
        let e = LockError::MissingVersion { field: "version" };
        let s = format!("{}", e);
        assert!(s.contains("Missing version value"));
        assert!(s.contains("version"));
    }

    #[test]
    fn test_lock_error_display_invalid_version() {
        let e = LockError::InvalidVersion {
            field: "version",
            value: -1,
        };
        let s = format!("{}", e);
        assert!(s.contains("Invalid version value"));
        assert!(s.contains("-1"));
    }

    #[test]
    fn test_lock_error_display_retries_exhausted() {
        let e = LockError::RetriesExhausted { attempts: 5 };
        let s = format!("{}", e);
        assert!(s.contains("Retries exhausted"));
        assert!(s.contains("5"));
    }

    // ===== OptimisticLock trait 测试 =====

    struct Product {
        _id: i64,
        _version: i64,
    }
    impl OptimisticLock for Product {
        fn version_field() -> &'static str {
            "version"
        }
    }

    #[test]
    fn test_optimistic_lock_trait_implementable() {
        // 验证 trait 可被实现
        assert_eq!(Product::version_field(), "version");
    }
}
