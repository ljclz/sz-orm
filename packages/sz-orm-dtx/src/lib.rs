use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub mod cross_shard;
pub mod saga;
pub mod tcc;

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
}

impl DistributedTransaction {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            state: TransactionState::Active,
            participants: Vec::new(),
        }
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
                    return Err(format!(
                        "Prepare failed at participant {}: {}",
                        resource_id, e
                    ));
                }
            }
        }
        self.state = TransactionState::Prepared;
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
        for participant in &mut self.participants {
            if let Err(e) = participant.commit() {
                self.state = TransactionState::Failed;
                return Err(format!(
                    "Commit failed at participant {}: {}",
                    participant.resource_id, e
                ));
            }
        }
        self.state = TransactionState::Committed;
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
        for participant in &mut self.participants {
            let _ = participant.rollback();
        }
        self.state = TransactionState::RolledBack;
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
        let mut txs = self.transactions.write().unwrap();
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
        let mut txs = self.transactions.write().unwrap();
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
        let mut txs = self.transactions.write().unwrap();
        let tx = txs
            .get_mut(tx_id)
            .ok_or_else(|| format!("Transaction {} not found", tx_id))?;
        tx.prepare()
    }

    pub fn commit(&self, tx_id: &str) -> Result<(), String> {
        let mut txs = self.transactions.write().unwrap();
        let tx = txs
            .get_mut(tx_id)
            .ok_or_else(|| format!("Transaction {} not found", tx_id))?;
        tx.commit()
    }

    pub fn rollback(&self, tx_id: &str) -> Result<(), String> {
        let mut txs = self.transactions.write().unwrap();
        let tx = txs
            .get_mut(tx_id)
            .ok_or_else(|| format!("Transaction {} not found", tx_id))?;
        tx.rollback()
    }

    pub fn get(&self, tx_id: &str) -> Option<TransactionState> {
        let txs = self.transactions.read().unwrap();
        txs.get(tx_id).map(|t| t.state.clone())
    }

    pub fn list(&self) -> Vec<String> {
        let txs = self.transactions.read().unwrap();
        let mut ids: Vec<String> = txs.keys().cloned().collect();
        ids.sort();
        ids
    }

    pub fn participant_states(&self, tx_id: &str) -> Option<Vec<ParticipantState>> {
        let txs = self.transactions.read().unwrap();
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
        assert!(result.is_err());
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
        assert!(m.transactions.read().unwrap().is_empty());
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
        assert!(m.commit("missing").is_err());
        assert!(m.rollback("missing").is_err());
        assert!(m.prepare("missing").is_err());
    }
}
