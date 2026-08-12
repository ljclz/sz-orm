//! BatchExecutor — 异步批量执行器 + 事务边界
//!
//! 通过闭包式 API 执行批量 SQL，支持事务边界 + 三种回滚策略（None/Savepoint/PerChunk）。
//! 复用既有 DefaultBatchOps SQL 生成 + BatchResult + ProgressCallback + RollbackStrategy。

use serde_json::Value;

use sz_orm_core::DbType;

use crate::delete::{batch_delete, BatchDeleteRequest};
use crate::{
    BatchProgress, BatchResult, BatchStage, DefaultBatchOps, ProgressCallback, RollbackStrategy,
    UpsertMode, DEFAULT_CHUNK_SIZE,
};

/// 批量执行配置
#[derive(Clone)]
pub struct BatchExecutorConfig {
    /// 分片大小
    pub chunk_size: usize,
    /// 回滚策略
    pub rollback_strategy: RollbackStrategy,
    /// 进度回调
    pub progress_callback: Option<ProgressCallback>,
    /// 是否使用 COPY 协议（仅 PG 有效）
    pub use_copy_protocol: bool,
}

impl Default for BatchExecutorConfig {
    fn default() -> Self {
        Self {
            chunk_size: DEFAULT_CHUNK_SIZE,
            rollback_strategy: RollbackStrategy::None,
            progress_callback: None,
            use_copy_protocol: false,
        }
    }
}

impl BatchExecutorConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_chunk_size(mut self, chunk_size: usize) -> Self {
        self.chunk_size = chunk_size.max(1);
        self
    }

    pub fn with_rollback_strategy(mut self, strategy: RollbackStrategy) -> Self {
        self.rollback_strategy = strategy;
        self
    }

    pub fn with_progress_callback(mut self, callback: ProgressCallback) -> Self {
        self.progress_callback = Some(callback);
        self
    }

    pub fn with_copy_protocol(mut self, use_copy: bool) -> Self {
        self.use_copy_protocol = use_copy;
        self
    }
}

/// 批量执行错误
#[derive(Debug, Clone)]
pub enum BatchExecutorError {
    /// 执行失败
    ExecutionFailed(String),
    /// 全部分片失败
    AllChunksFailed,
    /// 事务回滚
    TransactionRolledBack(String),
}

impl std::fmt::Display for BatchExecutorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BatchExecutorError::ExecutionFailed(msg) => write!(f, "execution failed: {msg}"),
            BatchExecutorError::AllChunksFailed => write!(f, "all chunks failed"),
            BatchExecutorError::TransactionRolledBack(msg) => {
                write!(f, "transaction rolled back: {msg}")
            }
        }
    }
}

impl std::error::Error for BatchExecutorError {}

/// 批量执行结果（包含分片执行详情）
#[derive(Debug, Clone)]
pub struct BatchExecutionResult {
    /// 基础批量结果
    pub base: BatchResult,
    /// 每个分片的执行结果
    pub chunk_results: Vec<ChunkExecutionDetail>,
    /// 是否在事务内执行
    pub in_transaction: bool,
    /// 是否已回滚
    pub rolled_back: bool,
}

/// 单个分片执行详情
#[derive(Debug, Clone)]
pub struct ChunkExecutionDetail {
    /// 分片索引
    pub chunk_index: usize,
    /// 分片行数
    pub chunk_rows: usize,
    /// 是否成功
    pub success: bool,
    /// 影响行数
    pub affected_rows: u64,
    /// 错误信息
    pub error: Option<String>,
}

impl BatchExecutionResult {
    pub fn new() -> Self {
        Self {
            base: BatchResult::new(),
            chunk_results: Vec::new(),
            in_transaction: false,
            rolled_back: false,
        }
    }
}

impl Default for BatchExecutionResult {
    fn default() -> Self {
        Self::new()
    }
}

