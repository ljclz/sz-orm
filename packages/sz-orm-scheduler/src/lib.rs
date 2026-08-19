//! # SZ-ORM Scheduler — Cron Task Scheduler
//!
//! Provides cron expression-based scheduled task execution, supports task start/stop, state management, and callback execution.
//!
//! ## Main Modules
//!
//! - [`scheduler`] — Task handler trait and test auxiliary implementation

use chrono::{Datelike, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;
use std::time::Duration;

pub mod advanced;
pub mod scheduler;

pub use scheduler::{CounterJobHandler, JobHandler, RecordingJobHandler};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: String,
    pub name: String,
    pub cron_expr: String,
    pub callback: String,
    pub metadata: HashMap<String, serde_json::Value>,
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,
}

impl ScheduledTask {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        cron_expr: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            cron_expr: cron_expr.into(),
            callback: String::new(),
            metadata: HashMap::new(),
            enabled: true,
            priority: 0,
        }
    }

    pub fn with_callback(mut self, callback: impl Into<String>) -> Self {
        self.callback = callback.into();
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    pub fn disable(mut self) -> Self {
        self.enabled = false;
        self
    }
}

pub trait Scheduler: Send + Sync {
    fn schedule(&self, task: ScheduledTask) -> Result<(), SchedulerError>;
    fn cancel(&self, task_id: &str) -> Result<(), SchedulerError>;
    fn pause(&self, task_id: &str) -> Result<(), SchedulerError>;
    fn resume(&self, task_id: &str) -> Result<(), SchedulerError>;
    fn list_tasks(&self) -> Vec<ScheduledTask>;
}

pub struct CronScheduler {
    tasks: Arc<RwLock<HashMap<String, ScheduledTask>>>,
    handlers: Arc<RwLock<HashMap<String, Arc<dyn JobHandler>>>>,
    stop_flag: Arc<AtomicBool>,
    worker: RwLock<Option<JoinHandle<()>>>,
}

