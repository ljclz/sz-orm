//! # SZ-ORM DTX — 分布式事务
//!
//! 提供跨服务分布式事务协调能力，包含 Saga、TCC 与跨分片事务模式，
//! 支持参与者状态机与回滚回调。
//!
//! ## 主要模块
//!
//! - [`saga`] — Saga 长事务编排
//! - [`tcc`] — Try-Confirm-Cancel 三阶段提交
//! - [`cross_shard`] — 跨分片事务协调

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub mod cross_shard;
pub mod saga;
pub mod tcc;

// XA 事务支持（feature 隔离，默认关闭）
#[cfg(feature = "xa")]
pub mod recovery;
#[cfg(feature = "xa")]
pub mod suspension;
#[cfg(feature = "xa")]
pub mod xa;

// ============================================================================
// TransactionLogStore — 事务日志持久化（用于崩溃恢复）
// ============================================================================

/// 分布式事务日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionLogEntry {
    /// 事务 ID
    pub tx_id: String,
    /// 事务状态
    pub state: String,
    /// 参与者列表
    pub participants: Vec<String>,
    /// 时间戳（Unix 毫秒）
    pub timestamp: String,
    /// 操作类型（prepare/commit/rollback）
    pub action: String,
}

/// 事务日志存储 trait
///
/// 手动解糖 async（不使用 `#[async_trait]`），与 `sz-orm-core` 的 `L2CacheBackend` 风格一致。
pub trait TransactionLogStore: Send + Sync {
    /// 追加日志
    fn append<'a>(
        &'a self,
        entry: TransactionLogEntry,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;

    /// 读取事务日志
    fn read<'a>(
        &'a self,
        tx_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<TransactionLogEntry>, String>> + Send + 'a>>;

    /// 读取所有未完成事务（state 不为 Committed/RolledBack/Failed 的最新条目）
    fn read_pending<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<TransactionLogEntry>, String>> + Send + 'a>>;
}

/// 内存事务日志存储（开发测试用）
pub struct InMemoryTransactionLog {
    logs: tokio::sync::RwLock<Vec<TransactionLogEntry>>,
}

impl Default for InMemoryTransactionLog {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryTransactionLog {
    pub fn new() -> Self {
        Self {
            logs: tokio::sync::RwLock::new(Vec::new()),
        }
    }
}

impl TransactionLogStore for InMemoryTransactionLog {
    fn append<'a>(
        &'a self,
        entry: TransactionLogEntry,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            let mut logs = self.logs.write().await;
            logs.push(entry);
            Ok(())
        })
    }

    fn read<'a>(
        &'a self,
        tx_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<TransactionLogEntry>, String>> + Send + 'a>> {
        Box::pin(async move {
            let logs = self.logs.read().await;
            Ok(logs.iter().filter(|e| e.tx_id == tx_id).cloned().collect())
        })
    }

    fn read_pending<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<TransactionLogEntry>, String>> + Send + 'a>> {
        Box::pin(async move {
            let logs = self.logs.read().await;
            // 按 tx_id 分组，取每个事务的最新条目
            let mut latest: HashMap<String, &TransactionLogEntry> = HashMap::new();
            for entry in logs.iter() {
                latest.insert(entry.tx_id.clone(), entry);
            }
            // 过滤未完成事务（非 Committed/RolledBack/Failed）
            const TERMINAL: &[&str] = &["Committed", "RolledBack", "Failed"];
            let pending: Vec<TransactionLogEntry> = latest
                .into_values()
                .filter(|e| !TERMINAL.iter().any(|t| e.state == *t))
                .cloned()
                .collect();
            Ok(pending)
        })
    }
}

