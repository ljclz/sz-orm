//! ABAC 模块（v7.1.0）
//!
//! 基于属性的访问控制（Attribute-Based Access Control）。

pub mod audit_logger;
pub mod composite_authorizer;
pub mod decision_cache;
pub mod expr_sandbox;
pub mod hot_reloader;
pub mod policy_engine;

pub use audit_logger::PolicyAuditLogger;
pub use composite_authorizer::{CombineMode, CompositeAuthorizer};
pub use decision_cache::DecisionCache;
pub use expr_sandbox::ExprSandbox;
pub use hot_reloader::PolicyHotReloader;
pub use policy_engine::{
    AbacPolicy, AbacPolicyEngine, AccessRequest, AttributeScope, AttributeValue, Condition, Effect,
};
