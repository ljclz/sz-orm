//! M16.9：哈希链审计端到端测试

#[cfg(feature = "hash-chain-enhanced")]
mod tests {
    use sz_orm_audit::hash_chain_enhanced::{AuditorWithHashChain, HashChainEnhancedAuditor};
    use sz_orm_audit::{AuditEnhanceConfig, SqlAuditContext, SqlAuditor};

    #[tokio::test]
    async fn test_hash_chain_audit_e2e() {
        let inner = SqlAuditor::new();
        let hash_chain = HashChainEnhancedAuditor::new();
        let config = AuditEnhanceConfig {
            hash_chain: true,
            ..AuditEnhanceConfig::new()
        };
        let auditor = AuditorWithHashChain::new(inner, hash_chain, config);

        let ctx = SqlAuditContext {
            sql: "SELECT * FROM users WHERE id = 1".to_string(),
            user: "admin".to_string(),
            timestamp: 1000,
        };
        auditor.log(&ctx).unwrap();
        assert!(auditor.verify_chain());
        assert_eq!(auditor.hash_chain().len(), 1);

        let ctx2 = SqlAuditContext {
            sql: "UPDATE users SET name = 'Bob' WHERE id = 1".to_string(),
            user: "admin".to_string(),
            timestamp: 2000,
        };
        auditor.log(&ctx2).unwrap();
        assert!(auditor.verify_chain());
        assert_eq!(auditor.hash_chain().len(), 2);

        let entries = auditor.hash_chain().get_entries();
        assert_ne!(entries[0].hash, entries[1].hash, "不同条目应有不同 hash");
        assert_eq!(
            entries[1].prev_hash, entries[0].hash,
            "第二条目的 prev_hash 应等于第一条目的 hash"
        );

        let empty_auditor = HashChainEnhancedAuditor::new();
        assert!(empty_auditor.verify_chain(), "空链应验证通过");
        assert_eq!(empty_auditor.len(), 0);
    }
}
