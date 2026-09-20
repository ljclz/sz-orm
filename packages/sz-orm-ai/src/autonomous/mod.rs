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

pub use action_executor::AutonomousActionExecutor;
pub use boundary_validator::BoundaryValidator;
pub use circuit_breaker::AutonomousCircuitBreaker;
pub use idempotency::IdempotencyDeduplicator;
pub use llm_advisor::LlmAdvisor;
pub use policy_engine::AutonomousPolicyEngine;
pub use policy_matcher::PolicyMatcher;
pub use types::*;
pub use verification_loop::AutonomousVerificationLoop;
