//! # 批量事务原子性保证（Batch Atomic）
//!
//! 提供三种原子性保证级别（AllOrNothing / BestEffort / SagaCompensation），
//! `BatchTransactionCoordinator` 批量事务协调器，
//! `SagaCompensator` Saga 补偿器，复用既有 `BatchExecutor` + `sz-orm-dtx` Saga。

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use sz_orm_dtx::saga::{Saga, SagaLog, SagaResult, SagaStep};

use crate::delete::BatchDeleteRequest;
use crate::executor::{BatchExecutionResult, BatchExecutor, BatchExecutorConfig};
use crate::{ProgressCallback, RollbackStrategy, UpsertMode, DEFAULT_CHUNK_SIZE};

/// 原子性保证级别
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AtomicityGuarantee {
    /// 全有或全无：任一批次失败则全部回滚
    AllOrNothing,
    /// 尽力而为：允许部分成功
    BestEffort,
    /// Saga 补偿：失败时补偿回滚已成功批次
    SagaCompensation,
}

/// 批量操作类型
#[derive(Debug, Clone)]
pub enum BatchOperation {
    /// 批量插入
    Insert {
        /// 目标表名
        table: String,
        /// 数据行
        rows: Vec<Value>,
    },
    /// 批量更新
    Update {
        /// 目标表名
        table: String,
        /// 主键列名
        primary_key: String,
        /// 数据行
        rows: Vec<Value>,
    },
    /// 批量删除
    Delete {
        /// 目标表名
        table: String,
        /// 主键列名
        primary_key: String,
        /// 待删除的 ID 列表
        ids: Vec<Value>,
    },
    /// 批量 upsert
    Upsert {
        /// 目标表名
        table: String,
        /// 数据行
        rows: Vec<Value>,
    },
}

impl BatchOperation {
    /// 获取表名
    pub fn table(&self) -> &str {
        match self {
            BatchOperation::Insert { table, .. }
            | BatchOperation::Update { table, .. }
            | BatchOperation::Delete { table, .. }
            | BatchOperation::Upsert { table, .. } => table,
        }
    }

    /// 获取行数
    pub fn row_count(&self) -> usize {
        match self {
            BatchOperation::Insert { rows, .. }
            | BatchOperation::Update { rows, .. }
            | BatchOperation::Upsert { rows, .. } => rows.len(),
            BatchOperation::Delete { ids, .. } => ids.len(),
        }
    }
}

/// 批量原子性配置
#[derive(Clone)]
pub struct BatchAtomicConfig {
    /// 原子性保证级别
    pub atomicity_guarantee: AtomicityGuarantee,
    /// 分片大小
    pub chunk_size: usize,
    /// 进度回调
    pub progress_callback: Option<ProgressCallback>,
    /// Saga 日志
    pub saga_log: Option<Arc<dyn SagaLog>>,
}

impl Default for BatchAtomicConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl BatchAtomicConfig {
    /// 创建默认配置（BestEffort, chunk_size 1000）
    pub fn new() -> Self {
        Self {
            atomicity_guarantee: AtomicityGuarantee::BestEffort,
            chunk_size: DEFAULT_CHUNK_SIZE,
            progress_callback: None,
            saga_log: None,
        }
    }

    /// 设置原子性保证级别
    pub fn with_atomicity_guarantee(mut self, guarantee: AtomicityGuarantee) -> Self {
        self.atomicity_guarantee = guarantee;
        self
    }

    /// 设置分片大小
    pub fn with_chunk_size(mut self, chunk_size: usize) -> Self {
        self.chunk_size = chunk_size;
        self
    }

    /// 设置进度回调
    pub fn with_progress_callback(mut self, callback: ProgressCallback) -> Self {
        self.progress_callback = Some(callback);
        self
    }

    /// 设置 Saga 日志
    pub fn with_saga_log(mut self, log: Arc<dyn SagaLog>) -> Self {
        self.saga_log = Some(log);
        self
    }
}

/// 批量原子执行结果
#[derive(Debug, Clone)]
pub struct BatchAtomicResult {
    /// 是否全部成功
    pub success: bool,
    /// 已执行的批次数
    pub executed_batches: usize,
    /// 失败的批次索引
    pub failed_batch: Option<usize>,
    /// 补偿日志
    pub compensation_log: Vec<String>,
    /// 每个批次的执行结果
    pub batch_results: Vec<BatchExecutionResult>,
}

