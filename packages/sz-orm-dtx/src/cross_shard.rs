//! 跨分片 ACID 协调器
//!
//! 在分片集群中，一笔业务操作往往需要同时写入多个分片（如订单分片 + 库存分片 + 账户分片）。
//! 本模块基于 2PC（[`crate::DistributedTransaction`]）实现跨分片的原子提交：
//!
//! 1. 业务方按分片维度注册多个操作（每个操作包含 prepare/commit/rollback 三个回调）
//! 2. 协调器将同一分片的多个操作合并为一个分支事务（[`crate::TransactionParticipant`]）
//! 3. 调用 [`CrossShardCoordinator::execute`] 走完整 2PC 流程：prepare 全部分片 → 全部成功后 commit；任一失败则回滚已 prepare 的分片
//!
//! # 与 2PC/TCC 的关系
//!
//! - 与 [`crate::DistributedTransaction`] 的关系：跨分片协调器是基于 2PC 的上层封装，自动按 shard_id 分组操作并生成 participant
//! - 与 [`crate::tcc::TccCoordinator`] 的关系：TCC 适合业务级补偿（每个分支需实现 try/confirm/cancel 三套业务逻辑），跨分片协调器适合数据库级 2PC（每个分片只需 prepare/commit/rollback）
//!
//! # 示例
//!
//! ```
//! use sz_orm_dtx::cross_shard::CrossShardCoordinator;
//! use std::sync::{Arc, atomic::{AtomicU32, Ordering}};
//!
//! let prepared = Arc::new(AtomicU32::new(0));
//! let committed = Arc::new(AtomicU32::new(0));
//!
//! let mut coord = CrossShardCoordinator::new("tx-order-001");
//!
//! let p1 = prepared.clone();
//! let c1 = committed.clone();
//! coord.add_operation("shard-orders", move || { p1.fetch_add(1, Ordering::SeqCst); Ok(()) },
//!     move || { c1.fetch_add(1, Ordering::SeqCst); Ok(()) },
//!     || Ok(()));
//!
//! let p2 = prepared.clone();
//! let c2 = committed.clone();
//! coord.add_operation("shard-inventory", move || { p2.fetch_add(1, Ordering::SeqCst); Ok(()) },
//!     move || { c2.fetch_add(1, Ordering::SeqCst); Ok(()) },
//!     || Ok(()));
//!
//! coord.execute().unwrap();
//! assert_eq!(prepared.load(Ordering::SeqCst), 2);
//! assert_eq!(committed.load(Ordering::SeqCst), 2);
//! ```

use crate::{DistributedTransaction, TransactionParticipant, TransactionState};
use std::collections::HashMap;
use std::sync::Arc;

/// 跨分片操作回调类型
pub type OperationCallback = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

/// 单个分片上的一个操作
///
/// 每个操作需提供 prepare/commit/rollback 三个回调。同一分片上的多个操作会被合并为
/// 一个 [`TransactionParticipant`]，按注册顺序依次调用。
#[derive(Clone)]
pub struct ShardOperation {
    /// 操作所属的分片 ID
    pub shard_id: String,
    /// 操作名（用于诊断/日志）
    pub name: String,
    prepare: Option<OperationCallback>,
    commit: Option<OperationCallback>,
    rollback: Option<OperationCallback>,
}

impl ShardOperation {
    /// 创建一个新的分片操作
    pub fn new(shard_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            shard_id: shard_id.into(),
            name: name.into(),
            prepare: None,
            commit: None,
            rollback: None,
        }
    }

    /// 设置 prepare 回调（用于写 undo/redo log）
    #[must_use]
    pub fn with_prepare<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        self.prepare = Some(Arc::new(f));
        self
    }

    /// 设置 commit 回调（实际写入业务数据）
    #[must_use]
    pub fn with_commit<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        self.commit = Some(Arc::new(f));
        self
    }

    /// 设置 rollback 回调（清理 undo log）
    #[must_use]
    pub fn with_rollback<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        self.rollback = Some(Arc::new(f));
        self
    }
}

