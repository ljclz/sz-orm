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
    /// v7.5.0 DDL 操作（CREATE / ALTER / DROP / TRUNCATE）
    Ddl,
    /// v7.5.0 权限变更（GRANT / REVOKE）
    PermissionChange,
    /// v7.5.0 脱敏配置变更（MASKING_RULE_UPDATE）
    MaskingConfigChange,
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
        } else if lower.starts_with("create")
            || lower.starts_with("alter")
            || lower.starts_with("drop")
            || lower.starts_with("truncate")
        {
            AuditOpType::Ddl
        } else if lower.starts_with("grant") || lower.starts_with("revoke") {
            AuditOpType::PermissionChange
        } else if lower.starts_with("masking_rule_update") {
            AuditOpType::MaskingConfigChange
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

    // ----- v7.5.0 AuditOpType 扩展测试 -----

    #[test]
    fn op_type_ddl_create() {
        assert_eq!(
            AuditOpType::from_sql("CREATE TABLE users (id INT)"),
            AuditOpType::Ddl
        );
    }

    #[test]
    fn op_type_ddl_alter() {
        assert_eq!(
            AuditOpType::from_sql("ALTER TABLE users ADD COLUMN name TEXT"),
            AuditOpType::Ddl
        );
    }

    #[test]
    fn op_type_ddl_drop() {
        assert_eq!(AuditOpType::from_sql("DROP TABLE users"), AuditOpType::Ddl);
    }

    #[test]
    fn op_type_ddl_truncate() {
        assert_eq!(
            AuditOpType::from_sql("TRUNCATE TABLE logs"),
            AuditOpType::Ddl
        );
    }

    #[test]
    fn op_type_permission_grant() {
        assert_eq!(
            AuditOpType::from_sql("GRANT SELECT ON users TO analyst"),
            AuditOpType::PermissionChange
        );
    }

    #[test]
    fn op_type_permission_revoke() {
        assert_eq!(
            AuditOpType::from_sql("REVOKE SELECT ON users FROM analyst"),
            AuditOpType::PermissionChange
        );
    }

    #[test]
    fn op_type_masking_config_change() {
        assert_eq!(
            AuditOpType::from_sql("MASKING_RULE_UPDATE phone SET strategy=hash"),
            AuditOpType::MaskingConfigChange
        );
    }

    #[test]
    fn op_type_ddl_case_insensitive() {
        assert_eq!(
            AuditOpType::from_sql("create table foo (id int)"),
            AuditOpType::Ddl
        );
        assert_eq!(AuditOpType::from_sql("  DROP TABLE foo"), AuditOpType::Ddl);
    }

    #[test]
    fn audit_ddl_chain_integrity() {
        let auditor = HashChainEnhancedAuditor::new();
        auditor
            .log(make_entry("CREATE TABLE foo (id INT)", "admin", "10.0.0.1"))
            .unwrap();
        auditor
            .log(make_entry(
                "GRANT SELECT ON foo TO bob",
                "admin",
                "10.0.0.1",
            ))
            .unwrap();
        auditor
            .log(make_entry(
                "MASKING_RULE_UPDATE phone SET strategy=hash",
                "admin",
                "10.0.0.1",
            ))
            .unwrap();
        assert_eq!(auditor.len(), 3);
        assert!(auditor.verify_chain());
    }

    #[test]
    fn audit_ddl_tamper_detected() {
        let auditor = HashChainEnhancedAuditor::new();
        auditor
            .log(make_entry("CREATE TABLE foo (id INT)", "admin", "10.0.0.1"))
            .unwrap();
        auditor
            .log(make_entry("DROP TABLE foo", "admin", "10.0.0.1"))
            .unwrap();
        assert!(auditor.verify_chain());
        {
            let mut entries = auditor.entries.lock().unwrap();
            entries[0].entry.sql = "SELECT * FROM foo".to_string();
        }
        assert!(!auditor.verify_chain());
    }
}
// ============================================================================
// v7.6.0 跨节点审计哈希链
// ============================================================================

