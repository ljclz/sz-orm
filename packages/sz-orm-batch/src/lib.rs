//! # SZ-ORM Batch — Batch Operations
//!
//! Provides batch insert, update, and UPSERT capabilities, supporting
//! multi-value INSERT, CASE WHEN UPDATE, and shard-aware batch execution,
//! and returns the generated SQL for auditing.
//!
//! ## Main Types
//!
//! - [`BatchResult`] — Batch operation result
//! - [`BatchOperations`] trait — Batch operation interface

#[cfg(feature = "batch-stream")]
pub mod stream;

#[cfg(feature = "batch-v2")]
pub mod copy;
#[cfg(feature = "batch-v2")]
pub mod delete;
#[cfg(feature = "batch-v2")]
pub mod dialect;
#[cfg(feature = "batch-v2")]
pub mod executor;

#[cfg(feature = "batch-atomic")]
pub mod atomic;

#[cfg(feature = "copy-parallel-shard")]
pub mod copy_parallel_shard;

#[cfg(feature = "batch-v2")]
pub use copy::CopyProtocolExecutor;
#[cfg(feature = "batch-v2")]
pub use delete::{batch_delete, BatchDeleteError, BatchDeleteRequest, BatchDeleteResult};
#[cfg(feature = "batch-v2")]
pub use dialect::BatchDialect;
#[cfg(feature = "batch-v2")]
pub use executor::{
    BatchExecutionResult, BatchExecutor, BatchExecutorConfig, BatchExecutorError,
    ChunkExecutionDetail,
};

#[cfg(feature = "copy-parallel-shard")]
pub use copy_parallel_shard::{
    ConflictResolution, CopyBatchResult, CopyDialect, CopyParallelShardError, CopyProtocolAdapter,
    ParallelShardExecutor, ShardConfig, ShardResult, ShardStrategy,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Batch operation result. `generated_sqls` holds the actual generated SQL
/// statements for the caller to execute and audit.
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

/// Upsert syntax mode: MySQL style (ON DUPLICATE KEY UPDATE) or PostgreSQL
/// style (ON CONFLICT DO UPDATE).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertMode {
    MysqlOnDuplicate,
    PostgresOnConflict,
    #[cfg(feature = "batch-v2")]
    SqliteOnConflict,
    #[cfg(feature = "batch-v2")]
    OracleMerge,
    #[cfg(feature = "batch-v2")]
    MssqlMerge,
}

/// Default batch operation implementation. Generates multi-value INSERT,
/// CASE WHEN UPDATE, and ON CONFLICT/ON DUPLICATE UPSERT.
///
/// L-5 fix: added example documentation
///
/// # Example
///
/// ```ignore
/// use sz_orm_batch::{DefaultBatchOps, UpsertMode};
/// use serde_json::json;
///
/// // Create default config (primary key "id", MySQL ON DUPLICATE mode, chunk size 1000)
/// let ops = DefaultBatchOps::new();
///
/// // Customize primary key and chunk size
/// let ops = DefaultBatchOps::with_primary_key("user_id")
///     .with_chunk_size(500)
///     .with_upsert_mode(UpsertMode::PostgresOnConflict);
///
/// let rows = vec![
///     json!({ "user_id": 1, "name": "Alice" }),
///     json!({ "user_id": 2, "name": "Bob" }),
/// ];
///
/// // Generate batch insert SQL (actual invocation requires the BatchOperations trait)
/// // let sql = ops.batch_insert("users", &rows).unwrap();
/// ```
#[derive(Clone)]
pub struct DefaultBatchOps {
    pub primary_key: String,
    pub upsert_mode: UpsertMode,
    /// H-9 fix: batch insert chunk size
    ///
    /// When `rows.len() > chunk_size`, `batch_insert` / `batch_upsert` splits
    /// the data into chunks of `chunk_size`, each producing an independent SQL
    /// statement. This avoids triggering database parameter limits on very
    /// large batch inserts (e.g. MySQL `max_allowed_packet`, PostgreSQL
    /// placeholder limit of 65535).
    ///
    /// Defaults to `DEFAULT_CHUNK_SIZE` (1000). Setting to 0 is equivalent to 1
    /// (one SQL per row).
    pub chunk_size: usize,
    /// Rollback strategy (default None)
    pub rollback_strategy: RollbackStrategy,
    /// Progress callback (default None)
    pub progress_callback: Option<ProgressCallback>,
    /// UPSERT conflict target (default None, uses the primary key)
    pub conflict_target: Option<ConflictTarget>,
}

