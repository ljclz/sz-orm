//! v6.7.0 审计日志链式哈希增强：增强审计字段 + 写入失败处理。
//!
//! 增强字段：操作主体、操作对象、时间戳、操作类型、结果、来源 IP。
//! 写入失败时查询拒绝执行（审计优先于业务），告警 AUDIT_LOG_FAILED。

use std::sync::Mutex;

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditOpType {
    Select,
    Insert,
    Update,
    Delete,
}

impl AuditOpType {
    pub fn from_sql(sql: &str) -> Self {
        let lower = sql.trim_start().to_lowercase();
        if lower.starts_with("select") {
            AuditOpType::Select
        } else if lower.starts_with("insert") {
            AuditOpType::Insert
        } else if lower.starts_with("update") {
            AuditOpType::Update
        } else if lower.starts_with("delete") {
            AuditOpType::Delete
        } else {
            AuditOpType::Select
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditResult {
    Success,
    Failed,
}

#[derive(Debug, Clone)]
pub struct EnhancedAuditEntry {
    pub subject: String,
    pub object: String,
    pub timestamp: i64,
    pub op_type: AuditOpType,
    pub result: AuditResult,
    pub source_ip: String,
    pub sql: String,
}

#[derive(Debug, Clone)]
pub struct HashChainEnhancedEntry {
    pub entry: EnhancedAuditEntry,
    pub prev_hash: String,
    pub hash: String,
}

impl HashChainEnhancedEntry {
    pub fn genesis(entry: EnhancedAuditEntry) -> Self {
        let prev_hash = "0".repeat(64);
        let hash = Self::compute_hash(&prev_hash, &entry);
        Self {
            entry,
            prev_hash,
            hash,
        }
    }

    pub fn append(prev_hash: &str, entry: EnhancedAuditEntry) -> Self {
        let hash = Self::compute_hash(prev_hash, &entry);
        Self {
            entry,
            prev_hash: prev_hash.to_string(),
            hash,
        }
    }

    pub fn compute_hash(prev_hash: &str, entry: &EnhancedAuditEntry) -> String {
        let mut hasher = Sha256::new();
        hasher.update(prev_hash.as_bytes());
        hasher.update(entry.subject.as_bytes());
        hasher.update(entry.object.as_bytes());
        hasher.update(entry.timestamp.to_le_bytes());
        hasher.update(format!("{:?}", entry.op_type).as_bytes());
        hasher.update(format!("{:?}", entry.result).as_bytes());
        hasher.update(entry.source_ip.as_bytes());
        hasher.update(entry.sql.as_bytes());
        let result = hasher.finalize();
        result.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

pub struct HashChainEnhancedAuditor {
    entries: Mutex<Vec<HashChainEnhancedEntry>>,
    write_failed: Mutex<bool>,
}

impl HashChainEnhancedAuditor {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            write_failed: Mutex::new(false),
        }
    }

    pub fn log(&self, entry: EnhancedAuditEntry) -> Result<(), String> {
        if *self.write_failed.lock().unwrap() {
            return Err("AUDIT_LOG_FAILED: 审计日志写入失败，查询拒绝执行".to_string());
        }
        let mut entries = self.entries.lock().unwrap();
        let prev_hash = entries
            .last()
            .map(|e| e.hash.clone())
            .unwrap_or_else(|| "0".repeat(64));
        let chain_entry = if entries.is_empty() {
            HashChainEnhancedEntry::genesis(entry)
        } else {
            HashChainEnhancedEntry::append(&prev_hash, entry)
        };
        entries.push(chain_entry);
        Ok(())
    }

    pub fn verify_chain(&self) -> bool {
        let entries = self.entries.lock().unwrap();
        for i in 0..entries.len() {
            let entry = &entries[i];
            let expected_prev = if i == 0 {
                "0".repeat(64)
            } else {
                entries[i - 1].hash.clone()
            };
            if entry.prev_hash != expected_prev {
                return false;
            }
            let recomputed = HashChainEnhancedEntry::compute_hash(&entry.prev_hash, &entry.entry);
            if entry.hash != recomputed {
                return false;
            }
        }
        true
    }

    pub fn set_write_failed(&self) {
        *self.write_failed.lock().unwrap() = true;
    }

    pub fn clear_write_failed(&self) {
        *self.write_failed.lock().unwrap() = false;
    }

    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.lock().unwrap().is_empty()
    }

    pub fn get_entries(&self) -> Vec<HashChainEnhancedEntry> {
        self.entries.lock().unwrap().clone()
    }
}

impl Default for HashChainEnhancedAuditor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(sql: &str, user: &str, ip: &str) -> EnhancedAuditEntry {
        EnhancedAuditEntry {
            subject: user.to_string(),
            object: "users".to_string(),
            timestamp: 1000,
            op_type: AuditOpType::from_sql(sql),
            result: AuditResult::Success,
            source_ip: ip.to_string(),
            sql: sql.to_string(),
        }
    }

    #[test]
    fn audit_all_op_types() {
        let auditor = HashChainEnhancedAuditor::new();
        auditor
            .log(make_entry("SELECT * FROM users", "admin", "10.0.0.1"))
            .unwrap();
        auditor
            .log(make_entry(
                "INSERT INTO users VALUES(1)",
                "admin",
                "10.0.0.1",
            ))
            .unwrap();
        auditor
            .log(make_entry("UPDATE users SET name='x'", "admin", "10.0.0.1"))
            .unwrap();
        auditor
            .log(make_entry(
                "DELETE FROM users WHERE id=1",
                "admin",
                "10.0.0.1",
            ))
            .unwrap();
        assert_eq!(auditor.len(), 4);
        assert!(auditor.verify_chain());
    }

    #[test]
    fn tampered_entry_detected() {
        let auditor = HashChainEnhancedAuditor::new();
        auditor
            .log(make_entry("SELECT * FROM users", "admin", "10.0.0.1"))
            .unwrap();
        auditor
            .log(make_entry(
                "INSERT INTO logs VALUES(1)",
                "admin",
                "10.0.0.1",
            ))
            .unwrap();
        assert!(auditor.verify_chain());
        {
            let mut entries = auditor.entries.lock().unwrap();
            entries[1].entry.sql = "DROP TABLE users".to_string();
        }
        assert!(!auditor.verify_chain());
    }

    #[test]
    fn write_failed_rejects_query() {
        let auditor = HashChainEnhancedAuditor::new();
        auditor.set_write_failed();
        let result = auditor.log(make_entry("SELECT 1", "admin", "10.0.0.1"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("AUDIT_LOG_FAILED"));
    }

    #[test]
    fn write_failed_cleared_resumes() {
        let auditor = HashChainEnhancedAuditor::new();
        auditor.set_write_failed();
        auditor.clear_write_failed();
        let result = auditor.log(make_entry("SELECT 1", "admin", "10.0.0.1"));
        assert!(result.is_ok());
    }

    #[test]
    fn op_type_from_sql() {
        assert_eq!(AuditOpType::from_sql("SELECT 1"), AuditOpType::Select);
        assert_eq!(
            AuditOpType::from_sql("INSERT INTO t VALUES(1)"),
            AuditOpType::Insert
        );
        assert_eq!(
            AuditOpType::from_sql("UPDATE t SET x=1"),
            AuditOpType::Update
        );
        assert_eq!(AuditOpType::from_sql("DELETE FROM t"), AuditOpType::Delete);
    }

    #[test]
    fn empty_chain_verifies() {
        let auditor = HashChainEnhancedAuditor::new();
        assert!(auditor.verify_chain());
    }

    #[test]
    fn broken_link_detected() {
        let auditor = HashChainEnhancedAuditor::new();
        auditor
            .log(make_entry("SELECT 1", "admin", "10.0.0.1"))
            .unwrap();
        auditor
            .log(make_entry("SELECT 2", "admin", "10.0.0.1"))
            .unwrap();
        {
            let mut entries = auditor.entries.lock().unwrap();
            entries[1].prev_hash = "0".repeat(64);
        }
        assert!(!auditor.verify_chain());
    }

    #[test]
    fn different_ips_different_hashes() {
        let e1 = HashChainEnhancedEntry::genesis(make_entry("SELECT 1", "admin", "10.0.0.1"));
        let e2 = HashChainEnhancedEntry::genesis(make_entry("SELECT 1", "admin", "10.0.0.2"));
        assert_ne!(e1.hash, e2.hash);
    }
}
