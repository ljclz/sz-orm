//! # 分片自动 rebalance — 数据结构与核心实现
//!
//! 提供 `ShardRebalancer`，扩缩容时计算最小数据搬迁计划，分批迁移数据到新分片，
//! 支持断点续传、双写/影子读保证查询不中断、进度可观测。

pub mod checkpoint;
pub mod executor;
pub mod planner;

pub use checkpoint::{CheckpointStore, MemoryCheckpointStore};
pub use executor::ShardRebalancer;
pub use planner::{
    plan_migration, RebalanceError, RebalancePlan, RebalanceProgress, RebalanceReport,
    ShardMigration,
};

use std::time::Duration;

/// 计算进度百分比
pub fn calculate_percentage(migrated: u64, remaining: u64) -> f64 {
    let total = migrated.saturating_add(remaining);
    if total == 0 {
        100.0
    } else {
        (migrated as f64 / total as f64) * 100.0
    }
}

/// 估算迁移时间
pub fn estimate_time(total_rows: u64, rows_per_second: u64) -> Duration {
    Duration::from_secs(total_rows.checked_div(rows_per_second).unwrap_or(0))
}