impl std::fmt::Debug for DefaultBatchOps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultBatchOps")
            .field("primary_key", &self.primary_key)
            .field("upsert_mode", &self.upsert_mode)
            .field("chunk_size", &self.chunk_size)
            .field("rollback_strategy", &self.rollback_strategy)
            .field(
                "progress_callback",
                &self.progress_callback.as_ref().map(|_| "<fn>"),
            )
            .field("conflict_target", &self.conflict_target)
            .finish()
    }
}

/// H-9 default chunk size
pub const DEFAULT_CHUNK_SIZE: usize = 1000;

impl Default for DefaultBatchOps {
    fn default() -> Self {
        Self {
            primary_key: "id".to_string(),
            upsert_mode: UpsertMode::MysqlOnDuplicate,
            chunk_size: DEFAULT_CHUNK_SIZE,
            rollback_strategy: RollbackStrategy::None,
            progress_callback: None,
            conflict_target: None,
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
            rollback_strategy: RollbackStrategy::None,
            progress_callback: None,
            conflict_target: None,
        }
    }

    pub fn with_upsert_mode(mut self, mode: UpsertMode) -> Self {
        self.upsert_mode = mode;
        self
    }

    /// H-9 fix: set the batch insert chunk size
    pub fn with_chunk_size(mut self, chunk_size: usize) -> Self {
        self.chunk_size = chunk_size.max(1);
        self
    }

    /// H-9 fix: split a slice into chunks of `chunk_size`
    ///
    /// Returns an index iterator where each element is a (start, end)
    /// half-open range.
    fn chunk_indices(&self, total: usize) -> impl Iterator<Item = (usize, usize)> {
        let chunk_size = self.chunk_size.max(1);
        (0..total).step_by(chunk_size).map(move |start| {
            let end = (start + chunk_size).min(total);
            (start, end)
        })
    }

    /// Wrap an identifier in backticks (MySQL style), escaping inner backticks
    /// as double backticks.
    ///
    /// v1.2.1 fix for High H-3 (CWE-89 SQL injection): the original
    /// implementation did not escape backticks in column names. When the JSON
    /// data source is untrusted (e.g. directly accepting an API request body),
    /// an attacker could inject SQL via JSON keys. MySQL backtick escaping
    /// rule: ` -> `` (double backtick).
    pub(crate) fn quote(name: &str) -> String {
        let escaped = name.replace('`', "``");
        format!("`{}`", escaped)
    }

    /// Extract field names from a JSON object.
    ///
    /// Column order depends on the serde_json feature configuration:
    /// - Default (no `preserve_order`): uses BTreeMap, lexicographic order
    /// - With `preserve_order` enabled: uses IndexMap, insertion order
    ///
    /// Under workspace `--all-features` compilation, other crates may enable
    /// `preserve_order`, which propagates to this crate via feature unification
    /// and changes the column order. Callers should not assume any particular
    /// column order.
    fn extract_columns(row: &Value) -> Option<Vec<String>> {
        match row {
            Value::Object(map) => Some(map.keys().map(|k| k.to_string()).collect()),
            _ => None,
        }
    }

    /// Return the non-primary-key columns.
    fn non_pk_columns(&self, columns: &[String]) -> Vec<String> {
        columns
            .iter()
            .filter(|c| **c != self.primary_key)
            .cloned()
            .collect()
    }

    /// Check whether `row` has all the specified columns.
    fn row_has_all_columns(row: &Value, columns: &[String]) -> bool {
        match row {
            Value::Object(map) => columns.iter().all(|c| map.contains_key(c)),
            _ => false,
        }
    }

    /// Generate a single-row placeholder: "(?, ?, ?)".
    fn placeholder_row(col_count: usize) -> String {
        let placeholders = vec!["?"; col_count].join(", ");
        format!("({})", placeholders)
    }

    /// Extract column definitions and validate the first row; on failure
    /// returns None and counts all rows as failed.
    fn validate_and_extract(&self, rows: &[Value]) -> Option<Vec<String>> {
        let first = rows.first()?;
        match Self::extract_columns(first) {
            Some(c) if !c.is_empty() => Some(c),
            _ => None,
        }
    }

    /// Filter out valid rows that have all fields, returning
    /// (valid_refs, failed_count).
    fn filter_valid_rows<'a>(&self, rows: &'a [Value], columns: &[String]) -> Vec<&'a Value> {
        rows.iter()
            .filter(|r| Self::row_has_all_columns(r, columns))
            .collect()
    }

    /// Shared: generate the INSERT header and multi-value placeholder part.
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

// ============================================================================
// 深度扩展：批量进度回调、回滚策略、UPSERT 冲突目标、分块处理编排
// ============================================================================

/// Batch operation stage, used for progress callback reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatchStage {
    /// Batch operation started
    Started,
    /// Processing a single chunk
    ProcessingChunk,
    /// Single chunk processing completed
    ChunkCompleted,
    /// All chunks processing completed
    Finished,
}

