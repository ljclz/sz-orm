//! 冻结期管理器：冻结期内变更拒绝，例外白名单匹配通过

use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use super::SafetyNetError;

/// 冻结配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreezeConfig {
    pub window_start: SystemTime,
    pub window_end: SystemTime,
    pub exceptions: HashSet<String>,
    pub max_exemption_per_hour: u32,
}

/// 冻结检查结果
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FreezeCheckResult {
    Allowed,
    Frozen,
    Exempted,
}

/// 豁免结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExemptionResult {
    pub change_id: String,
    pub approved: bool,
    pub reason: String,
}

/// 冻结期管理器
pub struct FreezeWindow {
    config: FreezeConfig,
    exemption_counts: Mutex<HashMap<String, u32>>,
}

impl FreezeWindow {
    pub fn new(config: FreezeConfig) -> Self {
        Self {
            config,
            exemption_counts: Mutex::new(HashMap::new()),
        }
    }

    pub fn config(&self) -> &FreezeConfig {
        &self.config
    }

    /// 检查变更是否允许
    pub fn check(&self, change_id: &str) -> FreezeCheckResult {
        let now = SystemTime::now();
        let in_window = now >= self.config.window_start && now <= self.config.window_end;
        if !in_window {
            return FreezeCheckResult::Allowed;
        }
        if self.config.exceptions.contains(change_id) {
            return FreezeCheckResult::Exempted;
        }
        FreezeCheckResult::Frozen
    }

    /// 请求豁免
    pub fn request_exemption(&self, change_id: &str) -> Result<ExemptionResult, SafetyNetError> {
        let mut counts = self.exemption_counts.lock();
        let count = counts.entry(change_id.to_string()).or_insert(0);
        *count += 1;
        if *count > self.config.max_exemption_per_hour {
            return Err(SafetyNetError::FreezeExemptionAbused(format!(
                "变更 {} 豁免申请 {} 次/小时超限",
                change_id, count
            )));
        }
        let check_result = self.check(change_id);
        let approved = check_result == FreezeCheckResult::Frozen;
        Ok(ExemptionResult {
            change_id: change_id.to_string(),
            approved,
            reason: if approved {
                "豁免审批通过".to_string()
            } else {
                "变更不在冻结期或已在白名单".to_string()
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_config() -> FreezeConfig {
        let now = SystemTime::now();
        FreezeConfig {
            window_start: now,
            window_end: now + Duration::from_secs(3600),
            exceptions: vec!["critical_fix".to_string()].into_iter().collect(),
            max_exemption_per_hour: 3,
        }
    }

    #[test]
    fn test_check_frozen() {
        let fw = FreezeWindow::new(make_config());
        assert_eq!(fw.check("random_change"), FreezeCheckResult::Frozen);
    }

    #[test]
    fn test_check_exempted() {
        let fw = FreezeWindow::new(make_config());
        assert_eq!(fw.check("critical_fix"), FreezeCheckResult::Exempted);
    }

    #[test]
    fn test_check_outside_window() {
        let now = SystemTime::now();
        let config = FreezeConfig {
            window_start: now + Duration::from_secs(3600),
            window_end: now + Duration::from_secs(7200),
            exceptions: HashSet::new(),
            max_exemption_per_hour: 3,
        };
        let fw = FreezeWindow::new(config);
        assert_eq!(fw.check("any_change"), FreezeCheckResult::Allowed);
    }

    #[test]
    fn test_request_exemption_approved() {
        let fw = FreezeWindow::new(make_config());
        let result = fw.request_exemption("new_change").unwrap();
        assert!(result.approved);
    }

    #[test]
    fn test_request_exemption_abused() {
        let fw = FreezeWindow::new(make_config());
        for _ in 0..3 {
            fw.request_exemption("repeated_change").unwrap();
        }
        let result = fw.request_exemption("repeated_change");
        assert!(matches!(
            result,
            Err(SafetyNetError::FreezeExemptionAbused(_))
        ));
    }
}