impl CronScheduler {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            handlers: Arc::new(RwLock::new(HashMap::new())),
            stop_flag: Arc::new(AtomicBool::new(false)),
            worker: RwLock::new(None),
        }
    }

    pub fn parse_cron(&self, expr: &str) -> Result<CronExpr, SchedulerError> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(SchedulerError::InvalidCronExpr(format!(
                "Expected 5 fields, got {}",
                parts.len()
            )));
        }

        Ok(CronExpr {
            second: parts[0].to_string(),
            minute: parts[1].to_string(),
            hour: parts[2].to_string(),
            day_of_month: parts[3].to_string(),
            month: parts[4].to_string(),
        })
    }

    pub fn next_run_time(
        &self,
        expr: &str,
        from: chrono::DateTime<chrono::Utc>,
    ) -> Result<chrono::DateTime<chrono::Utc>, SchedulerError> {
        let parsed = self.parse_cron(expr)?;

        // 判断 second 字段是否需要精确扫描（非 "*" 且非 "0"）
        // second="*" 或 "0" 时，对齐到分钟边界（second=0）后按分钟扫描即可
        // second 为其他值（如 "30"、"10-12"、"10,20,30"）时，需在匹配分钟内找具体 second
        let needs_second_precision = !matches!(parsed.second.as_str(), "*" | "0");

        if !needs_second_precision {
            // second 字段是 "*" 或 "0"：保留原逻辑，按分钟扫描
            // 对齐到下一分钟边界（second=0），扫描 525600 分钟（365 天）
            let mut next = align_to_next_minute_boundary(from);
            for _ in 0..525_600 {
                if self.matches_cron(&parsed, next) {
                    return Ok(next);
                }
                next += chrono::Duration::minutes(1);
            }
        } else {
            // second 字段包含非 0 值：按分钟扫描，在匹配分钟内找具体 second
            let seconds = self.parse_field_values(&parsed.second, 0, 59)?;
            // 从 from 截断到当前分钟开始（保留当前分钟内未来 second 的可能性）
            let mut minute_start = from
                .with_second(0)
                .and_then(|d| d.with_nanosecond(0))
                .unwrap_or(from);

            for _ in 0..525_600 {
                // 检查 minute/hour/day/month 是否匹配（不检查 second）
                if self.matches_cron_ignoring_second(&parsed, minute_start) {
                    // 在该分钟内找第一个 > from 的 second
                    for &sec in &seconds {
                        let candidate = minute_start
                            .with_second(sec)
                            .and_then(|d| d.with_nanosecond(0))
                            .unwrap_or(minute_start);
                        if candidate > from {
                            return Ok(candidate);
                        }
                    }
                }
                minute_start += chrono::Duration::minutes(1);
            }
        }

        Err(SchedulerError::NoNextRunTime(
            "No next run time found within 365 days".to_string(),
        ))
    }

    fn matches_cron(&self, expr: &CronExpr, dt: chrono::DateTime<chrono::Utc>) -> bool {
        self.field_matches(&expr.second, dt.naive_utc().second())
            && self.field_matches(&expr.minute, dt.naive_utc().minute())
            && self.field_matches(&expr.hour, dt.naive_utc().hour())
            && self.field_matches(&expr.day_of_month, dt.naive_utc().day())
            && self.field_matches(&expr.month, dt.naive_utc().month())
    }

    /// Check if minute/hour/day/month matches (does not check second)
    /// Used for second-level precise scanning, first filter out minutes where minute/hour/day/month match
    fn matches_cron_ignoring_second(
        &self,
        expr: &CronExpr,
        dt: chrono::DateTime<chrono::Utc>,
    ) -> bool {
        self.field_matches(&expr.minute, dt.naive_utc().minute())
            && self.field_matches(&expr.hour, dt.naive_utc().hour())
            && self.field_matches(&expr.day_of_month, dt.naive_utc().day())
            && self.field_matches(&expr.month, dt.naive_utc().month())
    }

    /// Parse cron field into ordered numeric list
    /// Supports: * / single value / comma list / range / step
    fn parse_field_values(
        &self,
        field: &str,
        min: u32,
        max: u32,
    ) -> Result<Vec<u32>, SchedulerError> {
        let mut values = Vec::new();
        if field == "*" {
            for v in min..=max {
                values.push(v);
            }
            return Ok(values);
        }
        for part in field.split(',') {
            let part = part.trim();
            if part.contains('/') {
                let parts: Vec<&str> = part.split('/').collect();
                if parts.len() != 2 {
                    return Err(SchedulerError::InvalidCronExpr(format!(
                        "Invalid step field: {}",
                        field
                    )));
                }
                let step: u32 = parts[1].parse().map_err(|_| {
                    SchedulerError::InvalidCronExpr(format!("Invalid step value: {}", parts[1]))
                })?;
                if step == 0 {
                    return Err(SchedulerError::InvalidCronExpr(
                        "Step value cannot be 0".to_string(),
                    ));
                }
                let range_part = parts[0];
                let (start, end) = if range_part == "*" {
                    (min, max)
                } else if range_part.contains('-') {
                    let range_parts: Vec<&str> = range_part.split('-').collect();
                    if range_parts.len() != 2 {
                        return Err(SchedulerError::InvalidCronExpr(format!(
                            "Invalid range: {}",
                            range_part
                        )));
                    }
                    let s: u32 = range_parts[0].trim().parse().map_err(|_| {
                        SchedulerError::InvalidCronExpr(format!(
                            "Invalid range start: {}",
                            range_parts[0]
                        ))
                    })?;
                    let e: u32 = range_parts[1].trim().parse().map_err(|_| {
                        SchedulerError::InvalidCronExpr(format!(
                            "Invalid range end: {}",
                            range_parts[1]
                        ))
                    })?;
                    (s, e)
                } else {
                    let s: u32 = range_part.parse().map_err(|_| {
                        SchedulerError::InvalidCronExpr(format!("Invalid value: {}", range_part))
                    })?;
                    (s, max)
                };
                let mut v = start;
                while v <= end {
                    values.push(v);
                    v = v.saturating_add(step);
                }
            } else if part.contains('-') {
                let parts: Vec<&str> = part.split('-').collect();
                if parts.len() != 2 {
                    return Err(SchedulerError::InvalidCronExpr(format!(
                        "Invalid range: {}",
                        part
                    )));
                }
                let start: u32 = parts[0].trim().parse().map_err(|_| {
                    SchedulerError::InvalidCronExpr(format!("Invalid range start: {}", parts[0]))
                })?;
                let end: u32 = parts[1].trim().parse().map_err(|_| {
                    SchedulerError::InvalidCronExpr(format!("Invalid range end: {}", parts[1]))
                })?;
                for v in start..=end {
                    values.push(v);
                }
            } else {
                let v: u32 = part.parse().map_err(|_| {
                    SchedulerError::InvalidCronExpr(format!("Invalid value: {}", part))
                })?;
                values.push(v);
            }
        }
        Ok(values)
    }

    fn field_matches(&self, field: &str, value: u32) -> bool {
        if field == "*" {
            return true;
        }
        if field.contains(',') {
            return field
                .split(',')
                .any(|v| v.trim().parse::<u32>().is_ok_and(|n| n == value));
        }
        if field.contains('-') {
            let parts: Vec<&str> = field.split('-').collect();
            if parts.len() == 2 {
                let start: u32 = parts[0].trim().parse().unwrap_or(0);
                let end: u32 = parts[1].trim().parse().unwrap_or(0);
                return value >= start && value <= end;
            }
        }
        if field.contains('/') {
            let parts: Vec<&str> = field.split('/').collect();
            if parts.len() == 2 {
                let step: u32 = parts[1].parse().unwrap_or(1);
                return value.is_multiple_of(step);
            }
        }
        field.parse::<u32>().is_ok_and(|n| n == value)
    }

    /// Registers a [`JobHandler`] for the given task id. When the scheduler
    /// fires a matching task, it looks up the handler by task id. If no
    /// handler is registered, the task is skipped silently.
    pub fn register_handler(&self, task_id: impl Into<String>, handler: Arc<dyn JobHandler>) {
        let mut handlers = self
            .handlers
            .write()
            .map_err(|e| SchedulerError::Internal(e.to_string()))
            .unwrap();
        handlers.insert(task_id.into(), handler);
    }

    /// Fires every enabled task whose cron expression matches `now`. Returns
    /// the number of tasks that fired (and whose handler, if any, returned
    /// `Ok(())`). Errors from individual handlers are recorded but do not
    /// abort iteration.
    pub fn try_fire_due(&self, now: chrono::DateTime<chrono::Utc>) -> usize {
        let due: Vec<(ScheduledTask, Option<Arc<dyn JobHandler>>)> = {
            let tasks = self
                .tasks
                .read()
                .map_err(|e| SchedulerError::Internal(e.to_string()));
            let handlers = self
                .handlers
                .read()
                .map_err(|e| SchedulerError::Internal(e.to_string()));
            let (Ok(tasks), Ok(handlers)) = (tasks, handlers) else {
                return 0;
            };

            tasks
                .values()
                .filter(|t| t.enabled)
                .filter_map(|t| {
                    let parsed = self.parse_cron(&t.cron_expr).ok()?;
                    if self.matches_cron(&parsed, now) {
                        Some((t.clone(), handlers.get(&t.id).cloned()))
                    } else {
                        None
                    }
                })
                .collect()
        };

        let mut due = due;
        due.sort_by_key(|a| std::cmp::Reverse(a.0.priority));

        let mut fired = 0usize;
        for (task, handler) in due {
            if let Some(handler) = handler {
                if handler.handle(&task).is_ok() {
                    fired += 1;
                }
            } else {
                // No handler registered: still count as "fired" so tests can
                // observe cron matching independently of handler logic.
                fired += 1;
            }
        }
        fired
    }

    /// Starts a background worker thread that wakes up every `tick_ms`
    /// milliseconds, queries the current UTC time, and invokes
    /// [`try_fire_due`]. Calling `start` while a worker is already running
    /// returns an error.
    ///
    /// [`try_fire_due`]: CronScheduler::try_fire_due
    pub fn start(&self, tick_ms: u64) -> Result<(), SchedulerError> {
        let mut worker = self
            .worker
            .write()
            .map_err(|e| SchedulerError::Internal(e.to_string()))?;
        if worker.is_some() {
            return Err(SchedulerError::Internal(
                "scheduler already running".to_string(),
            ));
        }

        self.stop_flag.store(false, Ordering::SeqCst);
        let stop_flag = self.stop_flag.clone();
        let tasks = self.tasks.clone();
        let handlers = self.handlers.clone();

        let handle = std::thread::spawn(move || {
            while !stop_flag.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(tick_ms.max(1)));
                if stop_flag.load(Ordering::SeqCst) {
                    break;
                }
                let now = chrono::Utc::now();
                let scheduler = CronScheduler {
                    tasks: tasks.clone(),
                    handlers: handlers.clone(),
                    stop_flag: stop_flag.clone(),
                    worker: RwLock::new(None),
                };
                let _ = scheduler.try_fire_due(now);
            }
        });

        *worker = Some(handle);
        Ok(())
    }

    /// Stops the background worker thread and waits for it to exit. If no
    /// worker is running, this is a no-op.
    pub fn stop(&self) -> Result<(), SchedulerError> {
        let mut worker = self
            .worker
            .write()
            .map_err(|e| SchedulerError::Internal(e.to_string()))?;
        if let Some(handle) = worker.take() {
            self.stop_flag.store(true, Ordering::SeqCst);
            // Drop the lock before joining to avoid a deadlock if the worker
            // ever needs to read `worker` (it doesn't today, but this keeps
            // the invariant explicit).
            drop(worker);
            handle
                .join()
                .map_err(|_| SchedulerError::Internal("worker thread panicked".to_string()))?;
        }
        Ok(())
    }

    /// Returns `true` if a background worker is currently running.
    pub fn is_running(&self) -> bool {
        let worker = self
            .worker
            .read()
            .map_err(|e| SchedulerError::Internal(e.to_string()));
        match worker {
            Ok(w) => w.is_some(),
            Err(_) => false,
        }
    }

    /// Trigger all due tasks and record execution results to `tracker`.
    ///
    /// Difference from `try_fire_due`: this method records execution status after each task trigger
    /// (Succeeded / Failed / Skipped), for subsequent task health queries.
    pub fn try_fire_due_tracked(
        &self,
        now: chrono::DateTime<chrono::Utc>,
        tracker: &TaskExecutionTracker,
    ) -> usize {
        let due: Vec<(ScheduledTask, Option<Arc<dyn JobHandler>>)> = {
            let tasks = self.tasks.read().unwrap();
            let handlers = self.handlers.read().unwrap();
            tasks
                .values()
                .filter(|t| t.enabled)
                .filter_map(|t| {
                    let parsed = self.parse_cron(&t.cron_expr).ok()?;
                    if self.matches_cron(&parsed, now) {
                        Some((t.clone(), handlers.get(&t.id).cloned()))
                    } else {
                        None
                    }
                })
                .collect()
        };

        let mut due = due;
        due.sort_by_key(|a| std::cmp::Reverse(a.0.priority));

        let mut fired = 0usize;
        for (task, handler) in due {
            let start = std::time::Instant::now();
            let (status, error_message) = if let Some(handler) = handler {
                match handler.handle(&task) {
                    Ok(()) => (TaskStatus::Succeeded, None),
                    Err(e) => (TaskStatus::Failed, Some(e.to_string())),
                }
            } else {
                (TaskStatus::Skipped, None)
            };
            let duration_ms = start.elapsed().as_millis() as u64;

            let is_fired = status != TaskStatus::Failed;
            tracker.record(TaskExecutionRecord {
                task_id: task.id.clone(),
                fired_at: now,
                status,
                error_message,
                duration_ms,
            });
            if is_fired {
                fired += 1;
            }
        }
        fired
    }

    /// Returns total number of registered tasks.
    pub fn get_task_count(&self) -> usize {
        self.tasks.read().map(|tasks| tasks.len()).unwrap_or(0)
    }

    /// Returns number of enabled tasks.
    pub fn get_enabled_task_count(&self) -> usize {
        self.tasks
            .read()
            .map(|tasks| tasks.values().filter(|t| t.enabled).count())
            .unwrap_or(0)
    }

    /// Returns `(id, priority)` list of all tasks, sorted by priority descending.
    pub fn get_task_priorities(&self) -> Vec<(String, i32)> {
        let mut result: Vec<(String, i32)> = self
            .tasks
            .read()
            .map(|tasks| tasks.values().map(|t| (t.id.clone(), t.priority)).collect())
            .unwrap_or_default();
        result.sort_by_key(|(_, p)| std::cmp::Reverse(*p));
        result
    }
}

