//! v7.7.0 任务 4.6：MaskingAuditLinker 端到端测试

#![cfg(feature = "masking-audit-link")]

use sz_orm_masking::{MaskingAuditLinker, MaskingOperation};

#[tokio::test]
async fn e2e_masking_audit_link() {
    let linker = MaskingAuditLinker::new();
    let op = MaskingOperation {
        field_name: "phone".to_string(),
        strategy: "phone".to_string(),
        operator: "admin".to_string(),
        timestamp: 1700000000,
        atomic_switch: true,
        audit_logged: true,
    };
    let entry = linker.link_audit(&op).await.unwrap();
    assert_eq!(entry.field(), "phone");
    assert_eq!(linker.entry_count(), 1);
}

#[tokio::test]
async fn e2e_masking_audit_multiple() {
    let linker = MaskingAuditLinker::new();
    for i in 0..3 {
        let op = MaskingOperation {
            field_name: format!("field_{}", i),
            strategy: "mask".to_string(),
            operator: "admin".to_string(),
            timestamp: 1700000000 + i,
            atomic_switch: true,
            audit_logged: true,
        };
        linker.link_audit(&op).await.unwrap();
    }
    assert_eq!(linker.entry_count(), 3);
}
