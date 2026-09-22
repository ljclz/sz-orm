//! v7.9.0 演进安全网与回滚沙箱
//!
//! 冻结期管理 + 影响面分析 + 影子流量验证 + 回滚沙箱 + 审计时间线。
//! 复用 v7.8.0 的 `GrayReleaseOrchestrator`、`GrayTrafficRouter`、`AutoRollbackTrigger`。

pub mod audit_timeline;
pub mod freeze_window;
pub mod impact_analyzer;

#[cfg(feature = "shadow-traffic-verify")]
pub mod shadow_verifier;

#[cfg(feature = "rollback-sandbox")]
pub mod rollback_sandbox;

pub use audit_timeline::{AuditTimeline, EvolutionAuditTimeline, TimelineEventType, TimelineNode};
pub use freeze_window::{ExemptionResult, FreezeCheckResult, FreezeConfig, FreezeWindow};
pub use impact_analyzer::{ImpactAnalyzer, ImpactConfig, ImpactReport};

#[cfg(feature = "shadow-traffic-verify")]
pub use shadow_verifier::{ShadowConfig, ShadowTrafficVerifier, ShadowVerifyResult};

#[cfg(feature = "rollback-sandbox")]
pub use rollback_sandbox::{RollbackSandbox, SandboxConfig, SandboxResult};

/// 安全网错误类型
#[derive(Debug, thiserror::Error)]
pub enum SafetyNetError {
    #[error("冻结期内变更拒绝: {0}")]
    EvolutionFrozen(String),
    #[error("豁免申请频率超限: {0}")]
    FreezeExemptionAbused(String),
    #[error("影响面分析超时: {0}")]
    ImpactAnalysisTimeout(String),
    #[error("影子流量发现缺陷: {0}")]
    ShadowTrafficDefectDetected(String),
    #[error("回滚沙箱预演失败: {0}")]
    RollbackSandboxFailed(String),
    #[error("发布未找到: {0}")]
    ReleaseNotFound(String),
}
