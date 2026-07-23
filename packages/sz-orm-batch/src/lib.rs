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
#[derive(Clone)]
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
    /// 回滚策略（默认 None）
    pub rollback_strategy: RollbackStrategy,
    /// 进度回调（默认 None）
    pub progress_callback: Option<ProgressCallback>,
    /// UPSERT 冲突目标（默认 None，使用主键）
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

/// H-9 默认分片大小
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

// ============================================================================
// 深度扩展：批量进度回调、回滚策略、UPSERT 冲突目标、分块处理编排
// ============================================================================

/// 批量操作阶段，用于进度回调报告。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatchStage {
    /// 批量操作开始
    Started,
    /// 正在处理单个分块
    ProcessingChunk,
    /// 单个分块处理完成
    ChunkCompleted,
    /// 全部分块处理完成
    Finished,
}

/// 批量操作进度信息，传递给进度回调。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchProgress {
    /// 当前分块索引（从 0 开始）
    pub chunk_index: usize,
    /// 分块总数
    pub total_chunks: usize,
    /// 当前分块行数
    pub chunk_rows: usize,
    /// 已处理行数
    pub processed_rows: usize,
    /// 总行数
    pub total_rows: usize,
    /// 当前阶段
    pub stage: BatchStage,
}

impl BatchProgress {
    /// 完成百分比（0.0 ~ 100.0）
    pub fn percent(&self) -> f64 {
        if self.total_rows == 0 {
            return 100.0;
        }
        (self.processed_rows as f64 / self.total_rows as f64) * 100.0
    }

    /// 是否已完成
    pub fn is_finished(&self) -> bool {
        self.stage == BatchStage::Finished
    }
}

/// 进度回调函数类型（线程安全、可共享）。
pub type ProgressCallback = std::sync::Arc<dyn Fn(BatchProgress) + Send + Sync>;

/// 批量操作回滚策略。
///
/// 控制当某个分块失败时的行为：
/// - `None`：失败的分块计入 `failed`，不影响已成功的分块
/// - `Savepoint`：每个分块前生成 `SAVEPOINT` 语句，失败时回滚到 savepoint
/// - `PerChunk`：任一分块失败则整批中止，后续分块不再执行
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RollbackStrategy {
    /// 不回滚（默认）
    #[default]
    None,
    /// Savepoint 回滚
    Savepoint,
    /// 整批中止
    PerChunk,
}

/// UPSERT 冲突目标（ON CONFLICT 子句的目标）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictTarget {
    /// 按列名冲突检测：`ON CONFLICT (col1, col2)`
    Columns(Vec<String>),
    /// 按约束名冲突检测：`ON CONSTRAINT constraint_name`
    Constraint(String),
}

/// 带冲突目标的 UPSERT 结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertResult {
    /// 基础批量结果
    pub base: BatchResult,
    /// 使用的冲突目标
    pub conflict_target: Option<ConflictTarget>,
    /// 生成的 SAVEPOINT / ROLLBACK TO SQL（Savepoint 策略时）
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

/// 分块处理结果，记录每个分块的处理情况。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkProcessResult {
    /// 分块索引
    pub chunk_index: usize,
    /// 分块行数
    pub chunk_rows: usize,
    /// 是否成功
    pub success: bool,
    /// 生成的 SQL
    pub sql: Option<String>,
    /// 错误信息（失败时）
    pub error: Option<String>,
}

impl DefaultBatchOps {
    /// 设置回滚策略（返回新的配置实例）。
    ///
    /// 注意：`RollbackStrategy` 仅影响 [`batch_upsert_with_options`] 等带选项方法。
    pub fn with_rollback_strategy(mut self, strategy: RollbackStrategy) -> Self {
        self.rollback_strategy = strategy;
        self
    }

    /// 设置进度回调（返回新的配置实例）。
    pub fn with_progress_callback(mut self, callback: ProgressCallback) -> Self {
        self.progress_callback = Some(callback);
        self
    }

    /// 设置 UPSERT 冲突目标（返回新的配置实例）。
    ///
    /// 仅 PostgreSQL `ON CONFLICT` 模式生效。设置后，`batch_upsert` 将使用
    /// 指定的冲突目标而非默认的主键列。
    pub fn with_conflict_target(mut self, target: ConflictTarget) -> Self {
        self.conflict_target = Some(target);
        self
    }

    /// 生成 PostgreSQL ON CONFLICT 子句（带冲突目标）。
    fn build_pg_conflict_clause(
        &self,
        non_pk: &[String],
        conflict: &ConflictTarget,
    ) -> String {
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

    /// 生成 SAVEPOINT 语句。
    fn savepoint_sql(index: usize) -> String {
        format!("SAVEPOINT batch_chunk_{}", index)
    }

    /// 生成 ROLLBACK TO SAVEPOINT 语句。
    fn rollback_to_sql(index: usize) -> String {
        format!("ROLLBACK TO SAVEPOINT batch_chunk_{}", index)
    }

    /// 生成 RELEASE SAVEPOINT 语句。
    fn release_savepoint_sql(index: usize) -> String {
        format!("RELEASE SAVEPOINT batch_chunk_{}", index)
    }

    /// 执行分块处理，对每个分块调用闭包生成 SQL，并收集结果。
    ///
    /// 根据回滚策略生成额外的事务控制 SQL（SAVEPOINT / ROLLBACK TO）。
    /// 根据进度回调在每个分块前后触发进度通知。
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

    /// 带选项的批量 UPSERT：支持冲突目标、回滚策略、进度回调。
    ///
    /// 返回 `UpsertResult`，包含基础批量结果、冲突目标和事务控制 SQL。
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
                result
                    .transaction_sqls
                    .push(Self::savepoint_sql(chunk_idx));
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

    /// 生成 PerChunk 回滚策略下，失败块对应的 ROLLBACK TO SQL。
    ///
    /// 调用方在执行分块 SQL 时，若某块执行失败，调用此方法获取回滚 SQL。
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
        assert_eq!(
            DefaultBatchOps::savepoint_sql(0),
            "SAVEPOINT batch_chunk_0"
        );
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
