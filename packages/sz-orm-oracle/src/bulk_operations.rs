//! Oracle 批量操作支持
//!
//! 提供 [`BulkOperations`] 用于构建高效的批量 INSERT/UPDATE/DELETE，
//! 利用 Oracle 的 Array DML（Batch API）一次性提交多行，减少网络往返。

use std::fmt;

/// 批量操作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkOpKind {
    /// 批量插入
    Insert,
    /// 批量更新
    Update,
    /// 批量删除
    Delete,
    /// 批量合并（MERGE）
    Merge,
}

impl BulkOpKind {
    /// 返回 SQL 关键字
    #[must_use]
    pub fn as_keyword(&self) -> &'static str {
        match self {
            BulkOpKind::Insert => "INSERT",
            BulkOpKind::Update => "UPDATE",
            BulkOpKind::Delete => "DELETE",
            BulkOpKind::Merge => "MERGE",
        }
    }
}

impl fmt::Display for BulkOpKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_keyword())
    }
}

/// 批量操作配置
#[derive(Debug, Clone)]
pub struct BulkConfig {
    /// 批次大小（每次提交的行数，默认 1000）
    pub batch_size: usize,
    /// 是否启用行计数
    pub with_row_counts: bool,
    /// 错误处理模式
    pub error_mode: BulkErrorMode,
    /// 是否在每批后提交
    pub auto_commit_per_batch: bool,
}

impl Default for BulkConfig {
    fn default() -> Self {
        Self {
            batch_size: 1000,
            with_row_counts: true,
            error_mode: BulkErrorMode::StopOnFirst,
            auto_commit_per_batch: false,
        }
    }
}

impl BulkConfig {
    /// 创建默认配置
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置批次大小
    #[must_use]
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size.max(1);
        self
    }

    /// 启用行计数
    #[must_use]
    pub fn with_row_counts(mut self) -> Self {
        self.with_row_counts = true;
        self
    }

    /// 设置错误处理模式
    #[must_use]
    pub fn with_error_mode(mut self, mode: BulkErrorMode) -> Self {
        self.error_mode = mode;
        self
    }

    /// 启用每批自动提交
    #[must_use]
    pub fn with_auto_commit(mut self) -> Self {
        self.auto_commit_per_batch = true;
        self
    }

    /// 计算批次数
    #[must_use]
    pub fn batch_count(&self, total_rows: usize) -> usize {
        if total_rows == 0 {
            0
        } else {
            total_rows.div_ceil(self.batch_size)
        }
    }
}

/// 批量操作错误处理模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkErrorMode {
    /// 首个错误即停止
    StopOnFirst,
    /// 收集所有错误后停止
    CollectAll,
    /// 跳过错误行继续
    SkipErrors,
}

impl BulkErrorMode {
    /// 返回描述
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            BulkErrorMode::StopOnFirst => "stop on first error",
            BulkErrorMode::CollectAll => "collect all errors",
            BulkErrorMode::SkipErrors => "skip error rows",
        }
    }
}

/// 批量操作结果
#[derive(Debug, Clone, Default)]
pub struct BulkResult {
    /// 总影响行数
    pub affected_rows: u64,
    /// 处理的批次数
    pub batches_processed: usize,
    /// 总输入行数
    pub total_input_rows: usize,
    /// 错误行索引列表
    pub error_rows: Vec<usize>,
    /// 每批影响行数
    pub per_batch_counts: Vec<u64>,
}

impl BulkResult {
    /// 创建新的批量结果
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 是否有错误
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.error_rows.is_empty()
    }

    /// 错误率
    #[must_use]
    pub fn error_rate(&self) -> f64 {
        if self.total_input_rows == 0 {
            0.0
        } else {
            self.error_rows.len() as f64 / self.total_input_rows as f64
        }
    }

    /// 成功行数
    #[must_use]
    pub fn success_rows(&self) -> usize {
        self.total_input_rows - self.error_rows.len()
    }

    /// 平均每批行数
    #[must_use]
    pub fn avg_batch_size(&self) -> f64 {
        if self.batches_processed == 0 {
            0.0
        } else {
            self.total_input_rows as f64 / self.batches_processed as f64
        }
    }

    /// 合并两个结果
    #[must_use]
    pub fn merge(&self, other: &BulkResult) -> BulkResult {
        BulkResult {
            affected_rows: self.affected_rows + other.affected_rows,
            batches_processed: self.batches_processed + other.batches_processed,
            total_input_rows: self.total_input_rows + other.total_input_rows,
            error_rows: {
                let mut errs = self.error_rows.clone();
                let offset = self.total_input_rows;
                errs.extend(other.error_rows.iter().map(|&i| i + offset));
                errs
            },
            per_batch_counts: {
                let mut counts = self.per_batch_counts.clone();
                counts.extend(other.per_batch_counts.iter().copied());
                counts
            },
        }
    }
}

