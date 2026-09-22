//! v8.1.0 组3：AI 闭环自治生产化端到端接线测试
//!
//! 验证生产调用点可达：
//! ① ClosedLoopScheduler 持续闭环自治监控→决策→执行→验证→调整每轮 ≤ 60s 全链路
//! ② AbStatSignificanceEngine A/B 实验统计显著性判定 p < 0.05 胜出方案
//! ③ ModelVersionCanary 模型版本灰度切换 ≤ 10s 健康检查失败回滚
//! ④ AutonomousTakeover 人工接管暂停自治切换人工模式

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use sz_orm_ai::autonomous::types::{AnomalyEvent, Severity};
use sz_orm_ai::autonomous::xai::{
    AbExperimentData, AbGroup, AbGroupData, AbStatSignificanceEngine,
};
use sz_orm_ai::autonomous::{
    AutonomousTakeover, AutonomousTarget, BoundaryConstraint, ClosedLoopScheduler,
};
use sz_orm_ai::llm_provider::{
    DefaultHealthChecker, ModelVersionCanary, ModelVersionMeta, ModelVersionRegistry,
};

fn make_event(event_type: &str) -> AnomalyEvent {
    AnomalyEvent {
        event_type: event_type.to_string(),
        timestamp: SystemTime::now(),
        severity: Severity::Warning,
        context: HashMap::new(),
        event_hash: 1,
    }
}

/// ① ClosedLoopScheduler 持续闭环自治每轮 ≤ 60s 全链路
#[tokio::test]
async fn e2e_closed_loop_scheduler_full_chain_within_60s() {
    let takeover = Arc::new(AtomicBool::new(false));
    let scheduler = ClosedLoopScheduler::new(
        AutonomousTarget::default(),
        BoundaryConstraint::default(),
        Duration::from_secs(60),
        takeover,
    );
    scheduler.start().await.unwrap();
    assert!(scheduler.is_running());

    let event = make_event("high_latency");
    let record = scheduler
        .run_one_round(
            &event,
            sz_orm_ai::autonomous::types::AutonomousAction::AutoRemediation,
            &[("fault_type".to_string(), "pool_exhausted".to_string())],
        )
        .await
        .unwrap();

    assert!(record.in_boundary);
    assert!(record.execution.is_some());
    assert!(record.verification.is_some());
    assert!(record.alert.is_none());
    assert!(
        record.duration <= Duration::from_secs(60),
        "每轮应 ≤ 60s，实际 {:?}",
        record.duration
    );
    assert_eq!(scheduler.records().len(), 1);
    scheduler.stop().await.unwrap();
    assert!(!scheduler.is_running());
}

/// ② AbStatSignificanceEngine A/B 实验统计显著性判定 p < 0.05 胜出方案
#[tokio::test]
async fn e2e_ab_significance_p_less_than_0_05_winner() {
    let engine = AbStatSignificanceEngine::new(100, 0.05);
    let experiment = AbExperimentData {
        experiment_id: "e2e_exp_1".to_string(),
        control: AbGroupData {
            group: AbGroup::Control,
            sample_size: 2000,
            successes: 800,
            failures: 1200,
        },
        treatment: AbGroupData {
            group: AbGroup::Treatment,
            sample_size: 2000,
            successes: 1000,
            failures: 1000,
        },
    };
    let result = engine.evaluate(&experiment).await.unwrap();
    match result {
        sz_orm_ai::autonomous::xai::AbSignificanceResult::Significant {
            winner,
            p_value,
            confidence,
        } => {
            assert_eq!(winner, AbGroup::Treatment);
            assert!(p_value < 0.05, "p 值应 < 0.05，实际 {}", p_value);
            assert!(confidence > 0.95, "置信度应 > 0.95，实际 {}", confidence);
        }
        other => panic!("预期显著判定，实际 {:?}", other),
    }
}

