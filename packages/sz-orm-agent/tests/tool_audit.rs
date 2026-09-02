//! TASK-002 验证测试：审计日志

use std::collections::HashMap;
use sz_orm_agent::tool::{AuditLog, ToolRegistry};

#[tokio::test]
async fn test_audit_records_call() {
    let registry = ToolRegistry::with_defaults();
    let mut audit = AuditLog::new();
    let params = HashMap::from([("sql".to_string(), "SELECT 1".to_string())]);

    registry
        .call("query_execution", &params, &mut audit)
        .await
        .unwrap();

    assert_eq!(audit.entries().len(), 1);
    let entry = &audit.entries()[0];
    assert_eq!(entry.tool_name, "query_execution");
    assert!(entry.success);
    assert!(!entry.hash.is_empty());
}

#[tokio::test]
async fn test_audit_hash_chain_integrity() {
    let registry = ToolRegistry::with_defaults();
    let mut audit = AuditLog::new();

    let params1 = HashMap::from([("sql".to_string(), "SELECT 1".to_string())]);
    let params2 = HashMap::from([("table".to_string(), "users".to_string())]);

    registry
        .call("query_execution", &params1, &mut audit)
        .await
        .unwrap();
    registry
        .call("stats_collection", &params2, &mut audit)
        .await
        .unwrap();

    assert_eq!(audit.entries().len(), 2);
    assert!(audit.verify(), "哈希链完整");

    assert_eq!(audit.entries()[0].prev_hash, "");
    assert_eq!(audit.entries()[1].prev_hash, audit.entries()[0].hash);
}

#[tokio::test]
async fn test_audit_records_failure() {
    let registry = ToolRegistry::with_defaults();
    let mut audit = AuditLog::new();
    let params = HashMap::new();

    let result = registry.call("query_execution", &params, &mut audit).await;

    assert!(result.is_err());
    assert_eq!(audit.entries().len(), 1);
    assert!(!audit.entries()[0].success);
}