impl fmt::Display for BulkResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BulkResult(affected={}, batches={}, errors={})",
            self.affected_rows,
            self.batches_processed,
            self.error_rows.len()
        )
    }
}

/// 批量操作构建器
///
/// 构建批量 INSERT/UPDATE/DELETE SQL 语句。
#[derive(Debug, Clone)]
pub struct BulkOperations {
    /// 目标表名
    table: String,
    /// 操作类型
    kind: BulkOpKind,
    /// 列名列表（INSERT/UPDATE）
    columns: Vec<String>,
    /// WHERE 条件列（UPDATE/DELETE）
    where_columns: Vec<String>,
    /// 配置
    config: BulkConfig,
}

impl BulkOperations {
    /// 创建批量 INSERT 构建器
    #[must_use]
    pub fn insert(table: &str) -> Self {
        Self {
            table: table.to_string(),
            kind: BulkOpKind::Insert,
            columns: Vec::new(),
            where_columns: Vec::new(),
            config: BulkConfig::default(),
        }
    }

    /// 创建批量 UPDATE 构建器
    #[must_use]
    pub fn update(table: &str) -> Self {
        Self {
            table: table.to_string(),
            kind: BulkOpKind::Update,
            columns: Vec::new(),
            where_columns: Vec::new(),
            config: BulkConfig::default(),
        }
    }

    /// 创建批量 DELETE 构建器
    #[must_use]
    pub fn delete(table: &str) -> Self {
        Self {
            table: table.to_string(),
            kind: BulkOpKind::Delete,
            columns: Vec::new(),
            where_columns: Vec::new(),
            config: BulkConfig::default(),
        }
    }

    /// 设置列名（INSERT/UPDATE SET 子句）
    #[must_use]
    pub fn columns(mut self, cols: &[&str]) -> Self {
        self.columns = cols.iter().map(|s| s.to_string()).collect();
        self
    }

    /// 设置 WHERE 条件列（UPDATE/DELETE）
    #[must_use]
    pub fn where_columns(mut self, cols: &[&str]) -> Self {
        self.where_columns = cols.iter().map(|s| s.to_string()).collect();
        self
    }

    /// 设置批量配置
    #[must_use]
    pub fn with_config(mut self, config: BulkConfig) -> Self {
        self.config = config;
        self
    }

    /// 设置批次大小
    #[must_use]
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.config = self.config.with_batch_size(size);
        self
    }

    /// 生成单行 SQL 模板（使用 `?` 占位符）
    #[must_use]
    pub fn build_single_sql(&self) -> String {
        match self.kind {
            BulkOpKind::Insert => {
                if self.columns.is_empty() {
                    return String::new();
                }
                let cols = self.columns.join(", ");
                let placeholders: Vec<String> =
                    (1..=self.columns.len()).map(|i| format!(":{i}")).collect();
                format!(
                    "INSERT INTO {} ({}) VALUES ({})",
                    self.table,
                    cols,
                    placeholders.join(", ")
                )
            }
            BulkOpKind::Update => {
                if self.columns.is_empty() {
                    return String::new();
                }
                let set_clause: Vec<String> = self
                    .columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| format!("{c} = :{}", i + 1))
                    .collect();
                let where_clause = self.build_where_clause(self.columns.len());
                format!(
                    "UPDATE {} SET {}{}",
                    self.table,
                    set_clause.join(", "),
                    where_clause
                )
            }
            BulkOpKind::Delete => {
                let where_clause = self.build_where_clause(0);
                format!("DELETE FROM {}{}", self.table, where_clause)
            }
            BulkOpKind::Merge => self.build_merge_sql(),
        }
    }

    /// 生成 WHERE 子句
    fn build_where_clause(&self, offset: usize) -> String {
        if self.where_columns.is_empty() {
            return String::new();
        }
        let conditions: Vec<String> = self
            .where_columns
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{c} = :{}", offset + i + 1))
            .collect();
        format!(" WHERE {}", conditions.join(" AND "))
    }

    /// 生成 MERGE 语句
    fn build_merge_sql(&self) -> String {
        if self.columns.is_empty() {
            return String::new();
        }
        let on_clause: Vec<String> = self
            .where_columns
            .iter()
            .map(|c| format!("t.{c} = s.{c}"))
            .collect();
        let set_clause: Vec<String> = self
            .columns
            .iter()
            .map(|c| format!("t.{c} = s.{c}"))
            .collect();
        let insert_cols = self.columns.join(", ");
        let insert_vals: Vec<String> = self.columns.iter().map(|c| format!("s.{c}")).collect();
        format!(
            "MERGE INTO {} t USING source s ON ({}) WHEN MATCHED THEN UPDATE SET {} WHEN NOT MATCHED THEN INSERT ({}) VALUES ({})",
            self.table,
            on_clause.join(" AND "),
            set_clause.join(", "),
            insert_cols,
            insert_vals.join(", ")
        )
    }

    /// 生成 FORALL 批量 PL/SQL 块
    #[must_use]
    pub fn build_forall_block(&self, row_count: usize) -> String {
        let single = self.build_single_sql();
        if single.is_empty() || row_count == 0 {
            return String::new();
        }
        let forall = match self.kind {
            BulkOpKind::Insert => format!("FORALL i IN 1..{} EXECUTE IMMEDIATE", row_count),
            BulkOpKind::Update => format!("FORALL i IN 1..{} EXECUTE IMMEDIATE", row_count),
            BulkOpKind::Delete => format!("FORALL i IN 1..{} EXECUTE IMMEDIATE", row_count),
            BulkOpKind::Merge => format!("FOR i IN 1..{} LOOP EXECUTE IMMEDIATE", row_count),
        };
        format!(
            "BEGIN\n  {} '{}';\nEND;",
            forall,
            single.replace('\'', "''")
        )
    }

    /// 获取操作类型
    #[must_use]
    pub fn kind(&self) -> BulkOpKind {
        self.kind
    }

    /// 获取表名
    #[must_use]
    pub fn table(&self) -> &str {
        &self.table
    }

    /// 获取列数
    #[must_use]
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// 估算批次数
    #[must_use]
    pub fn estimated_batches(&self, total_rows: usize) -> usize {
        self.config.batch_count(total_rows)
    }

    /// 估算内存占用（字节）
    ///
    /// 每行每列约 32 字节（含绑定开销）。
    #[must_use]
    pub fn estimated_memory_bytes(&self, total_rows: usize) -> u64 {
        const PER_CELL_BYTES: u64 = 32;
        total_rows as u64 * self.columns.len() as u64 * PER_CELL_BYTES
    }
}

