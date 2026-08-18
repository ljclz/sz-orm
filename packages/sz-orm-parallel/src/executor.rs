//! 并行查询执行器（Parallel Query Executor）
//!
//! 同步并行执行多个查询任务，支持分片、合并、降级。
//! 不依赖 async runtime，使用线程池执行。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use crate::config::{FailureStrategy, MergeStrategy, ParallelQueryConfig};
use crate::outcome::{QueryFailure, QueryOutcome};
use crate::parallel_stats::ParallelStatsCollector;

/// 并行任务
#[derive(Debug, Clone)]
pub struct ParallelTask<T> {
    pub id: usize,
    pub key: Option<String>,
    pub fallback: Option<T>,
}

impl<T> ParallelTask<T> {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            key: None,
            fallback: None,
        }
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn with_fallback(mut self, value: T) -> Self {
        self.fallback = Some(value);
        self
    }
}

/// 并行执行结果
#[derive(Debug, Clone)]
pub struct ParallelResult<T> {
    pub outcomes: Vec<Option<QueryOutcome<T>>>,
    pub failures: Vec<QueryFailure>,
    pub total_elapsed_ms: u64,
    pub merged: Option<T>,
}

impl<T> ParallelResult<T> {
    pub fn success_count(&self) -> usize {
        self.outcomes.iter().filter(|o| o.is_some()).count()
    }

    pub fn failure_count(&self) -> usize {
        self.failures.len()
    }

