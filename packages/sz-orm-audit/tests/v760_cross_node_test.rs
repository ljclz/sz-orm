//! v7.6.0 方向 4 端到端测试：跨节点审计哈希链。

#![cfg(feature = "cross-node-audit")]

use sz_orm_audit::hash_chain_enhanced::{
    AuditOpType, AuditResult, CrossNodeAuditEntry, CrossNodeHashChain, EnhancedAuditEntry,
};

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
fn v760_e2e_cross_node_multi_node_integrity() {
    let chain = CrossNodeHashChain::new();

    for i in 0..3 {
        chain
            .log(CrossNodeAuditEntry {
                node_id: format!("node-{}", i),
                epoch: i as u64,
                entry: make_entry(
                    &format!("SELECT * FROM users WHERE id = {}", i),
                    "alice",
                    "10.0.0.{i}",
                ),
            })
            .unwrap();
    }

    let result = chain.verify_all_chains();
    assert!(result.is_valid(), "跨节点链应完整");
    assert_eq!(result.total_entries, 3);
    assert_eq!(chain.node_count(), 3);
}

#[test]
fn v760_e2e_cross_node_tamper_detection() {
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
            entry: make_entry("UPDATE users SET x=1", "bob", "10.0.0.2"),
        })
        .unwrap();

    let result = chain.verify_all_chains();
    assert!(result.is_valid(), "未篡改时链应完整");
    assert_eq!(result.total_entries, 2);
    assert_eq!(chain.node_count(), 2);
}

#[test]
fn v760_e2e_cross_node_export_sorted() {
    let chain = CrossNodeHashChain::new();

    chain
        .log(CrossNodeAuditEntry {
            node_id: "node-C".to_string(),
            epoch: 3,
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
    chain
        .log(CrossNodeAuditEntry {
            node_id: "node-B".to_string(),
            epoch: 2,
            entry: make_entry("SELECT 1", "u", "1.1.1.1"),
        })
        .unwrap();

    let exported = chain.export_global_chain();
    assert_eq!(exported.len(), 3);
    assert_eq!(exported[0].node_id, "node-A");
    assert_eq!(exported[1].node_id, "node-B");
    assert_eq!(exported[2].node_id, "node-C");
}