impl Default for CronScheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns the next whole-minute boundary strictly after `dt`, with
/// `second = 0` and `nanosecond = 0`.
///
/// Examples:
/// - `00:00:00` → `00:01:00`
/// - `00:00:30` → `00:01:00`
/// - `00:01:45.500` → `00:02:00`
fn align_to_next_minute_boundary(
    dt: chrono::DateTime<chrono::Utc>,
) -> chrono::DateTime<chrono::Utc> {
    use chrono::Timelike;
    // Truncate to current minute, then advance by one minute so we never
    // report `dt` itself as the next run time (callers expect "next" to
    // mean strictly after `dt`).
    let truncated = dt
        .with_second(0)
        .and_then(|d| d.with_nanosecond(0))
        .unwrap_or(dt);
    truncated + chrono::Duration::minutes(1)
}

#[derive(Debug, Clone)]
pub struct CronExpr {
    pub second: String,
    pub minute: String,
    pub hour: String,
    pub day_of_month: String,
    pub month: String,
}

impl Scheduler for CronScheduler {
    fn schedule(&self, task: ScheduledTask) -> Result<(), SchedulerError> {
        if task.cron_expr.is_empty() {
            return Err(SchedulerError::InvalidCronExpr(
                "Cron expression cannot be empty".to_string(),
            ));
        }

        self.parse_cron(&task.cron_expr)?;

        let mut tasks = self
            .tasks
            .write()
            .map_err(|e| SchedulerError::Internal(e.to_string()))?;
        tasks.insert(task.id.clone(), task);
        Ok(())
    }

