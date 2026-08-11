//! RebalancePlan 数据结构 + 最小搬迁计划计算

use std::fmt;
use std::time::Duration;

use crate::ShardingStrategy;

/// 单次分片迁移
#[derive(Debug, Clone)]
pub struct ShardMigration {
    pub source_shard: String,
    pub target_shard: String,
    pub row_count: u64,
    pub estimated_time: Duration,
}

/// 迁移计划
#[derive(Debug, Clone)]
pub struct RebalancePlan {
    pub migrations: Vec<ShardMigration>,
    pub total_rows: u64,
    pub estimated_time: Duration,
    pub strategy: ShardingStrategy,
}

/// 迁移进度
#[derive(Debug, Clone)]
pub struct RebalanceProgress {
    pub migrated_rows: u64,
    pub remaining_rows: u64,
    pub percentage: f64,
    pub eta: Duration,
    pub is_paused: bool,
}

impl RebalanceProgress {
    pub fn new(migrated_rows: u64, remaining_rows: u64, eta: Duration, is_paused: bool) -> Self {
        let percentage = super::calculate_percentage(migrated_rows, remaining_rows);
        Self {
            migrated_rows,
            remaining_rows,
            percentage,
            eta,
            is_paused,
        }
    }
}

/// 迁移报告
#[derive(Debug, Clone)]
pub struct RebalanceReport {
    pub total_migrated: u64,
    pub elapsed: Duration,
    pub consistency_passed: bool,
    pub new_shards: Vec<String>,
}

/// rebalance 错误
#[derive(Debug, Clone)]
pub enum RebalanceError {
    NodeFailed { shard: String },
    ConsistencyFailed,
    CheckpointFailed { reason: String },
    InvalidPlan { reason: String },
    TaskNotFound { task_id: String },
}

impl fmt::Display for RebalanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RebalanceError::NodeFailed { shard } => write!(f, "node failed: {shard}"),
            RebalanceError::ConsistencyFailed => write!(f, "consistency check failed"),
            RebalanceError::CheckpointFailed { reason } => write!(f, "checkpoint failed: {reason}"),
            RebalanceError::InvalidPlan { reason } => write!(f, "invalid plan: {reason}"),
            RebalanceError::TaskNotFound { task_id } => write!(f, "task not found: {task_id}"),
        }
    }
}

impl std::error::Error for RebalanceError {}

