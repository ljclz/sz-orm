//! v7.9.0 跨存储生命周期编排端到端接线测试

use std::sync::Arc;
use std::time::SystemTime;

use sz_orm_audit::AutonomousDecisionAuditor;
use sz_orm_governance::lifecycle::cross_storage::{
    AuthContext, ComplianceEvidenceChain, ComplianceStandard, CostSimulateConfig, CostSimulator,
    CrossStorageError, DestroyOperation, EvidenceConfig, FederatedConfig, FederatedQueryRouter,
    PyramidConfig, StoragePyramid, StorageTierClassifier,
};
use sz_orm_governance::lifecycle::rule_engine::LifecycleRuleEngine;
use sz_orm_governance::lifecycle::types::{DataRange, DataTemperature};

fn make_range(temp: DataTemperature, rows: u64) -> DataRange {
    DataRange {
        min_id: 1,
        max_id: 100,
        row_count: rows,
        temperature: temp,
        last_accessed_days_ago: 30,
        access_frequency_per_day: 0.1,
    }
}

/// 测试 1：StoragePyramid 多级迁移真实执行全链路
#[tokio::test]
async fn test_cross_storage_pyramid_full_chain() {
    let classifier = StorageTierClassifier::new();
    let engine = Arc::new(LifecycleRuleEngine::new());
    let pyramid = StoragePyramid::new(classifier, engine, PyramidConfig::default());

    let ranges = vec![
        make_range(DataTemperature::Hot, 1000),
        make_range(DataTemperature::Warm, 2000),
        make_range(DataTemperature::Cold, 3000),
    ];
    let plan = pyramid.plan_migration(&ranges).unwrap();
    assert_eq!(plan.len(), 3);

    let report = pyramid.execute_migration(&plan).await.unwrap();
    assert_eq!(report.completed_steps.len(), 3);
    assert_eq!(report.total_migrated, 6000);

    let sim = CostSimulator::new(CostSimulateConfig::default());
    let stats = sz_orm_governance::lifecycle::cross_storage::cost_simulator::StorageStats {
        total_gb: 100.0,
        avg_query_per_day: 50.0,
    };
    let cost_report = sim.simulate(&plan, &stats);
    assert!(cost_report.storage_cost_delta < 0.0);
}

/// 测试 2：FederatedQueryRouter 跨存储联邦查询权限校验
#[tokio::test]
async fn test_cross_storage_federated_query_auth() {
    let router = FederatedQueryRouter::new(FederatedConfig::default());

    let auth = AuthContext {
        user_id: "analyst1".to_string(),
        roles: vec!["analyst".to_string()],
        allowed_tiers: vec!["hot".to_string(), "warm".to_string()],
    };
    let result = router
        .federated_query("SELECT * FROM orders", &auth)
        .await
        .unwrap();
    assert!(!result.partial);
    assert_eq!(result.rows.len(), 2);
    for row in &result.rows {
        assert_eq!(row.source_tier, "***");
    }

    let unauthorized_auth = AuthContext {
        user_id: "guest".to_string(),
        roles: vec!["guest".to_string()],
        allowed_tiers: vec![],
    };
    let result = router
        .federated_query("SELECT * FROM orders", &unauthorized_auth)
        .await;
    assert!(matches!(result, Err(CrossStorageError::Unauthorized(_))));
}

/// 测试 3：ComplianceEvidenceChain 销毁证据链持久化到审计
#[tokio::test]
async fn test_cross_storage_evidence_chain_audit() {
    let auditor = Arc::new(AutonomousDecisionAuditor::new());
    let chain = ComplianceEvidenceChain::new(auditor.clone(), EvidenceConfig::default());

    let destroy_op = DestroyOperation {
        operator: "compliance_admin".to_string(),
        method: "cryptographic_erase".to_string(),
        timestamp: SystemTime::now(),
    };
    let result = chain
        .generate_chain(
            "user_pii_data_001",
            &destroy_op,
            &[ComplianceStandard::Gdpr, ComplianceStandard::Pipl],
        )
        .unwrap();

    assert_eq!(result.data_id, "user_pii_data_001");
    assert_eq!(result.operator, "compliance_admin");
    assert!(result.verification.verified);
    assert_eq!(result.standards.len(), 2);

    let entries = auditor.get_entries();
    assert!(entries
        .iter()
        .any(|e| e.record_id == result.audit_record_id));
    let audit_entry = entries
        .iter()
        .find(|e| e.record_id == result.audit_record_id)
        .unwrap();
    assert_eq!(audit_entry.event_type, "data_destroy");
    assert_eq!(audit_entry.policy_name, "lifecycle_compliance");
}
