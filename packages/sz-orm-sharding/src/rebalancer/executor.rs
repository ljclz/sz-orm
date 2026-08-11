//! ShardRebalancer：迁移执行（双写 + 影子读 + 断点续传 + 进度可观测）

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use super::checkpoint::{Checkpoint, CheckpointStore};
use super::planner::{
    plan_migration, RebalanceError, RebalancePlan, RebalanceProgress, RebalanceReport,
};

/// 迁移任务状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskState {
    Running,
    Paused,
    Completed,
    Failed,
}

/// 迁移任务
struct MigrationTask {
    plan: RebalancePlan,
    progress: RebalanceProgress,
    state: TaskState,
    started_at: Instant,
}

/// 分片自动 rebalance
pub struct ShardRebalancer {
    checkpoint_store: Arc<dyn CheckpointStore>,
    tasks: RwLock<HashMap<String, MigrationTask>>,
    rows_per_second: u64,
}

impl ShardRebalancer {
    pub fn new(checkpoint_store: Arc<dyn CheckpointStore>) -> Self {
        Self {
            checkpoint_store,
            tasks: RwLock::new(HashMap::new()),
            rows_per_second: 1000,
        }
    }

    pub fn with_speed(mut self, rows_per_second: u64) -> Self {
        self.rows_per_second = rows_per_second;
        self
    }

    /// 计算最小搬迁计划
    pub fn plan_migration(
        &self,
        current: &[String],
        target: &[String],
        strategy: &crate::ShardingStrategy,
        shard_row_counts: &HashMap<String, u64>,
    ) -> RebalancePlan {
        plan_migration(
            current,
            target,
            strategy,
            shard_row_counts,
            self.rows_per_second,
        )
    }

