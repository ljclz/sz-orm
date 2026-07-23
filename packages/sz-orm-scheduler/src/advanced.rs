//! # 高级调度功能
//!
//! 提供 Cron 表达式年份支持（6 字段）、任务依赖编排（DAG）、
//! 任务失败重试策略和分布式锁防重复执行等高级调度能力。

use crate::SchedulerError;
use chrono::{Datelike, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ====================================================================
// Cron 表达式增强：支持 6 字段（含年份）
// ====================================================================

/// 增强的 Cron 表达式，支持可选的年份字段
#[derive(Debug, Clone)]
pub struct EnhancedCronExpr {
    pub second: String,
    pub minute: String,
    pub hour: String,
    pub day_of_month: String,
    pub month: String,
    pub day_of_week: String,
    /// 可选的年份字段（None 表示不限制年份）
    pub year: Option<String>,
}

impl EnhancedCronExpr {
    /// 解析 cron 表达式，支持 5 字段和 6 字段格式
    ///
    /// - 5 字段：`second minute hour day_of_month month`
    /// - 6 字段：`second minute hour day_of_month month year`
    /// - 7 字段：`second minute hour day_of_month month day_of_week year`
    pub fn parse(expr: &str) -> Result<Self, SchedulerError> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        match parts.len() {
            5 => Ok(Self {
                second: parts[0].to_string(),
                minute: parts[1].to_string(),
                hour: parts[2].to_string(),
                day_of_month: parts[3].to_string(),
                month: parts[4].to_string(),
                day_of_week: "*".to_string(),
                year: None,
            }),
            6 => Ok(Self {
                second: parts[0].to_string(),
                minute: parts[1].to_string(),
                hour: parts[2].to_string(),
                day_of_month: parts[3].to_string(),
                month: parts[4].to_string(),
                day_of_week: "*".to_string(),
                year: Some(parts[5].to_string()),
            }),
            7 => Ok(Self {
                second: parts[0].to_string(),
                minute: parts[1].to_string(),
                hour: parts[2].to_string(),
                day_of_month: parts[3].to_string(),
                month: parts[4].to_string(),
                day_of_week: parts[5].to_string(),
                year: Some(parts[6].to_string()),
            }),
            n => Err(SchedulerError::InvalidCronExpr(format!(
                "Expected 5-7 fields, got {}",
                n
            ))),
        }
    }

    /// 判断给定时间是否匹配此 cron 表达式
    pub fn matches(&self, dt: chrono::DateTime<chrono::Utc>) -> bool {
        let naive = dt.naive_utc();
        if !field_matches(&self.second, naive.second()) {
            return false;
        }
        if !field_matches(&self.minute, naive.minute()) {
            return false;
        }
        if !field_matches(&self.hour, naive.hour()) {
            return false;
        }
        if !field_matches(&self.day_of_month, naive.day()) {
            return false;
        }
        if !field_matches(&self.month, naive.month()) {
            return false;
        }
        if !field_matches(&self.day_of_week, weekday_to_cron(dt.weekday())) {
            return false;
        }
        if let Some(ref year_field) = self.year {
            if !field_matches(year_field, naive.year() as u32) {
                return false;
            }
        }
        true
    }

    /// 返回字段数量
    pub fn field_count(&self) -> usize {
        if self.year.is_some() {
            6
        } else {
            5
        }
    }
}

/// 将 chrono::Weekday 转换为 cron 格式的数字（0=Sunday, 6=Saturday）
fn weekday_to_cron(wd: chrono::Weekday) -> u32 {
    use chrono::Weekday::*;
    match wd {
        Sun => 0,
        Mon => 1,
        Tue => 2,
        Wed => 3,
        Thu => 4,
        Fri => 5,
        Sat => 6,
    }
}

/// 通用的 cron 字段匹配函数
/// 支持：* / 单值 / 逗号列表 / 范围 / 步长
fn field_matches(field: &str, value: u32) -> bool {
    if field == "*" {
        return true;
    }
    if field.contains(',') {
        return field
            .split(',')
            .any(|v| v.trim().parse::<u32>().is_ok_and(|n| n == value));
    }
    if field.contains('/') {
        let parts: Vec<&str> = field.split('/').collect();
        if parts.len() == 2 {
            let step: u32 = parts[1].parse().unwrap_or(1);
            if step == 0 {
                return false;
            }
            let base_part = parts[0];
            if base_part == "*" {
                return value.is_multiple_of(step);
            }
            if base_part.contains('-') {
                let range: Vec<&str> = base_part.split('-').collect();
                if range.len() == 2 {
                    let start: u32 = range[0].trim().parse().unwrap_or(0);
                    let end: u32 = range[1].trim().parse().unwrap_or(0);
                    return value >= start && value <= end && (value - start).is_multiple_of(step);
                }
            }
            let start: u32 = base_part.trim().parse().unwrap_or(0);
            return value >= start && (value - start).is_multiple_of(step);
        }
    }
    if field.contains('-') {
        let parts: Vec<&str> = field.split('-').collect();
        if parts.len() == 2 {
            let start: u32 = parts[0].trim().parse().unwrap_or(0);
            let end: u32 = parts[1].trim().parse().unwrap_or(0);
            return value >= start && value <= end;
        }
    }
    field.parse::<u32>().is_ok_and(|n| n == value)
}

// ====================================================================
// 任务依赖编排（DAG）
// ====================================================================

/// DAG 中的任务节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagTask {
    /// 任务 ID
    pub id: String,
    /// 任务名称
    pub name: String,
    /// 依赖的任务 ID 列表（必须在当前任务执行前完成）
    pub dependencies: Vec<String>,
    /// 任务是否已完成
    pub completed: bool,
    /// 任务是否正在执行
    pub running: bool,
    /// 任务是否执行失败
    pub failed: bool,
}