/// 计算最小搬迁计划
pub fn plan_migration(
    current: &[String],
    target: &[String],
    strategy: &ShardingStrategy,
    shard_row_counts: &std::collections::HashMap<String, u64>,
    rows_per_second: u64,
) -> RebalancePlan {
    let mut migrations = Vec::new();

    let current_set: std::collections::HashSet<&String> = current.iter().collect();
    let target_set: std::collections::HashSet<&String> = target.iter().collect();

    let removed: Vec<&String> = current.iter().filter(|s| !target_set.contains(s)).collect();
    let added: Vec<&String> = target.iter().filter(|s| !current_set.contains(s)).collect();

    if !removed.is_empty() && !added.is_empty() {
        let rows_per_new_shard = if added.is_empty() {
            0
        } else {
            let total_removed_rows: u64 = removed
                .iter()
                .map(|s| shard_row_counts.get(*s).copied().unwrap_or(0))
                .sum();
            total_removed_rows / added.len() as u64
        };

        for src in &removed {
            for tgt in &added {
                let row_count =
                    shard_row_counts.get(*src).copied().unwrap_or(0) / added.len() as u64;
                if row_count > 0 {
                    migrations.push(ShardMigration {
                        source_shard: src.to_string(),
                        target_shard: tgt.to_string(),
                        row_count,
                        estimated_time: super::estimate_time(row_count, rows_per_second),
                    });
                }
            }
        }
        let _ = rows_per_new_shard;
    } else if !removed.is_empty() {
        let remaining_targets: Vec<&String> = target.iter().collect();
        if !remaining_targets.is_empty() {
            for src in &removed {
                let src_rows = shard_row_counts.get(*src).copied().unwrap_or(0);
                let rows_per_target = src_rows / remaining_targets.len() as u64;
                for tgt in &remaining_targets {
                    if rows_per_target > 0 {
                        migrations.push(ShardMigration {
                            source_shard: src.to_string(),
                            target_shard: tgt.to_string(),
                            row_count: rows_per_target,
                            estimated_time: super::estimate_time(rows_per_target, rows_per_second),
                        });
                    }
                }
            }
        }
    } else if !added.is_empty() && !current.is_empty() {
        let total_rows: u64 = current
            .iter()
            .map(|s| shard_row_counts.get(s).copied().unwrap_or(0))
            .sum();
        let rows_to_move = total_rows / target.len() as u64;

        for src in current {
            for tgt in &added {
                let row_count = rows_to_move / added.len() as u64;
                if row_count > 0 {
                    migrations.push(ShardMigration {
                        source_shard: src.clone(),
                        target_shard: tgt.to_string(),
                        row_count,
                        estimated_time: super::estimate_time(row_count, rows_per_second),
                    });
                }
            }
        }
    }

    let total_rows: u64 = migrations.iter().map(|m| m.row_count).sum();
    let estimated_time = super::estimate_time(total_rows, rows_per_second);

    RebalancePlan {
        migrations,
        total_rows,
        estimated_time,
        strategy: strategy.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_shards(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn make_row_counts(shards: &[(&str, u64)]) -> HashMap<String, u64> {
        shards.iter().map(|(s, c)| (s.to_string(), *c)).collect()
    }

    #[test]
    fn test_scale_out_hash_3_to_4() {
        let current = make_shards(&["s1", "s2", "s3"]);
        let target = make_shards(&["s1", "s2", "s3", "s4"]);
        let row_counts = make_row_counts(&[("s1", 300), ("s2", 300), ("s3", 300)]);

        let plan = plan_migration(
            &current,
            &target,
            &ShardingStrategy::Hash,
            &row_counts,
            1000,
        );

        assert!(!plan.migrations.is_empty());
        for m in &plan.migrations {
            assert_eq!(m.target_shard, "s4");
        }
        assert!(plan.total_rows < 900);
    }

    #[test]
    fn test_scale_in_hash_4_to_3() {
        let current = make_shards(&["s1", "s2", "s3", "s4"]);
        let target = make_shards(&["s1", "s2", "s3"]);
        let row_counts = make_row_counts(&[("s1", 250), ("s2", 250), ("s3", 250), ("s4", 250)]);

        let plan = plan_migration(
            &current,
            &target,
            &ShardingStrategy::Hash,
            &row_counts,
            1000,
        );

        assert!(!plan.migrations.is_empty());
        for m in &plan.migrations {
            assert_eq!(m.source_shard, "s4");
        }
    }

    #[test]
    fn test_no_change_no_migration() {
        let current = make_shards(&["s1", "s2", "s3"]);
        let target = make_shards(&["s1", "s2", "s3"]);
        let row_counts = make_row_counts(&[("s1", 100), ("s2", 100), ("s3", 100)]);

        let plan = plan_migration(
            &current,
            &target,
            &ShardingStrategy::Hash,
            &row_counts,
            1000,
        );

        assert!(plan.migrations.is_empty());
        assert_eq!(plan.total_rows, 0);
    }

    #[test]
    fn test_rebalance_progress_percentage() {
        let progress = RebalanceProgress::new(75, 25, Duration::from_secs(10), false);
        assert!((progress.percentage - 75.0).abs() < 0.01);
    }

    #[test]
    fn test_rebalance_progress_zero_total() {
        let progress = RebalanceProgress::new(0, 0, Duration::from_secs(0), false);
        assert!((progress.percentage - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_rebalance_progress_paused() {
        let progress = RebalanceProgress::new(50, 50, Duration::from_secs(5), true);
        assert!(progress.is_paused);
        assert!((progress.percentage - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_rebalance_error_display() {
        let err = RebalanceError::NodeFailed {
            shard: "s1".to_string(),
        };
        assert!(err.to_string().contains("s1"));

        let err = RebalanceError::ConsistencyFailed;
        assert!(err.to_string().contains("consistency"));
    }

    #[test]
    fn test_plan_with_range_strategy() {
        let current = make_shards(&["r1", "r2", "r3"]);
        let target = make_shards(&["r1", "r2", "r3", "r4"]);
        let row_counts = make_row_counts(&[("r1", 200), ("r2", 200), ("r3", 200)]);

        let plan = plan_migration(
            &current,
            &target,
            &ShardingStrategy::Range,
            &row_counts,
            500,
        );

        assert!(!plan.migrations.is_empty());
        assert_eq!(plan.strategy, ShardingStrategy::Range);
    }

    #[test]
    fn test_estimate_time() {
        let time = super::super::estimate_time(1000, 100);
        assert_eq!(time, Duration::from_secs(10));

        let time = super::super::estimate_time(1000, 0);
        assert_eq!(time, Duration::from_secs(0));
    }

    #[test]
    fn test_scale_out_double() {
        let current = make_shards(&["s1", "s2", "s3"]);
        let target = make_shards(&["s1", "s2", "s3", "s4", "s5"]);
        let row_counts = make_row_counts(&[("s1", 200), ("s2", 200), ("s3", 200)]);

        let plan = plan_migration(
            &current,
            &target,
            &ShardingStrategy::Hash,
            &row_counts,
            1000,
        );

        assert!(!plan.migrations.is_empty());
        let targets: std::collections::HashSet<&str> = plan
            .migrations
            .iter()
            .map(|m| m.target_shard.as_str())
            .collect();
        assert!(targets.contains("s4") || targets.contains("s5"));
    }
}
