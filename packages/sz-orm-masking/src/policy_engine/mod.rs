//! v8.0.0 脱敏策略引擎
//!
//! 按角色/字段/场景细粒度脱敏，策略匹配 → 按优先级仲裁 → 脱敏 → 热更新 → 审计。

pub mod masking_policy_engine;

pub use masking_policy_engine::{
    MaskingPolicy, MaskingPolicyConfig, MaskingPolicyEngine, MaskingScene, MaskingStrategyKind,
};