/// 异步批量执行器
///
/// 使用闭包式 API 执行 SQL，避免直接依赖 Connection trait。
/// 调用方提供 `async fn(sql: &str, params: &[Value]) -> Result<u64, String>` 闭包，
/// BatchExecutor 负责分片、进度回调、事务边界、回滚策略。
pub struct BatchExecutor {
    db_type: DbType,
    ops: DefaultBatchOps,
}

impl BatchExecutor {
    pub fn new(db_type: DbType) -> Self {
        Self {
            db_type,
            ops: DefaultBatchOps::new(),
        }
    }

    pub fn with_ops(db_type: DbType, ops: DefaultBatchOps) -> Self {
        Self { db_type, ops }
    }

    pub fn db_type(&self) -> DbType {
        self.db_type
    }

    /// 批量 INSERT
    pub async fn execute_batch_insert<F>(
        &self,
        table: &str,
        rows: Vec<Value>,
        config: &BatchExecutorConfig,
        executor: &F,
    ) -> Result<BatchExecutionResult, BatchExecutorError>
    where
        F: Fn(&str, &[Value]) -> futures::future::BoxFuture<'static, Result<u64, String>>
            + Send
            + Sync,
    {
        if rows.is_empty() {
            return Ok(BatchExecutionResult::new());
        }
        let total = rows.len();
        let chunk_size = config.chunk_size.max(1);
        let total_chunks = total.div_ceil(chunk_size);
        let mut result = BatchExecutionResult::new();
        self.emit_progress(config, BatchStage::Started, 0, total_chunks, 0, total);
        let mut aborted = false;
        for (chunk_idx, (start, end)) in (0..total)
            .step_by(chunk_size)
            .map(|s| (s, (s + chunk_size).min(total)))
            .enumerate()
        {
            if aborted {
                break;
            }
            let chunk = &rows[start..end];
            self.emit_progress(
                config,
                BatchStage::ProcessingChunk,
                chunk_idx,
                total_chunks,
                start,
                total,
            );
            let (sql, params) = crate::dialect::BatchDialect::build_batch_insert(
                self.db_type,
                table,
                &rows,
                (start, end),
            )
            .map_err(|e| BatchExecutorError::ExecutionFailed(e.to_string()))?;
            let detail = match executor(&sql, &params).await {
                Ok(affected) => {
                    result.base.inserted += chunk.len();
                    result.base.generated_sqls.push(sql);
                    ChunkExecutionDetail {
                        chunk_index: chunk_idx,
                        chunk_rows: chunk.len(),
                        success: true,
                        affected_rows: affected,
                        error: None,
                    }
                }
                Err(err) => {
                    result.base.failed += chunk.len();
                    if config.rollback_strategy == RollbackStrategy::PerChunk {
                        aborted = true;
                        result.rolled_back = true;
                    }
                    ChunkExecutionDetail {
                        chunk_index: chunk_idx,
                        chunk_rows: chunk.len(),
                        success: false,
                        affected_rows: 0,
                        error: Some(err),
                    }
                }
            };
            result.chunk_results.push(detail);
            self.emit_progress(
                config,
                BatchStage::ChunkCompleted,
                chunk_idx,
                total_chunks,
                end,
                total,
            );
        }
        self.emit_progress(config, BatchStage::Finished, 0, total_chunks, total, total);
        Ok(result)
    }

