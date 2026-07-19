//! Job handler abstractions used by [`super::CronScheduler`].
//!
//! This module is intentionally small and self-contained: it provides the
//! `JobHandler` trait and a couple of default implementations so the
//! scheduler can invoke user-supplied logic when a task fires.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::RwLock;

use super::ScheduledTask;

/// Handler invoked by the scheduler when a task's cron expression matches
/// the current time. Implementations must be `Send + Sync` because they may
/// be invoked from a background thread.
pub trait JobHandler: Send + Sync {
    fn handle(&self, task: &ScheduledTask) -> Result<(), String>;
}

/// Default `JobHandler` that increments an atomic counter every time it is
/// invoked. Useful both as a no-op default and for unit tests that need to
/// observe how many times a job fired.
pub struct CounterJobHandler {
    counter: Arc<AtomicU64>,
}

impl CounterJobHandler {
    pub fn new() -> Self {
        Self {
            counter: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn counter(&self) -> Arc<AtomicU64> {
        self.counter.clone()
    }

    pub fn count(&self) -> u64 {
        self.counter.load(Ordering::SeqCst)
    }
}

impl Default for CounterJobHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl JobHandler for CounterJobHandler {
    fn handle(&self, _task: &ScheduledTask) -> Result<(), String> {
        self.counter.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// `JobHandler` that records the IDs of every task it has handled, in the
/// order they were handled. Useful for tests that need to assert which tasks
/// fired and in what order.
pub struct RecordingJobHandler {
    handled: Arc<RwLock<Vec<String>>>,
}

impl RecordingJobHandler {
    pub fn new() -> Self {
        Self {
            handled: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn handled_ids(&self) -> Vec<String> {
        self.handled.read().map(|h| h.clone()).unwrap_or_default()
    }

    pub fn clear(&self) {
        if let Ok(mut h) = self.handled.write() {
            h.clear();
        }
    }
}

impl Default for RecordingJobHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl JobHandler for RecordingJobHandler {
    fn handle(&self, task: &ScheduledTask) -> Result<(), String> {
        if let Ok(mut h) = self.handled.write() {
            h.push(task.id.clone());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_task(id: &str) -> ScheduledTask {
        ScheduledTask::new(id, id, "* * * * *")
    }

    #[test]
    fn test_counter_handler_increments() {
        let handler = CounterJobHandler::new();
        assert_eq!(handler.count(), 0);
        handler.handle(&sample_task("t1")).unwrap();
        handler.handle(&sample_task("t2")).unwrap();
        assert_eq!(handler.count(), 2);
    }

    #[test]
    fn test_recording_handler_records_ids() {
        let handler = RecordingJobHandler::new();
        handler.handle(&sample_task("a")).unwrap();
        handler.handle(&sample_task("b")).unwrap();
        handler.handle(&sample_task("c")).unwrap();
        assert_eq!(handler.handled_ids(), vec!["a", "b", "c"]);
        handler.clear();
        assert!(handler.handled_ids().is_empty());
    }

    #[test]
    fn test_handler_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CounterJobHandler>();
        assert_send_sync::<RecordingJobHandler>();
        assert_send_sync::<Arc<dyn JobHandler>>();
    }
}
