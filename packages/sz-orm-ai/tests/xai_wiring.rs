//! v7.9.0 AI 自治闭环生产化端到端接线测试
//!
//! 验证生产调用点可达：DecisionExplainer / PolicyArbitrator / AbTestOrchestrator / DecisionReplayer

use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use sz_orm_ai::autonomous::types::{
    ActionBoundary, AnomalyEvent, AutonomousAction, AutonomousPolicy, CircuitBreakerConfig,
    Severity, TriggerCondition,
};
use sz_orm_ai::autonomous::xai::{
    AbTestConfig, AbTestOrchestrator, ArbitrationConfig, DecisionExplainer, DecisionReplayer,
    GrayTrafficRouter, PolicyArbitrator, XaiConfig,
};
use sz_orm_ai::autonomous::AutonomousPolicyEngine;
use sz_orm_audit::AuditEntryBuilder;
use sz_orm_audit::AutonomousDecisionAuditor;

use std::time::Duration;

fn make_event(event_type: &str) -> AnomalyEvent {
    AnomalyEvent {
        event_type: event_type.to_string(),
        timestamp: SystemTime::now(),
        severity: Severity::Critical,
        context: HashMap::new(),
        event_hash: 42,
    }
}

fn make_policy(name: &str, action: AutonomousAction) -> AutonomousPolicy {
    AutonomousPolicy {
        name: name.to_string(),
        version: "1.0".to_string(),
        trigger: TriggerCondition {
            event_type: "test".to_string(),
            severity_threshold: Severity::Info,
            context_match: HashMap::new(),
        },
        action,
        boundary: ActionBoundary {
            min: 0.0,
            max: 100.0,
        },
        circuit_breaker: CircuitBreakerConfig {
            failure_threshold: 5,
            break_duration: Duration::from_secs(1800),
        },
        enabled: true,
    }
}

/// 测试 1：DecisionExplainer 真实审计记录解释全链路
#[tokio::test]
async fn test_xai_explainer_real_audit_chain() {
    let auditor = Arc::new(AutonomousDecisionAuditor::new());
    let entry = AuditEntryBuilder::new("cpu_spike", "auto_scale_policy", "AutoScaling")
        .severity("Critical")
        .reasoning("CPU > 95%，触发自动扩容 2 实例")
        .execution(true, "scaled from 4 to 6 instances")
        .verification(true)
        .build();
    let record_id = auditor.record(entry).unwrap();

    let explainer = DecisionExplainer::new(auditor, XaiConfig::default());
    let report = explainer.explain(&record_id).unwrap();

    assert!(!report.incomplete);
    assert_eq!(report.decision_id, record_id);
    assert_eq!(report.decision_basis.policy_name, "auto_scale_policy");
    assert_eq!(report.decision_basis.event_type, "cpu_spike");
    assert_eq!(report.candidate_comparison.action_type, "AutoScaling");
    assert!(report.effect_attribution.execution_success.unwrap());
    assert!(report.effect_attribution.verification_passed.unwrap());
    assert!(report.desensitized);
}

/// 测试 2：PolicyArbitrator 多策略仲裁注入 AutonomousPolicyEngine
#[tokio::test]
async fn test_xai_arbitrator_multi_policy() {
    let engine = Arc::new(AutonomousPolicyEngine::new());
    let p1 = make_policy("high_priority_scale", AutonomousAction::AutoScaling);
    let p2 = make_policy(
        "low_priority_remediation",
        AutonomousAction::AutoRemediation,
    );
    engine.load_from_config(vec![p1.clone(), p2.clone()]);

    let config = ArbitrationConfig::default()
        .with_priority("high_priority_scale", 100)
        .with_priority("low_priority_remediation", 10);
    let arbitrator = PolicyArbitrator::new(config);

    let policies = engine.policies();
    let refs: Vec<&AutonomousPolicy> = policies.iter().collect();
    let result = arbitrator.arbitrate(refs).unwrap();

    assert_eq!(result.winner.name, "high_priority_scale");
    assert_eq!(result.arbitrated.len(), 1);
    assert_eq!(result.arbitrated[0].policy_name, "low_priority_remediation");
}

/// 测试 3：AbTestOrchestrator A/B 灰度分流真实执行
#[tokio::test]
async fn test_xai_abtest_gray_routing_real() {
    let engine = Arc::new(AutonomousPolicyEngine::new());
    let router = GrayTrafficRouter::new(30);
    let config = AbTestConfig {
        gray_percentage: 30,
        shadow_verify_required: true,
        max_requests_per_candidate: 100,
    };
    let orch = AbTestOrchestrator::new(engine, router, config);

    let event = make_event("latency_degradation");
    let candidates = vec![AutonomousAction::AutoScaling, AutonomousAction::AutoTuning];
    let ab_test_id = orch.start_ab_test(&event, candidates).await.unwrap();

    let mut gray_count = 0;
    for i in 0..100 {
        if orch.is_gray_traffic(i) {
            gray_count += 1;
            orch.record_outcome(&ab_test_id, &AutonomousAction::AutoScaling, true)
                .unwrap();
        } else {
            orch.record_outcome(&ab_test_id, &AutonomousAction::AutoTuning, true)
                .unwrap();
        }
    }
    assert_eq!(gray_count, 30);

    let result = orch.query_result(&ab_test_id).unwrap();
    assert_eq!(result.candidates.len(), 2);
    let total: u32 = result.candidates.iter().map(|c| c.request_count).sum();
    assert_eq!(total, 100);
}

/// 测试 4：DecisionReplayer 历史决策回放对比
#[tokio::test]
async fn test_xai_replayer_history_replay() {
    let auditor = Arc::new(AutonomousDecisionAuditor::new());
    let entry = AuditEntryBuilder::new("disk_full", "cleanup_policy", "AutoRemediation")
        .reasoning("磁盘使用率 > 90%，触发自动清理")
        .execution(true, "cleaned 50GB")
        .verification(true)
        .build();
    let record_id = auditor.record(entry).unwrap();

    let replayer = DecisionReplayer::new(auditor);
    let count_before = replayer.auditor().entry_count();
    let report = replayer.replay(&record_id).unwrap();
    let count_after = replayer.auditor().entry_count();

    assert_eq!(count_before, count_after);
    assert_eq!(report.decision_id, record_id);
    assert_eq!(report.decision_path.event_type, "disk_full");
    assert_eq!(report.decision_path.policy_name, "cleanup_policy");
    assert!(!report.expired);
}