impl std::fmt::Debug for ShardOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShardOperation")
            .field("shard_id", &self.shard_id)
            .field("name", &self.name)
            .field("has_prepare", &self.prepare.is_some())
            .field("has_commit", &self.commit.is_some())
            .field("has_rollback", &self.rollback.is_some())
            .finish()
    }
}

/// 跨分片 ACID 协调器
///
/// 基于 [`DistributedTransaction`] 实现。将按 shard_id 分组的操作转换为 2PC 分支事务。
///
/// # 流程
///
/// 1. `add_operation` / `add_shard_operation` 注册多个分片操作
/// 2. `execute` 调用：
///   - 按 shard_id 分组
///   - 为每个 shard 生成一个 [`TransactionParticipant`]，其 prepare/commit/rollback 依次调用该 shard 下所有操作的对应回调
///   - 执行 2PC：prepare 全部分片 → 全部成功 → commit；任一失败 → rollback 已 prepare 的分片
///
/// # 状态查询
///
/// - [`CrossShardCoordinator::state`]：返回底层 2PC 事务状态
/// - [`CrossShardCoordinator::participant_states`]：返回每个分片的最终状态
pub struct CrossShardCoordinator {
    tx_id: String,
    operations: Vec<ShardOperation>,
    tx: Option<DistributedTransaction>,
}

impl CrossShardCoordinator {
    /// 创建新的跨分片协调器
    pub fn new(tx_id: impl Into<String>) -> Self {
        Self {
            tx_id: tx_id.into(),
            operations: Vec::new(),
            tx: None,
        }
    }

    /// 事务 ID
    pub fn tx_id(&self) -> &str {
        &self.tx_id
    }

    /// 注册一个分片操作（builder 风格）
    pub fn add_shard_operation(&mut self, op: ShardOperation) -> &mut Self {
        self.operations.push(op);
        self
    }

