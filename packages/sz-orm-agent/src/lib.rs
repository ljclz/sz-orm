//! AI Agent for autonomous database operations.
//!
//! 提供 perceive-decide-act 循环驱动器，集成诊断、异常检测、优化建议等能力，
//! 支持工具调用协议、危险操作审批、权限边界拦截、状态持久化与检查点恢复。

#![cfg_attr(not(feature = "agent"), allow(unused_imports))]

#[cfg(feature = "agent")]
pub mod agent;
#[cfg(feature = "agent")]
pub mod approval;
#[cfg(feature = "agent")]
pub mod checkpoint;
#[cfg(feature = "agent")]
pub mod perception;
#[cfg(feature = "agent")]
pub mod permission;
#[cfg(feature = "agent")]
pub mod tool;
#[cfg(feature = "agent")]
pub mod types;

#[cfg(feature = "agent")]
pub use agent::{AgentDriver, DatabaseAgent};
#[cfg(feature = "agent")]
pub use approval::{ApprovalDecision, ApprovalGate, ApprovalRequest};
#[cfg(feature = "agent")]
pub use checkpoint::{Checkpoint, CheckpointManager, CheckpointStore};
#[cfg(feature = "agent")]
pub use perception::PerceptionCollector;
#[cfg(feature = "agent")]
pub use permission::{PermissionBoundary, ToolPermissionGuard};
#[cfg(feature = "agent")]
pub use tool::{AgentTool, AuditLog, RiskLevel, ToolRegistry};
#[cfg(feature = "agent")]
pub use types::*;
