//! # SZ-ORM Batch — 批量操作
//!
//! 提供批量插入、更新与 UPSERT 能力，支持多值 INSERT、CASE WHEN UPDATE
//! 与分片感知的批量执行，并返回生成的 SQL 供审计。
//!
//! ## 主要类型
//!
//! - [`BatchResult`] — 批量操作结果
//! - [`BatchOperations`] trait — 批量操作接口

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 批量操作结果。generated_sqls 持有实际生成的 SQL 语句，供调用方执行与审计。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult {
    pub inserted: usize,
    pub updated: usize,
    pub failed: usize,
    pub generated_sqls: Vec<String>,
}

impl BatchResult {
    pub fn new() -> Self {
        Self {
            inserted: 0,
            updated: 0,
            failed: 0,
            generated_sqls: Vec::new(),
        }
    }
}

impl Default for BatchResult {
    fn default() -> Self {
        Self::new()
    }
}

pub trait BatchOperations: Send + Sync {
    fn batch_insert(&self, table: &str, rows: Vec<Value>) -> BatchResult;
    fn batch_update(&self, table: &str, rows: Vec<Value>) -> BatchResult;
    fn batch_upsert(&self, table: &str, rows: Vec<Value>) -> BatchResult;
}

/// Upsert 语法模式：MySQL 风格（ON DUPLICATE KEY UPDATE）或 PostgreSQL 风格（ON CONFLICT DO UPDATE）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertMode {
    MysqlOnDuplicate,
    PostgresOnConflict,
}

/// 默认批量操作实现。生成多值 INSERT、CASE WHEN UPDATE、ON CONFLICT/ON DUPLICATE UPSERT。
///
/// L-5 修复：补充示例文档
///
/// # 示例
///
/// ```ignore
/// use sz_orm_batch::{DefaultBatchOps, UpsertMode};
/// use serde_json::json;
///
/// // 创建默认配置（主键 "id", MySQL ON DUPLICATE 模式, 分片 1000）
/// let ops = DefaultBatchOps::new();
///
/// // 自定义主键和分片大小
/// let ops = DefaultBatchOps::with_primary_key("user_id")
///     .with_chunk_size(500)
///     .with_upsert_mode(UpsertMode::PostgresOnConflict);
///
/// let rows = vec![
///     json!({ "user_id": 1, "name": "Alice" }),
///     json!({ "user_id": 2, "name": "Bob" }),
/// ];
///
/// // 生成批量插入 SQL（实际调用需通过 BatchOperations trait）
/// // let sql = ops.batch_insert("users", &rows).unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct DefaultBatchOps {
    pub primary_key: String,
    pub upsert_mode: UpsertMode,
    /// H-9 修复：批量插入分片大小
    ///
    /// 当 `rows.len() > chunk_size` 时，`batch_insert` / `batch_upsert` 会将数据
    /// 按 `chunk_size` 分片，每片生成独立的 SQL 语句。这避免了超大批量插入触发
    /// 数据库参数限制（如 MySQL `max_allowed_packet`、PostgreSQL 参数占位符上限 65535）。
    ///
    /// 默认 `DEFAULT_CHUNK_SIZE`（1000）。设为 0 等价于 1（每行一条 SQL）。
    pub chunk_size: usize,
}

/// H-9 默认分片大小
pub const DEFAULT_CHUNK_SIZE: usize = 1000;

impl Default for DefaultBatchOps {
    fn default() -> Self {
        Self {
            primary_key: "id".to_string(),
            upsert_mode: UpsertMode::MysqlOnDuplicate,
            chunk_size: DEFAULT_CHUNK_SIZE,
        }
    }
}

impl DefaultBatchOps {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_primary_key(primary_key: impl Into<String>) -> Self {
        Self {
            primary_key: primary_key.into(),
            upsert_mode: UpsertMode::MysqlOnDuplicate,
            chunk_size: DEFAULT_CHUNK_SIZE,
        }
    }