impl DagTask {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            dependencies: Vec::new(),
            completed: false,
            running: false,
            failed: false,
        }
    }

    pub fn depends_on(mut self, dep_id: impl Into<String>) -> Self {
        self.dependencies.push(dep_id.into());
        self
    }
}

/// DAG 任务图：管理任务之间的依赖关系，支持拓扑排序和并行调度
pub struct TaskDag {
    /// 所有任务节点（id -> DagTask）
    tasks: HashMap<String, DagTask>,
}

impl TaskDag {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
        }
    }

    /// 添加任务节点
    ///
    /// 验证依赖的任务是否已存在，防止引用不存在的依赖。
    /// 若需在所有任务注册后再配置依赖（例如构建循环以测试环检测），
    /// 请使用 [`Self::add_dependency`]。
    pub fn add_task(&mut self, task: DagTask) -> Result<(), SchedulerError> {
        if self.tasks.contains_key(&task.id) {
            return Err(SchedulerError::Internal(format!(
                "task '{}' already exists in DAG",
                task.id
            )));
        }
        // 验证依赖的任务是否存在
        for dep in &task.dependencies {
            if !self.tasks.contains_key(dep) {
                return Err(SchedulerError::Internal(format!(
                    "dependency '{}' not found for task '{}'",
                    dep, task.id
                )));
            }
        }
        self.tasks.insert(task.id.clone(), task);
        Ok(())
    }

    /// 为已存在的任务追加依赖关系。
    ///
    /// 与 [`Self::add_task`] 不同，此方法允许在所有任务注册后配置依赖，
    /// 因此可以构建出循环依赖以供 [`Self::has_cycle`] 检测。
    /// 若任务或依赖不存在则返回错误。
    pub fn add_dependency(
        &mut self,
        task_id: &str,
        dep_id: &str,
    ) -> Result<(), SchedulerError> {
        // 先做不可变借用检查 dep_id 是否存在
        if !self.tasks.contains_key(dep_id) {
            return Err(SchedulerError::Internal(format!(
                "dependency '{}' not found for task '{}'",
                dep_id, task_id
            )));
        }
        // 再获取可变借用修改 task
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| SchedulerError::TaskNotFound(task_id.to_string()))?;
        if !task.dependencies.contains(&dep_id.to_string()) {
            task.dependencies.push(dep_id.to_string());
        }
        Ok(())
    }

    /// 返回所有任务 ID
    pub fn task_ids(&self) -> Vec<String> {
        self.tasks.keys().cloned().collect()
    }

    /// 返回任务总数
    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    /// 获取任务
    pub fn get_task(&self, id: &str) -> Option<&DagTask> {
        self.tasks.get(id)
    }

    /// 获取可变任务
    pub fn get_task_mut(&mut self, id: &str) -> Option<&mut DagTask> {
        self.tasks.get_mut(id)
    }

    /// 检测是否存在循环依赖
    pub fn has_cycle(&self) -> bool {
        let mut visited = HashSet::new();
        let mut recursion_stack = HashSet::new();

        for task_id in self.tasks.keys() {
            if self.dfs_cycle_detect(task_id, &mut visited, &mut recursion_stack) {
                return true;
            }
        }
        false
    }

    fn dfs_cycle_detect(
        &self,
        task_id: &str,
        visited: &mut HashSet<String>,
        recursion_stack: &mut HashSet<String>,
    ) -> bool {
        if recursion_stack.contains(task_id) {
            return true;
        }
        if visited.contains(task_id) {
            return false;
        }
        visited.insert(task_id.to_string());
        recursion_stack.insert(task_id.to_string());

        if let Some(task) = self.tasks.get(task_id) {
            for dep in &task.dependencies {
                if self.dfs_cycle_detect(dep, visited, recursion_stack) {
                    return true;
                }
            }
        }

        recursion_stack.remove(task_id);
        false
    }

    /// 返回拓扑排序后的任务执行顺序
    pub fn topological_sort(&self) -> Result<Vec<String>, SchedulerError> {
        if self.has_cycle() {
            return Err(SchedulerError::Internal(
                "DAG contains a cycle, cannot topological sort".to_string(),
            ));
        }
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut all_ids: Vec<String> = self.tasks.keys().cloned().collect();
        all_ids.sort(); // 确保确定性顺序

        for id in &all_ids {
            self.topo_dfs(id, &mut visited, &mut result);
        }
        Ok(result)
    }

    fn topo_dfs(&self, id: &str, visited: &mut HashSet<String>, result: &mut Vec<String>) {
        if visited.contains(id) {
            return;
        }
        visited.insert(id.to_string());
        if let Some(task) = self.tasks.get(id) {
            for dep in &task.dependencies {
                self.topo_dfs(dep, visited, result);
            }
        }
        result.push(id.to_string());
    }

    /// 返回当前可执行的任务（所有依赖均已完成且自身未完成/未运行）
    pub fn ready_tasks(&self) -> Vec<String> {
        self.tasks
            .iter()
            .filter(|(_, t)| !t.completed && !t.running && !t.failed)
            .filter(|(_, t)| {
                t.dependencies
                    .iter()
                    .all(|dep| self.tasks.get(dep).map(|d| d.completed).unwrap_or(false))
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// 标记任务开始执行
    pub fn mark_running(&mut self, task_id: &str) -> Result<(), SchedulerError> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| SchedulerError::TaskNotFound(task_id.to_string()))?;
        task.running = true;
        Ok(())
    }

    /// 标记任务完成
    pub fn mark_completed(&mut self, task_id: &str) -> Result<(), SchedulerError> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| SchedulerError::TaskNotFound(task_id.to_string()))?;
        task.running = false;
        task.completed = true;
        Ok(())
    }

    /// 标记任务失败
    pub fn mark_failed(&mut self, task_id: &str) -> Result<(), SchedulerError> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| SchedulerError::TaskNotFound(task_id.to_string()))?;
        task.running = false;
        task.failed = true;
        Ok(())
    }

    /// 重置失败的任务，允许重试
    pub fn reset_failed(&mut self, task_id: &str) -> Result<(), SchedulerError> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| SchedulerError::TaskNotFound(task_id.to_string()))?;
        task.failed = false;
        task.running = false;
        task.completed = false;
        Ok(())
    }

    /// 返回是否所有任务都已完成
    pub fn all_completed(&self) -> bool {
        self.tasks.values().all(|t| t.completed)
    }

    /// 返回已完成的任务数
    pub fn completed_count(&self) -> usize {
        self.tasks.values().filter(|t| t.completed).count()
    }

    /// 返回失败的任务数
    pub fn failed_count(&self) -> usize {
        self.tasks.values().filter(|t| t.failed).count()
    }

    /// 返回指定任务的直接下游任务
    pub fn dependents(&self, task_id: &str) -> Vec<String> {
        self.tasks
            .iter()
            .filter(|(_, t)| t.dependencies.contains(&task_id.to_string()))
            .map(|(id, _)| id.clone())
            .collect()
    }
}