/// 同步调用 async trait 方法（用于 prepare/commit/rollback 同步上下文）
///
/// 优先使用当前 tokio 运行时 `block_on`；若无运行时（如纯同步测试上下文），
/// 创建临时运行时执行。失败时返回 `None`，调用方可忽略日志写入失败。
fn block_on_async<F>(fut: F) -> Option<F::Output>
where
    F: Future + Send,
    F::Output: Send,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        // 已在 tokio 运行时上下文中，使用 handle.block_on
        // 注意：若当前是 current-thread runtime 且未来会 re-enter，可能死锁。
        // 此处用于同步 trait 方法调用 async 日志写入，不会 re-enter 同一 runtime。
        return Some(handle.block_on(fut));
    }
    // 无运行时 → 创建临时运行时（仅需单线程，避免依赖 rt-multi-thread）
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()
        .map(|rt| rt.block_on(fut))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransactionState {
    Active,
    Preparing,
    Prepared,
    Committing,
    Committed,
    RollingBack,
    RolledBack,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ParticipantState {
    Active,
    Prepared,
    Committed,
    RolledBack,
    Failed,
}

pub type ParticipantCallback = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

#[derive(Clone)]
pub struct TransactionParticipant {
    pub resource_id: String,
    pub state: ParticipantState,
    prepare_fn: Option<ParticipantCallback>,
    commit_fn: Option<ParticipantCallback>,
    rollback_fn: Option<ParticipantCallback>,
}

impl TransactionParticipant {
    pub fn new(id: &str) -> Self {
        Self {
            resource_id: id.to_string(),
            state: ParticipantState::Active,
            prepare_fn: None,
            commit_fn: None,
            rollback_fn: None,
        }
    }

    pub fn with_prepare<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        self.prepare_fn = Some(Arc::new(f));
        self
    }

    pub fn with_commit<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        self.commit_fn = Some(Arc::new(f));
        self
    }

    pub fn with_rollback<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        self.rollback_fn = Some(Arc::new(f));
        self
    }

    pub fn prepare(&mut self) -> Result<(), String> {
        if let Some(cb) = &self.prepare_fn {
            cb()?;
        }
        self.state = ParticipantState::Prepared;
        Ok(())
    }

    pub fn commit(&mut self) -> Result<(), String> {
        if let Some(cb) = &self.commit_fn {
            cb()?;
        }
        self.state = ParticipantState::Committed;
        Ok(())
    }

    pub fn rollback(&mut self) -> Result<(), String> {
        if let Some(cb) = &self.rollback_fn {
            cb()?;
        }
        self.state = ParticipantState::RolledBack;
        Ok(())
    }

    pub fn fail(&mut self) {
        self.state = ParticipantState::Failed;
    }
}

impl std::fmt::Debug for TransactionParticipant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransactionParticipant")
            .field("resource_id", &self.resource_id)
            .field("state", &self.state)
            .field("has_prepare", &self.prepare_fn.is_some())
            .field("has_commit", &self.commit_fn.is_some())
            .field("has_rollback", &self.rollback_fn.is_some())
            .finish()
    }
}

pub struct DistributedTransaction {
    pub id: String,
    state: TransactionState,
    participants: Vec<TransactionParticipant>,
    /// 事务日志存储（可选，启用后会在 prepare/commit/rollback 写入日志）
    log_store: Option<Arc<dyn TransactionLogStore>>,
}