    /// 注册一个分片操作（闭包风格）
    ///
    /// - `shard_id`：分片 ID（如 "shard-orders"、"shard-inventory"）
    /// - `prepare`/`commit`/`rollback`：三个回调，闭包形式
    #[allow(clippy::type_complexity)]
    pub fn add_operation<P, C, R>(
        &mut self,
        shard_id: impl Into<String>,
        prepare: P,
        commit: C,
        rollback: R,
    ) -> &mut Self
    where
        P: Fn() -> Result<(), String> + Send + Sync + 'static,
        C: Fn() -> Result<(), String> + Send + Sync + 'static,
        R: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        let shard = shard_id.into();
        let op = ShardOperation::new(shard.clone(), shard)
            .with_prepare(prepare)
            .with_commit(commit)
            .with_rollback(rollback);
        self.operations.push(op);
        self
    }

    /// 按分片分组返回所有操作
    pub fn operations_by_shard(&self) -> HashMap<String, Vec<&ShardOperation>> {
        let mut grouped: HashMap<String, Vec<&ShardOperation>> = HashMap::new();
        for op in &self.operations {
            grouped.entry(op.shard_id.clone()).or_default().push(op);
        }
        grouped
    }

    /// 返回已注册操作数（未去重分片）
    pub fn operation_count(&self) -> usize {
        self.operations.len()
    }

    /// 返回涉及的分片数（去重）
    pub fn shard_count(&self) -> usize {
        let mut shards: Vec<&str> = self
            .operations
            .iter()
            .map(|o| o.shard_id.as_str())
            .collect();
        shards.sort();
        shards.dedup();
        shards.len()
    }

    /// 执行跨分片 2PC
    ///
    /// 流程：
    /// 1. 按分片分组操作
    /// 2. 为每个分片生成一个 participant（按注册顺序合并该分片下的所有操作回调）
    /// 3. 调用 `prepare` → 全部成功后 `commit`；任一失败自动 `rollback` 已 prepare 的分片
    pub fn execute(&mut self) -> Result<(), CrossShardError> {
        if self.operations.is_empty() {
            return Err(CrossShardError::NoOperations);
        }
        // 构造底层 2PC 事务
        let mut tx = DistributedTransaction::new(&self.tx_id);

        // 按分片分组并保持注册顺序
        let mut shard_order: Vec<String> = Vec::new();
        let mut grouped: HashMap<String, Vec<ShardOperation>> = HashMap::new();
        for op in self.operations.drain(..) {
            if !grouped.contains_key(&op.shard_id) {
                shard_order.push(op.shard_id.clone());
            }
            grouped.entry(op.shard_id.clone()).or_default().push(op);
        }

        // 为每个 shard 生成一个 participant
        for shard_id in &shard_order {
            let ops = grouped.remove(shard_id).unwrap_or_default();
            let participant = build_participant(shard_id, ops);
            tx.add_participant(participant);
        }

        // 先保存 tx 引用，便于失败后查询状态
        let tx_ref = &mut tx;
        // 执行 2PC
        let prepare_result = tx_ref.prepare();
        if let Err(e) = prepare_result {
            self.tx = Some(tx);
            return Err(CrossShardError::PrepareFailed(e));
        }
        let commit_result = tx_ref.commit();
        if let Err(e) = commit_result {
            self.tx = Some(tx);
            return Err(CrossShardError::CommitFailed(e));
        }

        self.tx = Some(tx);
        Ok(())
    }

    /// 仅执行 prepare 阶段（用于两阶段手动控制）
    pub fn prepare_only(&mut self) -> Result<(), CrossShardError> {
        if self.operations.is_empty() {
            return Err(CrossShardError::NoOperations);
        }
        let mut tx = DistributedTransaction::new(&self.tx_id);
        let mut shard_order: Vec<String> = Vec::new();
        let mut grouped: HashMap<String, Vec<ShardOperation>> = HashMap::new();
        for op in self.operations.drain(..) {
            if !grouped.contains_key(&op.shard_id) {
                shard_order.push(op.shard_id.clone());
            }
            grouped.entry(op.shard_id.clone()).or_default().push(op);
        }
        for shard_id in &shard_order {
            let ops = grouped.remove(shard_id).unwrap_or_default();
            let participant = build_participant(shard_id, ops);
            tx.add_participant(participant);
        }
        let result = tx.prepare();
        self.tx = Some(tx);
        match result {
            Ok(()) => Ok(()),
            Err(e) => Err(CrossShardError::PrepareFailed(e)),
        }
    }

    /// 在 prepare_only 之后手动提交
    pub fn commit(&mut self) -> Result<(), CrossShardError> {
        let tx = self.tx.as_mut().ok_or(CrossShardError::NotPrepared)?;
        tx.commit().map_err(CrossShardError::CommitFailed)
    }

    /// 在 prepare_only 之后手动回滚
    pub fn rollback(&mut self) -> Result<(), CrossShardError> {
        let tx = self.tx.as_mut().ok_or(CrossShardError::NotPrepared)?;
        tx.rollback().map_err(CrossShardError::RollbackFailed)
    }

    /// 当前事务状态
    pub fn state(&self) -> Option<TransactionState> {
        self.tx.as_ref().map(|t| t.state())
    }

    /// 各分片 participant 的状态
    pub fn participant_states(&self) -> Option<Vec<crate::ParticipantState>> {
        self.tx
            .as_ref()
            .map(|t| t.participants().iter().map(|p| p.state.clone()).collect())
    }

    /// 各分片 participant 的资源 ID 列表
    pub fn participant_ids(&self) -> Option<Vec<String>> {
        self.tx.as_ref().map(|t| {
            t.participants()
                .iter()
                .map(|p| p.resource_id.clone())
                .collect()
        })
    }
}

impl std::fmt::Debug for CrossShardCoordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CrossShardCoordinator")
            .field("tx_id", &self.tx_id)
            .field("operations", &self.operations.len())
            .field("state", &self.tx.as_ref().map(|t| t.state()))
            .finish()
    }
}

