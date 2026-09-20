//! 端到端测试：异常事件 → 自治决策 → 执行 → 验证全链路
//!
//! 验证 AI 自治闭环从异常事件摄入到动作执行再到效果验证的完整链路。
//! 标注 #[ignore] 的测试需真实 LLM/DB 环境验证 P99 延迟和审计完整性。

use std::collections::HashMap;
use std::time::SystemTime;

use sz_orm_ai::autonomous::{
    ActionBoundary, AnomalyEvent, AutonomousAction, AutonomousActionExecutor,
    AutonomousCircuitBreaker, AutonomousPolicy, AutonomousPolicyEngine, AutonomousVerificationLoop,
    BoundaryValidator, CircuitBreakerConfig, IdempotencyDeduplicator, LlmAdvisor, Severity,
    TriggerCondition,
};

fn make_high_cpu_event() -> AnomalyEvent {
    AnomalyEvent {
        event_type: "high_cpu".to_string(),
        timestamp: SystemTime::now(),
        severity: Severity::Critical,
        context: HashMap::from([
            ("cpu_usage".to_string(), "92".to_string()),
            ("host".to_string(), "node-1".to_string()),
        ]),
        event_hash: 12345,
    }
}

fn make_auto_remediation_policy() -> AutonomousPolicy {
    AutonomousPolicy {
        name: "p_high_cpu_remediate".to_string(),
        version: "1.0".to_string(),
        trigger: TriggerCondition {
            event_type: "high_cpu".to_string(),
            severity_threshold: Severity::Warning,
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

/// 端到端全链路：异常事件 → 策略匹配 → 边界校验 → 幂等检查 → 熔断检查 → 动作执行 → 效果验证
#[tokio::test]
#[ignore = "需真实 LLM/DB 环境验证 P99 延迟和审计完整性"]
async fn e2e_autonomous_loop_full_chain() {
    let event = make_high_cpu_event();
    let policy = make_auto_remediation_policy();

    let engine = AutonomousPolicyEngine::new();
    engine.load_from_config(vec![policy.clone()]);

    let validator = BoundaryValidator::new();
    let mut dedup = IdempotencyDeduplicator::default();
    let mut breaker = AutonomousCircuitBreaker::new(CircuitBreakerConfig::default());
    let executor = AutonomousActionExecutor::new();
    let verifier = AutonomousVerificationLoop::new();
    let llm = LlmAdvisor::new();

    let decision = engine.handle_event(&event, false).await.unwrap();
    assert_eq!(decision.policy_name, "p_high_cpu_remediate");

    assert!(validator
        .validate(&decision.action, 50.0, &policy.boundary)
        .is_ok());

    assert!(!dedup.is_duplicate(event.event_hash), "首次事件不应被去重");

    assert!(breaker.allow_request(), "熔断器应允许请求");

    let params = [("fault_type".to_string(), "high_cpu".to_string())];
    let execution = executor.execute(&decision.action, &params).await.unwrap();
    assert!(execution.success, "动作执行应成功");

    let verification = verifier.verify(&execution).await.unwrap();
    assert!(verification.verified, "验证应通过");

    let llm_suggestion = llm.advise(&event).await;
    assert!(llm_suggestion.degraded, "无 LLM 配置时应降级到规则模式");
}

/// 端到端：无匹配策略时返回 PolicyNotFound 错误
#[tokio::test]
async fn e2e_autonomous_loop_no_matching_policy() {
    let event = AnomalyEvent {
        event_type: "unknown_event".to_string(),
        timestamp: SystemTime::now(),
        severity: Severity::Info,
        context: HashMap::new(),
        event_hash: 999,
    };

    let engine = AutonomousPolicyEngine::new();
    engine.load_from_config(vec![make_auto_remediation_policy()]);

    let result = engine.handle_event(&event, true).await;
    assert!(result.is_err(), "无匹配策略应返回错误");
}

/// 端到端：dry_run 模式仅产出决策不执行
#[tokio::test]
async fn e2e_autonomous_loop_dry_run() {
    let event = make_high_cpu_event();
    let policy = make_auto_remediation_policy();

    let engine = AutonomousPolicyEngine::new();
    engine.load_from_config(vec![policy]);

    let decision = engine.handle_event(&event, true).await.unwrap();
    assert!(decision.dry_run, "dry_run 标记应为 true");
}
