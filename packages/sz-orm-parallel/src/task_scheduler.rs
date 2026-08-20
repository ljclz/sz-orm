//! 并行任务调度器（Parallel Scheduler）
//!
//! 同步任务调度，支持优先级、依赖关系、重试。

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

/// 任务优先级
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[derive(Default)]
pub enum Priority {
    Low = 0,
    #[default]
    Normal = 1,
    High = 2,
    Critical = 3,
}


impl Priority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::Low => "low",
            Priority::Normal => "normal",
            Priority::High => "high",
            Priority::Critical => "critical",
        }
    }
}

/// 调度任务
#[derive(Debug, Clone)]
pub struct ScheduledTask {
    pub id: u64,
    pub priority: Priority,
    pub dependencies: HashSet<u64>,
    pub max_retries: u32,
    pub retry_count: u32,
}

impl ScheduledTask {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            priority: Priority::default(),
            dependencies: HashSet::new(),
            max_retries: 0,
            retry_count: 0,
        }
    }

    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_dependency(mut self, dep: u64) -> Self {
        self.dependencies.insert(dep);
        self
    }

    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    pub fn has_dependencies(&self) -> bool {
        !self.dependencies.is_empty()
    }

    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }
}

/// 调度状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ScheduleState {
    Pending,
    Ready,
    Running,
    Completed,
    Failed,
    Skipped,
}

impl ScheduleState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScheduleState::Pending => "pending",
            ScheduleState::Ready => "ready",
            ScheduleState::Running => "running",
            ScheduleState::Completed => "completed",
            ScheduleState::Failed => "failed",
            ScheduleState::Skipped => "skipped",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ScheduleState::Completed | ScheduleState::Failed | ScheduleState::Skipped
        )
    }
}

/// 任务调度器
pub struct TaskScheduler {
    tasks: RwLock<HashMap<u64, ScheduledTask>>,
    states: RwLock<HashMap<u64, ScheduleState>>,
    ready_queue: RwLock<VecDeque<u64>>,
    completed: RwLock<HashSet<u64>>,
    failed: RwLock<HashSet<u64>>,
    next_id: AtomicU64,
    total_scheduled: AtomicU64,
    total_completed: AtomicU64,
    total_failed: AtomicU64,
}