/// 跨节点审计条目
#[derive(Debug, Clone)]
pub struct CrossNodeAuditEntry {
    pub node_id: String,
    pub epoch: u64,
    pub entry: EnhancedAuditEntry,
}

/// 跨节点哈希链条目
#[derive(Debug, Clone)]
pub struct CrossNodeHashEntry {
    pub node_id: String,
    pub epoch: u64,
    pub audit_entry: EnhancedAuditEntry,
    pub local_prev_hash: String,
    pub local_hash: String,
    pub cross_node_prev_hash: String,
    pub global_hash: String,
}

impl CrossNodeHashEntry {
    fn compute_local_hash(prev_hash: &str, entry: &EnhancedAuditEntry) -> String {
        HashChainEnhancedEntry::compute_hash(prev_hash, entry)
    }

    fn compute_global_hash(
        cross_node_prev_hash: &str,
        node_id: &str,
        epoch: u64,
        local_hash: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(cross_node_prev_hash.as_bytes());
        hasher.update(node_id.as_bytes());
        hasher.update(epoch.to_le_bytes());
        hasher.update(local_hash.as_bytes());
        let result = hasher.finalize();
        result.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

/// 跨节点哈希链协调器
pub struct CrossNodeHashChain {
    node_chains: Mutex<std::collections::HashMap<String, Vec<CrossNodeHashEntry>>>,
    last_global_hash: Mutex<String>,
}

impl CrossNodeHashChain {
    pub fn new() -> Self {
        Self {
            node_chains: Mutex::new(std::collections::HashMap::new()),
            last_global_hash: Mutex::new("0".repeat(64)),
        }
    }

    pub fn log(&self, cross_entry: CrossNodeAuditEntry) -> Result<(), String> {
        let mut chains = self.node_chains.lock().unwrap();
        let node_chain = chains.entry(cross_entry.node_id.clone()).or_default();

        let local_prev_hash = node_chain
            .last()
            .map(|e| e.local_hash.clone())
            .unwrap_or_else(|| "0".repeat(64));

        let local_hash =
            CrossNodeHashEntry::compute_local_hash(&local_prev_hash, &cross_entry.entry);

        let cross_node_prev_hash = {
            let last_global = self.last_global_hash.lock().unwrap();
            last_global.clone()
        };

        let global_hash = CrossNodeHashEntry::compute_global_hash(
            &cross_node_prev_hash,
            &cross_entry.node_id,
            cross_entry.epoch,
            &local_hash,
        );

        let hash_entry = CrossNodeHashEntry {
            node_id: cross_entry.node_id.clone(),
            epoch: cross_entry.epoch,
            audit_entry: cross_entry.entry,
            local_prev_hash,
            local_hash,
            cross_node_prev_hash,
            global_hash: global_hash.clone(),
        };

        node_chain.push(hash_entry);

        *self.last_global_hash.lock().unwrap() = global_hash;

        Ok(())
    }

    pub fn verify_all_chains(&self) -> CrossNodeVerifyResult {
        let chains = self.node_chains.lock().unwrap();
        let mut errors = Vec::new();
        let mut total_entries = 0usize;
        let mut verified_entries = 0usize;

        let mut expected_global = "0".repeat(64);

        let mut all_entries: Vec<&CrossNodeHashEntry> = Vec::new();
        for chain in chains.values() {
            for entry in chain {
                all_entries.push(entry);
            }
        }
        all_entries.sort_by(|a, b| {
            a.epoch
                .cmp(&b.epoch)
                .then_with(|| a.node_id.cmp(&b.node_id))
        });

        for entry in all_entries {
            total_entries += 1;
            let recomputed_local =
                CrossNodeHashEntry::compute_local_hash(&entry.local_prev_hash, &entry.audit_entry);
            if entry.local_hash != recomputed_local {
                errors.push(format!(
                    "节点 {} epoch {} 本地哈希不匹配",
                    entry.node_id, entry.epoch
                ));
                continue;
            }
            if entry.cross_node_prev_hash != expected_global {
                errors.push(format!(
                    "节点 {} epoch {} 跨节点前驱哈希不匹配",
                    entry.node_id, entry.epoch
                ));
                continue;
            }
            let recomputed_global = CrossNodeHashEntry::compute_global_hash(
                &entry.cross_node_prev_hash,
                &entry.node_id,
                entry.epoch,
                &entry.local_hash,
            );
            if entry.global_hash != recomputed_global {
                errors.push(format!(
                    "节点 {} epoch {} 全局哈希不匹配",
                    entry.node_id, entry.epoch
                ));
                continue;
            }
            expected_global = entry.global_hash.clone();
            verified_entries += 1;
        }

        CrossNodeVerifyResult {
            total_entries,
            verified_entries,
            tamper_detected: verified_entries != total_entries,
            errors,
        }
    }

    pub fn node_count(&self) -> usize {
        self.node_chains.lock().unwrap().len()
    }

    pub fn total_entries(&self) -> usize {
        self.node_chains
            .lock()
            .unwrap()
            .values()
            .map(|c| c.len())
            .sum()
    }

    pub fn get_node_chain(&self, node_id: &str) -> Vec<CrossNodeHashEntry> {
        self.node_chains
            .lock()
            .unwrap()
            .get(node_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn export_global_chain(&self) -> Vec<CrossNodeHashEntry> {
        let chains = self.node_chains.lock().unwrap();
        let mut all_entries: Vec<CrossNodeHashEntry> = Vec::new();
        for chain in chains.values() {
            for entry in chain {
                all_entries.push(entry.clone());
            }
        }
        all_entries.sort_by(|a, b| {
            a.epoch
                .cmp(&b.epoch)
                .then_with(|| a.node_id.cmp(&b.node_id))
        });
        all_entries
    }
}

impl Default for CrossNodeHashChain {
    fn default() -> Self {
        Self::new()
    }
}

/// 跨节点验证结果
#[derive(Debug, Clone)]
pub struct CrossNodeVerifyResult {
    pub total_entries: usize,
    pub verified_entries: usize,
    pub tamper_detected: bool,
    pub errors: Vec<String>,
}

impl CrossNodeVerifyResult {
    pub fn is_valid(&self) -> bool {
        !self.tamper_detected
    }
}

#[cfg(test)]
mod v760_cross_node_tests {
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
    fn cross_node_single_node_chain() {
        let chain = CrossNodeHashChain::new();
        chain
            .log(CrossNodeAuditEntry {
                node_id: "node-A".to_string(),
                epoch: 1,
                entry: make_entry("SELECT * FROM users", "alice", "10.0.0.1"),
            })
            .unwrap();
        chain
            .log(CrossNodeAuditEntry {
                node_id: "node-A".to_string(),
                epoch: 2,
                entry: make_entry("INSERT INTO users VALUES (1)", "bob", "10.0.0.2"),
            })
            .unwrap();
        let result = chain.verify_all_chains();
        assert!(result.is_valid());
        assert_eq!(result.total_entries, 2);
        assert_eq!(chain.node_count(), 1);
    }

    #[test]
    fn cross_node_multi_node_chain() {
        let chain = CrossNodeHashChain::new();
        chain
            .log(CrossNodeAuditEntry {
                node_id: "node-A".to_string(),
                epoch: 1,
                entry: make_entry("SELECT * FROM users", "alice", "10.0.0.1"),
            })
            .unwrap();
        chain
            .log(CrossNodeAuditEntry {
                node_id: "node-B".to_string(),
                epoch: 2,
                entry: make_entry("UPDATE users SET name='x'", "carol", "10.0.0.3"),
            })
            .unwrap();
        chain
            .log(CrossNodeAuditEntry {
                node_id: "node-A".to_string(),
                epoch: 3,
                entry: make_entry("DELETE FROM users WHERE id=1", "alice", "10.0.0.1"),
            })
            .unwrap();
        let result = chain.verify_all_chains();
        assert!(result.is_valid());
        assert_eq!(result.total_entries, 3);
        assert_eq!(chain.node_count(), 2);
    }

    #[test]
    fn cross_node_tamper_detected() {
        let chain = CrossNodeHashChain::new();
        chain
            .log(CrossNodeAuditEntry {
                node_id: "node-A".to_string(),
                epoch: 1,
                entry: make_entry("SELECT * FROM users", "alice", "10.0.0.1"),
            })
            .unwrap();
        chain
            .log(CrossNodeAuditEntry {
                node_id: "node-B".to_string(),
                epoch: 2,
                entry: make_entry("UPDATE users SET name='x'", "carol", "10.0.0.3"),
            })
            .unwrap();
        {
            let mut chains = chain.node_chains.lock().unwrap();
            let node_b = chains.get_mut("node-B").unwrap();
            node_b[0].audit_entry.sql = "DROP TABLE users".to_string();
        }
        let result = chain.verify_all_chains();
        assert!(!result.is_valid());
        assert!(result.tamper_detected);
    }

    #[test]
    fn cross_node_export_global_chain_sorted() {
        let chain = CrossNodeHashChain::new();
        chain
            .log(CrossNodeAuditEntry {
                node_id: "node-B".to_string(),
                epoch: 2,
                entry: make_entry("SELECT 1", "u", "1.1.1.1"),
            })
            .unwrap();
        chain
            .log(CrossNodeAuditEntry {
                node_id: "node-A".to_string(),
                epoch: 1,
                entry: make_entry("SELECT 1", "u", "1.1.1.1"),
            })
            .unwrap();
        let exported = chain.export_global_chain();
        assert_eq!(exported[0].node_id, "node-A");
        assert_eq!(exported[1].node_id, "node-B");
    }

    #[test]
    fn cross_node_empty_chain_valid() {
        let chain = CrossNodeHashChain::new();
        let result = chain.verify_all_chains();
        assert!(result.is_valid());
        assert_eq!(result.total_entries, 0);
    }

    #[test]
    fn cross_node_get_node_chain() {
        let chain = CrossNodeHashChain::new();
        chain
            .log(CrossNodeAuditEntry {
                node_id: "node-A".to_string(),
                epoch: 1,
                entry: make_entry("SELECT 1", "u", "1.1.1.1"),
            })
            .unwrap();
        chain
            .log(CrossNodeAuditEntry {
                node_id: "node-B".to_string(),
                epoch: 2,
                entry: make_entry("SELECT 2", "u", "1.1.1.1"),
            })
            .unwrap();
        let node_a = chain.get_node_chain("node-A");
        let node_b = chain.get_node_chain("node-B");
        let node_c = chain.get_node_chain("node-C");
        assert_eq!(node_a.len(), 1);
        assert_eq!(node_b.len(), 1);
        assert_eq!(node_c.len(), 0);
    }

    #[test]
    fn cross_node_total_entries() {
        let chain = CrossNodeHashChain::new();
        for i in 0..5 {
            chain
                .log(CrossNodeAuditEntry {
                    node_id: "node-A".to_string(),
                    epoch: i,
                    entry: make_entry("SELECT 1", "u", "1.1.1.1"),
                })
                .unwrap();
        }
        for i in 0..3 {
            chain
                .log(CrossNodeAuditEntry {
                    node_id: "node-B".to_string(),
                    epoch: i,
                    entry: make_entry("SELECT 1", "u", "1.1.1.1"),
                })
                .unwrap();
        }
        assert_eq!(chain.total_entries(), 8);
        assert_eq!(chain.node_count(), 2);
    }
}