impl DistributedTransaction {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            state: TransactionState::Active,
            participants: Vec::new(),
            log_store: None,
        }
    }

    /// 设置事务日志存储
    pub fn with_log_store(mut self, store: Arc<dyn TransactionLogStore>) -> Self {
        self.log_store = Some(store);
        self
    }

    /// 返回是否启用了日志存储
    pub fn has_log_store(&self) -> bool {
        self.log_store.is_some()
    }

    pub fn state(&self) -> TransactionState {
        self.state.clone()
    }

    pub fn participants(&self) -> &[TransactionParticipant] {
        &self.participants
    }

    pub fn add_participant(&mut self, p: TransactionParticipant) {
        self.participants.push(p);
    }

    /// 写入一条事务日志（失败时不影响主流程）
    fn write_log(&self, action: &str, state: &str) {
        let Some(store) = &self.log_store else {
            return;
        };
        let entry = TransactionLogEntry {
            tx_id: self.id.clone(),
            state: state.to_string(),
            participants: self
                .participants
                .iter()
                .map(|p| p.resource_id.clone())
                .collect(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis().to_string())
                .unwrap_or_else(|_| "0".to_string()),
            action: action.to_string(),
        };
        // 同步阻塞调用 async trait 方法
        let _ = block_on_async(store.append(entry));
    }

    pub fn prepare(&mut self) -> Result<(), String> {
        match self.state {
            TransactionState::Active => {}
            _ => {
                return Err(format!(
                    "Cannot prepare transaction in state {:?}",
                    self.state
                ))
            }
        }
        self.state = TransactionState::Preparing;
        self.write_log("prepare", "Preparing");

        let total = self.participants.len();
        let mut prepared_count = 0;
        for i in 0..total {
            match self.participants[i].prepare() {
                Ok(()) => prepared_count += 1,
                Err(e) => {
                    let resource_id = self.participants[i].resource_id.clone();
                    self.participants[i].fail();
                    for j in 0..prepared_count {
                        let _ = self.participants[j].rollback();
                    }
                    self.state = TransactionState::Failed;
                    self.write_log("prepare", "Failed");
                    return Err(format!(
                        "Prepare failed at participant {}: {}",
                        resource_id, e
                    ));
                }
            }
        }
        self.state = TransactionState::Prepared;
        self.write_log("prepare", "Prepared");
        Ok(())
    }

    pub fn commit(&mut self) -> Result<(), String> {
        match self.state {
            TransactionState::Prepared => {}
            TransactionState::Active if self.participants.is_empty() => {}
            _ => {
                return Err(format!(
                    "Cannot commit transaction in state {:?}",
                    self.state
                ))
            }
        }
        self.state = TransactionState::Committing;
        self.write_log("commit", "Committing");
        // 先收集首个失败参与者，避免在 &mut self.participants 借用期间调用 self.write_log
        let mut commit_error: Option<(String, String)> = None;
        for participant in &mut self.participants {
            if let Err(e) = participant.commit() {
                let resource_id = participant.resource_id.clone();
                commit_error = Some((resource_id, e));
                break;
            }
        }
        if let Some((resource_id, e)) = commit_error {
            self.state = TransactionState::Failed;
            self.write_log("commit", "Failed");
            return Err(format!(
                "Commit failed at participant {}: {}",
                resource_id, e
            ));
        }
        self.state = TransactionState::Committed;
        self.write_log("commit", "Committed");
        Ok(())
    }

    pub fn rollback(&mut self) -> Result<(), String> {
        match self.state {
            TransactionState::Active
            | TransactionState::Prepared
            | TransactionState::Failed
            | TransactionState::Preparing => {}
            TransactionState::RolledBack | TransactionState::Committed => {
                return Err(format!(
                    "Cannot rollback transaction in terminal state {:?}",
                    self.state
                ))
            }
            _ => {}
        }
        self.state = TransactionState::RollingBack;
        self.write_log("rollback", "RollingBack");
        for participant in &mut self.participants {
            let _ = participant.rollback();
        }
        self.state = TransactionState::RolledBack;
        self.write_log("rollback", "RolledBack");
        Ok(())
    }
}

pub struct DtxManager {
    transactions: Arc<RwLock<HashMap<String, DistributedTransaction>>>,
}

