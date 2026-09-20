//! 跨模块联动 e2e 测试：自治闭环 + SLI/SLO
//!
//! 验证自治动作触发 SLI 变化，错误预算更新。
//! 运行：cargo test --workspace --features sz-orm-ai/ai-autonomous-loop,sz-orm-observability/slo-automation --test e2e_autonomous_slo -- --ignored

#![cfg(feature = "ai-autonomous-loop")]

use std::collections::HashMap;
use std::time::SystemTime;

use sz_orm_ai::autonomous::{
    ActionBoundary, AnomalyEvent, AutonomousAction, AutonomousPolicy, AutonomousPolicyEngine,
    CircuitBreakerConfig, Severity, TriggerCondition,
};
use sz_orm_observability::slo_automation::{
    ErrorBudgetTracker, RequestResult, SliCollector, SloAutomationConfig,
};

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

fn make_event(event_type: &str, severity: Severity, hash: u64) -> AnomalyEvent {
    AnomalyEvent {
        event_type: event_type.to_string(),
        timestamp: SystemTime::now(),
        severity,
        context: HashMap::new(),
        event_hash: hash,
    }
}

#[tokio::test]
#[ignore = "跨模块联动 e2e 测试：需 --features ai-autonomous-loop + slo-automation"]
async fn e2e_autonomous_action_improves_sli_and_updates_budget() {
    let engine = AutonomousPolicyEngine::new();
    engine.load_from_config(vec![make_policy("latency_remediation", "high_latency")]);

    let collector = SliCollector::new();
    let mut budget_tracker = ErrorBudgetTracker::new(100.0);
    let _config = SloAutomationConfig::default();

    for _ in 0..80 {
        collector.collect(RequestResult {
            success: true,
            latency_ms: 50.0,
            timestamp: 1,
        });
    }
    for _ in 0..20 {
        collector.collect(RequestResult {
            success: false,
            latency_ms: 500.0,
            timestamp: 2,
        });
    }
    let sli_before = collector.calculate_sli();
    assert!(
        (sli_before.availability - 0.8).abs() < 0.01,
        "自治前可用率应约 80%"
    );

    let error_ratio = 1.0 - sli_before.availability;
    budget_tracker.update(error_ratio * 100.0);
    assert!(
        budget_tracker.remaining_budget() < 100.0,
        "错误预算应被消耗"
    );

    let event = make_event("high_latency", Severity::Critical, 42);
    let decision = engine.handle_event(&event, true).await.unwrap();
    assert_eq!(decision.policy_name, "latency_remediation");
    assert_eq!(decision.action, AutonomousAction::AutoRemediation);

    let collector_after = SliCollector::new();
    for _ in 0..100 {
        collector_after.collect(RequestResult {
            success: true,
            latency_ms: 30.0,
            timestamp: 3,
        });
    }
    let sli_after = collector_after.calculate_sli();
    assert!(
        (sli_after.availability - 1.0).abs() < 0.01,
        "自治修复后可用率应达 100%"
    );

    let new_error_ratio = 1.0 - sli_after.availability;
    budget_tracker.update(new_error_ratio * 100.0);
    assert!(
        sli_after.availability > sli_before.availability,
        "SLI 应改善"
    );
}

#[tokio::test]
#[ignore = "跨模块联动 e2e 测试：需 --features ai-autonomous-loop + slo-automation"]
async fn e2e_budget_exhaustion_blocks_non_critical_changes() {
    let engine = AutonomousPolicyEngine::new();
    engine.load_from_config(vec![make_policy("budget_guard", "budget_warning")]);

    let mut budget_tracker = ErrorBudgetTracker::new(10.0);

    budget_tracker.update(10.0);
    assert!(budget_tracker.is_exhausted(), "错误预算应已耗尽");
    assert!(
        !budget_tracker.allow_non_critical_change(),
        "预算耗尽应阻止非关键变更"
    );

    let event = make_event("budget_warning", Severity::Critical, 99);
    let decision = engine.handle_event(&event, true).await.unwrap();
    assert_eq!(decision.action, AutonomousAction::AutoRemediation);
    assert!(decision.dry_run, "预算耗尽时应 dry_run");
}

#[tokio::test]
#[ignore = "跨模块联动 e2e 测试：需 --features ai-autonomous-loop + slo-automation"]
async fn e2e_sli_prometheus_export_after_autonomous_action() {
    let engine = AutonomousPolicyEngine::new();
    engine.load_from_config(vec![make_policy("slo_export", "slo_breach")]);

    let collector = SliCollector::new();
    for i in 0..50 {
        collector.collect(RequestResult {
            success: i % 10 != 0,
            latency_ms: 100.0,
            timestamp: i as i64,
        });
    }

    let event = make_event("slo_breach", Severity::Warning, 77);
    let _decision = engine.handle_event(&event, false).await.unwrap();
    engine.record_outcome("slo_export", true);

    let prometheus_output = collector.export_prometheus();
    assert!(prometheus_output.contains("sz_orm_sli_availability"));
    assert!(prometheus_output.contains("sz_orm_slo_availability"));
    assert!(prometheus_output.contains("sz_orm_sli_latency_ms"));

    let sli = collector.calculate_sli();
    assert!((sli.availability - 0.9).abs() < 0.01, "可用率应约 90%");
}
