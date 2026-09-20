//! 端到端测试：LLM 推理超时降级到规则模式
//!
//! 验证当 LLM 推理超时（>3s）时，系统自动降级到规则决策模式，
//! 并记录 DEGRADED_RULE_MODE 审计标记。

use std::collections::HashMap;
use std::time::SystemTime;

use sz_orm_ai::autonomous::{AnomalyEvent, AutonomousAction, LlmAdvisor, Severity};

fn make_event(event_type: &str) -> AnomalyEvent {
    AnomalyEvent {
        event_type: event_type.to_string(),
        timestamp: SystemTime::now(),
        severity: Severity::Critical,
        context: HashMap::new(),
        event_hash: 1,
    }
}

/// 验证 LLM 顾问在无 LLM 配置时降级到规则模式
#[tokio::test]
async fn e2e_llm_degrade_to_rule_mode() {
    let event = make_event("high_cpu");
    let advisor = LlmAdvisor::new();

    let suggestion = advisor.advise(&event).await;
    assert!(
        suggestion.degraded,
        "无 LLM 配置时应降级到规则模式，degraded 应为 true"
    );
    assert!(
        suggestion.reasoning.contains("DEGRADED_RULE_MODE"),
        "降级原因应包含 DEGRADED_RULE_MODE 标记"
    );
}

/// 验证 LLM 顾问超时降级仍返回有效动作建议
#[tokio::test]
async fn e2e_llm_degrade_returns_valid_action() {
    let event = make_event("pool_exhausted");
    let advisor = LlmAdvisor::with_timeout(std::time::Duration::from_millis(1));

    let suggestion = advisor.advise(&event).await;
    assert!(suggestion.degraded, "超时应降级到规则模式");
    assert!(!suggestion.reasoning.is_empty(), "降级建议应包含推理说明");
    assert_eq!(
        suggestion.action,
        AutonomousAction::AutoRemediation,
        "pool_exhausted 应映射到 AutoRemediation"
    );
}

/// 验证 LLM 顾问对多种事件类型的降级处理
#[tokio::test]
async fn e2e_llm_degrade_multiple_event_types() {
    let advisor = LlmAdvisor::new();

    let test_cases = [
        ("high_cpu", AutonomousAction::AutoScaling),
        ("high_latency", AutonomousAction::SlowQueryGovernance),
        ("pool_exhausted", AutonomousAction::AutoRemediation),
        ("unknown_event", AutonomousAction::AutoTuning),
    ];

    for (event_type, expected_action) in test_cases {
        let event = make_event(event_type);
        let suggestion = advisor.advise(&event).await;
        assert!(suggestion.degraded, "事件 {} 应降级到规则模式", event_type);
        assert_eq!(
            suggestion.action, expected_action,
            "事件 {} 应映射到 {:?}",
            event_type, expected_action
        );
    }
}