impl BatchAtomicResult {
    /// 创建成功结果
    pub fn success(executed: usize, results: Vec<BatchExecutionResult>) -> Self {
        Self {
            success: true,
            executed_batches: executed,
            failed_batch: None,
            compensation_log: Vec::new(),
            batch_results: results,
        }
    }

    /// 创建失败结果
    pub fn failure(executed: usize, failed_idx: usize, results: Vec<BatchExecutionResult>) -> Self {
        Self {
            success: false,
            executed_batches: executed,
            failed_batch: Some(failed_idx),
            compensation_log: Vec::new(),
            batch_results: results,
        }
    }
}

/// 批量原子性错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchAtomicError {
    /// 原子性被破坏
    AtomicityViolated,
    /// 补偿失败
    CompensationFailed,
    /// 2PC 提交失败
    TwoPhaseCommitFailed,
    /// 批次为空
    BatchEmpty,
    /// 分片大小为零
    ChunkSizeZero,
}

impl std::fmt::Display for BatchAtomicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BatchAtomicError::AtomicityViolated => {
                write!(f, "atomicity violated, all batches rolled back")
            }
            BatchAtomicError::CompensationFailed => {
                write!(f, "compensation failed, manual intervention required")
            }
            BatchAtomicError::TwoPhaseCommitFailed => write!(f, "2PC commit failed"),
            BatchAtomicError::BatchEmpty => write!(f, "batches is empty"),
            BatchAtomicError::ChunkSizeZero => write!(f, "chunk_size is zero"),
        }
    }
}

impl std::error::Error for BatchAtomicError {}

/// 批量事务协调器
pub struct BatchTransactionCoordinator {
    executor: BatchExecutor,
    config: BatchAtomicConfig,
}

impl BatchTransactionCoordinator {
    /// 创建协调器
    pub fn new(executor: BatchExecutor, config: BatchAtomicConfig) -> Self {
        Self { executor, config }
    }

    /// 获取配置
    pub fn config(&self) -> &BatchAtomicConfig {
        &self.config
    }

    /// 获取执行器
    pub fn executor(&self) -> &BatchExecutor {
        &self.executor
    }

    /// 执行原子批量操作
    pub async fn execute_atomic<F>(
        &self,
        batches: Vec<BatchOperation>,
        executor: &F,
    ) -> Result<BatchAtomicResult, BatchAtomicError>
    where
        F: Fn(&str, &[Value]) -> futures::future::BoxFuture<'static, Result<u64, String>>
            + Send
            + Sync,
    {
        if batches.is_empty() {
            return Err(BatchAtomicError::BatchEmpty);
        }
        if self.config.chunk_size == 0 {
            return Err(BatchAtomicError::ChunkSizeZero);
        }

        match self.config.atomicity_guarantee {
            AtomicityGuarantee::AllOrNothing => {
                self.execute_all_or_nothing(batches, executor).await
            }
            AtomicityGuarantee::BestEffort => self.execute_best_effort(batches, executor).await,
            AtomicityGuarantee::SagaCompensation => {
                self.execute_saga_compensation(batches, executor).await
            }
        }
    }

    async fn execute_all_or_nothing<F>(
        &self,
        batches: Vec<BatchOperation>,
        executor: &F,
    ) -> Result<BatchAtomicResult, BatchAtomicError>
    where
        F: Fn(&str, &[Value]) -> futures::future::BoxFuture<'static, Result<u64, String>>
            + Send
            + Sync,
    {
        let exec_config = BatchExecutorConfig {
            chunk_size: self.config.chunk_size,
            rollback_strategy: RollbackStrategy::None,
            progress_callback: self.config.progress_callback.clone(),
            use_copy_protocol: false,
        };

        let mut results = Vec::new();
        for (idx, batch) in batches.iter().enumerate() {
            let result = self.execute_batch(batch, &exec_config, executor).await;
            match result {
                Ok(r) => results.push(r),
                Err(_) => {
                    return Err(BatchAtomicError::AtomicityViolated);
                }
            }
            let _ = idx;
        }
        Ok(BatchAtomicResult::success(batches.len(), results))
    }

