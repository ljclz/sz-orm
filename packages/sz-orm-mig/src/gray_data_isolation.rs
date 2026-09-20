//! 灰度数据隔离守卫

use std::sync::Arc;

use parking_lot::RwLock;

/// 脏数据检测报告
#[derive(Debug, Clone)]
pub struct ContaminationReport {
    pub detected: bool,
    pub source_instance: String,
    pub affected_tables: Vec<String>,
    pub action: ContainmentAction,
}

/// 隔离动作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainmentAction {
    None,
    IsolateGray,
    FullRollback,
}

/// 灰度数据隔离守卫
pub struct GrayDataIsolationGuard {
    isolated_instances: Arc<RwLock<Vec<String>>>,
    contamination_detected: Arc<RwLock<bool>>,
}

impl GrayDataIsolationGuard {
    pub fn new() -> Self {
        Self {
            isolated_instances: Arc::new(RwLock::new(Vec::new())),
            contamination_detected: Arc::new(RwLock::new(false)),
        }
    }

    pub fn check_contamination(
        &self,
        gray_instance: &str,
        dirty_data_tables: &[String],
    ) -> ContaminationReport {
        if dirty_data_tables.is_empty() {
            return ContaminationReport {
                detected: false,
                source_instance: gray_instance.to_string(),
                affected_tables: vec![],
                action: ContainmentAction::None,
            };
        }

        *self.contamination_detected.write() = true;
        self.isolated_instances
            .write()
            .push(gray_instance.to_string());

        ContaminationReport {
            detected: true,
            source_instance: gray_instance.to_string(),
            affected_tables: dirty_data_tables.to_vec(),
            action: ContainmentAction::FullRollback,
        }
    }

    pub fn is_isolated(&self, instance: &str) -> bool {
        self.isolated_instances.read().iter().any(|i| i == instance)
    }

    pub fn contamination_detected(&self) -> bool {
        *self.contamination_detected.read()
    }

    pub fn isolated_instances(&self) -> Vec<String> {
        self.isolated_instances.read().clone()
    }

    pub fn reset(&self) {
        self.isolated_instances.write().clear();
        *self.contamination_detected.write() = false;
    }
}

impl Default for GrayDataIsolationGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_contamination() {
        let guard = GrayDataIsolationGuard::new();
        let report = guard.check_contamination("gray-1", &[]);
        assert!(!report.detected);
        assert_eq!(report.action, ContainmentAction::None);
    }

    #[test]
    fn test_contamination_detected() {
        let guard = GrayDataIsolationGuard::new();
        let report = guard.check_contamination("gray-1", &["orders".to_string()]);
        assert!(report.detected);
        assert_eq!(report.action, ContainmentAction::FullRollback);
        assert!(guard.is_isolated("gray-1"));
    }

    #[test]
    fn test_multiple_contaminations() {
        let guard = GrayDataIsolationGuard::new();
        guard.check_contamination("gray-1", &["orders".to_string()]);
        guard.check_contamination("gray-2", &["users".to_string()]);
        assert_eq!(guard.isolated_instances().len(), 2);
    }

    #[test]
    fn test_reset() {
        let guard = GrayDataIsolationGuard::new();
        guard.check_contamination("gray-1", &["orders".to_string()]);
        guard.reset();
        assert!(!guard.contamination_detected());
        assert!(guard.isolated_instances().is_empty());
    }

    #[test]
    fn test_not_isolated() {
        let guard = GrayDataIsolationGuard::new();
        assert!(!guard.is_isolated("gray-1"));
    }
}
