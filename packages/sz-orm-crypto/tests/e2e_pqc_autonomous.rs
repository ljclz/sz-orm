//! 跨模块联动 e2e 测试：PQC + 自治闭环安全
//!
//! 验证 PQC 降级触发自治审计。
//! 运行：cargo test --workspace --features sz-orm-crypto/pqc-hybrid-kex,sz-orm-ai/ai-autonomous-loop --test e2e_pqc_autonomous -- --ignored

#![cfg(feature = "pqc-hybrid-kex")]

use std::collections::HashMap;
use std::time::SystemTime;

use sz_orm_ai::autonomous::{
    ActionBoundary, AnomalyEvent, AutonomousAction, AutonomousPolicy, AutonomousPolicyEngine,
    CircuitBreakerConfig, Severity, TriggerCondition,
};
use sz_orm_crypto::pqc::PqcDegradationManager;

fn make_policy(name: &str, event_type: &str) -> AutonomousPolicy {
    AutonomousPolicy {
        name: name.to_string(),
        version: "1.0".to_string(),
        trigger: TriggerCondition {
            event_type: event_type.to_string(),
            severity_threshold: Severity::Info,
            context_match: HashMap::new(),
        },
        action: AutonomousAction::AutoRemediation,
        boundary: ActionBoundary {
            min: 0.0,
            max: 100.0,
        },
        circuit_breaker: CircuitBreakerConfig::default(),
        enabled: true,
    }
}

fn make_anomaly_event(event_type: &str, hash: u64) -> AnomalyEvent {
    AnomalyEvent {
        event_type: event_type.to_string(),
        timestamp: SystemTime::now(),
        severity: Severity::Critical,
        context: HashMap::new(),
        event_hash: hash,
    }
}

#[tokio::test]
#[ignore = "跨模块联动 e2e 测试：需 --features pqc-hybrid-kex + ai-autonomous-loop"]
async fn e2e_pqc_degradation_triggers_autonomous_audit() {
    let degradation_manager = PqcDegradationManager::new();
    let engine = AutonomousPolicyEngine::new();
    engine.load_from_config(vec![make_policy("pqc_degradation_handler", "pqc_degraded")]);

    assert!(!degradation_manager.is_degraded(), "初始状态不应降级");

    degradation_manager
        .degrade("PQC 算法不可用", "TLS 握手")
        .unwrap();
    assert!(degradation_manager.is_degraded(), "降级后应标记为降级状态");
    assert_eq!(degradation_manager.log_count(), 1);

    let event = make_anomaly_event("pqc_degraded", 5001);
    let decision = engine.handle_event(&event, false).await.unwrap();
    assert_eq!(decision.policy_name, "pqc_degradation_handler");
    assert_eq!(decision.action, AutonomousAction::AutoRemediation);
    assert!(!decision.dry_run, "PQC 降级应触发实际修复动作");

    let logs = degradation_manager.get_logs();
    assert_eq!(logs[0].tag, "PQC_DEGRADED");
    assert!(logs[0].reason.contains("PQC"));
}

#[tokio::test]
#[ignore = "跨模块联动 e2e 测试：需 --features pqc-hybrid-kex + ai-autonomous-loop"]
async fn e2e_pqc_recover_clears_autonomous_alert() {
    let degradation_manager = PqcDegradationManager::new();
    let engine = AutonomousPolicyEngine::new();
    engine.load_from_config(vec![make_policy("pqc_recover_handler", "pqc_recovered")]);

    degradation_manager.degrade("临时降级", "测试").unwrap();
    assert!(degradation_manager.is_degraded());

    degradation_manager.recover();
    assert!(!degradation_manager.is_degraded(), "恢复后不应降级");

    let event = make_anomaly_event("pqc_recovered", 5002);
    let decision = engine.handle_event(&event, true).await.unwrap();
    assert!(decision.dry_run, "恢复后应 dry_run 模式");
}

#[tokio::test]
#[ignore = "跨模块联动 e2e 测试：需 --features pqc-hybrid-kex + ai-autonomous-loop"]
async fn e2e_pqc_multiple_degradations_dedup_autonomous_events() {
    let degradation_manager = PqcDegradationManager::new();
    let engine = AutonomousPolicyEngine::new();
    engine.load_from_config(vec![make_policy("pqc_dedup_handler", "pqc_degraded")]);

    degradation_manager.degrade("原因1", "scope1").unwrap();
    degradation_manager.degrade("原因2", "scope2").unwrap();
    assert_eq!(degradation_manager.log_count(), 2);

    let event = make_anomaly_event("pqc_degraded", 5003);
    let decision1 = engine.handle_event(&event, false).await.unwrap();
    assert_eq!(decision1.policy_name, "pqc_dedup_handler");

    let decision2 = engine.handle_event(&event, false).await.unwrap();
    assert_eq!(decision2.policy_name, "deduplicated", "重复事件应被去重");
}
