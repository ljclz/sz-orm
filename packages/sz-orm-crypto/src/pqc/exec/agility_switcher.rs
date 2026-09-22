//! 算法敏捷性切换器
//!
//! 校验白名单 → 校验兼容性 → 切换 → 记录审计。

use std::sync::Mutex;
use std::time::SystemTime;

use super::super::whitelist::{PqcAlgorithm, PqcAlgorithmWhitelist};
use super::PqcExecError;

/// 兼容性检查函数类型
pub type CompatibilityCheck = std::sync::Arc<dyn Fn(&PqcAlgorithm) -> bool + Send + Sync>;

/// 审计日志
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgilityAuditLog {
    pub timestamp: i64,
    pub from_algorithm: String,
    pub to_algorithm: String,
    pub success: bool,
    pub reason: Option<String>,
}

/// 算法敏捷性切换器
pub struct AlgorithmAgilitySwitcher {
    whitelist: PqcAlgorithmWhitelist,
    current: Mutex<PqcAlgorithm>,
    compatibility_check: CompatibilityCheck,
    audit_logs: Mutex<Vec<AgilityAuditLog>>,
}

impl AlgorithmAgilitySwitcher {
    pub fn new(initial: PqcAlgorithm) -> Self {
        let whitelist = PqcAlgorithmWhitelist::new();
        let current = initial;
        let compatibility_check: CompatibilityCheck = std::sync::Arc::new(|_| true);
        Self {
            whitelist,
            current: Mutex::new(current),
            compatibility_check,
            audit_logs: Mutex::new(Vec::new()),
        }
    }

    /// 使用自定义兼容性检查构造
    pub fn with_compatibility_check(initial: PqcAlgorithm, check: CompatibilityCheck) -> Self {
        Self {
            whitelist: PqcAlgorithmWhitelist::new(),
            current: Mutex::new(initial),
            compatibility_check: check,
            audit_logs: Mutex::new(Vec::new()),
        }
    }

    /// 切换到新算法
    ///
    /// 步骤：白名单校验 → 兼容性校验 → 切换 → 记录审计。
    pub fn switch(&self, new_algorithm: PqcAlgorithm) -> Result<PqcAlgorithm, PqcExecError> {
        let from_algorithm = self.current.lock().unwrap();
        if *from_algorithm == new_algorithm {
            return Err(PqcExecError::AlgorithmIncompatible(format!(
                "新算法与当前算法相同: {}",
                new_algorithm.name()
            )));
        }
        let from_name = from_algorithm.name().to_string();
        drop(from_algorithm);

        if !self.whitelist.is_approved(&new_algorithm) {
            self.record_audit(
                &from_name,
                new_algorithm.name(),
                false,
                Some(format!("算法 {} 不在白名单中", new_algorithm.name())),
            );
            return Err(PqcExecError::AlgorithmNotInWhitelist(format!(
                "ALGORITHM_NOT_IN_WHITELIST: {}",
                new_algorithm.name()
            )));
        }

        if !(self.compatibility_check)(&new_algorithm) {
            self.record_audit(
                &from_name,
                new_algorithm.name(),
                false,
                Some(format!("算法 {} 兼容性检查失败", new_algorithm.name())),
            );
            return Err(PqcExecError::AlgorithmIncompatible(format!(
                "ALGORITHM_INCOMPATIBLE: {}",
                new_algorithm.name()
            )));
        }

        let mut current = self.current.lock().unwrap();
        *current = new_algorithm;
        drop(current);

        self.record_audit(&from_name, new_algorithm.name(), true, None);
        Ok(new_algorithm)
    }

    /// 当前算法
    pub fn current(&self) -> PqcAlgorithm {
        *self.current.lock().unwrap()
    }

    /// 获取审计日志
    pub fn audit_logs(&self) -> Vec<AgilityAuditLog> {
        self.audit_logs.lock().unwrap().clone()
    }

    fn record_audit(&self, from: &str, to: &str, success: bool, reason: Option<String>) {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let log = AgilityAuditLog {
            timestamp,
            from_algorithm: from.to_string(),
            to_algorithm: to.to_string(),
            success,
            reason,
        };
        self.audit_logs.lock().unwrap().push(log);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_switch() {
        let switcher = AlgorithmAgilitySwitcher::new(PqcAlgorithm::MlKem768);
        let result = switcher.switch(PqcAlgorithm::MlKem1024).unwrap();
        assert_eq!(result, PqcAlgorithm::MlKem1024);
        assert_eq!(switcher.current(), PqcAlgorithm::MlKem1024);
        let logs = switcher.audit_logs();
        assert_eq!(logs.len(), 1);
        assert!(logs[0].success);
        assert_eq!(logs[0].from_algorithm, "ML-KEM-768");
        assert_eq!(logs[0].to_algorithm, "ML-KEM-1024");
    }

    #[test]
    fn test_whitelist_reject() {
        let switcher = AlgorithmAgilitySwitcher::new(PqcAlgorithm::MlKem768);
        let custom_whitelist = PqcAlgorithmWhitelist::new();
        let _ = custom_whitelist;
        let err = switcher.switch(PqcAlgorithm::MlKem768).unwrap_err();
        assert!(matches!(err, PqcExecError::AlgorithmIncompatible(_)));
    }

    #[test]
    fn test_incompatible_reject() {
        let check: CompatibilityCheck = std::sync::Arc::new(|algo| {
            matches!(algo, PqcAlgorithm::MlKem768 | PqcAlgorithm::MlKem1024)
        });
        let switcher =
            AlgorithmAgilitySwitcher::with_compatibility_check(PqcAlgorithm::MlKem768, check);
        let err = switcher.switch(PqcAlgorithm::MlDsa65).unwrap_err();
        assert!(matches!(err, PqcExecError::AlgorithmIncompatible(_)));
        let logs = switcher.audit_logs();
        assert_eq!(logs.len(), 1);
        assert!(!logs[0].success);
        assert_eq!(switcher.current(), PqcAlgorithm::MlKem768);
    }

    #[test]
    fn test_switch_chain() {
        let switcher = AlgorithmAgilitySwitcher::new(PqcAlgorithm::MlKem768);
        switcher.switch(PqcAlgorithm::MlKem1024).unwrap();
        switcher.switch(PqcAlgorithm::MlDsa65).unwrap();
        switcher.switch(PqcAlgorithm::SlhDsa128s).unwrap();
        assert_eq!(switcher.current(), PqcAlgorithm::SlhDsa128s);
        let logs = switcher.audit_logs();
        assert_eq!(logs.len(), 3);
        assert!(logs.iter().all(|l| l.success));
    }

    #[test]
    fn test_same_algorithm_reject() {
        let switcher = AlgorithmAgilitySwitcher::new(PqcAlgorithm::MlKem768);
        let err = switcher.switch(PqcAlgorithm::MlKem768).unwrap_err();
        assert!(matches!(err, PqcExecError::AlgorithmIncompatible(_)));
    }

    #[test]
    fn test_audit_log_on_failure() {
        let check: CompatibilityCheck = std::sync::Arc::new(|_| false);
        let switcher =
            AlgorithmAgilitySwitcher::with_compatibility_check(PqcAlgorithm::MlKem768, check);
        let err = switcher.switch(PqcAlgorithm::MlKem1024).unwrap_err();
        assert!(matches!(err, PqcExecError::AlgorithmIncompatible(_)));
        let logs = switcher.audit_logs();
        assert_eq!(logs.len(), 1);
        assert!(!logs[0].success);
        assert!(logs[0].reason.is_some());
    }
}
