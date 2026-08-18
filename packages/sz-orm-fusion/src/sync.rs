//! 数据同步器（Data Synchronizer）
//!
//! 在多个数据源之间同步数据，支持全量同步和增量同步。
//! 适用于主从同步、缓存刷新等场景。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// 同步方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SyncDirection {
    PrimaryToReplica,
    PrimaryToCache,
    ReplicaToPrimary,
    Bidirectional,
}

impl SyncDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncDirection::PrimaryToReplica => "primary_to_replica",
            SyncDirection::PrimaryToCache => "primary_to_cache",
            SyncDirection::ReplicaToPrimary => "replica_to_primary",
            SyncDirection::Bidirectional => "bidirectional",
        }
    }
}

/// 同步状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SyncState {
    Idle,
    Running,
    Completed,
    Failed,
    Partial,
}

impl SyncState {
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncState::Idle => "idle",
            SyncState::Running => "running",
            SyncState::Completed => "completed",
            SyncState::Failed => "failed",
            SyncState::Partial => "partial",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            SyncState::Completed | SyncState::Failed | SyncState::Partial
        )
    }
}

/// 同步任务
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncTask {
    pub id: String,
    pub source: String,
    pub target: String,
    pub direction: SyncDirection,
    pub table: String,
    pub batch_size: u64,
    pub last_synced_id: Option<u64>,
}

impl SyncTask {
    pub fn new(
        id: &str,
        source: &str,
        target: &str,
        direction: SyncDirection,
        table: &str,
    ) -> Self {
        Self {
            id: id.to_string(),
            source: source.to_string(),
            target: target.to_string(),
            direction,
            table: table.to_string(),
            batch_size: 1000,
            last_synced_id: None,
        }
    }

    pub fn with_batch_size(mut self, size: u64) -> Self {
        self.batch_size = size;
        self
    }

    pub fn with_last_synced(mut self, id: u64) -> Self {
        self.last_synced_id = Some(id);
        self
    }
}

/// 同步结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncResult {
    pub task_id: String,
    pub state: SyncState,
    pub rows_synced: u64,
    pub rows_skipped: u64,
    pub rows_failed: u64,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

impl SyncResult {
    pub fn success(task_id: &str, rows: u64, elapsed_ms: u64) -> Self {
        Self {
            task_id: task_id.to_string(),
            state: SyncState::Completed,
            rows_synced: rows,
            rows_skipped: 0,
            rows_failed: 0,
            elapsed_ms,
            error: None,
        }
    }

    pub fn failure(task_id: &str, error: &str, elapsed_ms: u64) -> Self {
        Self {
            task_id: task_id.to_string(),
            state: SyncState::Failed,
            rows_synced: 0,
            rows_skipped: 0,
            rows_failed: 0,
            elapsed_ms,
            error: Some(error.to_string()),
        }
    }

    pub fn partial(task_id: &str, synced: u64, failed: u64, elapsed_ms: u64) -> Self {
        Self {
            task_id: task_id.to_string(),
            state: SyncState::Partial,
            rows_synced: synced,
            rows_skipped: 0,
            rows_failed: failed,
            elapsed_ms,
            error: None,
        }
    }
}

/// 同步统计
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct SyncStats {
    pub total_tasks: u64,
    pub completed_tasks: u64,
    pub failed_tasks: u64,
    pub total_rows_synced: u64,
    pub total_rows_failed: u64,
    pub total_elapsed_ms: u64,
}

/// 数据同步器
pub struct DataSynchronizer {
    tasks: RwLock<HashMap<String, SyncTask>>,
    results: RwLock<HashMap<String, SyncResult>>,
    stats: RwLock<SyncStats>,
    default_batch_size: u64,
    sync_count: AtomicU64,
}