    pub fn all_succeeded(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn total_count(&self) -> usize {
        self.outcomes.len()
    }

    pub fn first_success(&self) -> Option<&T> {
        self.outcomes
            .iter()
            .find_map(|o| o.as_ref().map(|q| &q.value))
    }

    pub fn collect_values(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.outcomes
            .iter()
            .filter_map(|o| o.as_ref().map(|q| q.value.clone()))
            .collect()
    }
}

/// 同步并行执行器
///
/// 使用线程池并行执行多个任务。
pub struct ParallelExecutor<T> {
    config: ParallelQueryConfig,
    stats: Arc<ParallelStatsCollector>,
    _marker: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> ParallelExecutor<T> {
    pub fn new(config: ParallelQueryConfig) -> Self {
        Self {
            config,
            stats: Arc::new(ParallelStatsCollector::new()),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn config(&self) -> &ParallelQueryConfig {
        &self.config
    }

    pub fn stats(&self) -> &Arc<ParallelStatsCollector> {
        &self.stats
    }

    /// 并行执行多个任务
    ///
    /// `tasks`：任务列表
    /// `executor`：任务执行闭包，接收任务 ID，返回结果
    pub fn execute<F>(&self, count: usize, executor: F) -> ParallelResult<T>
    where
        F: Fn(usize) -> Result<(T, usize), String> + Send + Sync + 'static,
        T: Clone,
    {
        let start = Instant::now();
        let executor = Arc::new(executor);
        let outcomes: Arc<Mutex<Vec<Option<QueryOutcome<T>>>>> =
            Arc::new(Mutex::new(vec![None; count]));
        let failures: Arc<Mutex<Vec<QueryFailure>>> = Arc::new(Mutex::new(Vec::new()));

        let concurrency = if self.config.concurrency == 0 {
            count
        } else {
            self.config.concurrency.min(count)
        };

        let mut handles = Vec::new();
        for i in 0..count {
            let executor = executor.clone();
            let outcomes = outcomes.clone();
            let failures = failures.clone();
            let stats = self.stats.clone();
            let task_start = Instant::now();

            let handle = std::thread::spawn(move || {
                let task_start = Instant::now();
                match executor(i) {
                    Ok((value, rows)) => {
                        let elapsed_ms = task_start.elapsed().as_millis() as u64;
                        stats.record_query(None, true, false, elapsed_ms, rows as u64);
                        let outcome = QueryOutcome::new(value, rows, elapsed_ms);
                        if let Ok(mut out) = outcomes.lock() {
                            out[i] = Some(outcome);
                        }
                    }
                    Err(error) => {
                        let elapsed_ms = task_start.elapsed().as_millis() as u64;
                        stats.record_query(None, false, false, elapsed_ms, 0);
                        if let Ok(mut fail) = failures.lock() {
                            fail.push(QueryFailure::new(i, error));
                        }
                    }
                }
            });
            handles.push(handle);

            if handles.len() >= concurrency {
                for h in handles.drain(..) {
                    let _ = h.join();
                }
            }
        }

        for h in handles {
            let _ = h.join();
        }

        let outcomes = outcomes.lock().map(|o| o.clone()).unwrap_or_default();
        let failures = failures.lock().map(|f| f.clone()).unwrap_or_default();
        let total_elapsed_ms = start.elapsed().as_millis() as u64;

        let merged = self.merge_results(&outcomes);

        ParallelResult {
            outcomes,
            failures,
            total_elapsed_ms,
            merged,
        }
    }

    fn merge_results(&self, outcomes: &[Option<QueryOutcome<T>>]) -> Option<T>
    where
        T: Clone,
    {
        match &self.config.merge_strategy {
            MergeStrategy::First => outcomes
                .iter()
                .find_map(|o| o.as_ref().map(|q| q.value.clone())),
            MergeStrategy::Union => None,
            MergeStrategy::Join { .. } => None,
            MergeStrategy::Map => None,
        }
    }
}

/// 批量执行器
///
/// 将大批量任务分片后并行执行。
pub struct BatchExecutor<T> {
    executor: ParallelExecutor<T>,
    batch_size: usize,
}

impl<T: Send + 'static> BatchExecutor<T> {
    pub fn new(config: ParallelQueryConfig, batch_size: usize) -> Self {
        Self {
            executor: ParallelExecutor::new(config),
            batch_size: batch_size.max(1),
        }
    }

    pub fn batch_size(&self) -> usize {
        self.batch_size
    }

    /// 批量执行
    ///
    /// 将 `total` 个任务按 `batch_size` 分批，每批并行执行。
    pub fn execute<F>(&self, total: usize, executor: F) -> Vec<ParallelResult<T>>
    where
        F: Fn(usize) -> Result<(T, usize), String> + Send + Sync + 'static,
        T: Clone,
    {
        let executor = Arc::new(executor);
        let mut results = Vec::new();
        let mut start = 0;
        while start < total {
            let batch_count = self.batch_size.min(total - start);
            let batch_start = start;
            let executor_clone = executor.clone();
            let result = self
                .executor
                .execute(batch_count, move |i| executor_clone(batch_start + i));
            results.push(result);
            start += batch_count;
        }
        results
    }

    pub fn stats(&self) -> &Arc<ParallelStatsCollector> {
        self.executor.stats()
    }
}

/// 并行度监控器
///
/// 监控并行执行的运行时指标。
pub struct ParallelismMonitor {
    target_parallelism: AtomicU64,
    actual_parallelism: AtomicU64,
    peak_parallelism: AtomicU64,
    total_executed: AtomicU64,
    start_time: Instant,
}

impl ParallelismMonitor {
    pub fn new(target: u64) -> Self {
        Self {
            target_parallelism: AtomicU64::new(target),
            actual_parallelism: AtomicU64::new(0),
            peak_parallelism: AtomicU64::new(0),
            total_executed: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    pub fn target(&self) -> u64 {
        self.target_parallelism.load(Ordering::Relaxed)
    }

    pub fn actual(&self) -> u64 {
        self.actual_parallelism.load(Ordering::Relaxed)
    }

    pub fn peak(&self) -> u64 {
        self.peak_parallelism.load(Ordering::Relaxed)
    }

    pub fn total_executed(&self) -> u64 {
        self.total_executed.load(Ordering::Relaxed)
    }

    pub fn set_target(&self, target: u64) {
        self.target_parallelism.store(target, Ordering::Relaxed);
    }

    pub fn on_task_start(&self) {
        let current = self.actual_parallelism.fetch_add(1, Ordering::Relaxed) + 1;
        let mut peak = self.peak_parallelism.load(Ordering::Relaxed);
        while current > peak {
            match self.peak_parallelism.compare_exchange(
                peak,
                current,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(new_peak) => peak = new_peak,
            }
        }
    }

    pub fn on_task_end(&self) {
        self.actual_parallelism.fetch_sub(1, Ordering::Relaxed);
        self.total_executed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn utilization_rate(&self) -> f64 {
        let target = self.target();
        if target > 0 {
            self.actual() as f64 / target as f64
        } else {
            0.0
        }
    }

    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parallel_task_new() {
        let task: ParallelTask<i32> = ParallelTask::new(1);
        assert_eq!(task.id, 1);
        assert!(task.key.is_none());
    }

    #[test]
    fn test_parallel_task_with_key() {
        let task: ParallelTask<i32> = ParallelTask::new(1).with_key("query1");
        assert_eq!(task.key.as_deref(), Some("query1"));
    }

    #[test]
    fn test_parallel_task_with_fallback() {
        let task: ParallelTask<i32> = ParallelTask::new(1).with_fallback(42);
        assert_eq!(task.fallback, Some(42));
    }

    #[test]
    fn test_parallel_result_success_count() {
        let result: ParallelResult<i32> = ParallelResult {
            outcomes: vec![
                Some(QueryOutcome::new(1, 1, 10)),
                None,
                Some(QueryOutcome::new(3, 1, 20)),
            ],
            failures: vec![],
            total_elapsed_ms: 30,
            merged: None,
        };
        assert_eq!(result.success_count(), 2);
    }

    #[test]
    fn test_parallel_result_all_succeeded() {
        let result: ParallelResult<i32> = ParallelResult {
            outcomes: vec![Some(QueryOutcome::new(1, 1, 10))],
            failures: vec![],
            total_elapsed_ms: 10,
            merged: None,
        };
        assert!(result.all_succeeded());
    }

    #[test]
    fn test_parallel_result_first_success() {
        let result: ParallelResult<i32> = ParallelResult {
            outcomes: vec![None, Some(QueryOutcome::new(42, 1, 10))],
            failures: vec![],
            total_elapsed_ms: 10,
            merged: None,
        };
        assert_eq!(result.first_success(), Some(&42));
    }

    #[test]
    fn test_parallel_result_collect_values() {
        let result: ParallelResult<i32> = ParallelResult {
            outcomes: vec![
                Some(QueryOutcome::new(1, 1, 10)),
                Some(QueryOutcome::new(2, 1, 20)),
                None,
            ],
            failures: vec![],
            total_elapsed_ms: 30,
            merged: None,
        };
        assert_eq!(result.collect_values(), vec![1, 2]);
    }

    #[test]
    fn test_parallel_executor_basic() {
        let config = ParallelQueryConfig::new().with_concurrency(4);
        let executor: ParallelExecutor<i32> = ParallelExecutor::new(config);
        let result = executor.execute(5, |i| Ok((i as i32, 1)));
        assert_eq!(result.total_count(), 5);
        assert!(result.all_succeeded());
    }

    #[test]
    fn test_parallel_executor_with_failures() {
        let config = ParallelQueryConfig::new();
        let executor: ParallelExecutor<i32> = ParallelExecutor::new(config);
        let result = executor.execute(3, |i| {
            if i == 1 {
                Err("failed".to_string())
            } else {
                Ok((i as i32, 1))
            }
        });
        assert_eq!(result.failure_count(), 1);
        assert!(!result.all_succeeded());
    }

    #[test]
    fn test_parallel_executor_merge_first() {
        let config = ParallelQueryConfig::new().with_merge_strategy(MergeStrategy::First);
        let executor: ParallelExecutor<i32> = ParallelExecutor::new(config);
        let result = executor.execute(3, |i| Ok((i as i32, 1)));
        assert!(result.merged.is_some());
    }

    #[test]
    fn test_parallel_executor_stats() {
        let config = ParallelQueryConfig::new();
        let executor: ParallelExecutor<i32> = ParallelExecutor::new(config);
        executor.execute(5, |i| Ok((i as i32, 1)));
        let stats = executor.stats();
        assert_eq!(stats.query_count(), 5);
    }

    #[test]
    fn test_parallel_executor_config() {
        let config = ParallelQueryConfig::new().with_concurrency(8);
        let executor: ParallelExecutor<i32> = ParallelExecutor::new(config);
        assert_eq!(executor.config().concurrency(), 8);
    }

    #[test]
    fn test_batch_executor() {
        let config = ParallelQueryConfig::new();
        let batch = BatchExecutor::<i32>::new(config, 2);
        let results = batch.execute(5, |i| Ok((i as i32, 1)));
        assert!(results.len() >= 3);
    }

    #[test]
    fn test_batch_executor_batch_size() {
        let config = ParallelQueryConfig::new();
        let batch = BatchExecutor::<i32>::new(config, 10);
        assert_eq!(batch.batch_size(), 10);
    }

    #[test]
    fn test_batch_executor_stats() {
        let config = ParallelQueryConfig::new();
        let batch = BatchExecutor::<i32>::new(config, 2);
        batch.execute(4, |i| Ok((i as i32, 1)));
        assert_eq!(batch.stats().query_count(), 4);
    }

    #[test]
    fn test_parallelism_monitor_new() {
        let monitor = ParallelismMonitor::new(4);
        assert_eq!(monitor.target(), 4);
        assert_eq!(monitor.actual(), 0);
    }

    #[test]
    fn test_parallelism_monitor_task_start_end() {
        let monitor = ParallelismMonitor::new(4);
        monitor.on_task_start();
        assert_eq!(monitor.actual(), 1);
        monitor.on_task_end();
        assert_eq!(monitor.actual(), 0);
        assert_eq!(monitor.total_executed(), 1);
    }

    #[test]
    fn test_parallelism_monitor_peak() {
        let monitor = ParallelismMonitor::new(4);
        monitor.on_task_start();
        monitor.on_task_start();
        assert_eq!(monitor.peak(), 2);
        monitor.on_task_end();
        monitor.on_task_end();
        assert_eq!(monitor.peak(), 2);
    }

    #[test]
    fn test_parallelism_monitor_utilization() {
        let monitor = ParallelismMonitor::new(4);
        monitor.on_task_start();
        monitor.on_task_start();
        assert!((monitor.utilization_rate() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_parallelism_monitor_set_target() {
        let monitor = ParallelismMonitor::new(4);
        monitor.set_target(8);
        assert_eq!(monitor.target(), 8);
    }

    #[test]
    fn test_parallelism_monitor_uptime() {
        let monitor = ParallelismMonitor::new(4);
        std::thread::sleep(Duration::from_millis(10));
        assert!(monitor.uptime().as_millis() >= 10);
    }
}