/// 跨分片事务错误
#[derive(Debug)]
pub enum CrossShardError {
    /// 未注册任何操作
    NoOperations,
    /// 尚未执行 prepare，无法 commit/rollback
    NotPrepared,
    /// prepare 阶段失败（携带错误描述）
    PrepareFailed(String),
    /// commit 阶段失败
    CommitFailed(String),
    /// rollback 阶段失败
    RollbackFailed(String),
}

impl std::fmt::Display for CrossShardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CrossShardError::NoOperations => write!(f, "未注册任何分片操作"),
            CrossShardError::NotPrepared => write!(f, "尚未执行 prepare，无法 commit/rollback"),
            CrossShardError::PrepareFailed(e) => write!(f, "跨分片 prepare 失败: {}", e),
            CrossShardError::CommitFailed(e) => write!(f, "跨分片 commit 失败: {}", e),
            CrossShardError::RollbackFailed(e) => write!(f, "跨分片 rollback 失败: {}", e),
        }
    }
}

impl std::error::Error for CrossShardError {}

/// 为一个分片的所有操作合并生成一个 [`TransactionParticipant`]
fn build_participant(shard_id: &str, ops: Vec<ShardOperation>) -> TransactionParticipant {
    // 收集回调（按注册顺序）
    let prepares: Vec<OperationCallback> = ops.iter().filter_map(|o| o.prepare.clone()).collect();
    let commits: Vec<OperationCallback> = ops.iter().filter_map(|o| o.commit.clone()).collect();
    let rollbacks: Vec<OperationCallback> = ops.iter().filter_map(|o| o.rollback.clone()).collect();

    let prepare_fn = move || {
        for cb in &prepares {
            cb()?;
        }
        Ok(())
    };
    let commit_fn = move || {
        for cb in &commits {
            cb()?;
        }
        Ok(())
    };
    let rollback_fn = move || {
        // rollback 尽量全部执行（best-effort），不因单个失败短路；
        // 但收集所有错误并至少返回第一个，避免静默吞掉失败。
        let mut first_err: Option<String> = None;
        let mut fail_count = 0usize;
        let total = rollbacks.len();
        for cb in &rollbacks {
            if let Err(e) = cb() {
                fail_count += 1;
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
        if let Some(e) = first_err {
            return Err(format!("rollback 失败 {}/{}: {}", fail_count, total, e));
        }
        Ok(())
    };

    TransactionParticipant::new(shard_id)
        .with_prepare(prepare_fn)
        .with_commit(commit_fn)
        .with_rollback(rollback_fn)
}

// =====================================================================
// 测试
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn make_counters() -> (Arc<AtomicU32>, Arc<AtomicU32>, Arc<AtomicU32>) {
        (
            Arc::new(AtomicU32::new(0)),
            Arc::new(AtomicU32::new(0)),
            Arc::new(AtomicU32::new(0)),
        )
    }

    // ===== 基础测试 =====

    #[test]
    fn coordinator_new_initial_state() {
        let coord = CrossShardCoordinator::new("tx-001");
        assert_eq!(coord.tx_id(), "tx-001");
        assert_eq!(coord.operation_count(), 0);
        assert_eq!(coord.shard_count(), 0);
        assert_eq!(coord.state(), None);
    }

    #[test]
    fn add_operation_increments_counts() {
        let mut coord = CrossShardCoordinator::new("tx-002");
        coord.add_operation("shard-a", || Ok(()), || Ok(()), || Ok(()));
        assert_eq!(coord.operation_count(), 1);
        assert_eq!(coord.shard_count(), 1);

        coord.add_operation("shard-b", || Ok(()), || Ok(()), || Ok(()));
        assert_eq!(coord.operation_count(), 2);
        assert_eq!(coord.shard_count(), 2);

        // 同分片多次操作
        coord.add_operation("shard-a", || Ok(()), || Ok(()), || Ok(()));
        assert_eq!(coord.operation_count(), 3);
        assert_eq!(coord.shard_count(), 2);
    }

    #[test]
    fn add_shard_operation_builder_style() {
        let mut coord = CrossShardCoordinator::new("tx-003");
        let op = ShardOperation::new("shard-x", "op-1")
            .with_prepare(|| Ok(()))
            .with_commit(|| Ok(()))
            .with_rollback(|| Ok(()));
        coord.add_shard_operation(op);
        assert_eq!(coord.operation_count(), 1);
        assert_eq!(coord.shard_count(), 1);
    }

    #[test]
    fn operations_by_shard_groups_correctly() {
        let mut coord = CrossShardCoordinator::new("tx-004");
        coord.add_operation("shard-a", || Ok(()), || Ok(()), || Ok(()));
        coord.add_operation("shard-b", || Ok(()), || Ok(()), || Ok(()));
        coord.add_operation("shard-a", || Ok(()), || Ok(()), || Ok(()));

        let grouped = coord.operations_by_shard();
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped.get("shard-a").map(|v| v.len()), Some(2));
        assert_eq!(grouped.get("shard-b").map(|v| v.len()), Some(1));
    }

    #[test]
    fn execute_empty_returns_error() {
        let mut coord = CrossShardCoordinator::new("tx-empty");
        let result = coord.execute();
        assert!(matches!(result, Err(CrossShardError::NoOperations)));
    }

    // ===== 2PC 成功路径 =====

    #[test]
    fn execute_single_shard_success() {
        let (prepared, committed, rolled_back) = make_counters();
        let mut coord = CrossShardCoordinator::new("tx-single");

        let p = prepared.clone();
        let c = committed.clone();
        let r = rolled_back.clone();
        coord.add_operation(
            "shard-1",
            move || {
                p.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            move || {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            move || {
                r.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );

        coord.execute().unwrap();
        assert_eq!(prepared.load(Ordering::SeqCst), 1);
        assert_eq!(committed.load(Ordering::SeqCst), 1);
        assert_eq!(rolled_back.load(Ordering::SeqCst), 0);
        assert_eq!(coord.state(), Some(TransactionState::Committed));
    }

    #[test]
    fn execute_multi_shard_success() {
        let (prepared, committed, rolled_back) = make_counters();

        let mut coord = CrossShardCoordinator::new("tx-multi");

        let p1 = prepared.clone();
        let c1 = committed.clone();
        let r1 = rolled_back.clone();
        coord.add_operation(
            "shard-orders",
            move || {
                p1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            move || {
                c1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            move || {
                r1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );

        let p2 = prepared.clone();
        let c2 = committed.clone();
        let r2 = rolled_back.clone();
        coord.add_operation(
            "shard-inventory",
            move || {
                p2.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            move || {
                c2.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            move || {
                r2.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );

        let p3 = prepared.clone();
        let c3 = committed.clone();
        let r3 = rolled_back.clone();
        coord.add_operation(
            "shard-account",
            move || {
                p3.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            move || {
                c3.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            move || {
                r3.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );

        coord.execute().unwrap();
        assert_eq!(prepared.load(Ordering::SeqCst), 3);
        assert_eq!(committed.load(Ordering::SeqCst), 3);
        assert_eq!(rolled_back.load(Ordering::SeqCst), 0);
        assert_eq!(coord.state(), Some(TransactionState::Committed));

        // 应有 3 个分片 participant
        let states = coord.participant_states().unwrap();
        assert_eq!(states.len(), 3);
        for s in states {
            assert_eq!(s, crate::ParticipantState::Committed);
        }

        let ids = coord.participant_ids().unwrap();
        assert!(ids.contains(&"shard-orders".to_string()));
        assert!(ids.contains(&"shard-inventory".to_string()));
        assert!(ids.contains(&"shard-account".to_string()));
    }

    #[test]
    fn execute_multiple_ops_on_same_shard_are_merged() {
        // 同一分片上的多个操作应合并为一个 participant
        let prepared = Arc::new(AtomicU32::new(0));
        let committed = Arc::new(AtomicU32::new(0));

        let mut coord = CrossShardCoordinator::new("tx-merged");

        let p1 = prepared.clone();
        let c1 = committed.clone();
        coord.add_operation(
            "shard-a",
            move || {
                p1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            move || {
                c1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            || Ok(()),
        );

        let p2 = prepared.clone();
        let c2 = committed.clone();
        coord.add_operation(
            "shard-a",
            move || {
                p2.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            move || {
                c2.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            || Ok(()),
        );

        coord.execute().unwrap();
        // 2 个操作的 prepare 都应被执行
        assert_eq!(prepared.load(Ordering::SeqCst), 2);
        assert_eq!(committed.load(Ordering::SeqCst), 2);

        // 只应有 1 个 participant（合并）
        let states = coord.participant_states().unwrap();
        assert_eq!(states.len(), 1);
    }

    // ===== 2PC 失败路径 =====

    #[test]
    fn execute_prepare_failure_triggers_rollback() {
        let prepared = Arc::new(AtomicU32::new(0));
        let rolled_back = Arc::new(AtomicU32::new(0));

        let mut coord = CrossShardCoordinator::new("tx-fail");

        // 第一个分片 prepare 成功
        let p1 = prepared.clone();
        let r1 = rolled_back.clone();
        coord.add_operation(
            "shard-1",
            move || {
                p1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            || Ok(()),
            move || {
                r1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );

        // 第二个分片 prepare 失败
        coord.add_operation(
            "shard-2",
            || Err("shard-2 prepare failed".to_string()),
            || Ok(()),
            || Ok(()),
        );

        let result = coord.execute();
        assert!(result.is_err());
        assert!(matches!(result, Err(CrossShardError::PrepareFailed(_))));

        // shard-1 prepare 成功 → 应被 rollback
        assert_eq!(prepared.load(Ordering::SeqCst), 1);
        assert_eq!(rolled_back.load(Ordering::SeqCst), 1);

        // 状态应为 Failed
        assert_eq!(coord.state(), Some(TransactionState::Failed));
    }

    #[test]
    fn execute_first_shard_prepare_failure_no_rollback() {
        let rolled_back = Arc::new(AtomicU32::new(0));

        let mut coord = CrossShardCoordinator::new("tx-fail-first");

        // 第一个分片直接失败
        coord.add_operation(
            "shard-1",
            || Err("immediate failure".to_string()),
            || Ok(()),
            || Ok(()),
        );

        // 第二个分片不应被调用
        let r2 = rolled_back.clone();
        coord.add_operation(
            "shard-2",
            || Ok(()),
            || Ok(()),
            move || {
                r2.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );

        let result = coord.execute();
        assert!(result.is_err());
        // 没有 prepare 成功的分片，所以 rollback 不应被调用
        assert_eq!(rolled_back.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn execute_commit_failure_marks_failed() {
        let mut coord = CrossShardCoordinator::new("tx-commit-fail");

        // 第一个分片 commit 成功
        coord.add_operation("shard-1", || Ok(()), || Ok(()), || Ok(()));

        // 第二个分片 commit 失败
        coord.add_operation(
            "shard-2",
            || Ok(()),
            || Err("commit failed".to_string()),
            || Ok(()),
        );

        let result = coord.execute();
        assert!(result.is_err());
        assert!(matches!(result, Err(CrossShardError::CommitFailed(_))));
        assert_eq!(coord.state(), Some(TransactionState::Failed));
    }

    // ===== 手动两阶段 =====

    #[test]
    fn prepare_only_then_commit() {
        let prepared = Arc::new(AtomicU32::new(0));
        let committed = Arc::new(AtomicU32::new(0));

        let mut coord = CrossShardCoordinator::new("tx-manual");

        let p1 = prepared.clone();
        let c1 = committed.clone();
        coord.add_operation(
            "shard-1",
            move || {
                p1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            move || {
                c1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            || Ok(()),
        );

        coord.prepare_only().unwrap();
        assert_eq!(prepared.load(Ordering::SeqCst), 1);
        assert_eq!(committed.load(Ordering::SeqCst), 0);
        assert_eq!(coord.state(), Some(TransactionState::Prepared));

        coord.commit().unwrap();
        assert_eq!(committed.load(Ordering::SeqCst), 1);
        assert_eq!(coord.state(), Some(TransactionState::Committed));
    }

    #[test]
    fn prepare_only_then_rollback() {
        let prepared = Arc::new(AtomicU32::new(0));
        let rolled_back = Arc::new(AtomicU32::new(0));

        let mut coord = CrossShardCoordinator::new("tx-manual-rb");

        let p1 = prepared.clone();
        let r1 = rolled_back.clone();
        coord.add_operation(
            "shard-1",
            move || {
                p1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            || Ok(()),
            move || {
                r1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );

        coord.prepare_only().unwrap();
        assert_eq!(prepared.load(Ordering::SeqCst), 1);
        assert_eq!(coord.state(), Some(TransactionState::Prepared));

        coord.rollback().unwrap();
        assert_eq!(rolled_back.load(Ordering::SeqCst), 1);
        assert_eq!(coord.state(), Some(TransactionState::RolledBack));
    }

    #[test]
    fn commit_without_prepare_returns_error() {
        let mut coord = CrossShardCoordinator::new("tx-no-prepare");
        coord.add_operation("shard-1", || Ok(()), || Ok(()), || Ok(()));
        let result = coord.commit();
        assert!(matches!(result, Err(CrossShardError::NotPrepared)));
    }

    #[test]
    fn rollback_without_prepare_returns_error() {
        let mut coord = CrossShardCoordinator::new("tx-no-prepare-rb");
        coord.add_operation("shard-1", || Ok(()), || Ok(()), || Ok(()));
        let result = coord.rollback();
        assert!(matches!(result, Err(CrossShardError::NotPrepared)));
    }

    // ===== 端到端跨分片业务场景 =====

    #[test]
    fn end_to_end_cross_shard_order_creation() {
        // 模拟：创建订单需要同时写入
        //   shard-orders：订单记录
        //   shard-inventory：库存扣减
        //   shard-account：账户扣款
        let order_prepared = Arc::new(AtomicU32::new(0));
        let order_committed = Arc::new(AtomicU32::new(0));
        let inventory_prepared = Arc::new(AtomicU32::new(0));
        let inventory_committed = Arc::new(AtomicU32::new(0));
        let account_prepared = Arc::new(AtomicU32::new(0));
        let account_committed = Arc::new(AtomicU32::new(0));

        let mut coord = CrossShardCoordinator::new("tx-order-001");

        let op1 = order_prepared.clone();
        let oc1 = order_committed.clone();
        coord.add_operation(
            "shard-orders",
            move || {
                op1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            move || {
                oc1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            || Ok(()),
        );

        let ip = inventory_prepared.clone();
        let ic = inventory_committed.clone();
        coord.add_operation(
            "shard-inventory",
            move || {
                ip.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            move || {
                ic.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            || Ok(()),
        );

        let ap = account_prepared.clone();
        let ac = account_committed.clone();
        coord.add_operation(
            "shard-account",
            move || {
                ap.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            move || {
                ac.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            || Ok(()),
        );

        coord.execute().unwrap();

        assert_eq!(order_prepared.load(Ordering::SeqCst), 1);
        assert_eq!(order_committed.load(Ordering::SeqCst), 1);
        assert_eq!(inventory_prepared.load(Ordering::SeqCst), 1);
        assert_eq!(inventory_committed.load(Ordering::SeqCst), 1);
        assert_eq!(account_prepared.load(Ordering::SeqCst), 1);
        assert_eq!(account_committed.load(Ordering::SeqCst), 1);

        // 最终状态应为 Committed，所有分片 participant 应为 Committed
        assert_eq!(coord.state(), Some(TransactionState::Committed));
        for s in coord.participant_states().unwrap() {
            assert_eq!(s, crate::ParticipantState::Committed);
        }
    }

    #[test]
    fn end_to_end_cross_shard_with_inventory_failure() {
        // 模拟：库存不足，prepare 失败，应回滚已 prepare 的订单分片
        let order_rolled_back = Arc::new(AtomicU32::new(0));

        let mut coord = CrossShardCoordinator::new("tx-order-fail");

        // 订单分片 prepare 成功
        let orb = order_rolled_back.clone();
        coord.add_operation(
            "shard-orders",
            || Ok(()),
            || Ok(()),
            move || {
                orb.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );

        // 库存分片 prepare 失败（库存不足）
        coord.add_operation(
            "shard-inventory",
            || Err("insufficient stock".to_string()),
            || Ok(()),
            || Ok(()),
        );

        // 账户分片不应被调用
        let account_prepared = Arc::new(AtomicU32::new(0));
        let ap = account_prepared.clone();
        coord.add_operation(
            "shard-account",
            move || {
                ap.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            || Ok(()),
            || Ok(()),
        );

        let result = coord.execute();
        assert!(result.is_err());

        // 订单分片应被回滚
        assert_eq!(order_rolled_back.load(Ordering::SeqCst), 1);
        // 账户分片 prepare 不应被调用（库存先失败）
        assert_eq!(account_prepared.load(Ordering::SeqCst), 0);

        // 事务状态为 Failed
        assert_eq!(coord.state(), Some(TransactionState::Failed));
    }

    // ===== 错误格式化与 Debug =====

    #[test]
    fn error_display_formats() {
        let e1 = CrossShardError::NoOperations;
        assert!(format!("{}", e1).contains("未注册"));

        let e2 = CrossShardError::NotPrepared;
        assert!(format!("{}", e2).contains("prepare"));

        let e3 = CrossShardError::PrepareFailed("db down".into());
        assert!(format!("{}", e3).contains("prepare"));
        assert!(format!("{}", e3).contains("db down"));

        let e4 = CrossShardError::CommitFailed("timeout".into());
        assert!(format!("{}", e4).contains("commit"));

        let e5 = CrossShardError::RollbackFailed("rollback err".into());
        assert!(format!("{}", e5).contains("rollback"));
    }

    #[test]
    fn coordinator_debug_output() {
        let mut coord = CrossShardCoordinator::new("tx-debug");
        coord.add_operation("shard-1", || Ok(()), || Ok(()), || Ok(()));
        let s = format!("{:?}", coord);
        assert!(s.contains("tx-debug"));
        assert!(s.contains("operations: 1"));
    }

    #[test]
    fn shard_operation_debug() {
        let op = ShardOperation::new("shard-x", "op-1")
            .with_prepare(|| Ok(()))
            .with_commit(|| Ok(()));
        let s = format!("{:?}", op);
        assert!(s.contains("shard-x"));
        assert!(s.contains("op-1"));
        assert!(s.contains("has_prepare: true"));
        assert!(s.contains("has_commit: true"));
        assert!(s.contains("has_rollback: false"));
    }

    // ===== 边界测试 =====

    #[test]
    fn shard_count_with_duplicate_shards() {
        let mut coord = CrossShardCoordinator::new("tx-dup");
        coord.add_operation("shard-a", || Ok(()), || Ok(()), || Ok(()));
        coord.add_operation("shard-a", || Ok(()), || Ok(()), || Ok(()));
        coord.add_operation("shard-a", || Ok(()), || Ok(()), || Ok(()));
        assert_eq!(coord.operation_count(), 3);
        assert_eq!(coord.shard_count(), 1);
    }

    #[test]
    fn execute_with_callback_failures_in_commit() {
        // 测试 commit 阶段中间失败的情况
        let mut coord = CrossShardCoordinator::new("tx-commit-mid-fail");

        // shard-1 commit 成功
        coord.add_operation("shard-1", || Ok(()), || Ok(()), || Ok(()));
        // shard-2 commit 失败
        coord.add_operation(
            "shard-2",
            || Ok(()),
            || Err("commit error".to_string()),
            || Ok(()),
        );

        let result = coord.execute();
        assert!(result.is_err());
        // 应在 commit 阶段失败
        assert!(matches!(result, Err(CrossShardError::CommitFailed(_))));
        // 状态为 Failed
        assert_eq!(coord.state(), Some(TransactionState::Failed));
    }
}
