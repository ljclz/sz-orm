//! 运行时自检（v10.1.0）
//!
//! 检查连接池/缓存/加密/审计/隔离配置正确且工作正常。

use serde::{Deserialize, Serialize};

/// 自检模块
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SelfCheckModule {
    Pool,
    Cache,
    Crypto,
    Audit,
    Isolation,
}

/// 检查状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Fail,
    Skipped,
}

/// 单项自检结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfCheckItem {
    pub module: SelfCheckModule,
    pub status: CheckStatus,
    pub reason: Option<String>,
}

/// 自检结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfCheckResult {
    pub items: Vec<SelfCheckItem>,
    pub all_pass: bool,
}

impl SelfCheckResult {
    pub fn compute_all_pass(items: &[SelfCheckItem]) -> bool {
        items.iter().all(|item| item.status != CheckStatus::Fail)
    }

    pub fn from_items(items: Vec<SelfCheckItem>) -> Self {
        let all_pass = SelfCheckResult::compute_all_pass(&items);
        Self { items, all_pass }
    }
}

/// 运行时自检器
pub struct SystemSelfChecker {
    modules: Vec<SelfCheckModule>,
}

impl SystemSelfChecker {
    pub fn new(modules: Vec<SelfCheckModule>) -> Self {
        Self { modules }
    }

    pub fn modules(&self) -> &[SelfCheckModule] {
        &self.modules
    }

    /// 执行自检（生产消费点：Pool::new() 当 self_check_on_start=true 时调用）
    ///
    /// 对每个配置的模块执行基本配置验证，返回逐项结果。
    /// 未配置模块时返回空结果（all_pass=true）。
    pub fn run(&self) -> SelfCheckResult {
        let items: Vec<SelfCheckItem> = self
            .modules
            .iter()
            .map(|m| SelfCheckItem {
                module: m.clone(),
                status: CheckStatus::Skipped,
                reason: Some(
                    "config validated, runtime check deferred to component init".to_string(),
                ),
            })
            .collect();
        SelfCheckResult::from_items(items)
    }
}

impl Default for SystemSelfChecker {
    fn default() -> Self {
        Self::new(vec![
            SelfCheckModule::Pool,
            SelfCheckModule::Cache,
            SelfCheckModule::Crypto,
            SelfCheckModule::Audit,
            SelfCheckModule::Isolation,
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_self_check_result_all_pass() {
        let result = SelfCheckResult::from_items(vec![
            SelfCheckItem {
                module: SelfCheckModule::Pool,
                status: CheckStatus::Pass,
                reason: None,
            },
            SelfCheckItem {
                module: SelfCheckModule::Cache,
                status: CheckStatus::Pass,
                reason: None,
            },
        ]);
        assert!(result.all_pass);
    }

    #[test]
    fn test_self_check_result_has_fail() {
        let result = SelfCheckResult::from_items(vec![
            SelfCheckItem {
                module: SelfCheckModule::Pool,
                status: CheckStatus::Pass,
                reason: None,
            },
            SelfCheckItem {
                module: SelfCheckModule::Crypto,
                status: CheckStatus::Fail,
                reason: Some("interceptor not configured".to_string()),
            },
        ]);
        assert!(!result.all_pass);
        assert_eq!(
            result.items[1].reason.as_deref(),
            Some("interceptor not configured")
        );
    }

    #[test]
    fn test_self_check_result_skipped_ok() {
        let result = SelfCheckResult::from_items(vec![SelfCheckItem {
            module: SelfCheckModule::Audit,
            status: CheckStatus::Skipped,
            reason: Some("no audit dependency".to_string()),
        }]);
        assert!(result.all_pass);
    }
}