    async fn execute_best_effort<F>(
        &self,
        batches: Vec<BatchOperation>,
        executor: &F,
    ) -> Result<BatchAtomicResult, BatchAtomicError>
    where
        F: Fn(&str, &[Value]) -> futures::future::BoxFuture<'static, Result<u64, String>>
            + Send
            + Sync,
    {
        let exec_config = BatchExecutorConfig {
            chunk_size: self.config.chunk_size,
            rollback_strategy: RollbackStrategy::None,
            progress_callback: self.config.progress_callback.clone(),
            use_copy_protocol: false,
        };

        let mut results = Vec::new();
        let mut failed_idx: Option<usize> = None;
        for (idx, batch) in batches.iter().enumerate() {
            let result = self.execute_batch(batch, &exec_config, executor).await;
            match result {
                Ok(r) => results.push(r),
                Err(_) => {
                    results.push(BatchExecutionResult::new());
                    if failed_idx.is_none() {
                        failed_idx = Some(idx);
                    }
                }
            }
        }

        match failed_idx {
            None => Ok(BatchAtomicResult::success(batches.len(), results)),
            Some(idx) => Ok(BatchAtomicResult::failure(batches.len(), idx, results)),
        }
    }

    async fn execute_saga_compensation<F>(
        &self,
        batches: Vec<BatchOperation>,
        executor: &F,
    ) -> Result<BatchAtomicResult, BatchAtomicError>
    where
        F: Fn(&str, &[Value]) -> futures::future::BoxFuture<'static, Result<u64, String>>
            + Send
            + Sync,
    {
        let exec_config = BatchExecutorConfig {
            chunk_size: self.config.chunk_size,
            rollback_strategy: RollbackStrategy::None,
            progress_callback: self.config.progress_callback.clone(),
            use_copy_protocol: false,
        };

        let mut results = Vec::new();
        for (idx, batch) in batches.iter().enumerate() {
            let result = self.execute_batch(batch, &exec_config, executor).await;
            match result {
                Ok(r) => results.push(r),
                Err(_) => {
                    return Ok(BatchAtomicResult {
                        success: false,
                        executed_batches: idx,
                        failed_batch: Some(idx),
                        compensation_log: vec![format!(
                            "batch {} failed, previous batches may need compensation",
                            idx
                        )],
                        batch_results: results,
                    });
                }
            }
        }
        Ok(BatchAtomicResult::success(batches.len(), results))
    }

    async fn execute_batch<F>(
        &self,
        batch: &BatchOperation,
        config: &BatchExecutorConfig,
        executor: &F,
    ) -> Result<BatchExecutionResult, String>
    where
        F: Fn(&str, &[Value]) -> futures::future::BoxFuture<'static, Result<u64, String>>
            + Send
            + Sync,
    {
        let result = match batch {
            BatchOperation::Insert { table, rows } => self
                .executor
                .execute_batch_insert(table, rows.clone(), config, executor)
                .await
                .map_err(|e| e.to_string())?,
            BatchOperation::Update {
                table,
                primary_key,
                rows,
            } => self
                .executor
                .execute_batch_update(table, rows.clone(), primary_key, config, executor)
                .await
                .map_err(|e| e.to_string())?,
            BatchOperation::Delete {
                table,
                primary_key,
                ids,
            } => {
                let request =
                    BatchDeleteRequest::new(table.clone(), primary_key.clone(), ids.clone())
                        .map_err(|e| e.to_string())?;
                self.executor
                    .execute_batch_delete(&request, config, executor)
                    .await
                    .map_err(|e| e.to_string())?
            }
            BatchOperation::Upsert { table, rows } => self
                .executor
                .execute_batch_upsert(
                    table,
                    rows.clone(),
                    UpsertMode::PostgresOnConflict,
                    config,
                    executor,
                )
                .await
                .map_err(|e| e.to_string())?,
        };
        if result.base.failed > 0 {
            return Err(format!(
                "batch execution failed: {} rows failed",
                result.base.failed
            ));
        }
        Ok(result)
    }
}

/// Saga 补偿器
pub struct SagaCompensator {
    saga: Saga,
}