    pub fn with_upsert_mode(mut self, mode: UpsertMode) -> Self {
        self.upsert_mode = mode;
        self
    }

    /// H-9 修复：设置批量插入分片大小
    pub fn with_chunk_size(mut self, chunk_size: usize) -> Self {
        self.chunk_size = chunk_size.max(1);
        self
    }

    /// H-9 修复：将切片按 chunk_size 分片
    ///
    /// 返回索引迭代器，每个元素是 (start, end) 半开区间。
    fn chunk_indices(&self, total: usize) -> impl Iterator<Item = (usize, usize)> {
        let chunk_size = self.chunk_size.max(1);
        (0..total).step_by(chunk_size).map(move |start| {
            let end = (start + chunk_size).min(total);
            (start, end)
        })
    }

    /// 用反引号包裹标识符（MySQL 风格）。
    fn quote(name: &str) -> String {
        format!("`{}`", name)
    }

    /// 从 JSON 对象提取字段名。
    ///
    /// 列顺序取决于 serde_json 的 feature 配置：
    /// - 默认（无 `preserve_order`）：使用 BTreeMap，按字典序
    /// - 启用 `preserve_order`：使用 IndexMap，按插入序
    ///
    /// 在 workspace `--all-features` 编译下，其他包可能启用 `preserve_order`，
    /// 通过 feature unification 传导到本包，导致列顺序变化。
    /// 调用方不应假设特定的列顺序。
    fn extract_columns(row: &Value) -> Option<Vec<String>> {
        match row {
            Value::Object(map) => Some(map.keys().map(|k| k.to_string()).collect()),
            _ => None,
        }
    }

    /// 返回非主键列。
    fn non_pk_columns(&self, columns: &[String]) -> Vec<String> {
        columns
            .iter()
            .filter(|c| **c != self.primary_key)
            .cloned()
            .collect()
    }

    /// 校验 row 是否拥有所有指定列。
    fn row_has_all_columns(row: &Value, columns: &[String]) -> bool {
        match row {
            Value::Object(map) => columns.iter().all(|c| map.contains_key(c)),
            _ => false,
        }
    }

    /// 生成单行占位符："(?, ?, ?)"。
    fn placeholder_row(col_count: usize) -> String {
        let placeholders = vec!["?"; col_count].join(", ");
        format!("({})", placeholders)
    }

    /// 提取列定义并校验首行合法；失败时返回 None 并将所有 rows 计入 failed。
    fn validate_and_extract(&self, rows: &[Value]) -> Option<Vec<String>> {
        let first = rows.first()?;
        match Self::extract_columns(first) {
            Some(c) if !c.is_empty() => Some(c),
            _ => None,
        }
    }

    /// 过滤出字段齐全的有效行，返回 (valid_refs, failed_count)。
    fn filter_valid_rows<'a>(&self, rows: &'a [Value], columns: &[String]) -> Vec<&'a Value> {
        rows.iter()
            .filter(|r| Self::row_has_all_columns(r, columns))
            .collect()
    }

    /// 共用：生成 INSERT 头部与多值占位符部分。
    fn build_insert_clause(
        &self,
        table: &str,
        columns: &[String],
        valid_rows: &[&Value],
    ) -> String {
        let cols_str = columns
            .iter()
            .map(|c| Self::quote(c))
            .collect::<Vec<_>>()
            .join(", ");
        let row_ph = Self::placeholder_row(columns.len());
        let all_ph = valid_rows
            .iter()
            .map(|_| row_ph.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "INSERT INTO {} ({}) VALUES {}",
            Self::quote(table),
            cols_str,
            all_ph
        )
    }
}

