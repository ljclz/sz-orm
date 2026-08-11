//! Exactly-Once 去重：通过 transaction_id 幂等去重

use std::collections::HashSet;
use std::sync::RwLock;

use super::ChangeEvent;

/// Exactly-Once 去重器
pub struct ExactlyOnceDedup {
    processed_txids: RwLock<HashSet<String>>,
    capacity: usize,
}

impl ExactlyOnceDedup {
    pub fn new() -> Self {
        Self::with_capacity(100_000)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            processed_txids: RwLock::new(HashSet::new()),
            capacity,
        }
    }

    /// 检查事件是否已处理（重复），未处理则标记为已处理
    pub fn check_and_mark(&self, event: &ChangeEvent) -> bool {
        let mut txids = self.processed_txids.write().expect("txids lock poisoned");
        if txids.len() >= self.capacity {
            txids.clear();
        }
        txids.insert(event.transaction_id.clone())
    }

    /// 检查事件是否已处理（不标记）
    pub fn is_duplicate(&self, event: &ChangeEvent) -> bool {
        let txids = self.processed_txids.read().expect("txids lock poisoned");
        txids.contains(&event.transaction_id)
    }

    /// 手动标记事务 ID 为已处理
    pub fn mark_processed(&self, txid: &str) {
        let mut txids = self.processed_txids.write().expect("txids lock poisoned");
        if txids.len() >= self.capacity {
            txids.clear();
        }
        txids.insert(txid.to_string());
    }

    /// 已处理事务数量
    pub fn processed_count(&self) -> usize {
        self.processed_txids
            .read()
            .expect("txids lock poisoned")
            .len()
    }

    /// 清空已处理记录
    pub fn clear(&self) {
        let mut txids = self.processed_txids.write().expect("txids lock poisoned");
        txids.clear();
    }
}

impl Default for ExactlyOnceDedup {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cdc::ChangeOp;

    use std::collections::HashMap;

    fn make_event(txid: &str) -> ChangeEvent {
        ChangeEvent {
            op: ChangeOp::Insert,
            before: None,
            after: Some(HashMap::new()),
            timestamp: 0,
            transaction_id: txid.to_string(),
            table: "users".to_string(),
            schema: "public".to_string(),
        }
    }

    #[test]
    fn test_dedup_first_event_not_duplicate() {
        let dedup = ExactlyOnceDedup::new();
        let event = make_event("tx-001");
        assert!(dedup.check_and_mark(&event));
        assert!(dedup.is_duplicate(&event));
        assert!(!dedup.check_and_mark(&event));
    }

    #[test]
    fn test_dedup_duplicate_event() {
        let dedup = ExactlyOnceDedup::new();
        let event = make_event("tx-001");
        dedup.check_and_mark(&event);
        assert!(!dedup.check_and_mark(&event));
        assert!(dedup.is_duplicate(&event));
    }

    #[test]
    fn test_dedup_different_txids() {
        let dedup = ExactlyOnceDedup::new();
        let e1 = make_event("tx-001");
        let e2 = make_event("tx-002");
        assert!(dedup.check_and_mark(&e1));
        assert!(dedup.check_and_mark(&e2));
        assert_eq!(dedup.processed_count(), 2);
    }

    #[test]
    fn test_dedup_capacity_eviction() {
        let dedup = ExactlyOnceDedup::with_capacity(2);
        let e1 = make_event("tx-001");
        let e2 = make_event("tx-002");
        let e3 = make_event("tx-003");

        dedup.check_and_mark(&e1);
        dedup.check_and_mark(&e2);
        dedup.check_and_mark(&e3);

        assert_eq!(dedup.processed_count(), 1);
    }

    #[test]
    fn test_dedup_clear() {
        let dedup = ExactlyOnceDedup::new();
        let event = make_event("tx-001");
        dedup.check_and_mark(&event);
        assert_eq!(dedup.processed_count(), 1);
        dedup.clear();
        assert_eq!(dedup.processed_count(), 0);
    }

    #[test]
    fn test_dedup_mark_processed() {
        let dedup = ExactlyOnceDedup::new();
        dedup.mark_processed("tx-001");
        let event = make_event("tx-001");
        assert!(dedup.is_duplicate(&event));
    }
}