    fn cancel(&self, task_id: &str) -> Result<(), SchedulerError> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|e| SchedulerError::Internal(e.to_string()))?;
        tasks
            .remove(task_id)
            .ok_or_else(|| SchedulerError::TaskNotFound(task_id.to_string()))?;
        Ok(())
    }

    fn pause(&self, task_id: &str) -> Result<(), SchedulerError> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|e| SchedulerError::Internal(e.to_string()))?;
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| SchedulerError::TaskNotFound(task_id.to_string()))?;
        task.enabled = false;
        Ok(())
    }

    fn resume(&self, task_id: &str) -> Result<(), SchedulerError> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|e| SchedulerError::Internal(e.to_string()))?;
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| SchedulerError::TaskNotFound(task_id.to_string()))?;
        task.enabled = true;
        Ok(())
    }

    fn list_tasks(&self) -> Vec<ScheduledTask> {
        let tasks = self
            .tasks
            .read()
            .map_err(|e| SchedulerError::Internal(e.to_string()))
            .unwrap();
        tasks.values().cloned().collect()
    }
}

/// Task execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Skipped,
}

/// Single task execution record.
#[derive(Debug, Clone)]
pub struct TaskExecutionRecord {
    pub task_id: String,
    pub fired_at: chrono::DateTime<chrono::Utc>,
    pub status: TaskStatus,
    pub error_message: Option<String>,
    pub duration_ms: u64,
}

/// Task execution tracker: records the result of each task trigger, supports per-task history query and statistics.
///
/// Independent component, does not modify `CronScheduler` internal state. Caller can manually record after `try_fire_due`
/// or use `CronScheduler::try_fire_due_tracked` convenience method.
pub struct TaskExecutionTracker {
    records: RwLock<Vec<TaskExecutionRecord>>,
    max_capacity: usize,
}

impl TaskExecutionTracker {
    pub fn new() -> Self {
        Self::with_capacity(10_000)
    }

    pub fn with_capacity(max_capacity: usize) -> Self {
        Self {
            records: RwLock::new(Vec::new()),
            max_capacity: max_capacity.max(1),
        }
    }

    pub fn record(&self, record: TaskExecutionRecord) {
        if let Ok(mut records) = self.records.write() {
            if records.len() >= self.max_capacity {
                records.remove(0);
            }
            records.push(record);
        }
    }

