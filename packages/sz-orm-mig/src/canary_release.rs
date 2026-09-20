//! 金丝雀发布管理器

use std::sync::Arc;

use parking_lot::RwLock;

use super::gray_release::{GrayProgress, ReleaseStatus};

/// 金丝雀发布管理器：金丝雀实例级灰度
pub struct CanaryReleaseManager {
    canary_instances: Arc<RwLock<Vec<String>>>,
    total_instances: u32,
    progress: Arc<RwLock<GrayProgress>>,
}

impl CanaryReleaseManager {
    pub fn new(total_instances: u32) -> Self {
        Self {
            canary_instances: Arc::new(RwLock::new(Vec::new())),
            total_instances,
            progress: Arc::new(RwLock::new(GrayProgress {
                release_id: String::new(),
                status: ReleaseStatus::Pending,
                current_percentage: 0,
                canary_healthy: false,
                started_at: 0,
            })),
        }
    }

    pub fn add_canary(&self, instance: &str) {
        self.canary_instances.write().push(instance.to_string());
    }

    pub fn canary_count(&self) -> usize {
        self.canary_instances.read().len()
    }

    pub fn start_canary(&self, release_id: &str) -> GrayProgress {
        let progress = GrayProgress {
            release_id: release_id.to_string(),
            status: ReleaseStatus::Running,
            current_percentage: (self.canary_count() as u32 * 100) / self.total_instances,
            canary_healthy: true,
            started_at: 0,
        };
        *self.progress.write() = progress.clone();
        progress
    }

    pub fn promote_to_full(&self, healthy: bool) -> Result<GrayProgress, String> {
        if !healthy {
            return Err("金丝雀健康判定未通过，不能全量推广".to_string());
        }
        let mut progress = self.progress.write();
        progress.current_percentage = 100;
        progress.status = ReleaseStatus::Completed;
        Ok(progress.clone())
    }

    pub fn rollback_canary(&self, _reason: &str) -> Result<GrayProgress, String> {
        let mut progress = self.progress.write();
        progress.status = ReleaseStatus::RolledBack;
        self.canary_instances.write().clear();
        Ok(progress.clone())
    }

    pub fn query_progress(&self) -> GrayProgress {
        self.progress.read().clone()
    }

    pub fn canary_instances(&self) -> Vec<String> {
        self.canary_instances.read().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_canary() {
        let manager = CanaryReleaseManager::new(10);
        manager.add_canary("instance-1");
        assert_eq!(manager.canary_count(), 1);
    }

    #[test]
    fn test_start_canary() {
        let manager = CanaryReleaseManager::new(10);
        manager.add_canary("instance-1");
        let progress = manager.start_canary("rel-1");
        assert_eq!(progress.status, ReleaseStatus::Running);
        assert_eq!(progress.current_percentage, 10);
    }

    #[test]
    fn test_promote_to_full_healthy() {
        let manager = CanaryReleaseManager::new(10);
        manager.add_canary("instance-1");
        manager.start_canary("rel-1");
        let progress = manager.promote_to_full(true).unwrap();
        assert_eq!(progress.status, ReleaseStatus::Completed);
        assert_eq!(progress.current_percentage, 100);
    }

    #[test]
    fn test_promote_to_full_unhealthy_rejected() {
        let manager = CanaryReleaseManager::new(10);
        manager.add_canary("instance-1");
        manager.start_canary("rel-1");
        assert!(manager.promote_to_full(false).is_err());
    }

    #[test]
    fn test_rollback_canary() {
        let manager = CanaryReleaseManager::new(10);
        manager.add_canary("instance-1");
        manager.start_canary("rel-1");
        manager.rollback_canary("unhealthy").unwrap();
        assert_eq!(manager.query_progress().status, ReleaseStatus::RolledBack);
        assert_eq!(manager.canary_count(), 0);
    }
}
