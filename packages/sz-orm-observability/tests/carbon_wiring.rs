//! 端到端接线测试：碳中和路径规划全链路
//!
//! 验证 CarbonReductionTarget / CarbonOffsetAdvisor / CarbonNeutralityForecaster / GreenRlOptimizer
//! 四个组件的完整接线，确保从目标设定到策略学习的全链路可用。

use std::collections::HashMap;

use sz_orm_observability::green::carbon::{
    CarbonError, CarbonNeutralityForecaster, CarbonOffsetAdvisor, CarbonReductionTarget,
    GreenRlOptimizer,
};
use sz_orm_observability::green::{CarbonFootprint, ExportFormat, ReportPeriod};

fn make_footprint(total: f64) -> CarbonFootprint {
    let mut by_component = HashMap::new();
    by_component.insert("cpu".to_string(), total * 0.5);
    by_component.insert("memory".to_string(), total * 0.2);
    by_component.insert("io".to_string(), total * 0.2);
    by_component.insert("network".to_string(), total * 0.1);
    CarbonFootprint {
        period: ReportPeriod::Daily,
        total_kgco2e: total,
        by_component,
    }
}

#[tokio::test]
async fn e2e_carbon_neutrality_full_pipeline() {
    // 1. 设定碳减排目标：基线 1000 kgCO2e，目标 400 kgCO2e
    let now = chrono::Utc::now().timestamp();
    let mut target = CarbonReductionTarget::new(1000.0, now - 100 * 86400);
    target.set_target(400.0, now + 265 * 86400);

    // 2. 更新进度：当前排放 600 kgCO2e
    let footprint = make_footprint(600.0);
    let progress = target.update_progress(&footprint);
    assert!(progress.current_reduction > 0.0, "应有正减排量");
    assert!(progress.achievement_rate > 0.0 && progress.achievement_rate < 1.0);
    assert!(progress.data_gaps.is_empty(), "四组件齐全不应有数据空洞");

    // 3. 导出 Prometheus 格式
    let prom = target.export(ExportFormat::Prometheus).unwrap();
    assert!(prom.contains("sz_orm_carbon_"));
    assert!(prom.contains("# TYPE sz_orm_carbon_achievement_rate gauge"));

    // 4. 碳抵消建议：不可避免排放 100 kgCO2e
    let advisor = CarbonOffsetAdvisor::new(50.0, "VCS");
    let suggestion = advisor.advise(100.0).unwrap();
    assert!(!suggestion.executable, "建议不执行交易");

    // 5. 预测未来排放
    let forecaster = CarbonNeutralityForecaster::new();
    let history: Vec<CarbonFootprint> = (1..=10)
        .map(|i| make_footprint(1000.0 - i as f64 * 60.0))
        .collect();
    let report = forecaster.forecast(&history).unwrap();
    assert!(report.optimistic.predicted_kgco2e <= report.neutral.predicted_kgco2e);
    assert!(report.neutral.predicted_kgco2e <= report.pessimistic.predicted_kgco2e);

    // 6. 强化学习策略
    let optimizer = GreenRlOptimizer::new();
    let rl_history: Vec<CarbonFootprint> = (0..30)
        .map(|i| make_footprint(800.0 - i as f64 * 15.0))
        .collect();
    let strategy = optimizer.learn(&rl_history).await.unwrap();
    assert!(!strategy.suggested_strategy.is_empty());
    assert!(strategy.expected_reduction > 0.0);
}

#[tokio::test]
async fn e2e_carbon_neutrality_error_paths() {
    // 1. 碳抵消市场数据不可用
    let advisor = CarbonOffsetAdvisor::new(50.0, "VCS").with_market_unavailable();
    let result = advisor.advise(100.0);
    assert!(matches!(result, Err(CarbonError::OffsetDataUnavailable)));

    // 2. 预测历史数据不足
    let forecaster = CarbonNeutralityForecaster::new();
    let result = forecaster.forecast(&[make_footprint(500.0)]);
    assert!(matches!(
        result,
        Err(CarbonError::ForecastDataInsufficient { actual: 1 })
    ));

    // 3. 强化学习数据不足（< 30 天）
    let optimizer = GreenRlOptimizer::new();
    let short_history: Vec<CarbonFootprint> = (0..10).map(|_| make_footprint(500.0)).collect();
    let result = optimizer.learn(&short_history).await;
    assert!(matches!(
        result,
        Err(CarbonError::RlDataInsufficient { actual: 10 })
    ));
}