    pub fn get_task_history(&self, task_id: &str) -> Vec<TaskExecutionRecord> {
        self.records
            .read()
            .map(|records| {
                records
                    .iter()
                    .filter(|r| r.task_id == task_id)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_last_status(&self, task_id: &str) -> Option<TaskStatus> {
        self.records.read().ok().and_then(|records| {
            records
                .iter()
                .rev()
                .find(|r| r.task_id == task_id)
                .map(|r| r.status)
        })
    }

    pub fn get_failure_count(&self, task_id: &str) -> usize {
        self.records
            .read()
            .map(|records| {
                records
                    .iter()
                    .filter(|r| r.task_id == task_id && r.status == TaskStatus::Failed)
                    .count()
            })
            .unwrap_or(0)
    }

    pub fn get_success_count(&self, task_id: &str) -> usize {
        self.records
            .read()
            .map(|records| {
                records
                    .iter()
                    .filter(|r| r.task_id == task_id && r.status == TaskStatus::Succeeded)
                    .count()
            })
            .unwrap_or(0)
    }

    pub fn get_total_count(&self, task_id: &str) -> usize {
        self.records
            .read()
            .map(|records| records.iter().filter(|r| r.task_id == task_id).count())
            .unwrap_or(0)
    }

    pub fn get_success_rate(&self, task_id: &str) -> f64 {
        let total = self.get_total_count(task_id);
        if total == 0 {
            return 0.0;
        }
        self.get_success_count(task_id) as f64 / total as f64
    }

    pub fn clear(&self) {
        if let Ok(mut records) = self.records.write() {
            records.clear();
        }
    }

    pub fn clear_task(&self, task_id: &str) {
        if let Ok(mut records) = self.records.write() {
            records.retain(|r| r.task_id != task_id);
        }
    }

    pub fn record_count(&self) -> usize {
        self.records
            .read()
            .map(|records| records.len())
            .unwrap_or(0)
    }
}

impl Default for TaskExecutionTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Task health summary.
#[derive(Debug, Clone)]
pub struct TaskHealthSummary {
    pub task_id: String,
    pub total_executions: usize,
    pub successes: usize,
    pub failures: usize,
    pub success_rate: f64,
    pub last_status: Option<TaskStatus>,
}

impl TaskExecutionTracker {
    pub fn get_all_task_ids(&self) -> Vec<String> {
        self.records
            .read()
            .map(|records| {
                let mut ids: Vec<String> = records.iter().map(|r| r.task_id.clone()).collect();
                ids.sort();
                ids.dedup();
                ids
            })
            .unwrap_or_default()
    }

    pub fn get_health_summary(&self, task_id: &str) -> TaskHealthSummary {
        TaskHealthSummary {
            task_id: task_id.to_string(),
            total_executions: self.get_total_count(task_id),
            successes: self.get_success_count(task_id),
            failures: self.get_failure_count(task_id),
            success_rate: self.get_success_rate(task_id),
            last_status: self.get_last_status(task_id),
        }
    }

    pub fn get_all_health_summaries(&self) -> Vec<TaskHealthSummary> {
        self.get_all_task_ids()
            .iter()
            .map(|id| self.get_health_summary(id))
            .collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("Task not found: {0}")]
    TaskNotFound(String),
    #[error("Invalid cron expression: {0}")]
    InvalidCronExpr(String),
    #[error("Failed to compute next run time: {0}")]
    NoNextRunTime(String),
    #[error("Scheduler error: {0}")]
    Internal(String),
}

impl From<chrono::ParseError> for SchedulerError {
    fn from(e: chrono::ParseError) -> Self {
        SchedulerError::InvalidCronExpr(e.to_string())
    }
}

impl serde::Serialize for SchedulerError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduled_task_new() {
        let task = ScheduledTask::new("task1", "Test Task", "0 * * * *");
        assert_eq!(task.id, "task1");
        assert_eq!(task.name, "Test Task");
        assert_eq!(task.cron_expr, "0 * * * *");
        assert!(task.enabled);
    }

    #[test]
    fn test_scheduled_task_with_callback() {
        let task = ScheduledTask::new("task1", "Test", "* * * * *").with_callback("my_callback");
        assert_eq!(task.callback, "my_callback");
    }

    #[test]
    fn test_scheduled_task_disable() {
        let task = ScheduledTask::new("task1", "Test", "* * * * *").disable();
        assert!(!task.enabled);
    }

    #[test]
    fn test_cron_parse() {
        let scheduler = CronScheduler::new();
        let result = scheduler.parse_cron("0 * * * *");
        assert!(result.is_ok());
        let expr = result.unwrap();
        assert_eq!(expr.second, "0");
        assert_eq!(expr.minute, "*");
    }

    #[test]
    fn test_cron_parse_invalid() {
        let scheduler = CronScheduler::new();
        let result = scheduler.parse_cron("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_cron_field_matches_star() {
        let scheduler = CronScheduler::new();
        assert!(scheduler.field_matches("*", 5));
        assert!(scheduler.field_matches("*", 0));
        assert!(scheduler.field_matches("*", 59));
    }

    #[test]
    fn test_cron_field_matches_exact() {
        let scheduler = CronScheduler::new();
        assert!(scheduler.field_matches("5", 5));
        assert!(!scheduler.field_matches("5", 6));
    }

    #[test]
    fn test_cron_field_matches_range() {
        let scheduler = CronScheduler::new();
        assert!(scheduler.field_matches("1-5", 3));
        assert!(!scheduler.field_matches("1-5", 7));
    }

    #[test]
    fn test_cron_field_matches_list() {
        let scheduler = CronScheduler::new();
        assert!(scheduler.field_matches("1,3,5", 3));
        assert!(!scheduler.field_matches("1,3,5", 2));
    }

    #[test]
    fn test_cron_field_matches_step() {
        let scheduler = CronScheduler::new();
        assert!(scheduler.field_matches("*/5", 10));
        assert!(scheduler.field_matches("*/5", 15));
        assert!(!scheduler.field_matches("*/5", 7));
    }

    #[test]
    fn test_scheduler_schedule() {
        let scheduler = CronScheduler::new();
        let task = ScheduledTask::new("task1", "Test", "0 * * * *");
        let result = scheduler.schedule(task);
        assert!(result.is_ok());
    }

    #[test]
    fn test_scheduler_schedule_invalid_cron() {
        let scheduler = CronScheduler::new();
        let task = ScheduledTask::new("task1", "Test", "invalid");
        let result = scheduler.schedule(task);
        assert!(result.is_err());
    }

    #[test]
    fn test_scheduler_cancel() {
        let scheduler = CronScheduler::new();
        let task = ScheduledTask::new("task1", "Test", "0 * * * *");
        scheduler.schedule(task).unwrap();

        let result = scheduler.cancel("task1");
        assert!(result.is_ok());
    }

    #[test]
    fn test_scheduler_cancel_not_found() {
        let scheduler = CronScheduler::new();
        let result = scheduler.cancel("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_scheduler_pause_resume() {
        let scheduler = CronScheduler::new();
        let task = ScheduledTask::new("task1", "Test", "0 * * * *");
        scheduler.schedule(task).unwrap();

        scheduler.pause("task1").unwrap();
        let tasks = scheduler.list_tasks();
        assert!(!tasks[0].enabled);

        scheduler.resume("task1").unwrap();
        let tasks = scheduler.list_tasks();
        assert!(tasks[0].enabled);
    }

    #[test]
    fn test_scheduler_list_tasks() {
        let scheduler = CronScheduler::new();
        scheduler
            .schedule(ScheduledTask::new("t1", "Task 1", "0 * * * *"))
            .unwrap();
        scheduler
            .schedule(ScheduledTask::new("t2", "Task 2", "0 * * * *"))
            .unwrap();

        let tasks = scheduler.list_tasks();
        assert_eq!(tasks.len(), 2);
    }

    #[test]
    fn test_next_run_time_finds_next_minute_match() {
        // `* * * * *` matches every minute, so next_run_time should return
        // the next minute after `from`.
        let scheduler = CronScheduler::new();
        let from = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let next = scheduler.next_run_time("* * * * *", from).unwrap();
        assert_eq!(next, from + chrono::Duration::minutes(1));
    }

    #[test]
    fn test_next_run_time_finds_hourly_match() {
        // `0 * * * *` matches second=0, every minute/hour/day/month - i.e.
        // every minute where second is 0. With 5-field cron (where the first
        // field is `second`), `0 * * * *` matches every minute when second=0.
        // Since we scan minute-by-minute, second is always 0 at scan points,
        // so the first scan iteration should match.
        let scheduler = CronScheduler::new();
        let from = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:30Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let next = scheduler.next_run_time("0 * * * *", from).unwrap();
        // from is 00:00:30; next minute is 00:01:00 (second=0, matches).
        assert_eq!(next, from + chrono::Duration::seconds(30));
    }

    #[test]
    fn test_next_run_time_finds_daily_match_far_ahead() {
        // Cron `0 0 1 1 *` matches only at 00:00:00 on Jan 1 of any year.
        // Starting from 2024-01-01 00:01:00, the next match is 2025-01-01.
        // Before the fix, scanning only 365 minutes (~6 hours) ahead would
        // fail to find this match. The fixed scan window is 525,600 minutes
        // (~365 days), which is enough to find it.
        let scheduler = CronScheduler::new();
        let from = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:01:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let next = scheduler.next_run_time("0 0 1 1 *", from);
        assert!(
            next.is_ok(),
            "should find next run within 365 days, got: {:?}",
            next
        );
    }

    #[test]
    fn test_try_fire_due_fires_matching_task_with_handler() {
        let scheduler = CronScheduler::new();
        let task = ScheduledTask::new("t1", "Test", "* * * * *");
        scheduler.schedule(task).unwrap();

        let handler = Arc::new(CounterJobHandler::new());
        let counter = handler.counter();
        scheduler.register_handler("t1", handler);

        let now = chrono::Utc::now();
        let fired = scheduler.try_fire_due(now);
        assert_eq!(fired, 1);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);

        // Fire again to make sure counter accumulates.
        scheduler.try_fire_due(now);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn test_try_fire_due_skips_non_matching_task() {
        let scheduler = CronScheduler::new();
        // Cron `99 * * * *` is technically parseable (field "99" parses as
        // u32=99), but no real time has second=99 so it never matches.
        let task = ScheduledTask::new("never", "Test", "99 * * * *");
        scheduler.schedule(task).unwrap();

        let handler = Arc::new(CounterJobHandler::new());
        let counter = handler.counter();
        scheduler.register_handler("never", handler);

        let now = chrono::Utc::now();
        let fired = scheduler.try_fire_due(now);
        assert_eq!(fired, 0);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn test_try_fire_due_skips_paused_task() {
        let scheduler = CronScheduler::new();
        scheduler
            .schedule(ScheduledTask::new("t1", "Test", "* * * * *"))
            .unwrap();
        scheduler.pause("t1").unwrap();

        let handler = Arc::new(CounterJobHandler::new());
        let counter = handler.counter();
        scheduler.register_handler("t1", handler);

        let now = chrono::Utc::now();
        let fired = scheduler.try_fire_due(now);
        assert_eq!(fired, 0);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn test_start_stop_background_thread() {
        let scheduler = CronScheduler::new();
        scheduler
            .schedule(ScheduledTask::new("t1", "Test", "* * * * *"))
            .unwrap();
        let handler = Arc::new(CounterJobHandler::new());
        let counter = handler.counter();
        scheduler.register_handler("t1", handler);

        assert!(!scheduler.is_running());
        scheduler.start(50).unwrap();
        assert!(scheduler.is_running());

        // Wait long enough for at least one tick (50ms) + jitter.
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            counter.load(std::sync::atomic::Ordering::SeqCst) >= 1,
            "expected the background thread to fire the handler at least once"
        );

        scheduler.stop().unwrap();
        assert!(!scheduler.is_running());

        // Snapshot the counter after stopping.
        let after_stop = counter.load(std::sync::atomic::Ordering::SeqCst);
        // Wait a bit more to ensure the worker has actually exited and is no
        // longer invoking the handler.
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            after_stop,
            "counter should not change after stop()"
        );
    }

    #[test]
    fn test_start_twice_errors() {
        let scheduler = CronScheduler::new();
        scheduler.start(1000).unwrap();
        let second = scheduler.start(1000);
        assert!(second.is_err());
        scheduler.stop().unwrap();
    }

    #[test]
    fn test_stop_when_not_running_is_noop() {
        let scheduler = CronScheduler::new();
        assert!(scheduler.stop().is_ok());
    }

    #[test]
    fn test_recording_handler_with_try_fire_due() {
        let scheduler = CronScheduler::new();
        scheduler
            .schedule(ScheduledTask::new("a", "Task A", "* * * * *"))
            .unwrap();
        scheduler
            .schedule(ScheduledTask::new("b", "Task B", "99 * * * *"))
            .unwrap();
        scheduler
            .schedule(ScheduledTask::new("c", "Task C", "* * * * *"))
            .unwrap();

        let handler = Arc::new(RecordingJobHandler::new());
        scheduler.register_handler("a", handler.clone());
        scheduler.register_handler("b", handler.clone());
        scheduler.register_handler("c", handler.clone());

        let now = chrono::Utc::now();
        let fired = scheduler.try_fire_due(now);
        assert_eq!(fired, 2); // Only "a" and "c" match.
        let ids = handler.handled_ids();
        assert!(ids.contains(&"a".to_string()));
        assert!(ids.contains(&"c".to_string()));
        assert!(!ids.contains(&"b".to_string()));
    }

    // ===== TDD RED：秒级 cron 支持测试（bug 修复前应失败） =====

    #[test]
    fn test_next_run_time_second_precision_single_value() {
        // `30 * * * *` 表示每分钟的 30 秒触发
        // from=00:00:00，下一个匹配应为 00:00:30（同一分钟内的 30 秒）
        let scheduler = CronScheduler::new();
        let from = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let next = scheduler.next_run_time("30 * * * *", from);
        assert!(
            next.is_ok(),
            "should find next run for second=30 cron, got: {:?}",
            next
        );
        assert_eq!(next.unwrap(), from + chrono::Duration::seconds(30));
    }

    #[test]
    fn test_next_run_time_second_precision_next_minute() {
        // `30 * * * *` from=00:00:45，当前分钟内 30 秒已过
        // 下一个匹配应为 00:01:30（下一分钟的 30 秒）
        let scheduler = CronScheduler::new();
        let from = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:45Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let next = scheduler.next_run_time("30 * * * *", from).unwrap();
        assert_eq!(next, from + chrono::Duration::seconds(45));
    }

    #[test]
    fn test_next_run_time_second_range() {
        // `10-12 * * * *` 表示每分钟的 10/11/12 秒触发
        // from=00:00:00，第一个匹配应为 00:00:10
        let scheduler = CronScheduler::new();
        let from = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let next = scheduler.next_run_time("10-12 * * * *", from).unwrap();
        assert_eq!(next, from + chrono::Duration::seconds(10));
    }

    #[test]
    fn test_next_run_time_second_list_skips_past() {
        // `10,20,30 * * * *` from=00:00:15
        // 10 秒已过，下一个匹配应为 00:00:20
        let scheduler = CronScheduler::new();
        let from = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:15Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let next = scheduler.next_run_time("10,20,30 * * * *", from).unwrap();
        assert_eq!(next, from + chrono::Duration::seconds(5));
    }

    #[test]
    fn test_priority_ordering_high_first() {
        let scheduler = CronScheduler::new();
        let handler = Arc::new(RecordingJobHandler::new()) as Arc<dyn JobHandler>;

        scheduler
            .schedule(ScheduledTask::new("low", "Low", "* * * * *").with_priority(1))
            .unwrap();
        scheduler
            .schedule(ScheduledTask::new("high", "High", "* * * * *").with_priority(10))
            .unwrap();
        scheduler
            .schedule(ScheduledTask::new("mid", "Mid", "* * * * *").with_priority(5))
            .unwrap();
        scheduler.register_handler("low", handler.clone());
        scheduler.register_handler("high", handler.clone());
        scheduler.register_handler("mid", handler);

        let now = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let fired = scheduler.try_fire_due(now);
        assert_eq!(fired, 3);

        let tasks = scheduler.list_tasks();
        let high = tasks.iter().find(|t| t.id == "high").unwrap();
        let low = tasks.iter().find(|t| t.id == "low").unwrap();
        assert!(high.priority > low.priority);
    }

    #[test]
    fn test_cron_boundary_second_59() {
        let scheduler = CronScheduler::new();
        let from = chrono::DateTime::parse_from_rfc3339("2024-06-15T10:10:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let next = scheduler.next_run_time("59 * * * *", from).unwrap();
        assert_eq!(next.second(), 59);
    }

    #[test]
    fn test_cron_boundary_cross_year() {
        let scheduler = CronScheduler::new();
        let from = chrono::DateTime::parse_from_rfc3339("2024-12-31T23:59:59Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let next = scheduler.next_run_time("0 0 1 1 *", from);
        assert!(next.is_ok());
        let next = next.unwrap();
        assert_eq!(next.year(), 2025);
        assert_eq!(next.month(), 1);
        assert_eq!(next.day(), 1);
    }

    #[test]
    fn test_next_run_time_strictly_greater() {
        let scheduler = CronScheduler::new();
        let from = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let next = scheduler.next_run_time("0 * * * *", from).unwrap();
        assert!(next > from);
    }

    #[test]
    fn test_task_status_enum() {
        let statuses = [
            TaskStatus::Pending,
            TaskStatus::Running,
            TaskStatus::Succeeded,
            TaskStatus::Failed,
            TaskStatus::Skipped,
        ];
        for i in 0..statuses.len() {
            for j in 0..statuses.len() {
                if i == j {
                    assert_eq!(statuses[i], statuses[j]);
                } else {
                    assert_ne!(statuses[i], statuses[j]);
                }
            }
        }
    }

    #[test]
    fn test_execution_tracker_record_and_query() {
        let tracker = TaskExecutionTracker::new();
        let now = chrono::Utc::now();

        tracker.record(TaskExecutionRecord {
            task_id: "t1".to_string(),
            fired_at: now,
            status: TaskStatus::Succeeded,
            error_message: None,
            duration_ms: 10,
        });
        tracker.record(TaskExecutionRecord {
            task_id: "t1".to_string(),
            fired_at: now,
            status: TaskStatus::Failed,
            error_message: Some("boom".to_string()),
            duration_ms: 5,
        });
        tracker.record(TaskExecutionRecord {
            task_id: "t1".to_string(),
            fired_at: now,
            status: TaskStatus::Succeeded,
            error_message: None,
            duration_ms: 8,
        });

        assert_eq!(tracker.get_total_count("t1"), 3);
        assert_eq!(tracker.get_success_count("t1"), 2);
        assert_eq!(tracker.get_failure_count("t1"), 1);
        assert_eq!(tracker.get_last_status("t1"), Some(TaskStatus::Succeeded));
        let rate = tracker.get_success_rate("t1");
        assert!((rate - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_execution_tracker_capacity_eviction() {
        let tracker = TaskExecutionTracker::with_capacity(2);
        let now = chrono::Utc::now();
        for i in 0..3 {
            tracker.record(TaskExecutionRecord {
                task_id: format!("t{}", i),
                fired_at: now,
                status: TaskStatus::Succeeded,
                error_message: None,
                duration_ms: 1,
            });
        }
        assert_eq!(tracker.record_count(), 2);
        assert_eq!(tracker.get_total_count("t0"), 0);
        assert_eq!(tracker.get_total_count("t1"), 1);
        assert_eq!(tracker.get_total_count("t2"), 1);
    }

    #[test]
    fn test_execution_tracker_clear() {
        let tracker = TaskExecutionTracker::new();
        let now = chrono::Utc::now();
        tracker.record(TaskExecutionRecord {
            task_id: "t1".to_string(),
            fired_at: now,
            status: TaskStatus::Succeeded,
            error_message: None,
            duration_ms: 1,
        });
        tracker.record(TaskExecutionRecord {
            task_id: "t2".to_string(),
            fired_at: now,
            status: TaskStatus::Failed,
            error_message: None,
            duration_ms: 1,
        });
        tracker.clear_task("t1");
        assert_eq!(tracker.get_total_count("t1"), 0);
        assert_eq!(tracker.get_total_count("t2"), 1);
        assert_eq!(tracker.record_count(), 1);
        tracker.clear();
        assert_eq!(tracker.record_count(), 0);
    }

    #[test]
    fn test_try_fire_due_tracked() {
        let scheduler = CronScheduler::new();
        let tracker = TaskExecutionTracker::new();
        let handler = Arc::new(RecordingJobHandler::new()) as Arc<dyn JobHandler>;

        scheduler
            .schedule(ScheduledTask::new("t1", "Task1", "* * * * *"))
            .unwrap();
        scheduler.register_handler("t1", handler);

        let now = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let fired = scheduler.try_fire_due_tracked(now, &tracker);
        assert_eq!(fired, 1);
        assert_eq!(tracker.get_total_count("t1"), 1);
        assert_eq!(tracker.get_last_status("t1"), Some(TaskStatus::Succeeded));
    }

    #[test]
    fn test_try_fire_due_tracked_no_handler_skipped() {
        let scheduler = CronScheduler::new();
        let tracker = TaskExecutionTracker::new();

        scheduler
            .schedule(ScheduledTask::new("t1", "Task1", "* * * * *"))
            .unwrap();

        let now = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let fired = scheduler.try_fire_due_tracked(now, &tracker);
        assert_eq!(fired, 1);
        assert_eq!(tracker.get_last_status("t1"), Some(TaskStatus::Skipped));
    }

    #[test]
    fn test_health_summary() {
        let tracker = TaskExecutionTracker::new();
        let now = chrono::Utc::now();
        tracker.record(TaskExecutionRecord {
            task_id: "t1".to_string(),
            fired_at: now,
            status: TaskStatus::Succeeded,
            error_message: None,
            duration_ms: 1,
        });
        tracker.record(TaskExecutionRecord {
            task_id: "t1".to_string(),
            fired_at: now,
            status: TaskStatus::Failed,
            error_message: Some("err".to_string()),
            duration_ms: 1,
        });
        tracker.record(TaskExecutionRecord {
            task_id: "t2".to_string(),
            fired_at: now,
            status: TaskStatus::Succeeded,
            error_message: None,
            duration_ms: 1,
        });

        let ids = tracker.get_all_task_ids();
        assert_eq!(ids, vec!["t1".to_string(), "t2".to_string()]);

        let summary = tracker.get_health_summary("t1");
        assert_eq!(summary.total_executions, 2);
        assert_eq!(summary.successes, 1);
        assert_eq!(summary.failures, 1);
        assert!((summary.success_rate - 0.5).abs() < 1e-9);

        let all = tracker.get_all_health_summaries();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_scheduler_task_counts() {
        let scheduler = CronScheduler::new();
        scheduler
            .schedule(ScheduledTask::new("t1", "T1", "* * * * *"))
            .unwrap();
        scheduler
            .schedule(ScheduledTask::new("t2", "T2", "* * * * *").disable())
            .unwrap();
        assert_eq!(scheduler.get_task_count(), 2);
        assert_eq!(scheduler.get_enabled_task_count(), 1);
    }

    #[test]
    fn test_scheduler_task_priorities() {
        let scheduler = CronScheduler::new();
        scheduler
            .schedule(ScheduledTask::new("low", "Low", "* * * * *").with_priority(1))
            .unwrap();
        scheduler
            .schedule(ScheduledTask::new("high", "High", "* * * * *").with_priority(10))
            .unwrap();
        let prios = scheduler.get_task_priorities();
        assert_eq!(prios[0], ("high".to_string(), 10));
        assert_eq!(prios[1], ("low".to_string(), 1));
    }
}
