//! PQC 降级管理器

use std::sync::Mutex;
use std::time::SystemTime;

use super::PqcError;

/// 降级审计日志
#[derive(Debug, Clone)]
pub struct DegradationAuditLog {
    pub timestamp: i64,
    pub reason: String,
    pub affected_scope: String,
    pub tag: String,
}

/// PQC 降级管理器
pub struct PqcDegradationManager {
    logs: Mutex<Vec<DegradationAuditLog>>,
    degraded: Mutex<bool>,
}

impl PqcDegradationManager {
    pub fn new() -> Self {
        Self {
            logs: Mutex::new(Vec::new()),
            degraded: Mutex::new(false),
        }
    }

    pub fn degrade(&self, reason: &str, affected_scope: &str) -> Result<(), PqcError> {
        *self.degraded.lock().unwrap() = true;

        let log = DegradationAuditLog {
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
            reason: reason.to_string(),
            affected_scope: affected_scope.to_string(),
            tag: "PQC_DEGRADED".to_string(),
        };

        self.logs.lock().unwrap().push(log);
        Ok(())
    }

    pub fn is_degraded(&self) -> bool {
        *self.degraded.lock().unwrap()
    }

    pub fn recover(&self) {
        *self.degraded.lock().unwrap() = false;
    }

    pub fn get_logs(&self) -> Vec<DegradationAuditLog> {
        self.logs.lock().unwrap().clone()
    }

    pub fn log_count(&self) -> usize {
        self.logs.lock().unwrap().len()
    }
}

impl Default for PqcDegradationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_degrade() {
        let manager = PqcDegradationManager::new();
        manager.degrade("PQC 算法不可用", "TLS").unwrap();

        assert!(manager.is_degraded());
        assert_eq!(manager.log_count(), 1);
    }

    #[test]
    fn test_degrade_log_content() {
        let manager = PqcDegradationManager::new();
        manager.degrade("握手失败", "connection").unwrap();

        let logs = manager.get_logs();
        assert_eq!(logs[0].tag, "PQC_DEGRADED");
        assert_eq!(logs[0].reason, "握手失败");
        assert_eq!(logs[0].affected_scope, "connection");
    }

    #[test]
    fn test_recover() {
        let manager = PqcDegradationManager::new();
        manager.degrade("test", "test").unwrap();
        assert!(manager.is_degraded());

        manager.recover();
        assert!(!manager.is_degraded());
    }

    #[test]
    fn test_multiple_degradations() {
        let manager = PqcDegradationManager::new();
        manager.degrade("reason1", "scope1").unwrap();
        manager.degrade("reason2", "scope2").unwrap();

        assert_eq!(manager.log_count(), 2);
    }
}
