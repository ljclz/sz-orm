//! TASK-024 集成测试：洞察提取端到端验证

use sz_orm_nl_query::insight::{InsightExtractor, InsightType};

#[test]
fn test_extract_trend_increasing() {
    let extractor = InsightExtractor::new();
    let rows = vec![
        serde_json::json!({"v": 10.0}),
        serde_json::json!({"v": 20.0}),
        serde_json::json!({"v": 30.0}),
        serde_json::json!({"v": 40.0}),
        serde_json::json!({"v": 50.0}),
    ];
    let insights = extractor.extract(&rows);
    let trend = insights
        .iter()
        .find(|i| i.insight_type == InsightType::Trend)
        .expect("应检测到上升趋势");
    assert!(trend.description.contains("上升"));
    assert!(trend.confidence > 0.6);
}

#[test]
fn test_extract_trend_decreasing() {
    let extractor = InsightExtractor::new();
    let rows = vec![
        serde_json::json!({"v": 50.0}),
        serde_json::json!({"v": 40.0}),
        serde_json::json!({"v": 30.0}),
        serde_json::json!({"v": 20.0}),
        serde_json::json!({"v": 10.0}),
    ];
    let insights = extractor.extract(&rows);
    let trend = insights
        .iter()
        .find(|i| i.insight_type == InsightType::Trend)
        .expect("应检测到下降趋势");
    assert!(trend.description.contains("下降"));
}

#[test]
fn test_extract_anomaly_detection() {
    let extractor = InsightExtractor::new();
    let rows = vec![
        serde_json::json!({"v": 10.0}),
        serde_json::json!({"v": 10.1}),
        serde_json::json!({"v": 9.9}),
        serde_json::json!({"v": 10.0}),
        serde_json::json!({"v": 200.0}),
        serde_json::json!({"v": 10.2}),
    ];
    let insights = extractor.extract(&rows);
    assert!(
        insights
            .iter()
            .any(|i| i.insight_type == InsightType::Anomaly),
        "应检测到异常值 200.0"
    );
}

#[test]
fn test_extract_topn_finds_max() {
    let extractor = InsightExtractor::new();
    let rows = vec![
        serde_json::json!({"v": 15.0}),
        serde_json::json!({"v": 42.0}),
        serde_json::json!({"v": 7.0}),
    ];
    let insights = extractor.extract(&rows);
    let topn = insights
        .iter()
        .find(|i| i.insight_type == InsightType::TopN)
        .unwrap();
    assert!(topn.description.contains("42.00"));
}

#[test]
fn test_extract_proportion_dominant() {
    let extractor = InsightExtractor::new();
    let rows = vec![
        serde_json::json!({"v": 80.0}),
        serde_json::json!({"v": 10.0}),
        serde_json::json!({"v": 10.0}),
    ];
    let insights = extractor.extract(&rows);
    assert!(
        insights
            .iter()
            .any(|i| i.insight_type == InsightType::Proportion),
        "80 应占主导"
    );
}

#[test]
fn test_empty_data_no_insights() {
    let extractor = InsightExtractor::new();
    let insights = extractor.extract(&[]);
    assert!(insights.is_empty());
}
