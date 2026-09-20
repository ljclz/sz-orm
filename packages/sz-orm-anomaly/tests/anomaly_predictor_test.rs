//! v7.7.0 任务 2.7：AnomalyPredictor 端到端测试
//!
//! 验证异常预测的完整流程：
//! - 容量瓶颈预测（资源使用率上升趋势）
//! - 延迟飙升预测（延迟突增模式）
//! - 故障前兆预测（错误率上升）
//! - 预测准确率 ≥ 85%
//! - 预警提前量 ≥ 30s
//! - 预测延迟 ≤ 300ms
//! - 低置信度标注 + 数据补充建议

#![cfg(feature = "anomaly-predict")]

use sz_orm_anomaly::{
    AnomalyPredictor, PredictedAnomalyType, PredictionConfig, PredictionError, TimeSeriesData,
};

fn make_history(resource: &[f64], latency: &[f64], error: &[f64]) -> Vec<TimeSeriesData> {
    resource
        .iter()
        .zip(latency.iter())
        .zip(error.iter())
        .enumerate()
        .map(|(i, ((r, l), e))| TimeSeriesData {
            timestamp: 1700000000 + i as i64 * 60,
            qps: 100.0,
            latency_ms: *l,
            error_rate: *e,
            resource_usage: *r,
        })
        .collect()
}

#[tokio::test]
async fn e2e_predict_capacity_bottleneck() {
    let predictor = AnomalyPredictor::default();
    let history = make_history(
        &[0.5, 0.7, 0.85, 0.9, 0.95],
        &[50.0, 60.0, 70.0, 80.0, 90.0],
        &[0.01, 0.01, 0.01, 0.01, 0.01],
    );
    let result = predictor.predict(&history).await.unwrap();
    assert_eq!(
        result.anomaly_type,
        PredictedAnomalyType::CapacityBottleneck
    );
    assert_eq!(result.prediction_window_minutes, 5);
    assert!(result.confidence >= 85.0);
    assert!(result.early_warning_seconds >= 30.0);
    assert!(result.prediction_latency_ms <= 300.0);
    assert!(!result.history_data_basis.is_empty());
}

#[tokio::test]
async fn e2e_predict_latency_spike() {
    let predictor = AnomalyPredictor::default();
    let history = make_history(&[0.3, 0.4, 0.5], &[50.0, 120.0, 250.0], &[0.01, 0.01, 0.01]);
    let result = predictor.predict(&history).await.unwrap();
    assert_eq!(result.anomaly_type, PredictedAnomalyType::LatencySpike);
    assert!(result.confidence >= 85.0);
}

#[tokio::test]
async fn e2e_predict_failure_precursor() {
    let predictor = AnomalyPredictor::default();
    let history = make_history(
        &[0.3, 0.3, 0.3, 0.3, 0.3],
        &[50.0, 50.0, 50.0, 50.0, 50.0],
        &[0.01, 0.03, 0.05, 0.08, 0.12],
    );
    let result = predictor.predict(&history).await.unwrap();
    assert_eq!(result.anomaly_type, PredictedAnomalyType::FailurePrecursor);
    assert!(result.confidence >= 85.0);
}

#[tokio::test]
async fn e2e_predict_insufficient_history() {
    let predictor = AnomalyPredictor::default();
    let history = make_history(&[0.5], &[50.0], &[0.01]);
    let result = predictor.predict(&history).await;
    assert!(matches!(
        result,
        Err(PredictionError::InsufficientHistory(_))
    ));
}

#[tokio::test]
async fn e2e_predict_low_confidence_flagged() {
    let config = PredictionConfig {
        min_confidence: 95.0,
        ..Default::default()
    };
    let predictor = AnomalyPredictor::new(config);
    let history = make_history(&[0.5, 0.7, 0.85], &[50.0, 60.0, 70.0], &[0.01, 0.01, 0.01]);
    let result = predictor.predict(&history).await.unwrap();
    assert!(result.low_confidence_flagged);
    assert!(result.data_suggestion.is_some());
}

#[tokio::test]
async fn e2e_predict_explainability() {
    let predictor = AnomalyPredictor::default();
    let history = make_history(
        &[0.5, 0.7, 0.85, 0.9, 0.95],
        &[50.0, 60.0, 70.0, 80.0, 90.0],
        &[0.01, 0.01, 0.01, 0.01, 0.01],
    );
    let result = predictor.predict(&history).await.unwrap();
    assert!(result.history_data_basis.contains("数据点="));
    assert!(result.history_data_basis.contains("趋势分析=线性回归"));
}
