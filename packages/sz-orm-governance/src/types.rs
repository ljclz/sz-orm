//! Governance 核心类型定义

use serde::{Deserialize, Serialize};

/// 法规枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Regulation {
    Gdpr,
    Ccpa,
    Pipl,
}

/// 风险等级
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RiskLevel {
    High,
    Medium,
    Low,
}

/// Governance 错误
#[derive(Debug, thiserror::Error)]
pub enum GovernanceError {
    #[error("规则集缺失")]
    RulesetMissing,
    #[error("血缘构建失败: {0}")]
    LineageBuildFailed(String),
    #[error("合规审计失败: {0}")]
    ComplianceAuditFailed(String),
}