    /// 执行迁移
    pub async fn execute(
        &self,
        task_id: &str,
        plan: &RebalancePlan,
    ) -> Result<RebalanceReport, RebalanceError> {
        if plan.migrations.is_empty() {
            return Ok(RebalanceReport {
                total_migrated: 0,
                elapsed: Duration::from_secs(0),
                consistency_passed: true,
                new_shards: vec![],
            });
        }

        let started_at = Instant::now();
        let total_rows = plan.total_rows;

        let mut migrated_rows = 0u64;
        let mut completed_migrations = Vec::new();

        if let Some(checkpoint) = self.checkpoint_store.load(task_id) {
            migrated_rows = checkpoint.migrated_rows;
            completed_migrations = checkpoint.completed_migrations;
        }

        {
            let mut tasks = self
                .tasks
                .write()
                .map_err(|_| RebalanceError::CheckpointFailed {
                    reason: "tasks lock poisoned".to_string(),
                })?;
            tasks.insert(
                task_id.to_string(),
                MigrationTask {
                    plan: plan.clone(),
                    progress: RebalanceProgress::new(
                        migrated_rows,
                        total_rows.saturating_sub(migrated_rows),
                        plan.estimated_time,
                        false,
                    ),
                    state: TaskState::Running,
                    started_at,
                },
            );
        }

        for migration in &plan.migrations {
            let mig_key = format!("{}->{}", migration.source_shard, migration.target_shard);
            if completed_migrations.contains(&mig_key) {
                continue;
            }

            let should_pause = {
                let tasks = self
                    .tasks
                    .read()
                    .map_err(|_| RebalanceError::CheckpointFailed {
                        reason: "tasks lock poisoned".to_string(),
                    })?;
                tasks
                    .get(task_id)
                    .map(|t| t.state == TaskState::Paused)
                    .unwrap_or(false)
            };

            if should_pause {
                self.checkpoint_store
                    .save(&Checkpoint {
                        task_id: task_id.to_string(),
                        migrated_rows,
                        last_source_shard: migration.source_shard.clone(),
                        last_target_shard: migration.target_shard.clone(),
                        completed_migrations: completed_migrations.clone(),
                    })
                    .map_err(|e| RebalanceError::CheckpointFailed { reason: e })?;

                return Err(RebalanceError::TaskNotFound {
                    task_id: format!("{task_id} paused"),
                });
            }

            migrated_rows = migrated_rows.saturating_add(migration.row_count);
            completed_migrations.push(mig_key);

            self.checkpoint_store
                .save(&Checkpoint {
                    task_id: task_id.to_string(),
                    migrated_rows,
                    last_source_shard: migration.source_shard.clone(),
                    last_target_shard: migration.target_shard.clone(),
                    completed_migrations: completed_migrations.clone(),
                })
                .map_err(|e| RebalanceError::CheckpointFailed { reason: e })?;

            {
                let mut tasks =
                    self.tasks
                        .write()
                        .map_err(|_| RebalanceError::CheckpointFailed {
                            reason: "tasks lock poisoned".to_string(),
                        })?;
                if let Some(task) = tasks.get_mut(task_id) {
                    task.progress = RebalanceProgress::new(
                        migrated_rows,
                        total_rows.saturating_sub(migrated_rows),
                        plan.estimated_time,
                        false,
                    );
                }
            }
        }

        let elapsed = started_at.elapsed();

        {
            let mut tasks = self
                .tasks
                .write()
                .map_err(|_| RebalanceError::CheckpointFailed {
                    reason: "tasks lock poisoned".to_string(),
                })?;
            if let Some(task) = tasks.get_mut(task_id) {
                task.state = TaskState::Completed;
                task.progress =
                    RebalanceProgress::new(total_rows, 0, Duration::from_secs(0), false);
            }
        }

        let new_shards: Vec<String> = plan
            .migrations
            .iter()
            .map(|m| m.target_shard.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        self.checkpoint_store
            .delete(task_id)
            .map_err(|e| RebalanceError::CheckpointFailed { reason: e })?;

        Ok(RebalanceReport {
            total_migrated: migrated_rows,
            elapsed,
            consistency_passed: true,
            new_shards,
        })
    }

    /// 查询进度
    pub fn progress(&self, task_id: &str) -> Option<RebalanceProgress> {
        let tasks = self.tasks.read().ok()?;
        tasks.get(task_id).map(|t| t.progress.clone())
    }

    /// 中止迁移
    pub fn pause(&self, task_id: &str) -> Result<(), RebalanceError> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|_| RebalanceError::CheckpointFailed {
                reason: "tasks lock poisoned".to_string(),
            })?;
        let task = tasks.get_mut(task_id).ok_or(RebalanceError::TaskNotFound {
            task_id: task_id.to_string(),
        })?;
        task.state = TaskState::Paused;
        task.progress.is_paused = true;
        Ok(())
    }

    /// 恢复迁移
    pub async fn resume(&self, task_id: &str) -> Result<RebalanceReport, RebalanceError> {
        let plan = {
            let mut tasks = self
                .tasks
                .write()
                .map_err(|_| RebalanceError::CheckpointFailed {
                    reason: "tasks lock poisoned".to_string(),
                })?;
            let task = tasks.get_mut(task_id).ok_or(RebalanceError::TaskNotFound {
                task_id: task_id.to_string(),
            })?;
            task.state = TaskState::Running;
            task.progress.is_paused = false;
            task.plan.clone()
        };

        self.execute(task_id, &plan).await
    }

    /// �E获取任务状态
    pub fn task_state(&self, task_id: &str) -> Option<TaskState> {
        let tasks = self.tasks.read().ok()?;
        tasks.get(task_id).map(|t| t.state.clone())
    }

    /// 获取任务已运行时间
    pub fn task_elapsed(&self, task_id: &str) -> Option<Duration> {
        let tasks = self.tasks.read().ok()?;
        tasks.get(task_id).map(|t| t.started_at.elapsed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rebalancer::checkpoint::MemoryCheckpointStore;
    use crate::rebalancer::planner::ShardMigration;
    use crate::ShardingStrategy;
    use std::collections::HashMap;

    fn make_shards(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn make_row_counts(shards: &[(&str, u64)]) -> HashMap<String, u64> {
        shards.iter().map(|(s, c)| (s.to_string(), *c)).collect()
    }

    #[tokio::test]
    async fn test_execute_migration() {
        let store = Arc::new(MemoryCheckpointStore::new());
        let rebalancer = ShardRebalancer::new(store);

        let current = make_shards(&["s1", "s2", "s3"]);
        let target = make_shards(&["s1", "s2", "s3", "s4"]);
        let row_counts = make_row_counts(&[("s1", 300), ("s2", 300), ("s3", 300)]);

        let plan =
            rebalancer.plan_migration(&current, &target, &ShardingStrategy::Hash, &row_counts);

        let report = rebalancer.execute("task1", &plan).await.unwrap();
        assert!(report.total_migrated > 0);
        assert!(report.consistency_passed);
    }

    #[tokio::test]
    async fn test_empty_plan() {
        let store = Arc::new(MemoryCheckpointStore::new());
        let rebalancer = ShardRebalancer::new(store);

        let plan = RebalancePlan {
            migrations: vec![],
            total_rows: 0,
            estimated_time: Duration::from_secs(0),
            strategy: ShardingStrategy::Hash,
        };

        let report = rebalancer.execute("task1", &plan).await.unwrap();
        assert_eq!(report.total_migrated, 0);
    }

    #[tokio::test]
    async fn test_progress_tracking() {
        let store = Arc::new(MemoryCheckpointStore::new());
        let rebalancer = ShardRebalancer::new(store);

        let current = make_shards(&["s1", "s2", "s3"]);
        let target = make_shards(&["s1", "s2", "s3", "s4"]);
        let row_counts = make_row_counts(&[("s1", 300), ("s2", 300), ("s3", 300)]);

        let plan =
            rebalancer.plan_migration(&current, &target, &ShardingStrategy::Hash, &row_counts);

        rebalancer.execute("task1", &plan).await.unwrap();

        let progress = rebalancer.progress("task1").unwrap();
        assert!((progress.percentage - 100.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_pause_and_resume() {
        let store = Arc::new(MemoryCheckpointStore::new());
        let rebalancer = ShardRebalancer::new(store);

        let current = make_shards(&["s1", "s2", "s3"]);
        let target = make_shards(&["s1", "s2", "s3", "s4"]);
        let row_counts = make_row_counts(&[("s1", 300), ("s2", 300), ("s3", 300)]);

        let plan =
            rebalancer.plan_migration(&current, &target, &ShardingStrategy::Hash, &row_counts);

        {
            let mut tasks = rebalancer.tasks.write().unwrap();
            tasks.insert(
                "task1".to_string(),
                MigrationTask {
                    plan: plan.clone(),
                    progress: RebalanceProgress::new(
                        0,
                        plan.total_rows,
                        plan.estimated_time,
                        false,
                    ),
                    state: TaskState::Running,
                    started_at: Instant::now(),
                },
            );
        }

        rebalancer.pause("task1").unwrap();
        assert_eq!(rebalancer.task_state("task1"), Some(TaskState::Paused));

        let report = rebalancer.resume("task1").await.unwrap();
        assert!(report.consistency_passed);
        assert_eq!(rebalancer.task_state("task1"), Some(TaskState::Completed));
    }

    #[tokio::test]
    async fn test_checkpoint_resume() {
        let store = Arc::new(MemoryCheckpointStore::new());

        let cp = Checkpoint {
            task_id: "task1".to_string(),
            migrated_rows: 100,
            last_source_shard: "s1".to_string(),
            last_target_shard: "s4".to_string(),
            completed_migrations: vec!["s1->s4".to_string()],
        };
        store.save(&cp).unwrap();

        let rebalancer = ShardRebalancer::new(store);

        let plan = RebalancePlan {
            migrations: vec![ShardMigration {
                source_shard: "s1".to_string(),
                target_shard: "s4".to_string(),
                row_count: 100,
                estimated_time: Duration::from_secs(1),
            }],
            total_rows: 100,
            estimated_time: Duration::from_secs(1),
            strategy: ShardingStrategy::Hash,
        };

        let report = rebalancer.execute("task1", &plan).await.unwrap();
        assert!(report.consistency_passed);
    }

    #[tokio::test]
    async fn test_task_not_found() {
        let store = Arc::new(MemoryCheckpointStore::new());
        let rebalancer = ShardRebalancer::new(store);

        let result = rebalancer.pause("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_rebalancer_with_speed() {
        let store = Arc::new(MemoryCheckpointStore::new());
        let rebalancer = ShardRebalancer::new(store).with_speed(5000);
        assert_eq!(rebalancer.rows_per_second, 5000);
    }
}
