//! 合规审计报告生成（TASK-007）
//!
//! 根据法规类型（GDPR/CCPA/PIPL）生成合规审计报告，
//! 包含该法规下的关键合规检查项和建议。

use crate::types::{GovernanceError, Regulation, RiskLevel};
use serde::{Deserialize, Serialize};

/// 合规报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    pub regulation: Regulation,
    pub findings: Vec<ComplianceFinding>,
}

/// 合规发现项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceFinding {
    pub field: String,
    pub risk_level: RiskLevel,
    pub suggestion: String,
}

/// 合规审计器
///
/// 根据法规类型生成合规检查项，覆盖各法规的核心合规要求。
pub struct ComplianceAuditor;

impl ComplianceAuditor {
    pub fn new() -> Self {
        Self
    }

    /// 执行合规审计
    ///
    /// 根据指定的法规类型返回对应的合规检查项。
    /// `ruleset_empty` 为 true 时返回 `RulesetMissing` 错误。
    pub fn audit(
        &self,
        regulation: &Regulation,
        ruleset_empty: bool,
    ) -> Result<ComplianceReport, GovernanceError> {
        if ruleset_empty {
            return Err(GovernanceError::RulesetMissing);
        }

        let findings = Self::generate_findings(regulation);

        Ok(ComplianceReport {
            regulation: regulation.clone(),
            findings,
        })
    }

    /// 根据法规类型生成合规检查项
    fn generate_findings(regulation: &Regulation) -> Vec<ComplianceFinding> {
        match regulation {
            Regulation::Gdpr => vec![
                ComplianceFinding {
                    field: "personal_data".into(),
                    risk_level: RiskLevel::High,
                    suggestion: "确保个人数据处理的合法依据（同意/合同/合法利益）".into(),
                },
                ComplianceFinding {
                    field: "right_to_erasure".into(),
                    risk_level: RiskLevel::Medium,
                    suggestion: "实现被遗忘权接口，支持数据主体删除请求".into(),
                },
                ComplianceFinding {
                    field: "data_portability".into(),
                    risk_level: RiskLevel::Low,
                    suggestion: "提供数据可携性功能，支持结构化导出".into(),
                },
                ComplianceFinding {
                    field: "cross_border_transfer".into(),
                    risk_level: RiskLevel::High,
                    suggestion: "跨境数据传输需确保目标国提供充分保护水平".into(),
                },
            ],
            Regulation::Ccpa => vec![
                ComplianceFinding {
                    field: "consumer_privacy".into(),
                    risk_level: RiskLevel::High,
                    suggestion: "确保消费者隐私权声明公开透明".into(),
                },
                ComplianceFinding {
                    field: "opt_out_sale".into(),
                    risk_level: RiskLevel::Medium,
                    suggestion: "实现选择退出个人信息出售的链接或按钮".into(),
                },
                ComplianceFinding {
                    field: "data_deletion".into(),
                    risk_level: RiskLevel::Medium,
                    suggestion: "支持消费者请求删除其个人信息".into(),
                },
            ],
            Regulation::Pipl => vec![
                ComplianceFinding {
                    field: "consent".into(),
                    risk_level: RiskLevel::High,
                    suggestion: "个人信息处理需取得个人同意，同意应自愿且明确".into(),
                },
                ComplianceFinding {
                    field: "sensitive_info".into(),
                    risk_level: RiskLevel::High,
                    suggestion: "敏感个人信息需单独同意并采取严格保护措施".into(),
                },
                ComplianceFinding {
                    field: "cross_border".into(),
                    risk_level: RiskLevel::High,
                    suggestion: "个人信息出境需通过安全评估或认证".into(),
                },
                ComplianceFinding {
                    field: "data_minimization".into(),
                    risk_level: RiskLevel::Medium,
                    suggestion: "遵循最小必要原则，仅收集实现处理目的所需信息".into(),
                },
            ],
        }
    }
}

impl Default for ComplianceAuditor {
    fn default() -> Self {
        Self::new()
    }
}