impl BatchOperations for DefaultBatchOps {
    fn batch_insert(&self, table: &str, rows: Vec<Value>) -> BatchResult {
        let mut result = BatchResult::new();
        if rows.is_empty() {
            return result;
        }

        let columns = match self.validate_and_extract(&rows) {
            Some(c) => c,
            None => {
                result.failed = rows.len();
                return result;
            }
        };

        let valid_rows = self.filter_valid_rows(&rows, &columns);
        result.failed = rows.len() - valid_rows.len();
        if valid_rows.is_empty() {
            return result;
        }

        // H-9 修复：按 chunk_size 分片生成多条 INSERT
        let total = valid_rows.len();
        for (start, end) in self.chunk_indices(total) {
            let chunk = &valid_rows[start..end];
            let sql = self.build_insert_clause(table, &columns, chunk);
            result.generated_sqls.push(sql);
            result.inserted += chunk.len();
        }
        result
    }

    fn batch_update(&self, table: &str, rows: Vec<Value>) -> BatchResult {
        let mut result = BatchResult::new();
        if rows.is_empty() {
            return result;
        }

        let columns = match self.validate_and_extract(&rows) {
            Some(c) => c,
            None => {
                result.failed = rows.len();
                return result;
            }
        };

        if !columns.contains(&self.primary_key) {
            result.failed = rows.len();
            return result;
        }

        let valid_rows = self.filter_valid_rows(&rows, &columns);
        result.failed = rows.len() - valid_rows.len();
        if valid_rows.is_empty() {
            return result;
        }

        let non_pk = self.non_pk_columns(&columns);
        if non_pk.is_empty() {
            // 没有可更新列
            result.failed += valid_rows.len();
            return result;
        }

        // 为每个非主键列生成 CASE WHEN 子句
        let pk_quoted = Self::quote(&self.primary_key);
        let case_clauses: Vec<String> = non_pk
            .iter()
            .map(|col| {
                let col_quoted = Self::quote(col);
                let when_clauses: Vec<String> = valid_rows
                    .iter()
                    .map(|_| format!("{} = ? THEN ?", pk_quoted))
                    .collect();
                format!(
                    "{} = CASE WHEN {} ELSE {} END",
                    col_quoted,
                    when_clauses.join(" WHEN "),
                    col_quoted
                )
            })
            .collect();

        // WHERE IN 子句
        let pk_placeholders = vec!["?"; valid_rows.len()].join(", ");
        let where_clause = format!("{} IN ({})", pk_quoted, pk_placeholders);

        let sql = format!(
            "UPDATE {} SET {} WHERE {}",
            Self::quote(table),
            case_clauses.join(", "),
            where_clause
        );

        result.generated_sqls.push(sql);
        result.updated = valid_rows.len();
        result
    }