impl Default for TaskDag {
    fn default() -> Self {
        Self::new()
    }
}

// ====================================================================
// 任务失败重试策略
// ====================================================================

/// 重试策略类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetryPolicy {
    /// 固定间隔重试
    Fixed {
        /// 最大重试次数
        max_retries: u32,
        /// 每次重试间隔（毫秒）
        interval_ms: u64,
    },
    /// 指数退避重试
    ExponentialBackoff {
        /// 最大重试次数
        max_retries: u32,
        /// 初始重试间隔（毫秒）
        initial_interval_ms: u64,
        /// 退避乘数（每次重试间隔 = initial * multiplier^attempt）
        multiplier: f64,
        /// 最大重试间隔（毫秒）
        max_interval_ms: u64,
    },
}

impl RetryPolicy {
    pub fn fixed(max_retries: u32, interval: Duration) -> Self {
        Self::Fixed {
            max_retries,
            interval_ms: interval.as_millis() as u64,
        }
    }

    pub fn exponential(
        max_retries: u32,
        initial: Duration,
        multiplier: f64,
        max_interval: Duration,
    ) -> Self {
        Self::ExponentialBackoff {
            max_retries,
            initial_interval_ms: initial.as_millis() as u64,
            multiplier,
            max_interval_ms: max_interval.as_millis() as u64,
        }
    }

    /// 返回最大重试次数
    pub fn max_retries(&self) -> u32 {
        match self {
            RetryPolicy::Fixed { max_retries, .. } => *max_retries,
            RetryPolicy::ExponentialBackoff { max_retries, .. } => *max_retries,
        }
    }

    /// 计算第 attempt 次重试的等待时间（attempt 从 0 开始）
    pub fn retry_delay_ms(&self, attempt: u32) -> u64 {
        match self {
            RetryPolicy::Fixed { interval_ms, .. } => *interval_ms,
            RetryPolicy::ExponentialBackoff {
                initial_interval_ms,
                multiplier,
                max_interval_ms,
                ..
            } => {
                let delay = (*initial_interval_ms as f64) * multiplier.powi(attempt as i32);
                delay.min(*max_interval_ms as f64) as u64
            }
        }
    }
}

/// 重试执行器：根据重试策略执行任务，记录每次尝试的结果
pub struct RetryExecutor {
    /// 重试策略
    policy: RetryPolicy,
    /// 执行历史记录
    history: Mutex<Vec<RetryRecord>>,
}

/// 单次重试记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryRecord {
    /// 任务 ID
    pub task_id: String,
    /// 尝试次数（从 1 开始）
    pub attempt: u32,
    /// 是否成功
    pub success: bool,
    /// 错误信息（如果失败）
    pub error: Option<String>,
    /// 等待时间（毫秒）
    pub wait_ms: u64,
    /// 执行耗时（毫秒）
    pub duration_ms: u64,
}

impl RetryExecutor {
    pub fn new(policy: RetryPolicy) -> Self {
        Self {
            policy,
            history: Mutex::new(Vec::new()),
        }
    }

    /// 执行任务，根据重试策略在失败时自动重试
    /// 返回最终是否成功
    pub fn execute<F>(&self, task_id: &str, mut task_fn: F) -> bool
    where
        F: FnMut() -> Result<(), String>,
    {
        let max_retries = self.policy.max_retries();
        for attempt in 0..=max_retries {
            let start = Instant::now();
            let result = task_fn();
            let duration_ms = start.elapsed().as_millis() as u64;

            let success = result.is_ok();
            let error = result.err();
            let wait_ms = if success || attempt >= max_retries {
                0
            } else {
                self.policy.retry_delay_ms(attempt)
            };

            if let Ok(mut history) = self.history.lock() {
                history.push(RetryRecord {
                    task_id: task_id.to_string(),
                    attempt: attempt + 1,
                    success,
                    error: error.clone(),
                    wait_ms,
                    duration_ms,
                });
            }

            if success {
                return true;
            }

            if attempt < max_retries && wait_ms > 0 {
                std::thread::sleep(Duration::from_millis(wait_ms));
            }
        }
        false
    }

    /// 返回所有重试记录
    pub fn history(&self) -> Vec<RetryRecord> {
        self.history
            .lock()
            .map(|h| h.clone())
            .unwrap_or_default()
    }

    /// 返回指定任务的重试次数
    pub fn attempt_count(&self, task_id: &str) -> usize {
        self.history
            .lock()
            .map(|h| h.iter().filter(|r| r.task_id == task_id).count())
            .unwrap_or(0)
    }

    /// 返回指定任务是否最终成功
    pub fn is_successful(&self, task_id: &str) -> bool {
        self.history
            .lock()
            .map(|h| {
                h.iter()
                    .filter(|r| r.task_id == task_id)
                    .any(|r| r.success)
            })
            .unwrap_or(false)
    }