impl DtxManager {
    pub fn new() -> Self {
        Self {
            transactions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn begin(&self, id: &str) -> Result<(), String> {
        let mut txs = self.transactions.write();
        if txs.contains_key(id) {
            return Err(format!("Transaction {} already exists", id));
        }
        txs.insert(id.to_string(), DistributedTransaction::new(id));
        Ok(())
    }

    pub fn add_participant(
        &self,
        tx_id: &str,
        participant: TransactionParticipant,
    ) -> Result<(), String> {
        let mut txs = self.transactions.write();
        let tx = txs
            .get_mut(tx_id)
            .ok_or_else(|| format!("Transaction {} not found", tx_id))?;
        if tx.state != TransactionState::Active {
            return Err(format!("Transaction {} is not active", tx_id));
        }
        tx.add_participant(participant);
        Ok(())
    }

    pub fn prepare(&self, tx_id: &str) -> Result<(), String> {
        let mut txs = self.transactions.write();
        let tx = txs
            .get_mut(tx_id)
            .ok_or_else(|| format!("Transaction {} not found", tx_id))?;
        tx.prepare()
    }

    pub fn commit(&self, tx_id: &str) -> Result<(), String> {
        let mut txs = self.transactions.write();
        let tx = txs
            .get_mut(tx_id)
            .ok_or_else(|| format!("Transaction {} not found", tx_id))?;
        tx.commit()
    }

    pub fn rollback(&self, tx_id: &str) -> Result<(), String> {
        let mut txs = self.transactions.write();
        let tx = txs
            .get_mut(tx_id)
            .ok_or_else(|| format!("Transaction {} not found", tx_id))?;
        tx.rollback()
    }

    pub fn get(&self, tx_id: &str) -> Option<TransactionState> {
        let txs = self.transactions.read();
        txs.get(tx_id).map(|t| t.state.clone())
    }

    pub fn list(&self) -> Vec<String> {
        let txs = self.transactions.read();
        let mut ids: Vec<String> = txs.keys().cloned().collect();
        ids.sort();
        ids
    }

    pub fn participant_states(&self, tx_id: &str) -> Option<Vec<ParticipantState>> {
        let txs = self.transactions.read();
        txs.get(tx_id)
            .map(|t| t.participants.iter().map(|p| p.state.clone()).collect())
    }
}

impl Default for DtxManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn test_dtx_new() {
        let t = DistributedTransaction::new("tx1");
        assert_eq!(t.id, "tx1");
        assert_eq!(t.state(), TransactionState::Active);
    }

    #[test]
    fn test_dtx_empty_commit() {
        let mut t = DistributedTransaction::new("tx1");
        t.commit().unwrap();
        assert_eq!(t.state(), TransactionState::Committed);
    }

    #[test]
    fn test_dtx_rollback_active() {
        let mut t = DistributedTransaction::new("tx1");
        t.rollback().unwrap();
        assert_eq!(t.state(), TransactionState::RolledBack);
    }

    #[test]
    fn test_participant_new() {
        let p = TransactionParticipant::new("db1");
        assert_eq!(p.resource_id, "db1");
        assert_eq!(p.state, ParticipantState::Active);
    }

    #[test]
    fn test_participant_prepare() {
        let mut p = TransactionParticipant::new("db1");
        p.prepare().unwrap();
        assert_eq!(p.state, ParticipantState::Prepared);
    }

    #[test]
    fn test_participant_commit() {
        let mut p = TransactionParticipant::new("db1");
        p.commit().unwrap();
        assert_eq!(p.state, ParticipantState::Committed);
    }

    #[test]
    fn test_participant_rollback() {
        let mut p = TransactionParticipant::new("db1");
        p.rollback().unwrap();
        assert_eq!(p.state, ParticipantState::RolledBack);
    }

    #[test]
    fn test_dtx_two_phase_commit_success() {
        let prepared = Arc::new(AtomicU32::new(0));
        let committed = Arc::new(AtomicU32::new(0));

        let p1_prepare = prepared.clone();
        let p1_commit = committed.clone();
        let p2_prepare = prepared.clone();
        let p2_commit = committed.clone();

        let mut tx = DistributedTransaction::new("tx-2pc");
        tx.add_participant(
            TransactionParticipant::new("db1")
                .with_prepare(move || {
                    p1_prepare.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .with_commit(move || {
                    p1_commit.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
        );
        tx.add_participant(
            TransactionParticipant::new("db2")
                .with_prepare(move || {
                    p2_prepare.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .with_commit(move || {
                    p2_commit.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
        );

        tx.prepare().unwrap();
        assert_eq!(tx.state(), TransactionState::Prepared);
        assert_eq!(prepared.load(Ordering::SeqCst), 2);
        assert_eq!(committed.load(Ordering::SeqCst), 0);

        tx.commit().unwrap();
        assert_eq!(tx.state(), TransactionState::Committed);
        assert_eq!(committed.load(Ordering::SeqCst), 2);

        let states = tx.participants();
        assert_eq!(states[0].state, ParticipantState::Committed);
        assert_eq!(states[1].state, ParticipantState::Committed);
    }

    #[test]
    fn test_dtx_prepare_failure_triggers_rollback() {
        let rolled_back = Arc::new(AtomicU32::new(0));
        let rb1 = rolled_back.clone();
        let rb2 = rolled_back.clone();

        let mut tx = DistributedTransaction::new("tx-fail");
        tx.add_participant(
            TransactionParticipant::new("db1")
                .with_prepare(|| Ok(()))
                .with_rollback(move || {
                    rb1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
        );
        tx.add_participant(
            TransactionParticipant::new("db2")
                .with_prepare(|| Err("db2 prepare failed".to_string()))
                .with_rollback(move || {
                    rb2.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
        );

        let result = tx.prepare();
        assert!(result.is_err());
        assert_eq!(tx.state(), TransactionState::Failed);
        // First participant prepared successfully, then should be rolled back
        assert_eq!(rolled_back.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_dtx_commit_failure() {
        let mut tx = DistributedTransaction::new("tx-commit-fail");
        tx.add_participant(
            TransactionParticipant::new("db1")
                .with_prepare(|| Ok(()))
                .with_commit(|| Ok(())),
        );
        tx.add_participant(
            TransactionParticipant::new("db2")
                .with_prepare(|| Ok(()))
                .with_commit(|| Err("commit failed".to_string())),
        );

        tx.prepare().unwrap();
        let result = tx.commit();
        assert!(result.is_err());
        assert_eq!(tx.state(), TransactionState::Failed);
    }

    #[test]
    fn test_dtx_cannot_commit_without_prepare() {
        let mut tx = DistributedTransaction::new("tx-noprepare");
        tx.add_participant(TransactionParticipant::new("db1").with_commit(|| Ok(())));
        let result = tx.commit();
        // 有 participant 但未 prepare，应返回 "Cannot commit transaction in state Active"
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("Cannot commit transaction in state"),
            "expected state error, got: {}",
            err
        );
    }

    #[test]
    fn test_dtx_rollback_after_prepare() {
        let mut tx = DistributedTransaction::new("tx-rb");
        tx.add_participant(
            TransactionParticipant::new("db1")
                .with_prepare(|| Ok(()))
                .with_rollback(|| Ok(())),
        );
        tx.prepare().unwrap();
        assert_eq!(tx.state(), TransactionState::Prepared);
        tx.rollback().unwrap();
        assert_eq!(tx.state(), TransactionState::RolledBack);
        assert_eq!(tx.participants()[0].state, ParticipantState::RolledBack);
    }

    #[test]
    fn test_manager_new() {
        let m = DtxManager::new();
        assert!(m.transactions.read().is_empty());
    }

    #[test]
    fn test_manager_begin_and_get() {
        let m = DtxManager::new();
        m.begin("tx1").unwrap();
        assert_eq!(m.get("tx1"), Some(TransactionState::Active));
        assert_eq!(m.get("missing"), None);
    }

    #[test]
    fn test_manager_begin_duplicate() {
        let m = DtxManager::new();
        m.begin("tx1").unwrap();
        let result = m.begin("tx1");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("already exists"),
            "expected 'already exists' error, got: {}",
            err
        );
    }

    #[test]
    fn test_manager_list() {
        let m = DtxManager::new();
        m.begin("tx3").unwrap();
        m.begin("tx1").unwrap();
        m.begin("tx2").unwrap();
        assert_eq!(m.list(), vec!["tx1", "tx2", "tx3"]);
    }

    #[test]
    fn test_manager_full_two_phase_flow() {
        let m = DtxManager::new();
        m.begin("tx-flow").unwrap();
        m.add_participant(
            "tx-flow",
            TransactionParticipant::new("db1")
                .with_prepare(|| Ok(()))
                .with_commit(|| Ok(())),
        )
        .unwrap();
        m.add_participant(
            "tx-flow",
            TransactionParticipant::new("db2")
                .with_prepare(|| Ok(()))
                .with_commit(|| Ok(())),
        )
        .unwrap();

        m.prepare("tx-flow").unwrap();
        assert_eq!(m.get("tx-flow"), Some(TransactionState::Prepared));

        m.commit("tx-flow").unwrap();
        assert_eq!(m.get("tx-flow"), Some(TransactionState::Committed));

        let states = m.participant_states("tx-flow").unwrap();
        assert_eq!(
            states,
            vec![ParticipantState::Committed, ParticipantState::Committed]
        );
    }

    #[test]
    fn test_manager_rollback() {
        let m = DtxManager::new();
        m.begin("tx-rb").unwrap();
        m.add_participant(
            "tx-rb",
            TransactionParticipant::new("db1").with_rollback(|| Ok(())),
        )
        .unwrap();
        m.rollback("tx-rb").unwrap();
        assert_eq!(m.get("tx-rb"), Some(TransactionState::RolledBack));
    }

    #[test]
    fn test_manager_missing_transaction() {
        let m = DtxManager::new();
        // 所有操作对不存在的 tx 都应返回 "not found" 错误
        let err_commit = m.commit("missing").unwrap_err();
        assert!(err_commit.contains("not found"), "commit: {}", err_commit);
        let err_rollback = m.rollback("missing").unwrap_err();
        assert!(
            err_rollback.contains("not found"),
            "rollback: {}",
            err_rollback
        );
        let err_prepare = m.prepare("missing").unwrap_err();
        assert!(
            err_prepare.contains("not found"),
            "prepare: {}",
            err_prepare
        );
    }
}
