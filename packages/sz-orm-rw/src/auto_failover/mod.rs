//! # 数据库 failover 自动化
//!
//! 提供 `AutoFailoverManager`，持续监控主库健康，故障时自动选择最佳 slave 提升为新主库，
//! 更新路由，通知上层，记录审计，含数据丢失风险评估与脑裂检测。

pub mod manager;
pub mod split_brain;

pub use manager::{
    AutoFailoverManager, DataLossRisk, FailoverConfig, FailoverError, FailoverEvent,
    FailoverOperator, FailoverResult, SplitBrainStatus,
};
pub use split_brain::{DemotionStrategy, SplitBrainAlert, SplitBrainDetector};