    /// 清空历史记录
    pub fn clear_history(&self) {
        if let Ok(mut h) = self.history.lock() {
            h.clear();
        }
    }
}

// ====================================================================
// 分布式锁防重复执行
// ====================================================================

/// 分布式锁状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LockStatus {
    /// 已获取锁
    Acquired,
    /// 锁已被其他实例持有
    HeldByOther,
    /// 锁已过期
    Expired,
}

/// 分布式锁条目
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LockEntry {
    /// 持有者 ID（实例标识）
    owner: String,
    /// 获取时间戳（秒）
    acquired_at: u64,
    /// 过期时间戳（秒）
    expires_at: u64,
}

/// 分布式锁管理器：防止同一任务在多个实例上重复执行
pub struct DistributedLockManager {
    /// 锁表（task_id -> LockEntry）
    locks: Mutex<HashMap<String, LockEntry>>,
    /// 锁的默认 TTL（秒）
    pub default_ttl_secs: u64,
}

impl DistributedLockManager {
    pub fn new(default_ttl_secs: u64) -> Self {
        Self {
            locks: Mutex::new(HashMap::new()),
            default_ttl_secs,
        }
    }

    /// 尝试获取任务锁
    /// 如果锁不存在或已过期，则获取成功；如果锁被其他实例持有且未过期，则返回 HeldByOther
    pub fn try_acquire(&self, task_id: &str, owner: &str) -> LockStatus {
        self.try_acquire_with_ttl(task_id, owner, self.default_ttl_secs)
    }

    /// 尝试获取任务锁，指定自定义 TTL
    pub fn try_acquire_with_ttl(
        &self,
        task_id: &str,
        owner: &str,
        ttl_secs: u64,
    ) -> LockStatus {
        let now = current_timestamp_secs();
        let mut locks = match self.locks.lock() {
            Ok(l) => l,
            Err(_) => return LockStatus::HeldByOther,
        };

        if let Some(entry) = locks.get(task_id) {
            // 检查锁是否已过期
            if now >= entry.expires_at {
                // 锁已过期，可以重新获取
                let entry = LockEntry {
                    owner: owner.to_string(),
                    acquired_at: now,
                    expires_at: now + ttl_secs,
                };
                locks.insert(task_id.to_string(), entry);
                return LockStatus::Acquired;
            }
            // 锁未过期，检查是否是同一个持有者
            if entry.owner == owner {
                // 同一持有者可以续期
                let entry = LockEntry {
                    owner: owner.to_string(),
                    acquired_at: entry.acquired_at,
                    expires_at: now + ttl_secs,
                };
                locks.insert(task_id.to_string(), entry);
                return LockStatus::Acquired;
            }
            // 锁被其他实例持有
            return LockStatus::HeldByOther;
        }

        // 没有锁，直接获取
        let entry = LockEntry {
            owner: owner.to_string(),
            acquired_at: now,
            expires_at: now + ttl_secs,
        };
        locks.insert(task_id.to_string(), entry);
        LockStatus::Acquired
    }

    /// 释放任务锁（只有持有者才能释放）
    pub fn release(&self, task_id: &str, owner: &str) -> bool {
        let mut locks = match self.locks.lock() {
            Ok(l) => l,
            Err(_) => return false,
        };
        if let Some(entry) = locks.get(task_id) {
            if entry.owner == owner {
                locks.remove(task_id);
                return true;
            }
        }
        false
    }

    /// 续期锁（延长过期时间）
    pub fn renew(&self, task_id: &str, owner: &str, ttl_secs: u64) -> bool {
        let now = current_timestamp_secs();
        let mut locks = match self.locks.lock() {
            Ok(l) => l,
            Err(_) => return false,
        };
        if let Some(entry) = locks.get_mut(task_id) {
            if entry.owner == owner {
                entry.expires_at = now + ttl_secs;
                return true;
            }
        }
        false
    }

    /// 检查锁是否被持有
    pub fn is_locked(&self, task_id: &str) -> bool {
        let now = current_timestamp_secs();
        let locks = match self.locks.lock() {
            Ok(l) => l,
            Err(_) => return false,
        };
        locks
            .get(task_id)
            .map(|e| now < e.expires_at)
            .unwrap_or(false)
    }

    /// 返回锁的持有者
    pub fn lock_owner(&self, task_id: &str) -> Option<String> {
        let now = current_timestamp_secs();
        let locks = self.locks.lock().ok()?;
        locks.get(task_id).and_then(|e| {
            if now < e.expires_at {
                Some(e.owner.clone())
            } else {
                None
            }
        })
    }

    /// 清理所有过期的锁
    pub fn cleanup_expired(&self) -> usize {
        let now = current_timestamp_secs();
        let mut locks = match self.locks.lock() {
            Ok(l) => l,
            Err(_) => return 0,
        };
        let before = locks.len();
        locks.retain(|_, e| now < e.expires_at);
        before - locks.len()
    }

    /// 返回当前活跃锁的数量
    pub fn active_lock_count(&self) -> usize {
        let now = current_timestamp_secs();
        let locks = match self.locks.lock() {
            Ok(l) => l,
            Err(_) => return 0,
        };
        locks.values().filter(|e| now < e.expires_at).count()
    }
}

impl Default for DistributedLockManager {
    fn default() -> Self {
        Self::new(300) // 默认 5 分钟 TTL
    }
}

/// 获取当前时间戳（秒）
fn current_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ====================================================================
// 任务执行统计
// ====================================================================

/// 任务执行统计信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskStats {
    /// 总执行次数
    pub total_executions: u64,
    /// 成功次数
    pub successful_executions: u64,
    /// 失败次数
    pub failed_executions: u64,
    /// 总执行耗时（毫秒）
    pub total_duration_ms: u64,
    /// 最后一次执行时间戳（秒）
    pub last_execution_at: Option<u64>,
    /// 最后一次执行是否成功
    pub last_success: Option<bool>,
}