    fn batch_upsert(&self, table: &str, rows: Vec<Value>) -> BatchResult {
        let mut result = BatchResult::new();
        if rows.is_empty() {
            return result;
        }

        let columns = match self.validate_and_extract(&rows) {
            Some(c) => c,
            None => {
                result.failed = rows.len();
                return result;
            }
        };

        if !columns.contains(&self.primary_key) {
            result.failed = rows.len();
            return result;
        }

        let valid_rows = self.filter_valid_rows(&rows, &columns);
        result.failed = rows.len() - valid_rows.len();
        if valid_rows.is_empty() {
            return result;
        }

        let non_pk = self.non_pk_columns(&columns);

        // H-9 修复：按 chunk_size 分片生成多条 UPSERT
        let total = valid_rows.len();
        for (start, end) in self.chunk_indices(total) {
            let chunk = &valid_rows[start..end];
            let insert_part = self.build_insert_clause(table, &columns, chunk);

            let conflict_part = match self.upsert_mode {
                UpsertMode::MysqlOnDuplicate if !non_pk.is_empty() => {
                    let updates: Vec<String> = non_pk
                        .iter()
                        .map(|col| {
                            let q = Self::quote(col);
                            format!("{} = VALUES({})", q, q)
                        })
                        .collect();
                    format!(" ON DUPLICATE KEY UPDATE {}", updates.join(", "))
                }
                UpsertMode::PostgresOnConflict if !non_pk.is_empty() => {
                    let updates: Vec<String> = non_pk
                        .iter()
                        .map(|col| {
                            let q = Self::quote(col);
                            format!("{} = EXCLUDED.{}", q, q)
                        })
                        .collect();
                    format!(
                        " ON CONFLICT ({}) DO UPDATE SET {}",
                        Self::quote(&self.primary_key),
                        updates.join(", ")
                    )
                }
                _ => String::new(),
            };

            let sql = format!("{}{}", insert_part, conflict_part);
            result.generated_sqls.push(sql);
            result.inserted += chunk.len();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ============ BatchResult / DefaultBatchOps 基础 ============

    #[test]
    fn test_batch_result_default() {
        let r = BatchResult::default();
        assert_eq!(r.inserted, 0);
        assert_eq!(r.updated, 0);
        assert_eq!(r.failed, 0);
        assert!(r.generated_sqls.is_empty());
    }

    #[test]
    fn test_default_batch_ops_default() {
        let ops = DefaultBatchOps::default();
        assert_eq!(ops.primary_key, "id");
        assert_eq!(ops.upsert_mode, UpsertMode::MysqlOnDuplicate);
    }

    #[test]
    fn test_with_primary_key_custom() {
        let ops = DefaultBatchOps::with_primary_key("user_id");
        assert_eq!(ops.primary_key, "user_id");
        assert_eq!(ops.upsert_mode, UpsertMode::MysqlOnDuplicate);
    }

    #[test]
    fn test_with_upsert_mode_builder() {
        let ops = DefaultBatchOps::new().with_upsert_mode(UpsertMode::PostgresOnConflict);
        assert_eq!(ops.upsert_mode, UpsertMode::PostgresOnConflict);
    }

    // ============ batch_insert ============

    #[test]
    fn test_batch_insert_empty_rows() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_insert("users", vec![]);
        assert_eq!(result.inserted, 0);
        assert_eq!(result.failed, 0);
        assert!(result.generated_sqls.is_empty());
    }

    #[test]
    fn test_batch_insert_single_row() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_insert("users", vec![json!({"id": 1, "name": "Alice"})]);
        assert_eq!(result.inserted, 1);
        assert_eq!(result.failed, 0);
        assert_eq!(result.generated_sqls.len(), 1);
        let sql = &result.generated_sqls[0];
        assert!(sql.starts_with("INSERT INTO `users`"));
        assert!(sql.contains("`id`, `name`"));
        assert!(sql.contains("VALUES (?, ?)"));
        // 单行不应有逗号分隔的多值
        assert!(!sql.contains("), ("));
    }

