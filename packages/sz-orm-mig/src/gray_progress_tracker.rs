//! 灰度发布进度追踪器

use std::sync::Arc;

use parking_lot::RwLock;

use super::gray_release::{GrayProgress, ReleaseStatus};

/// 灰度发布进度追踪器
pub struct GrayReleaseProgressTracker {
    progress: Arc<RwLock<GrayProgress>>,
    history: Arc<RwLock<Vec<ProgressEvent>>>,
}

/// 进度事件
#[derive(Debug, Clone)]
pub struct ProgressEvent {
    pub timestamp: i64,
    pub event_type: String,
    pub percentage: u32,
    pub status: ReleaseStatus,
}

impl GrayReleaseProgressTracker {
    pub fn new() -> Self {
        Self {
            progress: Arc::new(RwLock::new(GrayProgress {
                release_id: String::new(),
                status: ReleaseStatus::Pending,
                current_percentage: 0,
                canary_healthy: false,
                started_at: 0,
            })),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn init(&self, release_id: &str) {
        let progress = GrayProgress {
            release_id: release_id.to_string(),
            status: ReleaseStatus::Running,
            current_percentage: 0,
            canary_healthy: true,
            started_at: 0,
        };
        *self.progress.write() = progress;
        self.record_event("init", 0, ReleaseStatus::Running);
    }

    pub fn update_percentage(&self, percentage: u32) {
        let mut progress = self.progress.write();
        progress.current_percentage = percentage;
        self.record_event("advance", percentage, progress.status);
    }

    pub fn pause(&self) {
        let mut progress = self.progress.write();
        progress.status = ReleaseStatus::Paused;
        self.record_event("pause", progress.current_percentage, ReleaseStatus::Paused);
    }

    pub fn resume(&self) {
        let mut progress = self.progress.write();
        progress.status = ReleaseStatus::Running;
        self.record_event(
            "resume",
            progress.current_percentage,
            ReleaseStatus::Running,
        );
    }

    pub fn complete(&self) {
        let mut progress = self.progress.write();
        progress.status = ReleaseStatus::Completed;
        progress.current_percentage = 100;
        self.record_event("complete", 100, ReleaseStatus::Completed);
    }

    pub fn rollback(&self) {
        let mut progress = self.progress.write();
        progress.status = ReleaseStatus::RolledBack;
        self.record_event(
            "rollback",
            progress.current_percentage,
            ReleaseStatus::RolledBack,
        );
    }

    pub fn query_progress(&self) -> GrayProgress {
        self.progress.read().clone()
    }

    pub fn history(&self) -> Vec<ProgressEvent> {
        self.history.read().clone()
    }

    fn record_event(&self, event_type: &str, percentage: u32, status: ReleaseStatus) {
        self.history.write().push(ProgressEvent {
            timestamp: 0,
            event_type: event_type.to_string(),
            percentage,
            status,
        });
    }
}

impl Default for GrayReleaseProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        let tracker = GrayReleaseProgressTracker::new();
        tracker.init("rel-1");
        let progress = tracker.query_progress();
        assert_eq!(progress.release_id, "rel-1");
        assert_eq!(progress.status, ReleaseStatus::Running);
    }

    #[test]
    fn test_update_percentage() {
        let tracker = GrayReleaseProgressTracker::new();
        tracker.init("rel-1");
        tracker.update_percentage(20);
        assert_eq!(tracker.query_progress().current_percentage, 20);
    }

    #[test]
    fn test_pause_resume() {
        let tracker = GrayReleaseProgressTracker::new();
        tracker.init("rel-1");
        tracker.update_percentage(30);
        tracker.pause();
        assert_eq!(tracker.query_progress().status, ReleaseStatus::Paused);
        assert_eq!(tracker.query_progress().current_percentage, 30);

        tracker.resume();
        assert_eq!(tracker.query_progress().status, ReleaseStatus::Running);
        assert_eq!(tracker.query_progress().current_percentage, 30);
    }

    #[test]
    fn test_complete() {
        let tracker = GrayReleaseProgressTracker::new();
        tracker.init("rel-1");
        tracker.complete();
        assert_eq!(tracker.query_progress().status, ReleaseStatus::Completed);
        assert_eq!(tracker.query_progress().current_percentage, 100);
    }

    #[test]
    fn test_rollback() {
        let tracker = GrayReleaseProgressTracker::new();
        tracker.init("rel-1");
        tracker.update_percentage(40);
        tracker.rollback();
        assert_eq!(tracker.query_progress().status, ReleaseStatus::RolledBack);
    }

    #[test]
    fn test_history() {
        let tracker = GrayReleaseProgressTracker::new();
        tracker.init("rel-1");
        tracker.update_percentage(20);
        tracker.pause();
        tracker.resume();
        assert_eq!(tracker.history().len(), 4);
    }
}