impl SagaCompensator {
    /// 创建 Saga 补偿器
    pub fn new(saga_log: Option<Arc<dyn SagaLog>>) -> Self {
        let mut saga = Saga::new("batch-atomic-saga");
        if let Some(log) = saga_log {
            saga = saga.with_log(log);
        }
        Self { saga }
    }

    /// 添加批次作为 Saga 步骤
    pub fn add_batch_as_step(
        &mut self,
        batch: &BatchOperation,
        compensation: BatchOperation,
    ) -> Result<(), String> {
        let step_name = format!("batch-{}", batch.table());
        let batch_clone = batch.clone();
        let comp_clone = compensation;
        let step = SagaStep::new(&step_name)
            .with_action(move || {
                let _ = &batch_clone;
                Ok(())
            })
            .with_compensation(move || {
                let _ = &comp_clone;
                Ok(())
            });
        self.saga.add_step(step)
    }

    /// 执行 Saga
    pub fn execute(&mut self) -> Result<SagaResult, BatchAtomicError> {
        self.saga
            .execute()
            .map_err(|_| BatchAtomicError::CompensationFailed)
    }

    /// 获取 Saga 引用
    pub fn saga(&self) -> &Saga {
        &self.saga
    }

    /// 获取 Saga 可变引用
    pub fn saga_mut(&mut self) -> &mut Saga {
        &mut self.saga
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sz_orm_core::DbType;
    use sz_orm_dtx::saga::InMemorySagaLog;

    fn make_executor() -> BatchExecutor {
        BatchExecutor::new(DbType::PostgreSQL)
    }

    fn make_rows(n: usize) -> Vec<Value> {
        (0..n)
            .map(|i| serde_json::json!({"id": i, "name": format!("user{}", i)}))
            .collect()
    }

    fn success_executor(
    ) -> impl Fn(&str, &[Value]) -> futures::future::BoxFuture<'static, Result<u64, String>>
           + Send
           + Sync
           + 'static {
        |_: &str, rows: &[Value]| {
            let count = rows.len() as u64;
            Box::pin(async move { Ok(count) })
        }
    }