/// Batch operation progress information, passed to the progress callback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchProgress {
    /// Current chunk index (starting from 0)
    pub chunk_index: usize,
    /// Total number of chunks
    pub total_chunks: usize,
    /// Number of rows in the current chunk
    pub chunk_rows: usize,
    /// Number of rows processed
    pub processed_rows: usize,
    /// Total number of rows
    pub total_rows: usize,
    /// Current stage
    pub stage: BatchStage,
}

impl BatchProgress {
    /// Completion percentage (0.0 ~ 100.0)
    pub fn percent(&self) -> f64 {
        if self.total_rows == 0 {
            return 100.0;
        }
        (self.processed_rows as f64 / self.total_rows as f64) * 100.0
    }

    /// Whether the operation is finished
    pub fn is_finished(&self) -> bool {
        self.stage == BatchStage::Finished
    }
}

/// Progress callback function type (thread-safe, shareable).
pub type ProgressCallback = std::sync::Arc<dyn Fn(BatchProgress) + Send + Sync>;

/// Batch operation rollback strategy.
///
/// Controls the behavior when a chunk fails:
/// - `None`: the failed chunk is counted in `failed`; successful chunks are
///   unaffected
/// - `Savepoint`: a `SAVEPOINT` statement is generated before each chunk; on
///   failure, rollback to the savepoint
/// - `PerChunk`: if any chunk fails, the entire batch is aborted and no
///   subsequent chunks are executed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RollbackStrategy {
    /// No rollback (default)
    #[default]
    None,
    /// Savepoint rollback
    Savepoint,
    /// Abort the entire batch
    PerChunk,
}

/// UPSERT conflict target (the target of the ON CONFLICT clause).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictTarget {
    /// Conflict detection by column names: `ON CONFLICT (col1, col2)`
    Columns(Vec<String>),
    /// Conflict detection by constraint name: `ON CONSTRAINT constraint_name`
    Constraint(String),
}

/// UPSERT result with a conflict target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertResult {
    /// Base batch result
    pub base: BatchResult,
    /// Conflict target used
    pub conflict_target: Option<ConflictTarget>,
    /// Generated SAVEPOINT / ROLLBACK TO SQL (when using the Savepoint strategy)
    pub transaction_sqls: Vec<String>,
}

impl UpsertResult {
    pub fn new(base: BatchResult) -> Self {
        Self {
            base,
            conflict_target: None,
            transaction_sqls: Vec::new(),
        }
    }
}

/// Chunk processing result, recording the processing of each chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkProcessResult {
    /// Chunk index
    pub chunk_index: usize,
    /// Number of rows in the chunk
    pub chunk_rows: usize,
    /// Whether it succeeded
    pub success: bool,
    /// Generated SQL
    pub sql: Option<String>,
    /// Error message (on failure)
    pub error: Option<String>,
}

impl DefaultBatchOps {
    /// Set the rollback strategy (returns a new config instance).
    ///
    /// Note: `RollbackStrategy` only affects option-aware methods such as
    /// [`batch_upsert_with_options`].
    pub fn with_rollback_strategy(mut self, strategy: RollbackStrategy) -> Self {
        self.rollback_strategy = strategy;
        self
    }

    /// Set the progress callback (returns a new config instance).
    pub fn with_progress_callback(mut self, callback: ProgressCallback) -> Self {
        self.progress_callback = Some(callback);
        self
    }

    /// Set the UPSERT conflict target (returns a new config instance).
    ///
    /// Only effective with PostgreSQL `ON CONFLICT` mode. Once set,
    /// `batch_upsert` will use the specified conflict target instead of the
    /// default primary key column.
    pub fn with_conflict_target(mut self, target: ConflictTarget) -> Self {
        self.conflict_target = Some(target);
        self
    }

    /// Generate a PostgreSQL ON CONFLICT clause (with conflict target).
    fn build_pg_conflict_clause(&self, non_pk: &[String], conflict: &ConflictTarget) -> String {
        if non_pk.is_empty() {
            return String::new();
        }
        let updates: Vec<String> = non_pk
            .iter()
            .map(|col| {
                let q = Self::quote(col);
                format!("{} = EXCLUDED.{}", q, q)
            })
            .collect();
        let target = match conflict {
            ConflictTarget::Columns(cols) => {
                let quoted: Vec<String> = cols.iter().map(|c| Self::quote(c)).collect();
                format!("({})", quoted.join(", "))
            }
            ConflictTarget::Constraint(name) => format!("ON CONSTRAINT {}", name),
        };
        format!(
            " ON CONFLICT {} DO UPDATE SET {}",
            target,
            updates.join(", ")
        )
    }

