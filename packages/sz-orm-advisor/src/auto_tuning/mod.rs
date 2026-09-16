//! 查询自调优模块（v7.1.0）
//!
//! 闭环自适应查询优化：分析 → 建议 → 审批 → 执行 → 反馈 → 回滚。

pub mod approval_gate;
pub mod change_auditor;
pub mod cooldown;
pub mod feedback_loop;
pub mod gateway;
pub mod join_reorder;

pub use approval_gate::{ApprovalGate, ApprovalGateConfig, ApprovalState};
pub use change_auditor::{ChangeKind, PlanSnapshot, TuningChangeAuditor, TuningChangeRecord};
pub use cooldown::{CooldownConfig, TuningCooldown};
pub use feedback_loop::{FeedbackLoop, PerformanceSample, PerformanceTrend};
pub use gateway::{
    AutoTuningConfig, AutoTuningGateway, TuningAction, TuningLoopReport, TuningSuggestion,
};
pub use join_reorder::{JoinReorderAdvisor, JoinReorderResult, JoinTable, TableStats};
