//! 脑裂检测：failover 后旧主库恢复导致双主，检测并降级旧主库

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// 旧主库追踪状态
#[derive(Debug, Clone)]
struct OldMasterTracker {
    name: String,
    failover_time: Instant,
    demoted: bool,
}

/// 旧主库降级方式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemotionStrategy {
    DemoteToSlave,
    Isolate,
}

/// 脑裂告警
#[derive(Debug, Clone)]
pub struct SplitBrainAlert {
    pub old_master: String,
    pub new_master: String,
    pub detected_at: Instant,
    pub action: DemotionStrategy,
    pub message: String,
}

/// 脑裂检测器
pub struct SplitBrainDetector {
    old_masters: RwLock<HashMap<String, OldMasterTracker>>,
    check_interval: Duration,
    alerts: RwLock<Vec<SplitBrainAlert>>,
}

impl SplitBrainDetector {
    pub fn new(check_interval: Duration) -> Self {
        Self {
            old_masters: RwLock::new(HashMap::new()),
            check_interval,
            alerts: RwLock::new(Vec::new()),
        }
    }

    /// 注册旧主库（failover 后调用）
    pub fn register_old_master(&self, name: &str) {
        let mut old_masters = self.old_masters.write().expect("old_masters lock poisoned");
        old_masters.insert(
            name.to_string(),
            OldMasterTracker {
                name: name.to_string(),
                failover_time: Instant::now(),
                demoted: false,
            },
        );
    }

    /// 检测脑裂：旧主库是否已恢复（仍健康）且未被降级
    /// `old_master_healthy` 由外部健康检查提供
    pub fn check_split_brain(
        &self,
        old_master: &str,
        new_master: &str,
        old_master_healthy: bool,
    ) -> SplitBrainStatus {
        let old_masters = self.old_masters.read().expect("old_masters lock poisoned");
        let tracker = old_masters.get(old_master);

        if let Some(tracker) = tracker {
            if old_master_healthy && !tracker.demoted && old_master != new_master {
                return SplitBrainStatus::Detected {
                    old_master: old_master.to_string(),
                    new_master: new_master.to_string(),
                };
            }
        }
        SplitBrainStatus::NoSplitBrain
    }

    /// 处理脑裂：降级或隔离旧主库
    pub fn resolve_split_brain(
        &self,
        old_master: &str,
        new_master: &str,
        strategy: DemotionStrategy,
    ) -> SplitBrainAlert {
        {
            let mut old_masters = self.old_masters.write().expect("old_masters lock poisoned");
            if let Some(tracker) = old_masters.get_mut(old_master) {
                tracker.demoted = true;
            }
        }

        let action_msg = match strategy {
            DemotionStrategy::DemoteToSlave => "demoted to slave",
            DemotionStrategy::Isolate => "isolated",
        };
        let message = format!(
            "split-brain detected, old master '{}' {} (new master: '{}')",
            old_master, action_msg, new_master
        );

        let alert = SplitBrainAlert {
            old_master: old_master.to_string(),
            new_master: new_master.to_string(),
            detected_at: Instant::now(),
            action: strategy,
            message: message.clone(),
        };

        {
            let mut alerts = self.alerts.write().expect("alerts lock poisoned");
            alerts.push(alert.clone());
        }

        alert
    }

    /// 定期检测脑裂：遍历所有已注册旧主库
    /// `health_fn` 返回某旧主库是否仍健康
    pub fn periodic_check<F>(&self, new_master: &str, health_fn: F) -> Vec<SplitBrainStatus>
    where
        F: Fn(&str) -> bool,
    {
        let old_masters = self.old_masters.read().expect("old_masters lock poisoned");
        let mut results = Vec::new();

        for tracker in old_masters.values() {
            if tracker.demoted {
                continue;
            }
            let is_healthy = health_fn(&tracker.name);
            if is_healthy && tracker.name != new_master {
                results.push(SplitBrainStatus::Detected {
                    old_master: tracker.name.clone(),
                    new_master: new_master.to_string(),
                });
            }
        }

        results
    }

    /// 标记脑裂已解决
    pub fn mark_resolved(&self, old_master: &str) {
        let mut old_masters = self.old_masters.write().expect("old_masters lock poisoned");
        if let Some(tracker) = old_masters.get_mut(old_master) {
            tracker.demoted = true;
        }
    }

    /// 获取所有告警
    pub fn alerts(&self) -> Vec<SplitBrainAlert> {
        self.alerts.read().expect("alerts lock poisoned").clone()
    }

    /// 检查间隔
    pub fn check_interval(&self) -> Duration {
        self.check_interval
    }

    /// 旧主库是否已降级
    pub fn is_demoted(&self, old_master: &str) -> bool {
        let old_masters = self.old_masters.read().expect("old_masters lock poisoned");
        old_masters
            .get(old_master)
            .map(|t| t.demoted)
            .unwrap_or(false)
    }