/// ③ ModelVersionCanary 模型版本灰度切换 ≤ 10s 健康检查失败回滚
#[tokio::test]
async fn e2e_model_version_canary_switch_within_10s_and_rollback() {
    let registry = Arc::new(ModelVersionRegistry::new());
    let meta_v1 = ModelVersionMeta {
        version_id: "v1".to_string(),
        model_name: "gpt-4o".to_string(),
        signature: "sig-v1-abc".to_string(),
        created_at: SystemTime::now(),
        is_canary: false,
        checksum: "sha256:v1".to_string(),
    };
    let meta_v2 = ModelVersionMeta {
        version_id: "v2".to_string(),
        model_name: "gpt-4o".to_string(),
        signature: "sig-v2-abc".to_string(),
        created_at: SystemTime::now(),
        is_canary: true,
        checksum: "sha256:v2".to_string(),
    };
    registry.register(meta_v1).await.unwrap();
    registry.register(meta_v2).await.unwrap();
    assert_eq!(registry.active_version(), Some("v1".to_string()));

    let health_checker = Arc::new(DefaultHealthChecker::new(registry.clone()));
    let canary = ModelVersionCanary::new(registry.clone(), health_checker);

    let result = canary.canary_switch("v2".to_string(), 0.1).await.unwrap();
    assert!(result.switched);
    assert!(result.health_check_passed);
    assert!(!result.rollback_executed);
    assert!(
        result.duration <= Duration::from_secs(10),
        "切换应 ≤ 10s，实际 {:?}",
        result.duration
    );
    assert_eq!(canary.active_version(), Some("v2".to_string()));
    assert!(result.audit_tag.contains("CANARY_SWITCH"));

    struct FailingHealthChecker;
    impl sz_orm_ai::llm_provider::HealthChecker for FailingHealthChecker {
        fn check(
            &self,
            version_id: &sz_orm_ai::llm_provider::ModelVersionId,
        ) -> sz_orm_ai::llm_provider::HealthCheckResult {
            sz_orm_ai::llm_provider::HealthCheckResult {
                passed: false,
                latency: Duration::from_millis(5),
                error_rate: 1.0,
                detail: format!("版本 {} 健康检查失败", version_id),
            }
        }
    }
    let canary_fail = ModelVersionCanary::new(registry.clone(), Arc::new(FailingHealthChecker));
    let rollback_result = canary_fail.canary_switch("v1".to_string(), 0.5).await;
    assert!(matches!(
        rollback_result,
        Err(sz_orm_ai::llm_provider::CanaryError::HealthCheckFailed(_))
    ));
    let history = canary_fail.history();
    assert!(history[0].rollback_executed);
    assert!(history[0].audit_tag.contains("MODEL_VERSION_ROLLBACK"));
}

/// ④ AutonomousTakeover 人工接管暂停自治切换人工模式
#[tokio::test]
async fn e2e_autonomous_takeover_pauses_loop() {
    let takeover_flag = Arc::new(AtomicBool::new(false));
    let takeover = AutonomousTakeover::new(takeover_flag.clone());

    let scheduler = ClosedLoopScheduler::new(
        AutonomousTarget::default(),
        BoundaryConstraint::default(),
        Duration::from_secs(60),
        takeover_flag.clone(),
    );
    scheduler.start().await.unwrap();
    assert!(!takeover.is_taken_over());
    assert!(!scheduler.is_taken_over());

    let record = takeover
        .takeover("ops_admin", "manual intervention for incident")
        .unwrap();
    assert_eq!(record.operator, "ops_admin");
    assert!(takeover.is_taken_over());
    assert!(scheduler.is_taken_over());

    let event = make_event("test_event");
    let result = scheduler
        .run_one_round(
            &event,
            sz_orm_ai::autonomous::types::AutonomousAction::AutoRemediation,
            &[],
        )
        .await;
    assert!(
        matches!(
            result,
            Err(sz_orm_ai::autonomous::ClosedLoopError::TakenOver)
        ),
        "接管后闭环应拒绝执行，实际 {:?}",
        result
    );

    let second_takeover = takeover.takeover("another_admin", "test");
    assert!(matches!(
        second_takeover,
        Err(sz_orm_ai::autonomous::TakeoverError::AlreadyTakenOver)
    ));

    takeover.release().unwrap();
    assert!(!takeover.is_taken_over());
    assert!(!scheduler.is_taken_over());

    let record_after = scheduler
        .run_one_round(
            &event,
            sz_orm_ai::autonomous::types::AutonomousAction::AutoRemediation,
            &[("fault_type".to_string(), "test".to_string())],
        )
        .await
        .unwrap();
    assert!(record_after.in_boundary);
    assert!(record_after.execution.is_some());
}