    /// Generate a SAVEPOINT statement.
    fn savepoint_sql(index: usize) -> String {
        format!("SAVEPOINT batch_chunk_{}", index)
    }

    /// Generate a ROLLBACK TO SAVEPOINT statement.
    fn rollback_to_sql(index: usize) -> String {
        format!("ROLLBACK TO SAVEPOINT batch_chunk_{}", index)
    }

    /// Generate a RELEASE SAVEPOINT statement.
    fn release_savepoint_sql(index: usize) -> String {
        format!("RELEASE SAVEPOINT batch_chunk_{}", index)
    }

    /// Execute chunk processing: invoke the closure on each chunk to generate
    /// SQL and collect the results.
    ///
    /// Based on the rollback strategy, generates additional transaction
    /// control SQL (SAVEPOINT / ROLLBACK TO). Based on the progress callback,
    /// triggers progress notifications before and after each chunk.
    pub fn chunk_process<F>(&self, rows: &[Value], mut sql_builder: F) -> Vec<ChunkProcessResult>
    where
        F: FnMut(&[&Value]) -> Result<String, String>,
    {
        let mut results = Vec::new();
        if rows.is_empty() {
            return results;
        }

        let total = rows.len();
        let chunk_size = self.chunk_size.max(1);
        let total_chunks = total.div_ceil(chunk_size);

        // 触发 Started 进度
        if let Some(ref cb) = self.progress_callback {
            cb(BatchProgress {
                chunk_index: 0,
                total_chunks,
                chunk_rows: 0,
                processed_rows: 0,
                total_rows: total,
                stage: BatchStage::Started,
            });
        }

        let mut processed = 0usize;
        for (chunk_idx, (start, end)) in self.chunk_indices(total).enumerate() {
            let chunk: Vec<&Value> = rows[start..end].iter().collect();
            let chunk_rows = chunk.len();

            // 触发 ProcessingChunk 进度
            if let Some(ref cb) = self.progress_callback {
                cb(BatchProgress {
                    chunk_index: chunk_idx,
                    total_chunks,
                    chunk_rows,
                    processed_rows: processed,
                    total_rows: total,
                    stage: BatchStage::ProcessingChunk,
                });
            }

            let result = match sql_builder(&chunk) {
                Ok(sql) => ChunkProcessResult {
                    chunk_index: chunk_idx,
                    chunk_rows,
                    success: true,
                    sql: Some(sql),
                    error: None,
                },
                Err(err) => ChunkProcessResult {
                    chunk_index: chunk_idx,
                    chunk_rows,
                    success: false,
                    sql: None,
                    error: Some(err),
                },
            };

            let success = result.success;
            results.push(result);
            processed += chunk_rows;

            // 触发 ChunkCompleted 进度
            if let Some(ref cb) = self.progress_callback {
                cb(BatchProgress {
                    chunk_index: chunk_idx,
                    total_chunks,
                    chunk_rows,
                    processed_rows: processed,
                    total_rows: total,
                    stage: BatchStage::ChunkCompleted,
                });
            }

            // PerChunk 策略：失败即中止
            if !success && self.rollback_strategy == RollbackStrategy::PerChunk {
                break;
            }
        }

        // 触发 Finished 进度
        if let Some(ref cb) = self.progress_callback {
            cb(BatchProgress {
                chunk_index: results.len(),
                total_chunks,
                chunk_rows: 0,
                processed_rows: processed,
                total_rows: total,
                stage: BatchStage::Finished,
            });
        }

        results
    }

