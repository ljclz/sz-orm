//! v8.0.0 ABAC 生产化引擎
//!
//! 属性评估 → 策略匹配 → 默认拒绝 → 最小权限 → 策略变更审计 → 兼容 RBAC。

pub mod abac_production_engine;

pub use abac_production_engine::{
    AbacAccessRequest, AbacProductionConfig, AbacProductionEngine, AuthzDecision,
};