impl TaskStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一次执行
    pub fn record(&mut self, success: bool, duration_ms: u64) {
        self.total_executions += 1;
        if success {
            self.successful_executions += 1;
        } else {
            self.failed_executions += 1;
        }
        self.total_duration_ms += duration_ms;
        self.last_execution_at = Some(current_timestamp_secs());
        self.last_success = Some(success);
    }

    /// 返回成功率（0.0 - 100.0）
    pub fn success_rate(&self) -> f64 {
        if self.total_executions == 0 {
            return 0.0;
        }
        (self.successful_executions as f64 / self.total_executions as f64) * 100.0
    }

    /// 返回平均执行耗时（毫秒）
    pub fn avg_duration_ms(&self) -> f64 {
        if self.total_executions == 0 {
            return 0.0;
        }
        self.total_duration_ms as f64 / self.total_executions as f64
    }
}

/// 任务执行统计管理器
pub struct TaskStatsManager {
    stats: Mutex<HashMap<String, TaskStats>>,
}

impl TaskStatsManager {
    pub fn new() -> Self {
        Self {
            stats: Mutex::new(HashMap::new()),
        }
    }

    /// 记录一次任务执行
    pub fn record(&self, task_id: &str, success: bool, duration_ms: u64) {
        if let Ok(mut stats) = self.stats.lock() {
            stats
                .entry(task_id.to_string())
                .or_default()
                .record(success, duration_ms);
        }
    }

    /// 获取任务统计信息
    pub fn get_stats(&self, task_id: &str) -> Option<TaskStats> {
        self.stats
            .lock()
            .ok()
            .and_then(|s| s.get(task_id).cloned())
    }