    /// 旧主库注册距今时长
    pub fn time_since_failover(&self, old_master: &str) -> Option<Duration> {
        let old_masters = self.old_masters.read().expect("old_masters lock poisoned");
        old_masters
            .get(old_master)
            .map(|t| t.failover_time.elapsed())
    }
}

use super::manager::SplitBrainStatus;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_check_no_split_brain() {
        let detector = SplitBrainDetector::new(Duration::from_secs(1));
        detector.register_old_master("old_master");

        let status = detector.check_split_brain("old_master", "new_master", false);
        assert_eq!(status, SplitBrainStatus::NoSplitBrain);
    }

    #[test]
    fn test_split_brain_detected_when_old_master_healthy() {
        let detector = SplitBrainDetector::new(Duration::from_secs(1));
        detector.register_old_master("old_master");

        let status = detector.check_split_brain("old_master", "new_master", true);
        assert_eq!(
            status,
            SplitBrainStatus::Detected {
                old_master: "old_master".to_string(),
                new_master: "new_master".to_string(),
            }
        );
    }

    #[test]
    fn test_resolve_split_brain_demote() {
        let detector = SplitBrainDetector::new(Duration::from_secs(1));
        detector.register_old_master("old_master");

        let alert = detector.resolve_split_brain(
            "old_master",
            "new_master",
            DemotionStrategy::DemoteToSlave,
        );

        assert_eq!(alert.old_master, "old_master");
        assert_eq!(alert.new_master, "new_master");
        assert_eq!(alert.action, DemotionStrategy::DemoteToSlave);
        assert!(alert.message.contains("demoted to slave"));
        assert!(detector.is_demoted("old_master"));
    }

    #[test]
    fn test_resolve_split_brain_isolate() {
        let detector = SplitBrainDetector::new(Duration::from_secs(1));
        detector.register_old_master("old_master");

        let alert =
            detector.resolve_split_brain("old_master", "new_master", DemotionStrategy::Isolate);

        assert_eq!(alert.action, DemotionStrategy::Isolate);
        assert!(alert.message.contains("isolated"));
        assert!(detector.is_demoted("old_master"));
    }

    #[test]
    fn test_no_split_brain_after_demotion() {
        let detector = SplitBrainDetector::new(Duration::from_secs(1));
        detector.register_old_master("old_master");
        detector.resolve_split_brain("old_master", "new_master", DemotionStrategy::DemoteToSlave);

        let status = detector.check_split_brain("old_master", "new_master", true);
        assert_eq!(status, SplitBrainStatus::NoSplitBrain);
    }

    #[test]
    fn test_periodic_check_detects_multiple() {
        let detector = SplitBrainDetector::new(Duration::from_secs(1));
        detector.register_old_master("old1");
        detector.register_old_master("old2");

        let results =
            detector.periodic_check("new_master", |name| name == "old1" || name == "old2");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_periodic_check_skips_demoted() {
        let detector = SplitBrainDetector::new(Duration::from_secs(1));
        detector.register_old_master("old1");
        detector.register_old_master("old2");
        detector.resolve_split_brain("old1", "new_master", DemotionStrategy::DemoteToSlave);

        let results = detector.periodic_check("new_master", |_| true);
        assert_eq!(results.len(), 1);
        match &results[0] {
            SplitBrainStatus::Detected { old_master, .. } => {
                assert_eq!(old_master, "old2");
            }
            _ => panic!("expected Detected"),
        }
    }

    #[test]
    fn test_alerts_recorded() {
        let detector = SplitBrainDetector::new(Duration::from_secs(1));
        detector.register_old_master("old1");
        detector.register_old_master("old2");

        detector.resolve_split_brain("old1", "new1", DemotionStrategy::DemoteToSlave);
        detector.resolve_split_brain("old2", "new2", DemotionStrategy::Isolate);

        let alerts = detector.alerts();
        assert_eq!(alerts.len(), 2);
        assert_eq!(alerts[0].old_master, "old1");
        assert_eq!(alerts[1].old_master, "old2");
    }

    #[test]
    fn test_mark_resolved() {
        let detector = SplitBrainDetector::new(Duration::from_secs(1));
        detector.register_old_master("old_master");
        detector.mark_resolved("old_master");

        assert!(detector.is_demoted("old_master"));
        let status = detector.check_split_brain("old_master", "new_master", true);
        assert_eq!(status, SplitBrainStatus::NoSplitBrain);
    }

    #[test]
    fn test_check_interval() {
        let detector = SplitBrainDetector::new(Duration::from_secs(5));
        assert_eq!(detector.check_interval(), Duration::from_secs(5));
    }

    #[test]
    fn test_no_split_brain_same_master() {
        let detector = SplitBrainDetector::new(Duration::from_secs(1));
        detector.register_old_master("master");

        let status = detector.check_split_brain("master", "master", true);
        assert_eq!(status, SplitBrainStatus::NoSplitBrain);
    }
}
