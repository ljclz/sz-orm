//! # SZ-ORM Scheduler — 定时任务调度器
//!
//! 提供基于 cron 表达式的定时任务调度，支持任务启停、状态管理与回调执行。
//!
//! ## 主要模块
//!
//! - [`scheduler`] — 任务处理器 trait 与测试辅助实现

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

    /// 检查 minute/hour/day/month 是否匹配（不检查 second）
    /// 用于秒级精确扫描时，先筛选出 minute/hour/day/month 匹配的分钟
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

    /// 将 cron 字段解析为有序的数值列表
    /// 支持：* / 单值 / 逗号列表 / 范围 / 步长
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
}