    #[test]
    fn test_batch_insert_multiple_rows() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_insert(
            "users",
            vec![
                json!({"id": 1, "name": "Alice"}),
                json!({"id": 2, "name": "Bob"}),
                json!({"id": 3, "name": "Carol"}),
            ],
        );
        assert_eq!(result.inserted, 3);
        assert_eq!(result.failed, 0);
        assert_eq!(result.generated_sqls.len(), 1);
        let sql = &result.generated_sqls[0];
        // 应有 3 个 (?, ?)
        assert_eq!(sql.matches("(?, ?)").count(), 3);
        assert!(sql.contains("(?, ?), (?, ?), (?, ?)"));
    }

    #[test]
    fn test_batch_insert_single_column() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_insert("logs", vec![json!({"msg": "hello"})]);
        assert_eq!(result.inserted, 1);
        let sql = &result.generated_sqls[0];
        assert!(sql.contains("`msg`"));
        assert!(sql.contains("VALUES (?)"));
    }

    #[test]
    fn test_batch_insert_filters_non_object_rows() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_insert(
            "users",
            vec![
                json!({"id": 1, "name": "Alice"}),
                json!("not an object"),
                json!(42),
            ],
        );
        assert_eq!(result.inserted, 1);
        assert_eq!(result.failed, 2);
        let sql = &result.generated_sqls[0];
        assert_eq!(sql.matches("(?, ?)").count(), 1);
    }

    #[test]
    fn test_batch_insert_filters_rows_missing_fields() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_insert(
            "users",
            vec![
                json!({"id": 1, "name": "Alice"}),
                json!({"id": 2}), // 缺 name
            ],
        );
        assert_eq!(result.inserted, 1);
        assert_eq!(result.failed, 1);
    }

    #[test]
    fn test_batch_insert_all_invalid() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_insert(
            "users",
            vec![json!("not an object"), json!("another invalid")],
        );
        assert_eq!(result.inserted, 0);
        assert_eq!(result.failed, 2);
        assert!(result.generated_sqls.is_empty());
    }

    #[test]
    fn test_batch_insert_preserves_column_order_from_btreemap() {
        // 列顺序取决于 serde_json feature：
        // - 默认 BTreeMap，按字典序：age, id, name
        // - preserve_order 启用 IndexMap，按插入序：name, id, age
        // 两种顺序均为合法行为，测试应兼容两者。
        let ops = DefaultBatchOps::new();
        let result = ops.batch_insert("users", vec![json!({"name": "Alice", "id": 1, "age": 30})]);
        assert_eq!(result.inserted, 1);
        let sql = &result.generated_sqls[0];
        let is_btree_order = sql.contains("`age`, `id`, `name`");
        let is_index_order = sql.contains("`name`, `id`, `age`");
        assert!(
            is_btree_order || is_index_order,
            "列顺序应为 BTreeMap 字典序或 IndexMap 插入序，实际 SQL: {sql}"
        );
        // 三列必须全部出现
        assert!(sql.contains("`age`"));
        assert!(sql.contains("`id`"));
        assert!(sql.contains("`name`"));
    }

    // ============ batch_update ============

    #[test]
    fn test_batch_update_empty_rows() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_update("users", vec![]);
        assert_eq!(result.updated, 0);
        assert_eq!(result.failed, 0);
        assert!(result.generated_sqls.is_empty());
    }

    #[test]
    fn test_batch_update_single_row_single_col() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_update("users", vec![json!({"id": 1, "name": "Alice"})]);
        assert_eq!(result.updated, 1);
        assert_eq!(result.failed, 0);
        let sql = &result.generated_sqls[0];
        assert!(sql.starts_with("UPDATE `users` SET"));
        assert!(sql.contains("`name` = CASE WHEN `id` = ? THEN ?"));
        assert!(sql.contains("ELSE `name` END"));
        assert!(sql.contains("WHERE `id` IN (?)"));
    }

    #[test]
    fn test_batch_update_multiple_rows_multiple_cols() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_update(
            "users",
            vec![
                json!({"id": 1, "name": "Alice", "age": 30}),
                json!({"id": 2, "name": "Bob", "age": 25}),
            ],
        );
        assert_eq!(result.updated, 2);
        let sql = &result.generated_sqls[0];
        // 应有 2 个 CASE 子句（name 和 age）
        assert_eq!(sql.matches("CASE").count(), 2);
        // WHERE IN 应有 2 个 ?
        assert!(sql.contains("WHERE `id` IN (?, ?)"));
        // 每个 CASE 内部应有 2 个 WHEN 子句
        assert_eq!(sql.matches("WHEN").count(), 4);
    }

    #[test]
    fn test_batch_update_requires_primary_key() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_update("users", vec![json!({"name": "Alice"})]);
        assert_eq!(result.updated, 0);
        assert_eq!(result.failed, 1);
        assert!(result.generated_sqls.is_empty());
    }

    #[test]
    fn test_batch_update_custom_primary_key() {
        let ops = DefaultBatchOps::with_primary_key("user_id");
        let result = ops.batch_update("users", vec![json!({"user_id": 1, "name": "Alice"})]);
        assert_eq!(result.updated, 1);
        let sql = &result.generated_sqls[0];
        assert!(sql.contains("`user_id` = ? THEN ?"));
        assert!(sql.contains("WHERE `user_id` IN"));
    }

    #[test]
    fn test_batch_update_only_pk_no_other_cols() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_update("users", vec![json!({"id": 1})]);
        assert_eq!(result.updated, 0);
        assert_eq!(result.failed, 1);
        assert!(result.generated_sqls.is_empty());
    }

    #[test]
    fn test_batch_update_filters_invalid_rows() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_update(
            "users",
            vec![
                json!({"id": 1, "name": "Alice"}),
                json!({"id": 2}), // 缺 name
                json!("invalid"), // 非 object
            ],
        );
        assert_eq!(result.updated, 1);
        assert_eq!(result.failed, 2);
    }

    #[test]
    fn test_batch_update_case_when_structure_correct() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_update(
            "users",
            vec![
                json!({"id": 1, "name": "Alice"}),
                json!({"id": 2, "name": "Bob"}),
            ],
        );
        let sql = &result.generated_sqls[0];
        // 期望形如：name = CASE WHEN id = ? THEN ? WHEN id = ? THEN ? ELSE name END
        assert!(sql.contains("CASE WHEN `id` = ? THEN ? WHEN `id` = ? THEN ? ELSE `name` END"));
    }

    // ============ batch_upsert ============

    #[test]
    fn test_batch_upsert_empty_rows() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_upsert("users", vec![]);
        assert_eq!(result.inserted, 0);
        assert!(result.generated_sqls.is_empty());
    }

    #[test]
    fn test_batch_upsert_mysql_mode_single_row() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_upsert("users", vec![json!({"id": 1, "name": "Alice"})]);
        assert_eq!(result.inserted, 1);
        assert_eq!(result.failed, 0);
        let sql = &result.generated_sqls[0];
        assert!(sql.starts_with("INSERT INTO `users`"));
        assert!(sql.contains("ON DUPLICATE KEY UPDATE"));
        assert!(sql.contains("`name` = VALUES(`name`)"));
        assert!(!sql.contains("ON CONFLICT"));
    }

    #[test]
    fn test_batch_upsert_mysql_mode_multiple_rows() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_upsert(
            "users",
            vec![
                json!({"id": 1, "name": "Alice"}),
                json!({"id": 2, "name": "Bob"}),
            ],
        );
        assert_eq!(result.inserted, 2);
        let sql = &result.generated_sqls[0];
        // 应有 2 个值组
        assert_eq!(sql.matches("(?, ?)").count(), 2);
        assert!(sql.contains("ON DUPLICATE KEY UPDATE"));
    }

    #[test]
    fn test_batch_upsert_postgres_mode() {
        let ops = DefaultBatchOps::new().with_upsert_mode(UpsertMode::PostgresOnConflict);
        let result = ops.batch_upsert("users", vec![json!({"id": 1, "name": "Alice"})]);
        assert_eq!(result.inserted, 1);
        let sql = &result.generated_sqls[0];
        assert!(sql.contains("ON CONFLICT (`id`) DO UPDATE SET"));
        assert!(sql.contains("`name` = EXCLUDED.`name`"));
        assert!(!sql.contains("ON DUPLICATE KEY"));
    }

    #[test]
    fn test_batch_upsert_multiple_cols_does_not_update_pk() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_upsert("users", vec![json!({"id": 1, "name": "Alice", "age": 30})]);
        assert_eq!(result.inserted, 1);
        let sql = &result.generated_sqls[0];
        assert!(sql.contains("`name` = VALUES(`name`)"));
        assert!(sql.contains("`age` = VALUES(`age`)"));
        // 不应更新主键
        assert!(!sql.contains("`id` = VALUES"));
    }

    #[test]
    fn test_batch_upsert_postgres_does_not_update_pk() {
        let ops = DefaultBatchOps::new().with_upsert_mode(UpsertMode::PostgresOnConflict);
        let result = ops.batch_upsert("users", vec![json!({"id": 1, "name": "Alice", "age": 30})]);
        let sql = &result.generated_sqls[0];
        assert!(sql.contains("`name` = EXCLUDED.`name`"));
        assert!(sql.contains("`age` = EXCLUDED.`age`"));
        assert!(!sql.contains("`id` = EXCLUDED"));
    }

    #[test]
    fn test_batch_upsert_requires_primary_key() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_upsert("users", vec![json!({"name": "Alice"})]);
        assert_eq!(result.inserted, 0);
        assert_eq!(result.failed, 1);
        assert!(result.generated_sqls.is_empty());
    }

    #[test]
    fn test_batch_upsert_only_pk_no_other_cols() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_upsert("users", vec![json!({"id": 1})]);
        assert_eq!(result.inserted, 1);
        let sql = &result.generated_sqls[0];
        // 应有 INSERT 但无 ON DUPLICATE KEY（无列可更新）
        assert!(sql.starts_with("INSERT INTO"));
        assert!(!sql.contains("ON DUPLICATE KEY"));
        assert!(!sql.contains("ON CONFLICT"));
    }

    #[test]
    fn test_batch_upsert_filters_invalid_rows() {
        let ops = DefaultBatchOps::new();
        let result = ops.batch_upsert(
            "users",
            vec![
                json!({"id": 1, "name": "Alice"}),
                json!("invalid"),
                json!({"name": "Bob"}), // 无 id
            ],
        );
        assert_eq!(result.inserted, 1);
        assert_eq!(result.failed, 2);
    }

    #[test]
    fn test_batch_upsert_custom_pk_mysql() {
        let ops = DefaultBatchOps::with_primary_key("email");
        let result = ops.batch_upsert("users", vec![json!({"email": "a@b.com", "name": "Alice"})]);
        assert_eq!(result.inserted, 1);
        let sql = &result.generated_sqls[0];
        // 主键 email 不应在 VALUES(...) 更新列表中
        assert!(sql.contains("`name` = VALUES(`name`)"));
        assert!(!sql.contains("`email` = VALUES"));
    }

    // ==================== H-9 批量插入分片测试 ====================

    #[test]
    fn test_h9_default_chunk_size_is_1000() {
        let ops = DefaultBatchOps::default();
        assert_eq!(ops.chunk_size, DEFAULT_CHUNK_SIZE);
        assert_eq!(ops.chunk_size, 1000);
    }

    #[test]
    fn test_h9_with_chunk_size_builder() {
        let ops = DefaultBatchOps::new().with_chunk_size(50);
        assert_eq!(ops.chunk_size, 50);
    }

    #[test]
    fn test_h9_with_chunk_size_zero_clamps_to_one() {
        let ops = DefaultBatchOps::new().with_chunk_size(0);
        assert_eq!(ops.chunk_size, 1);
    }

    #[test]
    fn test_h9_batch_insert_single_chunk_when_below_threshold() {
        // 默认 chunk_size=1000，3 行应只生成 1 条 SQL
        let ops = DefaultBatchOps::new();
        let rows: Vec<Value> = (1..=3)
            .map(|i| json!({"id": i, "name": format!("user{}", i)}))
            .collect();
        let result = ops.batch_insert("users", rows);
        assert_eq!(result.inserted, 3);
        assert_eq!(result.failed, 0);
        assert_eq!(result.generated_sqls.len(), 1);
    }

    #[test]
    fn test_h9_batch_insert_chunks_when_above_threshold() {
        // chunk_size=2，5 行应生成 3 条 SQL（2+2+1）
        let ops = DefaultBatchOps::new().with_chunk_size(2);
        let rows: Vec<Value> = (1..=5)
            .map(|i| json!({"id": i, "name": format!("user{}", i)}))
            .collect();
        let result = ops.batch_insert("users", rows);
        assert_eq!(result.inserted, 5);
        assert_eq!(result.failed, 0);
        assert_eq!(result.generated_sqls.len(), 3);

        // 验证每片 SQL 的行数：第 1 片 2 行、第 2 片 2 行、第 3 片 1 行
        let counts: Vec<usize> = result
            .generated_sqls
            .iter()
            .map(|sql| sql.matches("(?, ?)").count())
            .collect();
        assert_eq!(counts, vec![2, 2, 1]);
    }

    #[test]
    fn test_h9_batch_insert_chunk_size_one_generates_one_sql_per_row() {
        // chunk_size=1，3 行应生成 3 条 SQL，每条 1 行
        let ops = DefaultBatchOps::new().with_chunk_size(1);
        let rows: Vec<Value> = (1..=3)
            .map(|i| json!({"id": i, "name": format!("user{}", i)}))
            .collect();
        let result = ops.batch_insert("users", rows);
        assert_eq!(result.inserted, 3);
        assert_eq!(result.generated_sqls.len(), 3);
        for sql in &result.generated_sqls {
            assert_eq!(sql.matches("(?, ?)").count(), 1);
        }
    }

    #[test]
    fn test_h9_batch_insert_chunks_preserve_failed_count() {
        // chunk_size=2，5 行中有 2 行无效，应正确统计
        let ops = DefaultBatchOps::new().with_chunk_size(2);
        let result = ops.batch_insert(
            "users",
            vec![
                json!({"id": 1, "name": "Alice"}),
                json!("invalid"),
                json!({"id": 3, "name": "Carol"}),
                json!(42),
                json!({"id": 5, "name": "Eve"}),
            ],
        );
        // 3 行有效，2 行无效
        assert_eq!(result.inserted, 3);
        assert_eq!(result.failed, 2);
        // 3 行有效，chunk_size=2 → 2 片（2+1）
        assert_eq!(result.generated_sqls.len(), 2);
    }

    #[test]
    fn test_h9_batch_upsert_chunks_when_above_threshold() {
        // chunk_size=2，4 行应生成 2 条 SQL（2+2）
        let ops = DefaultBatchOps::new().with_chunk_size(2);
        let rows: Vec<Value> = (1..=4)
            .map(|i| json!({"id": i, "name": format!("user{}", i)}))
            .collect();
        let result = ops.batch_upsert("users", rows);
        assert_eq!(result.inserted, 4);
        assert_eq!(result.failed, 0);
        assert_eq!(result.generated_sqls.len(), 2);

        // 验证每片都包含 ON DUPLICATE KEY UPDATE
        for sql in &result.generated_sqls {
            assert!(sql.contains("ON DUPLICATE KEY UPDATE"));
            assert_eq!(sql.matches("(?, ?)").count(), 2);
        }
    }

    #[test]
    fn test_h9_batch_upsert_postgres_mode_chunks() {
        let ops = DefaultBatchOps::new()
            .with_chunk_size(2)
            .with_upsert_mode(UpsertMode::PostgresOnConflict);
        let rows: Vec<Value> = (1..=5)
            .map(|i| json!({"id": i, "name": format!("user{}", i)}))
            .collect();
        let result = ops.batch_upsert("users", rows);
        assert_eq!(result.inserted, 5);
        assert_eq!(result.generated_sqls.len(), 3); // 2+2+1

        for sql in &result.generated_sqls {
            assert!(sql.contains("ON CONFLICT (`id`) DO UPDATE SET"));
        }
    }

    #[test]
    fn test_h9_batch_update_does_not_chunk() {
        // batch_update 是单条 UPDATE 语句，不参与分片
        let ops = DefaultBatchOps::new().with_chunk_size(1);
        let rows: Vec<Value> = (1..=3)
            .map(|i| json!({"id": i, "name": format!("user{}", i)}))
            .collect();
        let result = ops.batch_update("users", rows);
        assert_eq!(result.updated, 3);
        assert_eq!(result.generated_sqls.len(), 1);
    }

    #[test]
    fn test_h9_batch_insert_large_batch_100_rows_with_chunk_10() {
        // 验证大批量分片
        let ops = DefaultBatchOps::new().with_chunk_size(10);
        let rows: Vec<Value> = (1..=100)
            .map(|i| json!({"id": i, "name": format!("user{}", i)}))
            .collect();
        let result = ops.batch_insert("users", rows);
        assert_eq!(result.inserted, 100);
        assert_eq!(result.failed, 0);
        assert_eq!(result.generated_sqls.len(), 10);

        // 每片 10 行
        for sql in &result.generated_sqls {
            assert_eq!(sql.matches("(?, ?)").count(), 10);
        }
    }
}
