//! 策略审计日志
//!
//! 记录授权决策用于合规审计。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::policy_engine::Effect;

/// 审计记录
#[derive(Debug, Clone)]
pub struct AuditRecord {
    /// 时间戳（毫秒）
    pub timestamp_ms: u64,
    /// 用户 ID
    pub user_id: String,
    /// 操作
    pub action: String,
    /// 资源
    pub resource: String,
    /// 决策
    pub decision: Effect,
    /// 策略 ID
    pub policy_id: String,
}

/// 策略审计日志
pub struct PolicyAuditLogger {
    records: parking_lot::RwLock<Vec<AuditRecord>>,
    max_records: usize,
    total_count: AtomicU64,
}

impl PolicyAuditLogger {
    /// 创建审计日志
    pub fn new(max_records: usize) -> Self {
        Self {
            records: parking_lot::RwLock::new(Vec::new()),
            max_records,
            total_count: AtomicU64::new(0),
        }
    }

    /// 记录决策
    pub fn log(
        &self,
        user_id: &str,
        action: &str,
        resource: &str,
        decision: Effect,
        policy_id: &str,
    ) {
        let record = AuditRecord {
            timestamp_ms: current_time_ms(),
            user_id: user_id.to_string(),
            action: action.to_string(),
            resource: resource.to_string(),
            decision,
            policy_id: policy_id.to_string(),
        };
        let mut records = self.records.write();
        if records.len() >= self.max_records {
            records.remove(0);
        }
        records.push(record);
        self.total_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 获取所有记录
    pub fn records(&self) -> Vec<AuditRecord> {
        self.records.read().clone()
    }

    /// 记录数
    pub fn len(&self) -> usize {
        self.records.read().len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.records.read().is_empty()
    }

    /// 总记录数（含已淘汰）
    pub fn total_count(&self) -> u64 {
        self.total_count.load(Ordering::Relaxed)
    }

    /// 按用户查询
    pub fn filter_by_user(&self, user_id: &str) -> Vec<AuditRecord> {
        self.records
            .read()
            .iter()
            .filter(|r| r.user_id == user_id)
            .cloned()
            .collect()
    }
}

fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_and_retrieve() {
        let logger = PolicyAuditLogger::new(100);
        logger.log("user1", "read", "data", Effect::Allow, "p1");
        assert_eq!(logger.len(), 1);
        let records = logger.records();
        assert_eq!(records[0].user_id, "user1");
    }

    #[test]
    fn test_max_records_eviction() {
        let logger = PolicyAuditLogger::new(2);
        logger.log("user1", "read", "data", Effect::Allow, "p1");
        logger.log("user2", "read", "data", Effect::Allow, "p1");
        logger.log("user3", "read", "data", Effect::Allow, "p1");
        assert_eq!(logger.len(), 2);
        let records = logger.records();
        assert_eq!(records[0].user_id, "user2");
    }

    #[test]
    fn test_filter_by_user() {
        let logger = PolicyAuditLogger::new(100);
        logger.log("user1", "read", "data", Effect::Allow, "p1");
        logger.log("user2", "read", "data", Effect::Deny, "p2");
        logger.log("user1", "write", "data", Effect::Allow, "p1");
        let user1_records = logger.filter_by_user("user1");
        assert_eq!(user1_records.len(), 2);
    }

    #[test]
    fn test_total_count() {
        let logger = PolicyAuditLogger::new(1);
        logger.log("user1", "read", "data", Effect::Allow, "p1");
        logger.log("user2", "read", "data", Effect::Allow, "p1");
        assert_eq!(logger.total_count(), 2);
        assert_eq!(logger.len(), 1);
    }
}
