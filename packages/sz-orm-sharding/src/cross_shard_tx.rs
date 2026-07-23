//! # 跨分片事务协调
//!
//! 提供 [`ShardTransactionCoordinator`] 用于协调跨多个 shard 的事务：
//!
//! - [`ShardTransactionCoordinator::execute_2pc`]：两阶段提交（2PC），强一致
//! - [`ShardTransactionCoordinator::execute_best_effort`]：最大努力通知，最终一致
//!
//! ## 2PC 流程
//!
//! 1. **Prepare 阶段**：依次调用每个 participant 的 `prepare`，全部成功才进入提交
//! 2. 若任一 `prepare` 失败：回滚所有已 prepare 的 participant，返回 `PrepareFailed`
//! 3. **Commit 阶段**：依次调用每个 participant 的 `commit`
//! 4. 若任一 `commit` 失败：返回 `CommitFailed`（已提交的不回滚，需人工介入）
//!
//! ## Best Effort 流程
//!
//! 依次对每个 participant 调用 `commit`（可重试 `max_retries` 次），
//! 失败记录但不中断后续 participant。

use std::collections::HashSet;

/// 跨分片事务错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShardTxError {
    /// Prepare 阶段失败：`shard_id` 的 prepare 返回错误
    PrepareFailed {
        /// 失败的 shard 标识
        shard_id: String,
        /// 失败原因
        reason: String,
    },
    /// Commit 阶段失败：`shard_id` 的 commit 返回错误
    CommitFailed {
        /// 失败的 shard 标识
        shard_id: String,
        /// 失败原因
        reason: String,
    },
    /// Rollback 失败：`shard_id` 的 rollback 返回错误
    RollbackFailed {
        /// 失败的 shard 标识
        shard_id: String,
        /// 失败原因
        reason: String,
    },
}

impl std::fmt::Display for ShardTxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShardTxError::PrepareFailed { shard_id, reason } => {
                write!(f, "prepare failed on shard {}: {}", shard_id, reason)
            }
            ShardTxError::CommitFailed { shard_id, reason } => {
                write!(f, "commit failed on shard {}: {}", shard_id, reason)
            }
            ShardTxError::RollbackFailed { shard_id, reason } => {
                write!(f, "rollback failed on shard {}: {}", shard_id, reason)
            }
        }
    }
}

impl std::error::Error for ShardTxError {}

/// 单个 participant 的执行结果（Best Effort 模式）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardTxResult {
    /// shard 标识
    pub shard_id: String,
    /// 是否最终成功
    pub success: bool,
    /// 失败原因（成功时为 None）
    pub error: Option<String>,
}

/// 事务参与者：封装某个 shard 的 prepare/commit/rollback 闭包
pub struct ShardParticipant {
    /// shard 标识
    pub shard_id: String,
    /// Prepare 闭包：返回 `Ok(())` 表示可提交
    pub prepare: Box<dyn Fn() -> Result<(), String> + Send + Sync>,
    /// Commit 闭包：返回 `Ok(())` 表示已提交
    pub commit: Box<dyn Fn() -> Result<(), String> + Send + Sync>,
    /// Rollback 闭包：返回 `Ok(())` 表示已回滚
    pub rollback: Box<dyn Fn() -> Result<(), String> + Send + Sync>,
}

impl ShardParticipant {
    /// 创建 participant
    ///
    /// # 参数
    ///
    /// - `shard_id`: shard 标识
    /// - `prepare`/`commit`/`rollback`: 三个阶段闭包，返回 `Ok(())` 或 `Err(reason)`
    pub fn new<P, C, R>(shard_id: &str, prepare: P, commit: C, rollback: R) -> Self
    where
        P: Fn() -> Result<(), String> + Send + Sync + 'static,
        C: Fn() -> Result<(), String> + Send + Sync + 'static,
        R: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        Self {
            shard_id: shard_id.to_string(),
            prepare: Box::new(prepare),
            commit: Box::new(commit),
            rollback: Box::new(rollback),
        }
    }
}

impl std::fmt::Debug for ShardParticipant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShardParticipant")
            .field("shard_id", &self.shard_id)
            .field("prepare", &"<closure>")
            .field("commit", &"<closure>")
            .field("rollback", &"<closure>")
            .finish()
    }
}

/// 跨分片事务协调器
pub struct ShardTransactionCoordinator {
    /// 参与者列表
    participants: Vec<ShardParticipant>,
    /// Best Effort 模式下每个 participant 的最大重试次数
    max_retries: u32,
}