impl TaskScheduler {
    pub fn new() -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
            states: RwLock::new(HashMap::new()),
            ready_queue: RwLock::new(VecDeque::new()),
            completed: RwLock::new(HashSet::new()),
            failed: RwLock::new(HashSet::new()),
            next_id: AtomicU64::new(1),
            total_scheduled: AtomicU64::new(0),
            total_completed: AtomicU64::new(0),
            total_failed: AtomicU64::new(0),
        }
    }

    pub fn submit(&self, task: ScheduledTask) -> u64 {
        let id = task.id;
        self.total_scheduled.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut tasks) = self.tasks.write() {
            tasks.insert(id, task);
        }
        if let Ok(mut states) = self.states.write() {
            states.insert(id, ScheduleState::Pending);
        }
        self.update_ready_status(id);
        id
    }

    fn update_ready_status(&self, id: u64) {
        let tasks = self.tasks.read().ok();
        let completed = self.completed.read().ok();
        if let (Some(tasks), Some(completed)) = (tasks, completed) {
            if let Some(task) = tasks.get(&id) {
                let deps_met = task.dependencies.iter().all(|dep| completed.contains(dep));
                if deps_met {
                    drop(tasks);
                    drop(completed);
                    if let Ok(mut states) = self.states.write() {
                        states.insert(id, ScheduleState::Ready);
                    }
                    if let Ok(mut queue) = self.ready_queue.write() {
                        queue.push_back(id);
                    }
                }
            }
        }
    }

    pub fn next_ready(&self) -> Option<u64> {
        let next = self
            .ready_queue
            .write()
            .ok()
            .and_then(|mut q| q.pop_front());
        if let Some(id) = next {
            if let Ok(mut states) = self.states.write() {
                states.insert(id, ScheduleState::Running);
            }
            Some(id)
        } else {
            None
        }
    }

    pub fn mark_completed(&self, id: u64) {
        if let Ok(mut states) = self.states.write() {
            states.insert(id, ScheduleState::Completed);
        }
        if let Ok(mut completed) = self.completed.write() {
            completed.insert(id);
        }
        self.total_completed.fetch_add(1, Ordering::Relaxed);
        if let Ok(tasks) = self.tasks.read() {
            let dependents: Vec<u64> = tasks
                .iter()
                .filter(|(_, t)| t.dependencies.contains(&id))
                .map(|(&id, _)| id)
                .collect();
            drop(tasks);
            for dep_id in dependents {
                self.update_ready_status(dep_id);
            }
        }
    }

    pub fn mark_failed(&self, id: u64) {
        if let Ok(mut states) = self.states.write() {
            states.insert(id, ScheduleState::Failed);
        }
        if let Ok(mut failed) = self.failed.write() {
            failed.insert(id);
        }
        self.total_failed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn mark_skipped(&self, id: u64) {
        if let Ok(mut states) = self.states.write() {
            states.insert(id, ScheduleState::Skipped);
        }
    }

    pub fn state(&self, id: u64) -> ScheduleState {
        self.states
            .read()
            .ok()
            .and_then(|s| s.get(&id).copied())
            .unwrap_or(ScheduleState::Pending)
    }

    pub fn task(&self, id: u64) -> Option<ScheduledTask> {
        self.tasks.read().ok().and_then(|t| t.get(&id).cloned())
    }

    pub fn pending_count(&self) -> usize {
        self.states
            .read()
            .ok()
            .map(|s| {
                s.values()
                    .filter(|&&st| st == ScheduleState::Pending || st == ScheduleState::Ready)
                    .count()
            })
            .unwrap_or(0)
    }

    pub fn completed_count(&self) -> usize {
        self.completed.read().map(|c| c.len()).unwrap_or(0)
    }

    pub fn failed_count(&self) -> usize {
        self.failed.read().map(|f| f.len()).unwrap_or(0)
    }

    pub fn total_scheduled(&self) -> u64 {
        self.total_scheduled.load(Ordering::Relaxed)
    }

    pub fn total_completed(&self) -> u64 {
        self.total_completed.load(Ordering::Relaxed)
    }

    pub fn total_failed(&self) -> u64 {
        self.total_failed.load(Ordering::Relaxed)
    }

    pub fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    pub fn clear(&self) {
        if let Ok(mut tasks) = self.tasks.write() {
            tasks.clear();
        }
        if let Ok(mut states) = self.states.write() {
            states.clear();
        }
        if let Ok(mut queue) = self.ready_queue.write() {
            queue.clear();
        }
        if let Ok(mut completed) = self.completed.write() {
            completed.clear();
        }
        if let Ok(mut failed) = self.failed.write() {
            failed.clear();
        }
    }
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Normal);
        assert!(Priority::Normal > Priority::Low);
    }

    #[test]
    fn test_priority_as_str() {
        assert_eq!(Priority::Low.as_str(), "low");
        assert_eq!(Priority::Normal.as_str(), "normal");
        assert_eq!(Priority::High.as_str(), "high");
        assert_eq!(Priority::Critical.as_str(), "critical");
    }

    #[test]
    fn test_scheduled_task_new() {
        let task = ScheduledTask::new(1);
        assert_eq!(task.id, 1);
        assert_eq!(task.priority, Priority::Normal);
        assert!(!task.has_dependencies());
    }

    #[test]
    fn test_scheduled_task_with_priority() {
        let task = ScheduledTask::new(1).with_priority(Priority::High);
        assert_eq!(task.priority, Priority::High);
    }

    #[test]
    fn test_scheduled_task_with_dependency() {
        let task = ScheduledTask::new(1).with_dependency(2);
        assert!(task.has_dependencies());
        assert!(task.dependencies.contains(&2));
    }

    #[test]
    fn test_scheduled_task_with_max_retries() {
        let task = ScheduledTask::new(1).with_max_retries(3);
        assert_eq!(task.max_retries, 3);
        assert!(task.can_retry());
    }

    #[test]
    fn test_scheduled_task_can_retry() {
        let mut task = ScheduledTask::new(1).with_max_retries(2);
        assert!(task.can_retry());
        task.increment_retry();
        assert!(task.can_retry());
        task.increment_retry();
        assert!(!task.can_retry());
    }

    #[test]
    fn test_schedule_state_as_str() {
        assert_eq!(ScheduleState::Pending.as_str(), "pending");
        assert_eq!(ScheduleState::Completed.as_str(), "completed");
    }

    #[test]
    fn test_schedule_state_is_terminal() {
        assert!(ScheduleState::Completed.is_terminal());
        assert!(ScheduleState::Failed.is_terminal());
        assert!(!ScheduleState::Running.is_terminal());
    }

    #[test]
    fn test_task_scheduler_submit() {
        let scheduler = TaskScheduler::new();
        let id = scheduler.submit(ScheduledTask::new(1));
        assert_eq!(id, 1);
        assert_eq!(scheduler.total_scheduled(), 1);
    }

    #[test]
    fn test_task_scheduler_next_ready() {
        let scheduler = TaskScheduler::new();
        scheduler.submit(ScheduledTask::new(1));
        let next = scheduler.next_ready().unwrap();
        assert_eq!(next, 1);
        assert_eq!(scheduler.state(1), ScheduleState::Running);
    }

    #[test]
    fn test_task_scheduler_mark_completed() {
        let scheduler = TaskScheduler::new();
        scheduler.submit(ScheduledTask::new(1));
        scheduler.next_ready();
        scheduler.mark_completed(1);
        assert_eq!(scheduler.state(1), ScheduleState::Completed);
        assert_eq!(scheduler.completed_count(), 1);
    }

    #[test]
    fn test_task_scheduler_mark_failed() {
        let scheduler = TaskScheduler::new();
        scheduler.submit(ScheduledTask::new(1));
        scheduler.next_ready();
        scheduler.mark_failed(1);
        assert_eq!(scheduler.state(1), ScheduleState::Failed);
        assert_eq!(scheduler.failed_count(), 1);
    }

    #[test]
    fn test_task_scheduler_dependencies() {
        let scheduler = TaskScheduler::new();
        scheduler.submit(ScheduledTask::new(1));
        scheduler.submit(ScheduledTask::new(2).with_dependency(1));
        assert_eq!(scheduler.state(2), ScheduleState::Pending);
        scheduler.mark_completed(1);
        assert_eq!(scheduler.state(2), ScheduleState::Ready);
    }

    #[test]
    fn test_task_scheduler_pending_count() {
        let scheduler = TaskScheduler::new();
        scheduler.submit(ScheduledTask::new(1));
        scheduler.submit(ScheduledTask::new(2));
        assert_eq!(scheduler.pending_count(), 2);
    }

    #[test]
    fn test_task_scheduler_clear() {
        let scheduler = TaskScheduler::new();
        scheduler.submit(ScheduledTask::new(1));
        scheduler.clear();
        assert_eq!(scheduler.pending_count(), 0);
    }

    #[test]
    fn test_task_scheduler_next_id() {
        let scheduler = TaskScheduler::new();
        let id1 = scheduler.next_id();
        let id2 = scheduler.next_id();
        assert_eq!(id2, id1 + 1);
    }

    #[test]
    fn test_task_scheduler_mark_skipped() {
        let scheduler = TaskScheduler::new();
        scheduler.submit(ScheduledTask::new(1));
        scheduler.mark_skipped(1);
        assert_eq!(scheduler.state(1), ScheduleState::Skipped);
    }

    #[test]
    fn test_task_scheduler_task() {
        let scheduler = TaskScheduler::new();
        scheduler.submit(ScheduledTask::new(1).with_priority(Priority::High));
        let task = scheduler.task(1).unwrap();
        assert_eq!(task.priority, Priority::High);
    }

    #[test]
    fn test_task_scheduler_total_counts() {
        let scheduler = TaskScheduler::new();
        scheduler.submit(ScheduledTask::new(1));
        scheduler.submit(ScheduledTask::new(2));
        scheduler.next_ready();
        scheduler.mark_completed(1);
        scheduler.next_ready();
        scheduler.mark_failed(2);
        assert_eq!(scheduler.total_completed(), 1);
        assert_eq!(scheduler.total_failed(), 1);
    }
}
