//! v7.9.0 AI 自治闭环生产化：决策可解释性 + A/B 测试 + 冲突仲裁 + 回放
//!
//! 复用 v7.8.0 的 `AutonomousDecisionAuditor`、`AutonomousPolicyEngine`，
//! 以及 `GrayTrafficRouter` 灰度分流能力。

pub mod ab_test;
pub mod arbitrator;
pub mod explainer;
pub mod replayer;

// v8.1.0 组3：A/B 统计显著性引擎
#[cfg(feature = "ai-ab-testing")]
pub mod ab_significance;

pub use ab_test::{AbTestConfig, AbTestOrchestrator, AbTestResult, GrayTrafficRouter};
pub use arbitrator::{ArbitratedPolicy, ArbitrationConfig, ArbitrationResult, PolicyArbitrator};
pub use explainer::{DecisionExplainer, ExplanationReport, XaiConfig};
pub use replayer::{DecisionReplayer, ReplayReport};

// v8.1.0 组3 导出
#[cfg(feature = "ai-ab-testing")]
pub use ab_significance::{
    AbExperimentData, AbGroup, AbGroupData, AbSignificanceResult, AbStatSignificanceEngine,
};

/// XAI 错误类型
#[derive(Debug, thiserror::Error)]
pub enum XaiError {
    #[error("审计器不可用")]
    AuditorUnavailable,
    #[error("决策解释不完整: {0}")]
    ExplanationIncomplete(String),
    #[error("回放数据已过期: {0}")]
    ReplayDataExpired(String),
    #[error("策略冲突无法仲裁: {0}")]
    PolicyConflictUnresolved(String),
    #[error("A/B 测试回退单决策: {0}")]
    AbTestFallbackSingleDecision(String),
    #[error("候选未通过影子验证: {0}")]
    CandidateNotShadowVerified(String),
    #[error("A/B 测试未找到: {0}")]
    AbTestNotFound(String),
    #[error("候选数量不足: 需要 >= 2, 实际 {0}")]
    InsufficientCandidates(usize),
}