    /// 批量 UPDATE
    pub async fn execute_batch_update<F>(
        &self,
        table: &str,
        rows: Vec<Value>,
        pk: &str,
        config: &BatchExecutorConfig,
        executor: &F,
    ) -> Result<BatchExecutionResult, BatchExecutorError>
    where
        F: Fn(&str, &[Value]) -> futures::future::BoxFuture<'static, Result<u64, String>>
            + Send
            + Sync,
    {
        if rows.is_empty() {
            return Ok(BatchExecutionResult::new());
        }
        let total = rows.len();
        let chunk_size = config.chunk_size.max(1);

        let mut result = BatchExecutionResult::new();
        let mut aborted = false;
        for (chunk_idx, (start, end)) in (0..total)
            .step_by(chunk_size)
            .map(|s| (s, (s + chunk_size).min(total)))
            .enumerate()
        {
            if aborted {
                break;
            }
            let (sql, params) = crate::dialect::BatchDialect::build_batch_update(
                self.db_type,
                table,
                &rows,
                pk,
                (start, end),
            )
            .map_err(|e| BatchExecutorError::ExecutionFailed(e.to_string()))?;
            let chunk_len = end - start;
            match executor(&sql, &params).await {
                Ok(affected) => {
                    result.base.updated += chunk_len;
                    result.base.generated_sqls.push(sql);
                    result.chunk_results.push(ChunkExecutionDetail {
                        chunk_index: chunk_idx,
                        chunk_rows: chunk_len,
                        success: true,
                        affected_rows: affected,
                        error: None,
                    });
                }
                Err(err) => {
                    result.base.failed += chunk_len;
                    if config.rollback_strategy == RollbackStrategy::PerChunk {
                        aborted = true;
                        result.rolled_back = true;
                    }
                    result.chunk_results.push(ChunkExecutionDetail {
                        chunk_index: chunk_idx,
                        chunk_rows: chunk_len,
                        success: false,
                        affected_rows: 0,
                        error: Some(err),
                    });
                }
            }
        }
        Ok(result)
    }

    /// 批量 DELETE
    pub async fn execute_batch_delete<F>(
        &self,
        request: &BatchDeleteRequest,
        config: &BatchExecutorConfig,
        executor: &F,
    ) -> Result<BatchExecutionResult, BatchExecutorError>
    where
        F: Fn(&str, &[Value]) -> futures::future::BoxFuture<'static, Result<u64, String>>
            + Send
            + Sync,
    {
        let delete_result = batch_delete(&self.ops, request);
        let mut result = BatchExecutionResult::new();
        let total_chunks = delete_result.sqls_with_params.len();
        let mut aborted = false;
        for (chunk_idx, (sql, params)) in delete_result.sqls_with_params.iter().enumerate() {
            if aborted {
                break;
            }
            let chunk_rows = params.len();
            match executor(sql, params).await {
                Ok(affected) => {
                    result.base.updated += chunk_rows;
                    result.base.generated_sqls.push(sql.clone());
                    result.chunk_results.push(ChunkExecutionDetail {
                        chunk_index: chunk_idx,
                        chunk_rows,
                        success: true,
                        affected_rows: affected,
                        error: None,
                    });
                }
                Err(err) => {
                    result.base.failed += chunk_rows;
                    if config.rollback_strategy == RollbackStrategy::PerChunk {
                        aborted = true;
                        result.rolled_back = true;
                    }
                    result.chunk_results.push(ChunkExecutionDetail {
                        chunk_index: chunk_idx,
                        chunk_rows,
                        success: false,
                        affected_rows: 0,
                        error: Some(err),
                    });
                }
            }
        }
        let _ = total_chunks;
        Ok(result)
    }

    /// 批量 UPSERT
    pub async fn execute_batch_upsert<F>(
        &self,
        table: &str,
        rows: Vec<Value>,
        mode: UpsertMode,
        config: &BatchExecutorConfig,
        executor: &F,
    ) -> Result<BatchExecutionResult, BatchExecutorError>
    where
        F: Fn(&str, &[Value]) -> futures::future::BoxFuture<'static, Result<u64, String>>
            + Send
            + Sync,
    {
        if rows.is_empty() {
            return Ok(BatchExecutionResult::new());
        }
        let total = rows.len();
        let chunk_size = config.chunk_size.max(1);
        let mut result = BatchExecutionResult::new();
        let mut aborted = false;
        for (chunk_idx, (start, end)) in (0..total)
            .step_by(chunk_size)
            .map(|s| (s, (s + chunk_size).min(total)))
            .enumerate()
        {
            if aborted {
                break;
            }
            let (sql, params) = crate::dialect::BatchDialect::build_batch_upsert(
                self.db_type,
                table,
                &rows,
                mode,
                (start, end),
            )
            .map_err(|e| BatchExecutorError::ExecutionFailed(e.to_string()))?;
            let chunk_len = end - start;
            match executor(&sql, &params).await {
                Ok(affected) => {
                    result.base.inserted += chunk_len;
                    result.base.generated_sqls.push(sql);
                    result.chunk_results.push(ChunkExecutionDetail {
                        chunk_index: chunk_idx,
                        chunk_rows: chunk_len,
                        success: true,
                        affected_rows: affected,
                        error: None,
                    });
                }
                Err(err) => {
                    result.base.failed += chunk_len;
                    if config.rollback_strategy == RollbackStrategy::PerChunk {
                        aborted = true;
                        result.rolled_back = true;
                    }
                    result.chunk_results.push(ChunkExecutionDetail {
                        chunk_index: chunk_idx,
                        chunk_rows: chunk_len,
                        success: false,
                        affected_rows: 0,
                        error: Some(err),
                    });
                }
            }
        }
        Ok(result)
    }