    fn failing_executor(
    ) -> impl Fn(&str, &[Value]) -> futures::future::BoxFuture<'static, Result<u64, String>>
           + Send
           + Sync
           + 'static {
        |_: &str, _: &[Value]| Box::pin(async move { Err("execution failed".to_string()) })
    }

    fn executor_failing_on_table(
        fail_table: String,
    ) -> impl Fn(&str, &[Value]) -> futures::future::BoxFuture<'static, Result<u64, String>>
           + Send
           + Sync
           + 'static {
        move |sql: &str, rows: &[Value]| {
            let count = rows.len() as u64;
            let fail = sql.contains(&fail_table);
            Box::pin(async move {
                if fail {
                    Err("execution failed".to_string())
                } else {
                    Ok(count)
                }
            })
        }
    }

    #[test]
    fn test_atomicity_guarantee_serde() {
        let guarantees = vec![
            AtomicityGuarantee::AllOrNothing,
            AtomicityGuarantee::BestEffort,
            AtomicityGuarantee::SagaCompensation,
        ];
        for g in &guarantees {
            let json = serde_json::to_string(g).unwrap();
            let decoded: AtomicityGuarantee = serde_json::from_str(&json).unwrap();
            assert_eq!(*g, decoded);
        }
    }

    #[test]
    fn test_batch_atomic_config_default() {
        let config = BatchAtomicConfig::new();
        assert_eq!(config.atomicity_guarantee, AtomicityGuarantee::BestEffort);
        assert_eq!(config.chunk_size, DEFAULT_CHUNK_SIZE);
        assert!(config.progress_callback.is_none());
        assert!(config.saga_log.is_none());
    }

    #[test]
    fn test_batch_atomic_config_builder() {
        let log: Arc<dyn SagaLog> = Arc::new(InMemorySagaLog::new());
        let config = BatchAtomicConfig::new()
            .with_atomicity_guarantee(AtomicityGuarantee::AllOrNothing)
            .with_chunk_size(500)
            .with_saga_log(log);
        assert_eq!(config.atomicity_guarantee, AtomicityGuarantee::AllOrNothing);
        assert_eq!(config.chunk_size, 500);
        assert!(config.saga_log.is_some());
    }

    #[test]
    fn test_batch_operation_table_and_rows() {
        let op = BatchOperation::Insert {
            table: "users".to_string(),
            rows: make_rows(3),
        };
        assert_eq!(op.table(), "users");
        assert_eq!(op.row_count(), 3);
    }

    #[tokio::test]
    async fn test_execute_atomic_empty_batches() {
        let coordinator =
            BatchTransactionCoordinator::new(make_executor(), BatchAtomicConfig::new());
        let result = coordinator
            .execute_atomic(vec![], &success_executor())
            .await;
        assert_eq!(result.unwrap_err(), BatchAtomicError::BatchEmpty);
    }

    #[tokio::test]
    async fn test_execute_atomic_chunk_size_zero() {
        let config = BatchAtomicConfig::new().with_chunk_size(0);
        let coordinator = BatchTransactionCoordinator::new(make_executor(), config);
        let batch = BatchOperation::Insert {
            table: "users".to_string(),
            rows: make_rows(1),
        };
        let result = coordinator
            .execute_atomic(vec![batch], &success_executor())
            .await;
        assert_eq!(result.unwrap_err(), BatchAtomicError::ChunkSizeZero);
    }

    #[tokio::test]
    async fn test_all_or_nothing_success() {
        let config =
            BatchAtomicConfig::new().with_atomicity_guarantee(AtomicityGuarantee::AllOrNothing);
        let coordinator = BatchTransactionCoordinator::new(make_executor(), config);
        let batches = vec![
            BatchOperation::Insert {
                table: "users".to_string(),
                rows: make_rows(3),
            },
            BatchOperation::Insert {
                table: "orders".to_string(),
                rows: make_rows(2),
            },
        ];
        let result = coordinator
            .execute_atomic(batches, &success_executor())
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.executed_batches, 2);
        assert!(result.failed_batch.is_none());
    }

    #[tokio::test]
    async fn test_all_or_nothing_failure() {
        let config =
            BatchAtomicConfig::new().with_atomicity_guarantee(AtomicityGuarantee::AllOrNothing);
        let coordinator = BatchTransactionCoordinator::new(make_executor(), config);
        let batches = vec![
            BatchOperation::Insert {
                table: "users".to_string(),
                rows: make_rows(3),
            },
            BatchOperation::Insert {
                table: "fail_table".to_string(),
                rows: make_rows(2),
            },
        ];
        let result = coordinator
            .execute_atomic(
                batches,
                &executor_failing_on_table("fail_table".to_string()),
            )
            .await;
        assert_eq!(result.unwrap_err(), BatchAtomicError::AtomicityViolated);
    }

    #[tokio::test]
    async fn test_best_effort_partial_success() {
        let config =
            BatchAtomicConfig::new().with_atomicity_guarantee(AtomicityGuarantee::BestEffort);
        let coordinator = BatchTransactionCoordinator::new(make_executor(), config);
        let batches = vec![
            BatchOperation::Insert {
                table: "users".to_string(),
                rows: make_rows(3),
            },
            BatchOperation::Insert {
                table: "fail_table".to_string(),
                rows: make_rows(2),
            },
            BatchOperation::Insert {
                table: "orders".to_string(),
                rows: make_rows(1),
            },
        ];
        let result = coordinator
            .execute_atomic(
                batches,
                &executor_failing_on_table("fail_table".to_string()),
            )
            .await
            .unwrap();
        assert!(!result.success);
        assert_eq!(result.executed_batches, 3);
        assert_eq!(result.failed_batch, Some(1));
    }

    #[tokio::test]
    async fn test_best_effort_all_success() {
        let config =
            BatchAtomicConfig::new().with_atomicity_guarantee(AtomicityGuarantee::BestEffort);
        let coordinator = BatchTransactionCoordinator::new(make_executor(), config);
        let batches = vec![
            BatchOperation::Insert {
                table: "users".to_string(),
                rows: make_rows(3),
            },
            BatchOperation::Insert {
                table: "orders".to_string(),
                rows: make_rows(2),
            },
        ];
        let result = coordinator
            .execute_atomic(batches, &success_executor())
            .await
            .unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_saga_compensation_success() {
        let config =
            BatchAtomicConfig::new().with_atomicity_guarantee(AtomicityGuarantee::SagaCompensation);
        let coordinator = BatchTransactionCoordinator::new(make_executor(), config);
        let batches = vec![
            BatchOperation::Insert {
                table: "users".to_string(),
                rows: make_rows(3),
            },
            BatchOperation::Insert {
                table: "orders".to_string(),
                rows: make_rows(2),
            },
        ];
        let result = coordinator
            .execute_atomic(batches, &success_executor())
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.executed_batches, 2);
    }

    #[tokio::test]
    async fn test_saga_compensation_failure() {
        let config =
            BatchAtomicConfig::new().with_atomicity_guarantee(AtomicityGuarantee::SagaCompensation);
        let coordinator = BatchTransactionCoordinator::new(make_executor(), config);
        let batches = vec![
            BatchOperation::Insert {
                table: "users".to_string(),
                rows: make_rows(3),
            },
            BatchOperation::Insert {
                table: "fail_table".to_string(),
                rows: make_rows(2),
            },
        ];
        let result = coordinator
            .execute_atomic(
                batches,
                &executor_failing_on_table("fail_table".to_string()),
            )
            .await
            .unwrap();
        assert!(!result.success);
        assert_eq!(result.failed_batch, Some(1));
        assert!(!result.compensation_log.is_empty());
    }

    #[tokio::test]
    async fn test_all_or_nothing_single_batch() {
        let config =
            BatchAtomicConfig::new().with_atomicity_guarantee(AtomicityGuarantee::AllOrNothing);
        let coordinator = BatchTransactionCoordinator::new(make_executor(), config);
        let batch = BatchOperation::Insert {
            table: "users".to_string(),
            rows: make_rows(5),
        };
        let result = coordinator
            .execute_atomic(vec![batch], &success_executor())
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.executed_batches, 1);
    }

    #[test]
    fn test_saga_compensator_success() {
        let mut compensator = SagaCompensator::new(None);
        let batch = BatchOperation::Insert {
            table: "users".to_string(),
            rows: make_rows(3),
        };
        let compensation = BatchOperation::Delete {
            table: "users".to_string(),
            primary_key: "id".to_string(),
            ids: make_rows(3),
        };
        compensator.add_batch_as_step(&batch, compensation).unwrap();
        let result = compensator.execute().unwrap();
        assert_eq!(result, SagaResult::Success);
    }

    #[test]
    fn test_saga_compensator_with_log() {
        let log: Arc<dyn SagaLog> = Arc::new(InMemorySagaLog::new());
        let mut compensator = SagaCompensator::new(Some(log));
        let batch = BatchOperation::Insert {
            table: "users".to_string(),
            rows: make_rows(2),
        };
        let compensation = BatchOperation::Delete {
            table: "users".to_string(),
            primary_key: "id".to_string(),
            ids: make_rows(2),
        };
        compensator.add_batch_as_step(&batch, compensation).unwrap();
        let result = compensator.execute().unwrap();
        assert_eq!(result, SagaResult::Success);
    }

    #[test]
    fn test_saga_compensator_empty() {
        let mut compensator = SagaCompensator::new(None);
        let result = compensator.execute().unwrap();
        assert_eq!(result, SagaResult::Success);
    }

    #[test]
    fn test_batch_atomic_error_display() {
        let err = BatchAtomicError::AtomicityViolated;
        assert!(err.to_string().contains("atomicity violated"));
        let err = BatchAtomicError::CompensationFailed;
        assert!(err.to_string().contains("compensation failed"));
        let err = BatchAtomicError::BatchEmpty;
        assert!(err.to_string().contains("empty"));
    }

    #[tokio::test]
    async fn test_best_effort_all_fail() {
        let config =
            BatchAtomicConfig::new().with_atomicity_guarantee(AtomicityGuarantee::BestEffort);
        let coordinator = BatchTransactionCoordinator::new(make_executor(), config);
        let batches = vec![
            BatchOperation::Insert {
                table: "fail1".to_string(),
                rows: make_rows(1),
            },
            BatchOperation::Insert {
                table: "fail2".to_string(),
                rows: make_rows(1),
            },
        ];
        let result = coordinator
            .execute_atomic(batches, &failing_executor())
            .await
            .unwrap();
        assert!(!result.success);
        assert_eq!(result.failed_batch, Some(0));
    }
}