impl ShardTransactionCoordinator {
    /// 创建空协调器（max_retries = 0，即不重试）
    pub fn new() -> Self {
        Self {
            participants: Vec::new(),
            max_retries: 0,
        }
    }

    /// 设置 Best Effort 模式的最大重试次数
    pub fn with_max_retries(mut self, n: u32) -> Self {
        self.max_retries = n;
        self
    }

    /// 添加 participant
    pub fn add_participant(&mut self, p: ShardParticipant) {
        self.participants.push(p);
    }

    /// 返回 participant 数量
    pub fn participant_count(&self) -> usize {
        self.participants.len()
    }

    /// 两阶段提交（2PC）：prepare 所有 → 全部成功则 commit，否则 rollback 已 prepare 的
    ///
    /// # Errors
    ///
    /// - `PrepareFailed`：某 participant 的 prepare 失败（已 prepare 的会被 rollback）
    /// - `CommitFailed`：某 participant 的 commit 失败（已提交的不回滚）
    pub fn execute_2pc(&mut self) -> Result<(), ShardTxError> {
        // 记录已 prepare 的 participant 索引
        let mut prepared: HashSet<usize> = HashSet::new();

        // Phase 1: Prepare
        for (i, p) in self.participants.iter().enumerate() {
            match (p.prepare)() {
                Ok(()) => {
                    prepared.insert(i);
                }
                Err(reason) => {
                    // 回滚所有已 prepare 的 participant
                    for &idx in &prepared {
                        // rollback 失败不阻塞流程，仅记录（返回第一个 prepare 错误）
                        let _ = (self.participants[idx].rollback)();
                    }
                    return Err(ShardTxError::PrepareFailed {
                        shard_id: p.shard_id.clone(),
                        reason,
                    });
                }
            }
        }

        // Phase 2: Commit
        for p in &self.participants {
            if let Err(reason) = (p.commit)() {
                // 已提交的无法回滚，返回 commit 失败错误（需人工介入）
                return Err(ShardTxError::CommitFailed {
                    shard_id: p.shard_id.clone(),
                    reason,
                });
            }
        }
        Ok(())
    }

    /// 最大努力通知（Best Effort）：依次 commit，失败记录但不回滚
    ///
    /// 对每个 participant 调用 `commit`，最多重试 `max_retries` 次。
    /// 失败的 participant 不会中断后续 participant 的执行。
    ///
    /// # 返回
    ///
    /// 返回每个 participant 的执行结果 `Vec<ShardTxResult>`。
    pub fn execute_best_effort(&mut self) -> Vec<ShardTxResult> {
        let mut results = Vec::with_capacity(self.participants.len());
        for p in &self.participants {
            let mut success = false;
            let mut last_err: Option<String> = None;
            // attempt 0..=max_retries，共 max_retries+1 次尝试
            for attempt in 0..=self.max_retries {
                match (p.commit)() {
                    Ok(()) => {
                        success = true;
                        last_err = None;
                        break;
                    }
                    Err(reason) => {
                        last_err = Some(reason);
                        // 未达最大次数则继续重试
                        if attempt < self.max_retries {
                            continue;
                        }
                    }
                }
            }
            results.push(ShardTxResult {
                shard_id: p.shard_id.clone(),
                success,
                error: last_err,
            });
        }
        results
    }
}