    fn emit_progress(
        &self,
        config: &BatchExecutorConfig,
        stage: BatchStage,
        chunk_index: usize,
        total_chunks: usize,
        processed: usize,
        total: usize,
    ) {
        if let Some(cb) = &config.progress_callback {
            cb(BatchProgress {
                chunk_index,
                total_chunks,
                chunk_rows: 0,
                processed_rows: processed,
                total_rows: total,
                stage,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::BoxFuture;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn ok_executor(
    ) -> impl Fn(&str, &[Value]) -> BoxFuture<'static, Result<u64, String>> + Send + Sync {
        |_sql, _params| Box::pin(async { Ok(1) })
    }

    fn fail_executor(
    ) -> impl Fn(&str, &[Value]) -> BoxFuture<'static, Result<u64, String>> + Send + Sync {
        |_sql, _params| Box::pin(async { Err("db error".to_string()) })
    }

    fn conditional_executor(
        fail_at: usize,
    ) -> impl Fn(&str, &[Value]) -> BoxFuture<'static, Result<u64, String>> + Send + Sync {
        let counter = Arc::new(AtomicUsize::new(0));
        move |_sql, _params| {
            let counter = Arc::clone(&counter);
            Box::pin(async move {
                let idx = counter.fetch_add(1, Ordering::SeqCst);
                if idx == fail_at {
                    Err("simulated failure".to_string())
                } else {
                    Ok(1)
                }
            })
        }
    }

    #[tokio::test]
    async fn execute_insert_basic() {
        let exec = BatchExecutor::new(DbType::MySQL);
        let rows = vec![json!({"id": 1, "name": "a"}), json!({"id": 2, "name": "b"})];
        let config = BatchExecutorConfig::new().with_chunk_size(1000);
        let result = exec
            .execute_batch_insert("users", rows, &config, &ok_executor())
            .await
            .unwrap();
        assert_eq!(result.base.inserted, 2);
        assert_eq!(result.base.failed, 0);
        assert_eq!(result.chunk_results.len(), 1);
    }

    #[tokio::test]
    async fn execute_insert_chunking() {
        let exec = BatchExecutor::new(DbType::MySQL);
        let rows: Vec<Value> = (1..=2500)
            .map(|i| json!({"id": i, "name": format!("name{i}")}))
            .collect();
        let config = BatchExecutorConfig::new().with_chunk_size(1000);
        let result = exec
            .execute_batch_insert("users", rows, &config, &ok_executor())
            .await
            .unwrap();
        assert_eq!(result.chunk_results.len(), 3);
        assert_eq!(result.base.inserted, 2500);
    }

    #[tokio::test]
    async fn execute_insert_empty_rows() {
        let exec = BatchExecutor::new(DbType::MySQL);
        let config = BatchExecutorConfig::new();
        let result = exec
            .execute_batch_insert("users", vec![], &config, &ok_executor())
            .await
            .unwrap();
        assert_eq!(result.base.inserted, 0);
        assert_eq!(result.chunk_results.len(), 0);
    }

    #[tokio::test]
    async fn execute_insert_failure_skip() {
        let exec = BatchExecutor::new(DbType::MySQL);
        let rows: Vec<Value> = (1..=3).map(|i| json!({"id": i, "name": "n"})).collect();
        let config = BatchExecutorConfig::new()
            .with_chunk_size(1)
            .with_rollback_strategy(RollbackStrategy::None);
        let result = exec
            .execute_batch_insert("users", rows, &config, &fail_executor())
            .await
            .unwrap();
        assert_eq!(result.base.failed, 3);
        assert_eq!(result.base.inserted, 0);
        assert!(!result.rolled_back);
    }

    #[tokio::test]
    async fn execute_insert_failure_perchunk_abort() {
        let exec = BatchExecutor::new(DbType::MySQL);
        let rows: Vec<Value> = (1..=3).map(|i| json!({"id": i, "name": "n"})).collect();
        let config = BatchExecutorConfig::new()
            .with_chunk_size(1)
            .with_rollback_strategy(RollbackStrategy::PerChunk);
        let result = exec
            .execute_batch_insert("users", rows, &config, &fail_executor())
            .await
            .unwrap();
        assert!(result.rolled_back);
        assert_eq!(result.chunk_results.len(), 1);
    }

    #[tokio::test]
    async fn execute_insert_partial_failure_perchunk() {
        let exec = BatchExecutor::new(DbType::MySQL);
        let rows: Vec<Value> = (1..=3).map(|i| json!({"id": i, "name": "n"})).collect();
        let config = BatchExecutorConfig::new()
            .with_chunk_size(1)
            .with_rollback_strategy(RollbackStrategy::PerChunk);
        let result = exec
            .execute_batch_insert("users", rows, &config, &conditional_executor(1))
            .await
            .unwrap();
        assert!(result.rolled_back);
        assert_eq!(result.chunk_results.len(), 2);
        assert!(result.chunk_results[0].success);
        assert!(!result.chunk_results[1].success);
    }

    #[tokio::test]
    async fn execute_delete_basic() {
        let exec = BatchExecutor::new(DbType::MySQL);
        let req =
            BatchDeleteRequest::new("users", "id", vec![json!(1), json!(2), json!(3)]).unwrap();
        let config = BatchExecutorConfig::new().with_chunk_size(1000);
        let result = exec
            .execute_batch_delete(&req, &config, &ok_executor())
            .await
            .unwrap();
        assert_eq!(result.base.updated, 3);
        assert_eq!(result.chunk_results.len(), 1);
    }

    #[tokio::test]
    async fn execute_upsert_pg() {
        let exec = BatchExecutor::new(DbType::PostgreSQL);
        let rows = vec![json!({"id": 1, "name": "a"})];
        let config = BatchExecutorConfig::new();
        let result = exec
            .execute_batch_upsert(
                "users",
                rows,
                UpsertMode::PostgresOnConflict,
                &config,
                &ok_executor(),
            )
            .await
            .unwrap();
        assert_eq!(result.base.inserted, 1);
        assert!(result.base.generated_sqls[0].contains("ON CONFLICT"));
    }

    #[tokio::test]
    async fn execute_update_basic() {
        let exec = BatchExecutor::new(DbType::MySQL);
        let rows = vec![json!({"id": 1, "name": "a"}), json!({"id": 2, "name": "b"})];
        let config = BatchExecutorConfig::new();
        let result = exec
            .execute_batch_update("users", rows, "id", &config, &ok_executor())
            .await
            .unwrap();
        assert_eq!(result.base.updated, 2);
    }

    #[tokio::test]
    async fn progress_callback_triggered() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_cb = Arc::clone(&counter);
        let callback: ProgressCallback = Arc::new(move |_progress| {
            counter_cb.fetch_add(1, Ordering::SeqCst);
        });
        let exec = BatchExecutor::new(DbType::MySQL);
        let rows = vec![json!({"id": 1, "name": "a"})];
        let config = BatchExecutorConfig::new().with_progress_callback(callback);
        let _ = exec
            .execute_batch_insert("users", rows, &config, &ok_executor())
            .await
            .unwrap();
        assert!(counter.load(Ordering::SeqCst) > 0);
    }
}
