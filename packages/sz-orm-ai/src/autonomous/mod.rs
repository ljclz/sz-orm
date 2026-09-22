//! AI 自治闭环模块
//!
//! 自治策略引擎接收 AnomalyPredictor 异常事件，在策略边界内自动完成
//! "检测→决策→执行→验证"全链路，支持自动修复/扩缩容/调参执行，
//! 含熔断和幂等保护。

pub mod action_executor;
pub mod boundary_validator;
pub mod circuit_breaker;
pub mod idempotency;
pub mod llm_advisor;
pub mod policy_engine;
pub mod policy_matcher;
pub mod types;
pub mod verification_loop;

#[cfg(feature = "autonomous-xai")]
pub mod xai;

// v8.1.0 组3：AI 闭环自治生产化
#[cfg(feature = "ai-closed-loop")]
pub mod closed_loop_scheduler;
#[cfg(feature = "ai-closed-loop")]
pub mod loop_break_detector;
#[cfg(feature = "ai-closed-loop")]
pub mod takeover;

pub use action_executor::AutonomousActionExecutor;
pub use boundary_validator::BoundaryValidator;
pub use circuit_breaker::AutonomousCircuitBreaker;
pub use idempotency::IdempotencyDeduplicator;
pub use llm_advisor::LlmAdvisor;
pub use policy_engine::AutonomousPolicyEngine;
pub use policy_matcher::PolicyMatcher;
pub use types::*;
pub use verification_loop::AutonomousVerificationLoop;

// v8.1.0 组3 导出
#[cfg(feature = "ai-closed-loop")]
pub use closed_loop_scheduler::{
    AutonomousTarget, BoundaryConstraint, ClosedLoopError, ClosedLoopRecord, ClosedLoopScheduler,
    CostTarget, LatencyTarget, SloTarget,
};
#[cfg(feature = "ai-closed-loop")]
pub use loop_break_detector::{
    AutonomousLoopBreakDetector, BreakReason, LoopBreakAlert, LoopRecord,
};
#[cfg(feature = "ai-closed-loop")]
pub use takeover::{AutonomousTakeover, TakeoverError, TakeoverRecord};