impl fmt::Display for BulkOperations {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.build_single_sql())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bulk_op_kind_keyword() {
        assert_eq!(BulkOpKind::Insert.as_keyword(), "INSERT");
        assert_eq!(BulkOpKind::Update.as_keyword(), "UPDATE");
        assert_eq!(BulkOpKind::Delete.as_keyword(), "DELETE");
        assert_eq!(BulkOpKind::Merge.as_keyword(), "MERGE");
    }

    #[test]
    fn test_bulk_config_default() {
        let cfg = BulkConfig::default();
        assert_eq!(cfg.batch_size, 1000);
        assert!(cfg.with_row_counts);
    }

    #[test]
    fn test_bulk_config_builder() {
        let cfg = BulkConfig::new()
            .with_batch_size(500)
            .with_row_counts()
            .with_error_mode(BulkErrorMode::SkipErrors)
            .with_auto_commit();
        assert_eq!(cfg.batch_size, 500);
        assert_eq!(cfg.error_mode, BulkErrorMode::SkipErrors);
        assert!(cfg.auto_commit_per_batch);
    }

    #[test]
    fn test_bulk_config_batch_size_min_one() {
        let cfg = BulkConfig::new().with_batch_size(0);
        assert_eq!(cfg.batch_size, 1);
    }

    #[test]
    fn test_bulk_config_batch_count() {
        let cfg = BulkConfig::new().with_batch_size(100);
        assert_eq!(cfg.batch_count(0), 0);
        assert_eq!(cfg.batch_count(1), 1);
        assert_eq!(cfg.batch_count(100), 1);
        assert_eq!(cfg.batch_count(101), 2);
        assert_eq!(cfg.batch_count(250), 3);
    }

    #[test]
    fn test_bulk_error_mode_description() {
        assert_eq!(
            BulkErrorMode::StopOnFirst.description(),
            "stop on first error"
        );
        assert_eq!(
            BulkErrorMode::CollectAll.description(),
            "collect all errors"
        );
        assert_eq!(BulkErrorMode::SkipErrors.description(), "skip error rows");
    }

    #[test]
    fn test_bulk_result_default() {
        let r = BulkResult::default();
        assert_eq!(r.affected_rows, 0);
        assert!(!r.has_errors());
    }

    #[test]
    fn test_bulk_result_has_errors() {
        let r = BulkResult {
            error_rows: vec![1, 2],
            ..BulkResult::default()
        };
        assert!(r.has_errors());
    }

    #[test]
    fn test_bulk_result_error_rate() {
        let r = BulkResult {
            total_input_rows: 100,
            error_rows: vec![1, 2, 3],
            ..BulkResult::default()
        };
        let rate = r.error_rate();
        assert!((rate - 0.03).abs() < 1e-10);
    }

    #[test]
    fn test_bulk_result_success_rows() {
        let r = BulkResult {
            total_input_rows: 100,
            error_rows: vec![1, 2, 3],
            ..BulkResult::default()
        };
        assert_eq!(r.success_rows(), 97);
    }

    #[test]
    fn test_bulk_result_avg_batch_size() {
        let r = BulkResult {
            total_input_rows: 500,
            batches_processed: 5,
            ..BulkResult::default()
        };
        assert!((r.avg_batch_size() - 100.0).abs() < 1e-10);
    }

    #[test]
    fn test_bulk_result_merge() {
        let r1 = BulkResult {
            affected_rows: 10,
            total_input_rows: 10,
            batches_processed: 1,
            ..BulkResult::default()
        };
        let r2 = BulkResult {
            affected_rows: 20,
            total_input_rows: 20,
            batches_processed: 2,
            ..BulkResult::default()
        };
        let merged = r1.merge(&r2);
        assert_eq!(merged.affected_rows, 30);
        assert_eq!(merged.total_input_rows, 30);
        assert_eq!(merged.batches_processed, 3);
    }

    #[test]
    fn test_bulk_result_display() {
        let r = BulkResult {
            affected_rows: 100,
            batches_processed: 2,
            error_rows: vec![1],
            ..BulkResult::default()
        };
        let s = format!("{}", r);
        assert!(s.contains("affected=100"));
        assert!(s.contains("batches=2"));
    }

    #[test]
    fn test_bulk_insert_sql() {
        let op = BulkOperations::insert("users").columns(&["id", "name", "email"]);
        let sql = op.build_single_sql();
        assert!(sql.contains("INSERT INTO users (id, name, email) VALUES (:1, :2, :3)"));
    }

    #[test]
    fn test_bulk_update_sql() {
        let op = BulkOperations::update("users")
            .columns(&["name", "email"])
            .where_columns(&["id"]);
        let sql = op.build_single_sql();
        assert!(sql.contains("UPDATE users SET name = :1, email = :2 WHERE id = :3"));
    }

    #[test]
    fn test_bulk_delete_sql() {
        let op = BulkOperations::delete("users").where_columns(&["id"]);
        let sql = op.build_single_sql();
        assert!(sql.contains("DELETE FROM users WHERE id = :1"));
    }

    #[test]
    fn test_bulk_insert_no_columns() {
        let op = BulkOperations::insert("users");
        let sql = op.build_single_sql();
        assert!(sql.is_empty());
    }

    #[test]
    fn test_bulk_forall_block() {
        let op = BulkOperations::insert("users").columns(&["id", "name"]);
        let block = op.build_forall_block(100);
        assert!(block.contains("FORALL i IN 1..100"));
        assert!(block.contains("INSERT INTO users"));
    }

    #[test]
    fn test_bulk_forall_block_empty() {
        let op = BulkOperations::insert("users").columns(&["id"]);
        let block = op.build_forall_block(0);
        assert!(block.is_empty());
    }

    #[test]
    fn test_bulk_operations_kind() {
        let op = BulkOperations::insert("users");
        assert_eq!(op.kind(), BulkOpKind::Insert);
    }

    #[test]
    fn test_bulk_operations_table() {
        let op = BulkOperations::insert("users");
        assert_eq!(op.table(), "users");
    }

    #[test]
    fn test_bulk_operations_column_count() {
        let op = BulkOperations::insert("users").columns(&["a", "b", "c"]);
        assert_eq!(op.column_count(), 3);
    }

    #[test]
    fn test_bulk_operations_estimated_batches() {
        let op = BulkOperations::insert("users").with_batch_size(100);
        assert_eq!(op.estimated_batches(250), 3);
    }

    #[test]
    fn test_bulk_operations_estimated_memory() {
        let op = BulkOperations::insert("users").columns(&["a", "b"]);
        let mem = op.estimated_memory_bytes(1000);
        assert_eq!(mem, 1000 * 2 * 32);
    }

    #[test]
    fn test_bulk_operations_display() {
        let op = BulkOperations::insert("users").columns(&["id"]);
        let s = format!("{}", op);
        assert!(s.contains("INSERT INTO users"));
    }

    #[test]
    fn test_bulk_merge_sql() {
        let op = BulkOperations {
            table: "users".to_string(),
            kind: BulkOpKind::Merge,
            columns: vec!["name".to_string(), "email".to_string()],
            where_columns: vec!["id".to_string()],
            config: BulkConfig::default(),
        };
        let sql = op.build_single_sql();
        assert!(sql.contains("MERGE INTO users t USING source s"));
        assert!(sql.contains("WHEN MATCHED THEN UPDATE SET"));
        assert!(sql.contains("WHEN NOT MATCHED THEN INSERT"));
    }
}