impl Default for ShardTransactionCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ShardTransactionCoordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShardTransactionCoordinator")
            .field("participant_count", &self.participants.len())
            .field("max_retries", &self.max_retries)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // --- 2PC 成功路径 ---

    #[test]
    fn test_2pc_all_succeed() {
        let mut coord = ShardTransactionCoordinator::new();
        coord.add_participant(ShardParticipant::new("s0", || Ok(()), || Ok(()), || Ok(())));
        coord.add_participant(ShardParticipant::new("s1", || Ok(()), || Ok(()), || Ok(())));
        let result = coord.execute_2pc();
        assert!(result.is_ok());
    }

    #[test]
    fn test_2pc_empty_participants() {
        let mut coord = ShardTransactionCoordinator::new();
        // 空 participant 列表：prepare 和 commit 都无操作，应直接成功
        let result = coord.execute_2pc();
        assert!(result.is_ok());
    }

    #[test]
    fn test_2pc_single_participant_success() {
        let mut coord = ShardTransactionCoordinator::new();
        coord.add_participant(ShardParticipant::new("only", || Ok(()), || Ok(()), || Ok(())));
        assert!(coord.execute_2pc().is_ok());
    }

    // --- 2PC Prepare 失败 ---

    #[test]
    fn test_2pc_prepare_fails_rolls_back_prepared() {
        let rolled_back = Arc::new(Mutex::new(false));
        let rb = rolled_back.clone();
        let mut coord = ShardTransactionCoordinator::new();
        // s0 prepare ok，s1 prepare 失败
        coord.add_participant(ShardParticipant::new(
            "s0",
            || Ok(()),
            || Ok(()),
            move || {
                *rb.lock().unwrap() = true;
                Ok(())
            },
        ));
        coord.add_participant(ShardParticipant::new(
            "s1",
            || Err("prepare failed".to_string()),
            || Ok(()),
            || Ok(()),
        ));
        let result = coord.execute_2pc();
        assert!(matches!(result, Err(ShardTxError::PrepareFailed { .. })));
        assert!(
            *rolled_back.lock().unwrap(),
            "s0 should have been rolled back"
        );
        if let Err(ShardTxError::PrepareFailed { shard_id, reason }) = result {
            assert_eq!(shard_id, "s1");
            assert_eq!(reason, "prepare failed");
        }
    }

    #[test]
    fn test_2pc_single_participant_prepare_fails() {
        let mut coord = ShardTransactionCoordinator::new();
        coord.add_participant(ShardParticipant::new(
            "only",
            || Err("nope".to_string()),
            || Ok(()),
            || Ok(()),
        ));
        let result = coord.execute_2pc();
        assert!(matches!(result, Err(ShardTxError::PrepareFailed { .. })));
    }

    #[test]
    fn test_2pc_prepare_fails_at_third_rolls_back_first_two() {
        let rollback_count = Arc::new(Mutex::new(0u32));
        let mut coord = ShardTransactionCoordinator::new();
        for i in 0..2 {
            let rc = rollback_count.clone();
            coord.add_participant(ShardParticipant::new(
                &format!("s{}", i),
                || Ok(()),
                || Ok(()),
                move || {
                    *rc.lock().unwrap() += 1;
                    Ok(())
                },
            ));
        }
        coord.add_participant(ShardParticipant::new(
            "s2",
            || Err("fail".to_string()),
            || Ok(()),
            || Ok(()),
        ));
        let result = coord.execute_2pc();
        assert!(matches!(result, Err(ShardTxError::PrepareFailed { .. })));
        assert_eq!(*rollback_count.lock().unwrap(), 2);
    }

    // --- 2PC Commit 失败 ---

    #[test]
    fn test_2pc_commit_fails() {
        let mut coord = ShardTransactionCoordinator::new();
        coord.add_participant(ShardParticipant::new("s0", || Ok(()), || Ok(()), || Ok(())));
        coord.add_participant(ShardParticipant::new(
            "s1",
            || Ok(()),
            || Err("commit failed".to_string()),
            || Ok(()),
        ));
        let result = coord.execute_2pc();
        match result {
            Err(ShardTxError::CommitFailed { shard_id, reason }) => {
                assert_eq!(shard_id, "s1");
                assert_eq!(reason, "commit failed");
            }
            other => panic!("expected CommitFailed, got {:?}", other),
        }
    }

    #[test]
    fn test_2pc_commit_fails_at_first() {
        let mut coord = ShardTransactionCoordinator::new();
        coord.add_participant(ShardParticipant::new(
            "s0",
            || Ok(()),
            || Err("first commit fails".to_string()),
            || Ok(()),
        ));
        coord.add_participant(ShardParticipant::new("s1", || Ok(()), || Ok(()), || Ok(())));
        let result = coord.execute_2pc();
        assert!(matches!(result, Err(ShardTxError::CommitFailed { .. })));
    }

    // --- Best Effort ---

    #[test]
    fn test_best_effort_all_succeed() {
        let mut coord = ShardTransactionCoordinator::new();
        coord.add_participant(ShardParticipant::new("s0", || Ok(()), || Ok(()), || Ok(())));
        coord.add_participant(ShardParticipant::new("s1", || Ok(()), || Ok(()), || Ok(())));
        let results = coord.execute_best_effort();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.success));
        assert!(results.iter().all(|r| r.error.is_none()));
    }

    #[test]
    fn test_best_effort_partial_failure_continues() {
        let committed = Arc::new(Mutex::new(Vec::new()));
        let c0 = committed.clone();
        let c1 = committed.clone();
        let mut coord = ShardTransactionCoordinator::new();
        coord.add_participant(ShardParticipant::new(
            "s0",
            || Ok(()),
            move || {
                c0.lock().unwrap().push("s0");
                Ok(())
            },
            || Ok(()),
        ));
        coord.add_participant(ShardParticipant::new(
            "s1",
            || Ok(()),
            || Err("commit failed".to_string()),
            || Ok(()),
        ));
        coord.add_participant(ShardParticipant::new(
            "s2",
            || Ok(()),
            move || {
                c1.lock().unwrap().push("s2");
                Ok(())
            },
            || Ok(()),
        ));
        let results = coord.execute_best_effort();
        assert_eq!(results.len(), 3);
        assert!(results[0].success, "s0 should succeed");
        assert!(!results[1].success, "s1 should fail");
        assert!(results[2].success, "s2 should succeed even after s1 failed");
        let committed = committed.lock().unwrap();
        assert!(committed.contains(&"s0"));
        assert!(committed.contains(&"s2"));
    }

    #[test]
    fn test_best_effort_all_fail() {
        let mut coord = ShardTransactionCoordinator::new();
        coord.add_participant(ShardParticipant::new(
            "s0",
            || Ok(()),
            || Err("e0".to_string()),
            || Ok(()),
        ));
        coord.add_participant(ShardParticipant::new(
            "s1",
            || Ok(()),
            || Err("e1".to_string()),
            || Ok(()),
        ));
        let results = coord.execute_best_effort();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| !r.success));
        assert_eq!(results[0].error.as_deref(), Some("e0"));
        assert_eq!(results[1].error.as_deref(), Some("e1"));
    }

    #[test]
    fn test_best_effort_empty() {
        let mut coord = ShardTransactionCoordinator::new();
        let results = coord.execute_best_effort();
        assert!(results.is_empty());
    }

    #[test]
    fn test_best_effort_single() {
        let mut coord = ShardTransactionCoordinator::new();
        coord.add_participant(ShardParticipant::new("only", || Ok(()), || Ok(()), || Ok(())));
        let results = coord.execute_best_effort();
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
    }

    #[test]
    fn test_best_effort_with_retries() {
        let attempts = Arc::new(Mutex::new(0u32));
        let a = attempts.clone();
        let mut coord = ShardTransactionCoordinator::new().with_max_retries(2);
        coord.add_participant(ShardParticipant::new(
            "s0",
            || Ok(()),
            move || {
                let mut g = a.lock().unwrap();
                *g += 1;
                if *g < 3 {
                    Err("retry".to_string())
                } else {
                    Ok(())
                }
            },
            || Ok(()),
        ));
        let results = coord.execute_best_effort();
        assert_eq!(results.len(), 1);
        assert!(results[0].success, "should succeed after 2 retries (3rd attempt)");
        assert_eq!(*attempts.lock().unwrap(), 3);
    }

    #[test]
    fn test_best_effort_retry_exhausted() {
        let mut coord = ShardTransactionCoordinator::new().with_max_retries(2);
        coord.add_participant(ShardParticipant::new(
            "s0",
            || Ok(()),
            || Err("always fails".to_string()),
            || Ok(()),
        ));
        let results = coord.execute_best_effort();
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert_eq!(results[0].error.as_deref(), Some("always fails"));
    }

    // --- Debug / Display ---

    #[test]
    fn test_shard_tx_error_display() {
        let e1 = ShardTxError::PrepareFailed {
            shard_id: "s0".to_string(),
            reason: "boom".to_string(),
        };
        assert!(format!("{}", e1).contains("prepare failed on shard s0: boom"));
        let e2 = ShardTxError::CommitFailed {
            shard_id: "s1".to_string(),
            reason: "kaboom".to_string(),
        };
        assert!(format!("{}", e2).contains("commit failed on shard s1: kaboom"));
        let e3 = ShardTxError::RollbackFailed {
            shard_id: "s2".to_string(),
            reason: "oops".to_string(),
        };
        assert!(format!("{}", e3).contains("rollback failed on shard s2: oops"));
    }

    #[test]
    fn test_coordinator_debug() {
        let mut coord = ShardTransactionCoordinator::new().with_max_retries(3);
        coord.add_participant(ShardParticipant::new("s0", || Ok(()), || Ok(()), || Ok(())));
        let s = format!("{:?}", coord);
        assert!(s.contains("ShardTransactionCoordinator"));
        assert!(s.contains("participant_count"));
        assert!(s.contains("max_retries"));
    }

    #[test]
    fn test_participant_count() {
        let mut coord = ShardTransactionCoordinator::new();
        assert_eq!(coord.participant_count(), 0);
        coord.add_participant(ShardParticipant::new("a", || Ok(()), || Ok(()), || Ok(())));
        coord.add_participant(ShardParticipant::new("b", || Ok(()), || Ok(()), || Ok(())));
        assert_eq!(coord.participant_count(), 2);
    }
}