    /// Batch UPSERT with options: supports conflict target, rollback strategy,
    /// and progress callback.
    ///
    /// Returns an `UpsertResult` containing the base batch result, the conflict
    /// target, and the transaction control SQL.
    pub fn batch_upsert_with_options(&self, table: &str, rows: Vec<Value>) -> UpsertResult {
        let mut result = UpsertResult::new(BatchResult::new());
        if rows.is_empty() {
            return result;
        }

        let columns = match self.validate_and_extract(&rows) {
            Some(c) => c,
            None => {
                result.base.failed = rows.len();
                return result;
            }
        };

        if !columns.contains(&self.primary_key) {
            result.base.failed = rows.len();
            return result;
        }

        let valid_rows = self.filter_valid_rows(&rows, &columns);
        result.base.failed = rows.len() - valid_rows.len();
        if valid_rows.is_empty() {
            return result;
        }

        let non_pk = self.non_pk_columns(&columns);
        result.conflict_target = self.conflict_target.clone();

        let total = valid_rows.len();
        let chunk_size = self.chunk_size.max(1);
        let total_chunks = total.div_ceil(chunk_size);
        let mut processed = 0usize;

        // Started 进度
        if let Some(ref cb) = self.progress_callback {
            cb(BatchProgress {
                chunk_index: 0,
                total_chunks,
                chunk_rows: 0,
                processed_rows: 0,
                total_rows: total,
                stage: BatchStage::Started,
            });
        }

        for (chunk_idx, (start, end)) in self.chunk_indices(total).enumerate() {
            let chunk = &valid_rows[start..end];

            // Savepoint 策略：生成 SAVEPOINT
            if self.rollback_strategy == RollbackStrategy::Savepoint {
                result.transaction_sqls.push(Self::savepoint_sql(chunk_idx));
            }

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
                    match &self.conflict_target {
                        Some(target) => self.build_pg_conflict_clause(&non_pk, target),
                        None => {
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
                    }
                }
                _ => String::new(),
            };

            let sql = format!("{}{}", insert_part, conflict_part);
            result.base.generated_sqls.push(sql);
            result.base.inserted += chunk.len();
            processed += chunk.len();

            // Savepoint 策略：生成 RELEASE
            if self.rollback_strategy == RollbackStrategy::Savepoint {
                result
                    .transaction_sqls
                    .push(Self::release_savepoint_sql(chunk_idx));
            }

            // ProcessingChunk 进度
            if let Some(ref cb) = self.progress_callback {
                cb(BatchProgress {
                    chunk_index: chunk_idx,
                    total_chunks,
                    chunk_rows: chunk.len(),
                    processed_rows: processed,
                    total_rows: total,
                    stage: BatchStage::ProcessingChunk,
                });
            }
        }

        // Finished 进度
        if let Some(ref cb) = self.progress_callback {
            cb(BatchProgress {
                chunk_index: total_chunks,
                total_chunks,
                chunk_rows: 0,
                processed_rows: processed,
                total_rows: total,
                stage: BatchStage::Finished,
            });
        }