impl DataSynchronizer {
    pub fn new() -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
            results: RwLock::new(HashMap::new()),
            stats: RwLock::new(SyncStats::default()),
            default_batch_size: 1000,
            sync_count: AtomicU64::new(0),
        }
    }

    pub fn with_batch_size(mut self, size: u64) -> Self {
        self.default_batch_size = size;
        self
    }

    pub fn register_task(&self, task: SyncTask) -> Result<(), String> {
        let mut tasks = self.tasks.write().map_err(|e| e.to_string())?;
        if tasks.contains_key(&task.id) {
            return Err(format!("task {} already exists", task.id));
        }
        tasks.insert(task.id.clone(), task);
        Ok(())
    }

    pub fn remove_task(&self, task_id: &str) -> Option<SyncTask> {
        self.tasks.write().ok().and_then(|mut t| t.remove(task_id))
    }

    pub fn task(&self, task_id: &str) -> Option<SyncTask> {
        self.tasks.read().ok().and_then(|t| t.get(task_id).cloned())
    }

    pub fn task_count(&self) -> usize {
        self.tasks.read().map(|t| t.len()).unwrap_or(0)
    }

    pub fn all_tasks(&self) -> Vec<SyncTask> {
        self.tasks
            .read()
            .ok()
            .map(|t| t.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn execute_sync<F>(&self, task_id: &str, sync_fn: F) -> Result<SyncResult, String>
    where
        F: FnOnce(&SyncTask) -> Result<u64, String>,
    {
        self.sync_count.fetch_add(1, Ordering::Relaxed);
        let start = Instant::now();
        let task = {
            let tasks = self.tasks.read().map_err(|e| e.to_string())?;
            tasks
                .get(task_id)
                .cloned()
                .ok_or_else(|| format!("task {} not found", task_id))?
        };
        match sync_fn(&task) {
            Ok(rows) => {
                let result = SyncResult::success(task_id, rows, start.elapsed().as_millis() as u64);
                self.record_result(result.clone());
                Ok(result)
            }
            Err(e) => {
                let result = SyncResult::failure(task_id, &e, start.elapsed().as_millis() as u64);
                self.record_result(result.clone());
                Err(e)
            }
        }
    }

    fn record_result(&self, result: SyncResult) {
        if let Ok(mut stats) = self.stats.write() {
            stats.total_tasks += 1;
            stats.total_rows_synced += result.rows_synced;
            stats.total_rows_failed += result.rows_failed;
            stats.total_elapsed_ms += result.elapsed_ms;
            match result.state {
                SyncState::Completed => stats.completed_tasks += 1,
                SyncState::Failed => stats.failed_tasks += 1,
                _ => {}
            }
        }
        if let Ok(mut results) = self.results.write() {
            results.insert(result.task_id.clone(), result);
        }
    }

    pub fn result(&self, task_id: &str) -> Option<SyncResult> {
        self.results
            .read()
            .ok()
            .and_then(|r| r.get(task_id).cloned())
    }

    pub fn stats(&self) -> SyncStats {
        self.stats.read().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn sync_count(&self) -> u64 {
        self.sync_count.load(Ordering::Relaxed)
    }

    pub fn default_batch_size(&self) -> u64 {
        self.default_batch_size
    }

    pub fn all_results(&self) -> Vec<SyncResult> {
        self.results
            .read()
            .ok()
            .map(|r| r.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn clear_results(&self) {
        if let Ok(mut results) = self.results.write() {
            results.clear();
        }
    }
}

impl Default for DataSynchronizer {
    fn default() -> Self {
        Self::new()
    }
}

/// 同步调度器
///
/// 定期执行同步任务。
pub struct SyncScheduler {
    synchronizer: Arc<DataSynchronizer>,
    interval: Duration,
    last_run: RwLock<Option<Instant>>,
}

impl SyncScheduler {
    pub fn new(synchronizer: Arc<DataSynchronizer>, interval: Duration) -> Self {
        Self {
            synchronizer,
            interval,
            last_run: RwLock::new(None),
        }
    }

    pub fn should_run(&self) -> bool {
        match self.last_run.read().ok().and_then(|r| *r) {
            None => true,
            Some(last) => last.elapsed() >= self.interval,
        }
    }

    pub fn run_all<F>(&self, sync_fn: F) -> Vec<SyncResult>
    where
        F: Fn(&SyncTask) -> Result<u64, String>,
    {
        let tasks = self.synchronizer.all_tasks();
        let mut results = Vec::new();
        for task in &tasks {
            if let Ok(result) = self.synchronizer.execute_sync(&task.id, &sync_fn) {
                results.push(result);
            }
        }
        if let Ok(mut last) = self.last_run.write() {
            *last = Some(Instant::now());
        }
        results
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    pub fn synchronizer(&self) -> &Arc<DataSynchronizer> {
        &self.synchronizer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_direction_as_str() {
        assert_eq!(
            SyncDirection::PrimaryToReplica.as_str(),
            "primary_to_replica"
        );
        assert_eq!(SyncDirection::Bidirectional.as_str(), "bidirectional");
    }

    #[test]
    fn test_sync_state_as_str() {
        assert_eq!(SyncState::Idle.as_str(), "idle");
        assert_eq!(SyncState::Running.as_str(), "running");
    }

    #[test]
    fn test_sync_state_is_terminal() {
        assert!(SyncState::Completed.is_terminal());
        assert!(SyncState::Failed.is_terminal());
        assert!(!SyncState::Running.is_terminal());
    }

    #[test]
    fn test_sync_task_new() {
        let task = SyncTask::new(
            "t1",
            "primary",
            "replica",
            SyncDirection::PrimaryToReplica,
            "users",
        );
        assert_eq!(task.id, "t1");
        assert_eq!(task.batch_size, 1000);
    }

    #[test]
    fn test_sync_task_with_batch_size() {
        let task = SyncTask::new("t1", "p", "r", SyncDirection::PrimaryToReplica, "t")
            .with_batch_size(500);
        assert_eq!(task.batch_size, 500);
    }

    #[test]
    fn test_sync_task_with_last_synced() {
        let task = SyncTask::new("t1", "p", "r", SyncDirection::PrimaryToReplica, "t")
            .with_last_synced(100);
        assert_eq!(task.last_synced_id, Some(100));
    }

    #[test]
    fn test_sync_result_success() {
        let r = SyncResult::success("t1", 100, 50);
        assert_eq!(r.state, SyncState::Completed);
        assert_eq!(r.rows_synced, 100);
    }

    #[test]
    fn test_sync_result_failure() {
        let r = SyncResult::failure("t1", "error", 50);
        assert_eq!(r.state, SyncState::Failed);
        assert!(r.error.is_some());
    }

    #[test]
    fn test_sync_result_partial() {
        let r = SyncResult::partial("t1", 80, 20, 50);
        assert_eq!(r.state, SyncState::Partial);
        assert_eq!(r.rows_synced, 80);
        assert_eq!(r.rows_failed, 20);
    }

    #[test]
    fn test_data_synchronizer_register_task() {
        let sync = DataSynchronizer::new();
        let task = SyncTask::new("t1", "p", "r", SyncDirection::PrimaryToReplica, "users");
        assert!(sync.register_task(task).is_ok());
        assert_eq!(sync.task_count(), 1);
    }

    #[test]
    fn test_data_synchronizer_duplicate_task() {
        let sync = DataSynchronizer::new();
        let task = SyncTask::new("t1", "p", "r", SyncDirection::PrimaryToReplica, "users");
        sync.register_task(task).unwrap();
        let task2 = SyncTask::new("t1", "p", "r", SyncDirection::PrimaryToReplica, "users");
        assert!(sync.register_task(task2).is_err());
    }

    #[test]
    fn test_data_synchronizer_execute_success() {
        let sync = DataSynchronizer::new();
        let task = SyncTask::new("t1", "p", "r", SyncDirection::PrimaryToReplica, "users");
        sync.register_task(task).unwrap();
        let result = sync.execute_sync("t1", |_task| Ok(100)).unwrap();
        assert_eq!(result.state, SyncState::Completed);
        assert_eq!(result.rows_synced, 100);
    }

    #[test]
    fn test_data_synchronizer_execute_failure() {
        let sync = DataSynchronizer::new();
        let task = SyncTask::new("t1", "p", "r", SyncDirection::PrimaryToReplica, "users");
        sync.register_task(task).unwrap();
        let result = sync
            .execute_sync("t1", |_task| Err("connection failed".to_string()))
            .unwrap_err();
        assert_eq!(result, "connection failed");
    }

    #[test]
    fn test_data_synchronizer_task_not_found() {
        let sync = DataSynchronizer::new();
        assert!(sync.execute_sync("nonexistent", |_| Ok(0)).is_err());
    }

    #[test]
    fn test_data_synchronizer_stats() {
        let sync = DataSynchronizer::new();
        sync.register_task(SyncTask::new(
            "t1",
            "p",
            "r",
            SyncDirection::PrimaryToReplica,
            "users",
        ))
        .unwrap();
        sync.execute_sync("t1", |_| Ok(100)).unwrap();
        let stats = sync.stats();
        assert_eq!(stats.total_tasks, 1);
        assert_eq!(stats.completed_tasks, 1);
        assert_eq!(stats.total_rows_synced, 100);
    }

    #[test]
    fn test_data_synchronizer_remove_task() {
        let sync = DataSynchronizer::new();
        sync.register_task(SyncTask::new(
            "t1",
            "p",
            "r",
            SyncDirection::PrimaryToReplica,
            "users",
        ))
        .unwrap();
        assert!(sync.remove_task("t1").is_some());
        assert_eq!(sync.task_count(), 0);
    }

    #[test]
    fn test_data_synchronizer_all_tasks() {
        let sync = DataSynchronizer::new();
        sync.register_task(SyncTask::new(
            "t1",
            "p",
            "r",
            SyncDirection::PrimaryToReplica,
            "users",
        ))
        .unwrap();
        sync.register_task(SyncTask::new(
            "t2",
            "p",
            "c",
            SyncDirection::PrimaryToCache,
            "orders",
        ))
        .unwrap();
        assert_eq!(sync.all_tasks().len(), 2);
    }

    #[test]
    fn test_data_synchronizer_result() {
        let sync = DataSynchronizer::new();
        sync.register_task(SyncTask::new(
            "t1",
            "p",
            "r",
            SyncDirection::PrimaryToReplica,
            "users",
        ))
        .unwrap();
        sync.execute_sync("t1", |_| Ok(50)).unwrap();
        let result = sync.result("t1").unwrap();
        assert_eq!(result.rows_synced, 50);
    }

    #[test]
    fn test_data_synchronizer_sync_count() {
        let sync = DataSynchronizer::new();
        sync.register_task(SyncTask::new(
            "t1",
            "p",
            "r",
            SyncDirection::PrimaryToReplica,
            "users",
        ))
        .unwrap();
        sync.execute_sync("t1", |_| Ok(10)).unwrap();
        sync.execute_sync("t1", |_| Ok(20)).unwrap();
        assert_eq!(sync.sync_count(), 2);
    }

    #[test]
    fn test_data_synchronizer_with_batch_size() {
        let sync = DataSynchronizer::new().with_batch_size(500);
        assert_eq!(sync.default_batch_size(), 500);
    }

    #[test]
    fn test_data_synchronizer_clear_results() {
        let sync = DataSynchronizer::new();
        sync.register_task(SyncTask::new(
            "t1",
            "p",
            "r",
            SyncDirection::PrimaryToReplica,
            "users",
        ))
        .unwrap();
        sync.execute_sync("t1", |_| Ok(10)).unwrap();
        sync.clear_results();
        assert!(sync.result("t1").is_none());
    }

    #[test]
    fn test_sync_scheduler_should_run_initial() {
        let sync = Arc::new(DataSynchronizer::new());
        let scheduler = SyncScheduler::new(sync, Duration::from_secs(60));
        assert!(scheduler.should_run());
    }

    #[test]
    fn test_sync_scheduler_run_all() {
        let sync = Arc::new(DataSynchronizer::new());
        sync.register_task(SyncTask::new(
            "t1",
            "p",
            "r",
            SyncDirection::PrimaryToReplica,
            "users",
        ))
        .unwrap();
        let scheduler = SyncScheduler::new(sync, Duration::from_secs(60));
        let results = scheduler.run_all(|_| Ok(100));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].rows_synced, 100);
    }

    #[test]
    fn test_sync_scheduler_interval() {
        let sync = Arc::new(DataSynchronizer::new());
        let scheduler = SyncScheduler::new(sync, Duration::from_secs(30));
        assert_eq!(scheduler.interval(), Duration::from_secs(30));
    }
}
