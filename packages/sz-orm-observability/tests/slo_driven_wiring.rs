//! 端到端接线测试：SLO 驱动自治调度全链路
//!
//! 验证 SloDrivenScaler / SloDrivenDegrader / SloDrivenRouter / SloArbitrator
//! 四个组件的完整接线，确保从 SLO 达成率到扩缩容/降级/路由/仲裁的全链路可用。

use sz_orm_observability::slo_automation::driven::{
    ArbitrationOutcome, DegradeDecision, RouteAdjustment, ScaleAction, ScaleDecision, ScalerConfig,
    SloAction, SloArbitrator, SloDrivenDegrader, SloDrivenError, SloDrivenRouter, SloDrivenScaler,
};
use sz_orm_observability::slo_automation::types::{CalcWindow, SloAchievement};

fn make_achievement(rate: f64) -> SloAchievement {
    SloAchievement {
        window: CalcWindow::TwentyFourHours,
        achievement_rate: rate,
        sufficient_data: true,
    }
}

#[tokio::test]
async fn e2e_scaler_degrader_full_pipeline() {
    // 1. 扩缩容决策：SLO 达成率 0.60 < 扩容阈值 0.95 → 触发扩容
    let scaler_config = ScalerConfig::new(10, 0.95, 0.99, 0).unwrap();
    let scaler = SloDrivenScaler::new(scaler_config).unwrap();
    let low_achievement = make_achievement(0.60);
    let scale_decision: ScaleDecision = scaler.decide(&low_achievement, 600).unwrap();
    assert_eq!(scale_decision.action, ScaleAction::ScaleUp);
    assert!(!scale_decision.in_cooldown);

    // 2. 降级决策：SLO 达成率 0.60 < 降级阈值 0.80 → 降级非核心功能
    let degrader = SloDrivenDegrader::new(Default::default());
    let degrade_decision: DegradeDecision = degrader.decide(&low_achievement).unwrap();
    assert!(!degrade_decision.degraded_features.is_empty());
    assert!(!degrade_decision.core_blocked);

    // 3. SLO 恢复：达成率 0.999 → 触发缩容
    let high_achievement = make_achievement(0.999);
    let scale_decision = scaler.decide(&high_achievement, 600).unwrap();
    assert_eq!(scale_decision.action, ScaleAction::ScaleDown);

    // 4. SLO 恢复：达成率 0.90 → 无降级
    let degrade_decision = degrader.decide(&make_achievement(0.90)).unwrap();
    assert!(degrade_decision.degraded_features.is_empty());
}

#[tokio::test]
async fn e2e_router_arbitrator_full_pipeline() {
    // 1. 路由调整：高优先级 SLO 达成率下降 → 权重增加
    let router = SloDrivenRouter::new(
        sz_orm_observability::slo_automation::driven::RouterConfig::new(
            0.9,
            vec!["slo_critical".to_string()],
        )
        .unwrap(),
    )
    .unwrap();
    let achievements = vec![
        ("slo_critical".to_string(), make_achievement(0.70)),
        ("slo_normal".to_string(), make_achievement(0.70)),
    ];
    let adjustment: RouteAdjustment = router.adjust(&achievements).unwrap();
    let critical_weight = adjustment
        .route_weights
        .iter()
        .find(|w| w.slo_id == "slo_critical")
        .unwrap()
        .weight;
    let normal_weight = adjustment
        .route_weights
        .iter()
        .find(|w| w.slo_id == "slo_normal")
        .unwrap()
        .weight;
    assert!(critical_weight > normal_weight);

    // 2. 多 SLO 仲裁：按综合得分仲裁
    let arbitrator = SloArbitrator::new();
    let actions = vec![
        SloAction {
            slo_id: "slo_critical".to_string(),
            priority: 5,
            weight: 1.0,
            historical_success_rate: 0.95,
        },
        SloAction {
            slo_id: "slo_normal".to_string(),
            priority: 2,
            weight: 1.0,
            historical_success_rate: 0.90,
        },
    ];
    let outcome: ArbitrationOutcome = arbitrator.arbitrate(&actions).unwrap();
    assert_eq!(outcome.winner, "slo_critical");
    assert_eq!(outcome.sacrificed_slos, vec!["slo_normal".to_string()]);
    assert!(outcome.rationale.contains("综合得分"));
}

#[tokio::test]
async fn e2e_error_paths() {
    // 1. 冷却期振荡防护：cooldown_minutes < 5 → 错误
    let result = ScalerConfig::new(3, 0.95, 0.99, 0);
    assert!(matches!(
        result,
        Err(SloDrivenError::InvalidCooldownMinutes(3))
    ));

    // 2. 路由阈值边界：threshold = 1.0 → 错误
    let result = sz_orm_observability::slo_automation::driven::RouterConfig::new(1.0, vec![]);
    assert!(matches!(
        result,
        Err(SloDrivenError::InvalidRouterThreshold(1.0))
    ));

    // 3. 降级核心功能保护：无非核心功能可降级 → 错误
    let degrader = SloDrivenDegrader::new(
        sz_orm_observability::slo_automation::driven::DegraderConfig {
            degrade_threshold: 0.8,
            non_core_features: vec![],
            core_features: vec!["query".to_string()],
        },
    );
    let result = degrader.decide(&make_achievement(0.60));
    assert!(matches!(result, Err(SloDrivenError::DegradeCoreBlocked(_))));

    // 4. 仲裁无法解决：得分并列 → 错误
    let arbitrator = SloArbitrator::new();
    let actions = vec![
        SloAction {
            slo_id: "slo_a".to_string(),
            priority: 2,
            weight: 1.0,
            historical_success_rate: 0.9,
        },
        SloAction {
            slo_id: "slo_b".to_string(),
            priority: 2,
            weight: 1.0,
            historical_success_rate: 0.9,
        },
    ];
    let result = arbitrator.arbitrate(&actions);
    assert!(matches!(
        result,
        Err(SloDrivenError::SloArbitrationUnresolved(_))
    ));
}