    /// 返回所有被追踪的任务 ID
    pub fn tracked_tasks(&self) -> Vec<String> {
        self.stats
            .lock()
            .map(|s| s.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// 重置指定任务的统计
    pub fn reset(&self, task_id: &str) -> bool {
        if let Ok(mut stats) = self.stats.lock() {
            return stats.remove(task_id).is_some();
        }
        false
    }
}

impl Default for TaskStatsManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ====================================================================
    // EnhancedCronExpr 测试
    // ====================================================================

    #[test]
    fn test_enhanced_cron_parse_5_fields() {
        let expr = EnhancedCronExpr::parse("0 * * * *").unwrap();
        assert_eq!(expr.second, "0");
        assert_eq!(expr.minute, "*");
        assert_eq!(expr.hour, "*");
        assert_eq!(expr.day_of_month, "*");
        assert_eq!(expr.month, "*");
        assert!(expr.year.is_none());
        assert_eq!(expr.field_count(), 5);
    }

    #[test]
    fn test_enhanced_cron_parse_6_fields_with_year() {
        let expr = EnhancedCronExpr::parse("0 0 * * * 2025").unwrap();
        assert_eq!(expr.second, "0");
        assert_eq!(expr.minute, "0");
        assert_eq!(expr.year, Some("2025".to_string()));
        assert_eq!(expr.field_count(), 6);
    }

    #[test]
    fn test_enhanced_cron_parse_7_fields() {
        let expr = EnhancedCronExpr::parse("0 0 12 * * 1 2025").unwrap();
        assert_eq!(expr.second, "0");
        assert_eq!(expr.day_of_week, "1");
        assert_eq!(expr.year, Some("2025".to_string()));
    }

    #[test]
    fn test_enhanced_cron_parse_invalid_field_count() {
        assert!(EnhancedCronExpr::parse("0 * *").is_err());
        assert!(EnhancedCronExpr::parse("0 * * * * * * *").is_err());
    }

    #[test]
    fn test_enhanced_cron_matches_with_year() {
        let expr = EnhancedCronExpr::parse("0 0 0 1 1 * 2025").unwrap();
        // 2025-01-01 00:00:00 应匹配
        let dt = chrono::DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert!(expr.matches(dt));
    }

    #[test]
    fn test_enhanced_cron_does_not_match_wrong_year() {
        let expr = EnhancedCronExpr::parse("0 0 0 1 1 * 2025").unwrap();
        // 2024-01-01 00:00:00 不应匹配
        let dt = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert!(!expr.matches(dt));
    }

    #[test]
    fn test_enhanced_cron_matches_without_year() {
        let expr = EnhancedCronExpr::parse("* * * * *").unwrap();
        let dt = chrono::Utc::now();
        assert!(expr.matches(dt));
    }

    #[test]
    fn test_enhanced_cron_day_of_week_match() {
        // 每周一 00:00:00（7 字段格式：秒 分 时 日 月 周 年）
        let expr = EnhancedCronExpr::parse("0 0 0 * * 1 *").unwrap();
        // 2024-01-01 是星期一
        let dt = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert!(expr.matches(dt));
    }

    #[test]
    fn test_enhanced_cron_day_of_week_no_match() {
        // 每周一 00:00:00（7 字段格式：秒 分 时 日 月 周 年）
        let expr = EnhancedCronExpr::parse("0 0 0 * * 1 *").unwrap();
        // 2024-01-02 是星期二
        let dt = chrono::DateTime::parse_from_rfc3339("2024-01-02T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert!(!expr.matches(dt));
    }

    #[test]
    fn test_field_matches_step_with_range() {
        // 1-10/3 应匹配 1, 4, 7, 10
        assert!(field_matches("1-10/3", 1));
        assert!(field_matches("1-10/3", 4));
        assert!(field_matches("1-10/3", 7));
        assert!(field_matches("1-10/3", 10));
        assert!(!field_matches("1-10/3", 2));
        assert!(!field_matches("1-10/3", 5));
    }

    // ====================================================================
    // TaskDag 测试
    // ====================================================================

    #[test]
    fn test_dag_add_task() {
        let mut dag = TaskDag::new();
        dag.add_task(DagTask::new("a", "Task A")).unwrap();
        dag.add_task(DagTask::new("b", "Task B")).unwrap();
        assert_eq!(dag.task_count(), 2);
    }

    #[test]
    fn test_dag_add_duplicate_task_fails() {
        let mut dag = TaskDag::new();
        dag.add_task(DagTask::new("a", "Task A")).unwrap();
        let result = dag.add_task(DagTask::new("a", "Task A Duplicate"));
        assert!(result.is_err());
    }

    #[test]
    fn test_dag_add_task_with_missing_dependency_fails() {
        let mut dag = TaskDag::new();
        let result = dag.add_task(DagTask::new("a", "Task A").depends_on("nonexistent"));
        assert!(result.is_err());
    }

    #[test]
    fn test_dag_add_dependency_success() {
        let mut dag = TaskDag::new();
        dag.add_task(DagTask::new("a", "A")).unwrap();
        dag.add_task(DagTask::new("b", "B")).unwrap();
        // b 依赖 a
        assert!(dag.add_dependency("b", "a").is_ok());
        let task_b = dag.get_task("b").unwrap();
        assert!(task_b.dependencies.contains(&"a".to_string()));
    }

    #[test]
    fn test_dag_add_dependency_task_not_found() {
        let mut dag = TaskDag::new();
        dag.add_task(DagTask::new("a", "A")).unwrap();
        assert!(dag.add_dependency("nonexistent", "a").is_err());
    }

    #[test]
    fn test_dag_add_dependency_dep_not_found() {
        let mut dag = TaskDag::new();
        dag.add_task(DagTask::new("a", "A")).unwrap();
        assert!(dag.add_dependency("a", "nonexistent").is_err());
    }

    #[test]
    fn test_dag_add_dependency_dedup() {
        let mut dag = TaskDag::new();
        dag.add_task(DagTask::new("a", "A")).unwrap();
        dag.add_task(DagTask::new("b", "B").depends_on("a")).unwrap();
        // 重复添加同一依赖不应产生重复条目
        dag.add_dependency("b", "a").unwrap();
        let task_b = dag.get_task("b").unwrap();
        let count = task_b.dependencies.iter().filter(|d| *d == "a").count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_dag_no_cycle() {
        let mut dag = TaskDag::new();
        dag.add_task(DagTask::new("a", "A")).unwrap();
        dag.add_task(DagTask::new("b", "B").depends_on("a")).unwrap();
        dag.add_task(DagTask::new("c", "C").depends_on("b")).unwrap();
        assert!(!dag.has_cycle());
    }

    #[test]
    fn test_dag_detects_cycle() {
        // 先注册所有任务（无依赖），再通过 add_dependency 构建循环
        let mut dag = TaskDag::new();
        dag.add_task(DagTask::new("a", "A")).unwrap();
        dag.add_task(DagTask::new("b", "B").depends_on("a")).unwrap();
        dag.add_task(DagTask::new("c", "C").depends_on("b")).unwrap();
        // 追加 a -> c 依赖，形成 a->b->c->a 循环
        dag.add_dependency("a", "c").unwrap();
        assert!(dag.has_cycle());
    }

    #[test]
    fn test_dag_topological_sort() {
        let mut dag = TaskDag::new();
        dag.add_task(DagTask::new("a", "A")).unwrap();
        dag.add_task(DagTask::new("b", "B").depends_on("a")).unwrap();
        dag.add_task(DagTask::new("c", "C").depends_on("a")).unwrap();
        // d 依赖 b 和 c，通过链式 depends_on 声明多个依赖
        dag.add_task(DagTask::new("d", "D").depends_on("b").depends_on("c"))
            .unwrap();

        let order = dag.topological_sort().unwrap();
        // a 必须在 b、c 之前；b、c 必须在 d 之前
        let pos_a = order.iter().position(|x| x == "a").unwrap();
        let pos_b = order.iter().position(|x| x == "b").unwrap();
        let pos_c = order.iter().position(|x| x == "c").unwrap();
        let pos_d = order.iter().position(|x| x == "d").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_a < pos_c);
        assert!(pos_b < pos_d);
        assert!(pos_c < pos_d);
    }

    #[test]
    fn test_dag_topological_sort_valid() {
        let mut dag = TaskDag::new();
        dag.add_task(DagTask::new("a", "A")).unwrap();
        dag.add_task(DagTask::new("b", "B").depends_on("a")).unwrap();
        dag.add_task(DagTask::new("c", "C").depends_on("a")).unwrap();
        dag.add_task(DagTask::new("d", "D").depends_on("b")).unwrap();

        let order = dag.topological_sort().unwrap();
        // a 必须在 b 和 c 之前，b 必须在 d 之前
        let pos_a = order.iter().position(|x| x == "a").unwrap();
        let pos_b = order.iter().position(|x| x == "b").unwrap();
        let pos_c = order.iter().position(|x| x == "c").unwrap();
        let pos_d = order.iter().position(|x| x == "d").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_a < pos_c);
        assert!(pos_b < pos_d);
    }

    #[test]
    fn test_dag_topological_sort_with_cycle_fails() {
        // 先注册无依赖任务，再构建 a <-> b 双向依赖循环
        let mut dag = TaskDag::new();
        dag.add_task(DagTask::new("a", "A")).unwrap();
        dag.add_task(DagTask::new("b", "B")).unwrap();
        dag.add_dependency("a", "b").unwrap();
        dag.add_dependency("b", "a").unwrap();
        assert!(dag.topological_sort().is_err());
    }

    #[test]
    fn test_dag_ready_tasks_initial() {
        let mut dag = TaskDag::new();
        dag.add_task(DagTask::new("a", "A")).unwrap();
        dag.add_task(DagTask::new("b", "B").depends_on("a")).unwrap();
        // 初始只有 a 可以执行
        let ready = dag.ready_tasks();
        assert_eq!(ready.len(), 1);
        assert!(ready.contains(&"a".to_string()));
    }

    #[test]
    fn test_dag_ready_tasks_after_completion() {
        let mut dag = TaskDag::new();
        dag.add_task(DagTask::new("a", "A")).unwrap();
        dag.add_task(DagTask::new("b", "B").depends_on("a")).unwrap();
        dag.add_task(DagTask::new("c", "C").depends_on("a")).unwrap();

        dag.mark_completed("a").unwrap();
        let ready = dag.ready_tasks();
        assert_eq!(ready.len(), 2);
        assert!(ready.contains(&"b".to_string()));
        assert!(ready.contains(&"c".to_string()));
    }

    #[test]
    fn test_dag_mark_running_and_completed() {
        let mut dag = TaskDag::new();
        dag.add_task(DagTask::new("a", "A")).unwrap();
        dag.mark_running("a").unwrap();
        assert!(dag.get_task("a").unwrap().running);
        dag.mark_completed("a").unwrap();
        assert!(!dag.get_task("a").unwrap().running);
        assert!(dag.get_task("a").unwrap().completed);
    }

    #[test]
    fn test_dag_mark_failed_and_reset() {
        let mut dag = TaskDag::new();
        dag.add_task(DagTask::new("a", "A")).unwrap();
        dag.mark_running("a").unwrap();
        dag.mark_failed("a").unwrap();
        assert!(dag.get_task("a").unwrap().failed);
        dag.reset_failed("a").unwrap();
        assert!(!dag.get_task("a").unwrap().failed);
        assert!(!dag.get_task("a").unwrap().completed);
    }

    #[test]
    fn test_dag_all_completed() {
        let mut dag = TaskDag::new();
        dag.add_task(DagTask::new("a", "A")).unwrap();
        dag.add_task(DagTask::new("b", "B").depends_on("a")).unwrap();
        assert!(!dag.all_completed());
        dag.mark_completed("a").unwrap();
        assert!(!dag.all_completed());
        dag.mark_completed("b").unwrap();
        assert!(dag.all_completed());
    }

    #[test]
    fn test_dag_dependents() {
        let mut dag = TaskDag::new();
        dag.add_task(DagTask::new("a", "A")).unwrap();
        dag.add_task(DagTask::new("b", "B").depends_on("a")).unwrap();
        dag.add_task(DagTask::new("c", "C").depends_on("a")).unwrap();
        let deps = dag.dependents("a");
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&"b".to_string()));
        assert!(deps.contains(&"c".to_string()));
    }

    // ====================================================================
    // RetryPolicy 测试
    // ====================================================================

    #[test]
    fn test_retry_policy_fixed() {
        let policy = RetryPolicy::fixed(3, Duration::from_millis(100));
        assert_eq!(policy.max_retries(), 3);
        assert_eq!(policy.retry_delay_ms(0), 100);
        assert_eq!(policy.retry_delay_ms(1), 100);
        assert_eq!(policy.retry_delay_ms(2), 100);
    }

    #[test]
    fn test_retry_policy_exponential() {
        let policy = RetryPolicy::exponential(
            3,
            Duration::from_millis(100),
            2.0,
            Duration::from_millis(1000),
        );
        assert_eq!(policy.max_retries(), 3);
        assert_eq!(policy.retry_delay_ms(0), 100); // 100 * 2^0 = 100
        assert_eq!(policy.retry_delay_ms(1), 200); // 100 * 2^1 = 200
        assert_eq!(policy.retry_delay_ms(2), 400); // 100 * 2^2 = 400
    }

    #[test]
    fn test_retry_policy_exponential_capped() {
        let policy = RetryPolicy::exponential(
            5,
            Duration::from_millis(100),
            2.0,
            Duration::from_millis(500),
        );
        // 100 * 2^3 = 800, but capped at 500
        assert_eq!(policy.retry_delay_ms(3), 500);
    }

    // ====================================================================
    // RetryExecutor 测试
    // ====================================================================

    #[test]
    fn test_retry_executor_success_first_try() {
        let executor = RetryExecutor::new(RetryPolicy::fixed(3, Duration::from_millis(1)));
        let result = executor.execute("task1", || Ok(()));
        assert!(result);
        assert_eq!(executor.attempt_count("task1"), 1);
        assert!(executor.is_successful("task1"));
    }

    #[test]
    fn test_retry_executor_success_after_retries() {
        let executor = RetryExecutor::new(RetryPolicy::fixed(3, Duration::from_millis(1)));
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let attempts_clone = attempts.clone();
        let result = executor.execute("task1", || {
            let count = attempts_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if count < 2 {
                Err("not yet".to_string())
            } else {
                Ok(())
            }
        });
        assert!(result);
        assert_eq!(executor.attempt_count("task1"), 3);
    }

    #[test]
    fn test_retry_executor_all_fail() {
        let executor = RetryExecutor::new(RetryPolicy::fixed(2, Duration::from_millis(1)));
        let result = executor.execute("task1", || Err("always fail".to_string()));
        assert!(!result);
        assert_eq!(executor.attempt_count("task1"), 3); // 1 initial + 2 retries
        assert!(!executor.is_successful("task1"));
    }

    #[test]
    fn test_retry_executor_history() {
        let executor = RetryExecutor::new(RetryPolicy::fixed(2, Duration::from_millis(1)));
        executor.execute("task1", || Ok(()));
        executor.execute("task2", || Err("fail".to_string()));
        let history = executor.history();
        assert_eq!(history.len(), 4); // task1: 1, task2: 3
    }

    #[test]
    fn test_retry_executor_clear_history() {
        let executor = RetryExecutor::new(RetryPolicy::fixed(0, Duration::from_millis(1)));
        executor.execute("task1", || Ok(()));
        assert!(!executor.history().is_empty());
        executor.clear_history();
        assert!(executor.history().is_empty());
    }

    // ====================================================================
    // DistributedLockManager 测试
    // ====================================================================

    #[test]
    fn test_lock_acquire_success() {
        let mgr = DistributedLockManager::new(60);
        let status = mgr.try_acquire("task1", "instance-1");
        assert_eq!(status, LockStatus::Acquired);
        assert!(mgr.is_locked("task1"));
    }

    #[test]
    fn test_lock_acquire_held_by_other() {
        let mgr = DistributedLockManager::new(60);
        mgr.try_acquire("task1", "instance-1");
        let status = mgr.try_acquire("task1", "instance-2");
        assert_eq!(status, LockStatus::HeldByOther);
    }

    #[test]
    fn test_lock_acquire_same_owner_renews() {
        let mgr = DistributedLockManager::new(60);
        mgr.try_acquire("task1", "instance-1");
        // 同一持有者再次获取应成功（续期）
        let status = mgr.try_acquire("task1", "instance-1");
        assert_eq!(status, LockStatus::Acquired);
    }

    #[test]
    fn test_lock_release() {
        let mgr = DistributedLockManager::new(60);
        mgr.try_acquire("task1", "instance-1");
        assert!(mgr.release("task1", "instance-1"));
        assert!(!mgr.is_locked("task1"));
    }

    #[test]
    fn test_lock_release_wrong_owner_fails() {
        let mgr = DistributedLockManager::new(60);
        mgr.try_acquire("task1", "instance-1");
        assert!(!mgr.release("task1", "instance-2"));
        assert!(mgr.is_locked("task1"));
    }

    #[test]
    fn test_lock_renew() {
        let mgr = DistributedLockManager::new(60);
        mgr.try_acquire("task1", "instance-1");
        assert!(mgr.renew("task1", "instance-1", 120));
    }

    #[test]
    fn test_lock_renew_wrong_owner_fails() {
        let mgr = DistributedLockManager::new(60);
        mgr.try_acquire("task1", "instance-1");
        assert!(!mgr.renew("task1", "instance-2", 120));
    }

    #[test]
    fn test_lock_owner() {
        let mgr = DistributedLockManager::new(60);
        mgr.try_acquire("task1", "instance-1");
        assert_eq!(mgr.lock_owner("task1"), Some("instance-1".to_string()));
        assert_eq!(mgr.lock_owner("nonexistent"), None);
    }

    #[test]
    fn test_lock_expired_can_be_reacquired() {
        let mgr = DistributedLockManager::new(60);
        mgr.try_acquire_with_ttl("task1", "instance-1", 0); // TTL=0, immediately expired
        let status = mgr.try_acquire("task1", "instance-2");
        assert_eq!(status, LockStatus::Acquired);
    }

    #[test]
    fn test_lock_cleanup_expired() {
        let mgr = DistributedLockManager::new(60);
        mgr.try_acquire_with_ttl("task1", "instance-1", 0);
        mgr.try_acquire_with_ttl("task2", "instance-1", 0);
        mgr.try_acquire("task3", "instance-1"); // active
        let cleaned = mgr.cleanup_expired();
        assert_eq!(cleaned, 2);
        assert_eq!(mgr.active_lock_count(), 1);
    }

    #[test]
    fn test_lock_active_count() {
        let mgr = DistributedLockManager::new(60);
        assert_eq!(mgr.active_lock_count(), 0);
        mgr.try_acquire("task1", "instance-1");
        mgr.try_acquire("task2", "instance-1");
        assert_eq!(mgr.active_lock_count(), 2);
    }

    // ====================================================================
    // TaskStats 测试
    // ====================================================================

    #[test]
    fn test_task_stats_record() {
        let mut stats = TaskStats::new();
        stats.record(true, 100);
        stats.record(false, 200);
        stats.record(true, 150);
        assert_eq!(stats.total_executions, 3);
        assert_eq!(stats.successful_executions, 2);
        assert_eq!(stats.failed_executions, 1);
        assert_eq!(stats.total_duration_ms, 450);
    }

    #[test]
    fn test_task_stats_success_rate() {
        let mut stats = TaskStats::new();
        assert_eq!(stats.success_rate(), 0.0);
        stats.record(true, 100);
        stats.record(true, 100);
        stats.record(false, 100);
        assert!((stats.success_rate() - 66.66666666666667).abs() < 0.01);
    }

    #[test]
    fn test_task_stats_avg_duration() {
        let mut stats = TaskStats::new();
        stats.record(true, 100);
        stats.record(true, 200);
        assert_eq!(stats.avg_duration_ms(), 150.0);
    }

    #[test]
    fn test_task_stats_last_execution() {
        let mut stats = TaskStats::new();
        assert!(stats.last_execution_at.is_none());
        assert!(stats.last_success.is_none());
        stats.record(true, 100);
        assert!(stats.last_execution_at.is_some());
        assert_eq!(stats.last_success, Some(true));
    }

    #[test]
    fn test_task_stats_manager() {
        let mgr = TaskStatsManager::new();
        mgr.record("task1", true, 100);
        mgr.record("task1", false, 200);
        mgr.record("task2", true, 50);
        let stats1 = mgr.get_stats("task1").unwrap();
        assert_eq!(stats1.total_executions, 2);
        let stats2 = mgr.get_stats("task2").unwrap();
        assert_eq!(stats2.total_executions, 1);
        assert_eq!(mgr.tracked_tasks().len(), 2);
    }

    #[test]
    fn test_task_stats_manager_reset() {
        let mgr = TaskStatsManager::new();
        mgr.record("task1", true, 100);
        assert!(mgr.get_stats("task1").is_some());
        assert!(mgr.reset("task1"));
        assert!(mgr.get_stats("task1").is_none());
        assert!(!mgr.reset("nonexistent"));
    }
}