        result
    }

    /// Generate the ROLLBACK TO SQL for a failed chunk under the PerChunk
    /// rollback strategy.
    ///
    /// When the caller executes chunk SQL and a chunk fails, call this method
    /// to obtain the rollback SQL.
    pub fn rollback_sql_for_chunk(&self, chunk_index: usize) -> String {
        Self::rollback_to_sql(chunk_index)
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

    // ==================== 深度扩展：进度回调、回滚策略、冲突目标、分块处理测试 ====================

    #[test]
    fn test_batch_stage_variants() {
        // 验证 BatchStage 各变体可序列化/反序列化（向后兼容）
        let stages = vec![
            BatchStage::Started,
            BatchStage::ProcessingChunk,
            BatchStage::ChunkCompleted,
            BatchStage::Finished,
        ];
        for stage in &stages {
            let json = serde_json::to_string(stage).unwrap();
            let back: BatchStage = serde_json::from_str(&json).unwrap();
            assert_eq!(*stage, back);
        }
    }

    #[test]
    fn test_batch_progress_percent_zero_total() {
        // total_rows = 0 → percent = 100.0（视为已完成）
        let p = BatchProgress {
            chunk_index: 0,
            total_chunks: 0,
            chunk_rows: 0,
            processed_rows: 0,
            total_rows: 0,
            stage: BatchStage::Finished,
        };
        assert!((p.percent() - 100.0).abs() < 1e-6);
        assert!(p.is_finished());
    }

    #[test]
    fn test_batch_progress_percent_half() {
        let p = BatchProgress {
            chunk_index: 1,
            total_chunks: 2,
            chunk_rows: 5,
            processed_rows: 5,
            total_rows: 10,
            stage: BatchStage::ChunkCompleted,
        };
        assert!((p.percent() - 50.0).abs() < 1e-6);
        assert!(!p.is_finished());
    }

    #[test]
    fn test_batch_progress_percent_full() {
        let p = BatchProgress {
            chunk_index: 2,
            total_chunks: 2,
            chunk_rows: 0,
            processed_rows: 10,
            total_rows: 10,
            stage: BatchStage::Finished,
        };
        assert!((p.percent() - 100.0).abs() < 1e-6);
        assert!(p.is_finished());
    }

    #[test]
    fn test_rollback_strategy_default_is_none() {
        assert_eq!(RollbackStrategy::default(), RollbackStrategy::None);
    }

    #[test]
    fn test_conflict_target_columns_equality() {
        let a = ConflictTarget::Columns(vec!["id".to_string(), "name".to_string()]);
        let b = ConflictTarget::Columns(vec!["id".to_string(), "name".to_string()]);
        assert_eq!(a, b);

        let c = ConflictTarget::Columns(vec!["id".to_string()]);
        assert_ne!(a, c);
    }

    #[test]
    fn test_conflict_target_constraint_equality() {
        let a = ConflictTarget::Constraint("users_pkey".to_string());
        let b = ConflictTarget::Constraint("users_pkey".to_string());
        assert_eq!(a, b);

        let c = ConflictTarget::Constraint("other".to_string());
        assert_ne!(a, c);
    }

    #[test]
    fn test_upsert_result_new_empty() {
        let r = UpsertResult::new(BatchResult::new());
        assert_eq!(r.base.inserted, 0);
        assert!(r.conflict_target.is_none());
        assert!(r.transaction_sqls.is_empty());
    }

    #[test]
    fn test_with_rollback_strategy_builder() {
        let ops = DefaultBatchOps::new().with_rollback_strategy(RollbackStrategy::Savepoint);
        assert_eq!(ops.rollback_strategy, RollbackStrategy::Savepoint);

        let ops2 = DefaultBatchOps::new().with_rollback_strategy(RollbackStrategy::PerChunk);
        assert_eq!(ops2.rollback_strategy, RollbackStrategy::PerChunk);
    }

    #[test]
    fn test_with_conflict_target_builder_columns() {
        let target = ConflictTarget::Columns(vec!["email".to_string()]);
        let ops = DefaultBatchOps::new().with_conflict_target(target);
        match &ops.conflict_target {
            Some(ConflictTarget::Columns(c)) => assert_eq!(c, &["email".to_string()]),
            other => panic!("expected Columns, got {:?}", other),
        }
    }

    #[test]
    fn test_with_conflict_target_builder_constraint() {
        let target = ConflictTarget::Constraint("uniq_email".to_string());
        let ops = DefaultBatchOps::new().with_conflict_target(target);
        match &ops.conflict_target {
            Some(ConflictTarget::Constraint(n)) => assert_eq!(n, "uniq_email"),
            other => panic!("expected Constraint, got {:?}", other),
        }
    }

    #[test]
    fn test_with_progress_callback_invoked() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        let callback: ProgressCallback = Arc::new(move |_p| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        let ops = DefaultBatchOps::new()
            .with_chunk_size(2)
            .with_progress_callback(callback);

        let rows: Vec<Value> = (1..=5)
            .map(|i| json!({"id": i, "name": format!("user{}", i)}))
            .collect();
        let results = ops.chunk_process(&rows, |chunk| {
            Ok(format!("-- chunk of {} rows", chunk.len()))
        });

        assert_eq!(results.len(), 3); // 2+2+1
                                      // Started + 3 * (ProcessingChunk + ChunkCompleted) + Finished = 1 + 6 + 1 = 8
        assert_eq!(counter.load(Ordering::SeqCst), 8);
    }

    #[test]
    fn test_chunk_process_empty_rows() {
        let ops = DefaultBatchOps::new();
        let results = ops.chunk_process(&[], |_| Ok("sql".to_string()));
        assert!(results.is_empty());
    }

    #[test]
    fn test_chunk_process_basic_success() {
        let ops = DefaultBatchOps::new().with_chunk_size(2);
        let rows: Vec<Value> = (1..=4)
            .map(|i| json!({"id": i, "name": format!("user{}", i)}))
            .collect();
        let results = ops.chunk_process(&rows, |chunk| {
            Ok(format!("INSERT ... {} rows", chunk.len()))
        });

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.success));
        assert_eq!(results[0].chunk_rows, 2);
        assert_eq!(results[1].chunk_rows, 2);
        assert!(results[0].sql.as_ref().unwrap().contains("2 rows"));
    }

    #[test]
    fn test_chunk_process_per_chunk_aborts_on_failure() {
        let ops = DefaultBatchOps::new()
            .with_chunk_size(1)
            .with_rollback_strategy(RollbackStrategy::PerChunk);

        let rows: Vec<Value> = (1..=5)
            .map(|i| json!({"id": i, "name": format!("user{}", i)}))
            .collect();

        let call_count = std::cell::Cell::new(0usize);
        let results = ops.chunk_process(&rows, |chunk| {
            let n = call_count.get();
            call_count.set(n + 1);
            // 第 2 块（index=1）失败
            if n == 1 {
                Err("simulated failure".to_string())
            } else {
                Ok(format!("ok for {}", chunk.len()))
            }
        });

        // PerChunk 策略：失败后中止，应只处理 2 块
        assert_eq!(results.len(), 2);
        assert!(results[0].success);
        assert!(!results[1].success);
        assert_eq!(results[1].error.as_deref(), Some("simulated failure"));
    }

    #[test]
    fn test_chunk_process_none_strategy_continues_on_failure() {
        let ops = DefaultBatchOps::new()
            .with_chunk_size(1)
            .with_rollback_strategy(RollbackStrategy::None);

        let rows: Vec<Value> = (1..=3)
            .map(|i| json!({"id": i, "name": format!("user{}", i)}))
            .collect();

        let call_count = std::cell::Cell::new(0usize);
        let results = ops.chunk_process(&rows, |_chunk| {
            let n = call_count.get();
            call_count.set(n + 1);
            if n == 1 {
                Err("fail".to_string())
            } else {
                Ok("ok".to_string())
            }
        });

        // None 策略：失败不中止，应处理全部 3 块
        assert_eq!(results.len(), 3);
        assert!(results[0].success);
        assert!(!results[1].success);
        assert!(results[2].success);
    }

    #[test]
    fn test_savepoint_sql_format() {
        assert_eq!(DefaultBatchOps::savepoint_sql(0), "SAVEPOINT batch_chunk_0");
        assert_eq!(
            DefaultBatchOps::savepoint_sql(42),
            "SAVEPOINT batch_chunk_42"
        );
    }

    #[test]
    fn test_rollback_to_sql_format() {
        assert_eq!(
            DefaultBatchOps::rollback_to_sql(0),
            "ROLLBACK TO SAVEPOINT batch_chunk_0"
        );
        assert_eq!(
            DefaultBatchOps::rollback_to_sql(7),
            "ROLLBACK TO SAVEPOINT batch_chunk_7"
        );
    }

    #[test]
    fn test_release_savepoint_sql_format() {
        assert_eq!(
            DefaultBatchOps::release_savepoint_sql(0),
            "RELEASE SAVEPOINT batch_chunk_0"
        );
        assert_eq!(
            DefaultBatchOps::release_savepoint_sql(3),
            "RELEASE SAVEPOINT batch_chunk_3"
        );
    }

    #[test]
    fn test_rollback_sql_for_chunk() {
        let ops = DefaultBatchOps::new();
        assert_eq!(
            ops.rollback_sql_for_chunk(5),
            "ROLLBACK TO SAVEPOINT batch_chunk_5"
        );
    }

    #[test]
    fn test_batch_upsert_with_options_empty() {
        let ops = DefaultBatchOps::new();
        let r = ops.batch_upsert_with_options("users", vec![]);
        assert_eq!(r.base.inserted, 0);
        assert!(r.transaction_sqls.is_empty());
    }

    #[test]
    fn test_batch_upsert_with_options_invalid_first_row() {
        let ops = DefaultBatchOps::new();
        let r = ops.batch_upsert_with_options("users", vec![json!("not an object")]);
        assert_eq!(r.base.failed, 1);
        assert_eq!(r.base.inserted, 0);
    }

    #[test]
    fn test_batch_upsert_with_options_no_pk() {
        let ops = DefaultBatchOps::new();
        let r = ops.batch_upsert_with_options("users", vec![json!({"name": "Alice"})]);
        assert_eq!(r.base.failed, 1);
        assert_eq!(r.base.inserted, 0);
    }

    #[test]
    fn test_batch_upsert_with_options_mysql_basic() {
        let ops = DefaultBatchOps::new().with_chunk_size(2);
        let rows: Vec<Value> = (1..=3)
            .map(|i| json!({"id": i, "name": format!("user{}", i)}))
            .collect();
        let r = ops.batch_upsert_with_options("users", rows);

        assert_eq!(r.base.inserted, 3);
        assert_eq!(r.base.generated_sqls.len(), 2); // 2+1
        for sql in &r.base.generated_sqls {
            assert!(sql.contains("ON DUPLICATE KEY UPDATE"));
        }
        // 无冲突目标设置
        assert!(r.conflict_target.is_none());
        // 无 Savepoint 策略
        assert!(r.transaction_sqls.is_empty());
    }

    #[test]
    fn test_batch_upsert_with_options_pg_with_conflict_columns() {
        let ops = DefaultBatchOps::new()
            .with_chunk_size(2)
            .with_upsert_mode(UpsertMode::PostgresOnConflict)
            .with_conflict_target(ConflictTarget::Columns(vec!["email".to_string()]));

        let rows: Vec<Value> = (1..=3)
            .map(|i| json!({"id": i, "email": format!("u{}@x.com", i), "name": format!("user{}", i)}))
            .collect();
        let r = ops.batch_upsert_with_options("users", rows);

        assert_eq!(r.base.inserted, 3);
        // 应使用指定的冲突目标 email 而非默认 id
        for sql in &r.base.generated_sqls {
            assert!(sql.contains("ON CONFLICT (`email`) DO UPDATE SET"));
            assert!(!sql.contains("ON CONFLICT (`id`)"));
        }
        // conflict_target 应回填
        match &r.conflict_target {
            Some(ConflictTarget::Columns(c)) => assert_eq!(c, &["email".to_string()]),
            other => panic!("expected Columns, got {:?}", other),
        }
    }

    #[test]
    fn test_batch_upsert_with_options_pg_with_conflict_constraint() {
        let ops = DefaultBatchOps::new()
            .with_upsert_mode(UpsertMode::PostgresOnConflict)
            .with_conflict_target(ConflictTarget::Constraint("uniq_email".to_string()));

        let rows: Vec<Value> = vec![json!({"id": 1, "email": "a@b.com", "name": "Alice"})];
        let r = ops.batch_upsert_with_options("users", rows);

        assert_eq!(r.base.inserted, 1);
        let sql = &r.base.generated_sqls[0];
        assert!(sql.contains("ON CONSTRAINT uniq_email"));
        assert!(sql.contains("DO UPDATE SET"));
    }

    #[test]
    fn test_batch_upsert_with_options_savepoint_strategy() {
        let ops = DefaultBatchOps::new()
            .with_chunk_size(2)
            .with_rollback_strategy(RollbackStrategy::Savepoint);

        let rows: Vec<Value> = (1..=5)
            .map(|i| json!({"id": i, "name": format!("user{}", i)}))
            .collect();
        let r = ops.batch_upsert_with_options("users", rows);

        assert_eq!(r.base.inserted, 5);
        // 3 块 → 3 个 SAVEPOINT + 3 个 RELEASE = 6 条事务 SQL
        assert_eq!(r.transaction_sqls.len(), 6);
        assert_eq!(r.transaction_sqls[0], "SAVEPOINT batch_chunk_0");
        assert_eq!(r.transaction_sqls[1], "RELEASE SAVEPOINT batch_chunk_0");
        assert_eq!(r.transaction_sqls[2], "SAVEPOINT batch_chunk_1");
        assert_eq!(r.transaction_sqls[3], "RELEASE SAVEPOINT batch_chunk_1");
        assert_eq!(r.transaction_sqls[4], "SAVEPOINT batch_chunk_2");
        assert_eq!(r.transaction_sqls[5], "RELEASE SAVEPOINT batch_chunk_2");
    }

    #[test]
    fn test_batch_upsert_with_options_progress_callback() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        let callback: ProgressCallback = Arc::new(move |_p| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        let ops = DefaultBatchOps::new()
            .with_chunk_size(2)
            .with_progress_callback(callback);

        let rows: Vec<Value> = (1..=5)
            .map(|i| json!({"id": i, "name": format!("user{}", i)}))
            .collect();
        let _r = ops.batch_upsert_with_options("users", rows);

        // 3 块 → Started + 3*ProcessingChunk + Finished = 5
        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }

    #[test]
    fn test_batch_upsert_with_options_filters_invalid_rows() {
        let ops = DefaultBatchOps::new();
        let rows: Vec<Value> = vec![
            json!({"id": 1, "name": "Alice"}),
            json!("invalid"),
            json!({"name": "Bob"}), // 无 id
        ];
        let r = ops.batch_upsert_with_options("users", rows);
        assert_eq!(r.base.inserted, 1);
        assert_eq!(r.base.failed, 2);
    }

    #[test]
    fn test_chunk_process_result_serialization() {
        let r = ChunkProcessResult {
            chunk_index: 2,
            chunk_rows: 10,
            success: true,
            sql: Some("INSERT ...".to_string()),
            error: None,
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: ChunkProcessResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.chunk_index, 2);
        assert_eq!(back.chunk_rows, 10);
        assert!(back.success);
        assert_eq!(back.sql.as_deref(), Some("INSERT ..."));
    }

    #[test]
    fn test_upsert_result_serialization() {
        let r = UpsertResult {
            base: BatchResult {
                inserted: 5,
                updated: 0,
                failed: 1,
                generated_sqls: vec!["INSERT ...".to_string()],
            },
            conflict_target: Some(ConflictTarget::Columns(vec!["id".to_string()])),
            transaction_sqls: vec!["SAVEPOINT batch_chunk_0".to_string()],
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: UpsertResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.base.inserted, 5);
        assert!(back.conflict_target.is_some());
        assert_eq!(back.transaction_sqls.len(), 1);
    }

    #[test]
    fn test_batch_progress_serialization_roundtrip() {
        let p = BatchProgress {
            chunk_index: 1,
            total_chunks: 3,
            chunk_rows: 5,
            processed_rows: 10,
            total_rows: 20,
            stage: BatchStage::ProcessingChunk,
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: BatchProgress = serde_json::from_str(&json).unwrap();
        assert_eq!(back.chunk_index, 1);
        assert_eq!(back.total_chunks, 3);
        assert_eq!(back.processed_rows, 10);
        assert_eq!(back.stage, BatchStage::ProcessingChunk);
    }
}
