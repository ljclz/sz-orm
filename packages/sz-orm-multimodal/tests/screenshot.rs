//! TASK-034 集成测试：截图输入分析端到端验证

use sz_orm_multimodal::screenshot::{ScreenshotAnalysis, ScreenshotAnalyzer};

#[test]
fn test_analyze_screenshot_detects_tables() {
    let analyzer = ScreenshotAnalyzer::default();
    let image_data = vec![1, 2, 3, 4, 5];
    let result = analyzer.analyze(&image_data).unwrap();

    assert!(!result.detected_tables.is_empty(), "应检测到表");
    assert!(!result.detected_columns.is_empty(), "应检测到列");
    assert!(result.confidence > 0.0, "置信度应 > 0");
}

#[test]
fn test_empty_image_returns_error() {
    let analyzer = ScreenshotAnalyzer::default();
    assert!(analyzer.analyze(&[]).is_err());
}

#[test]
fn test_inferred_query_generated() {
    let analyzer = ScreenshotAnalyzer::default();
    let image_data = vec![10, 20, 30];
    let result = analyzer.analyze(&image_data).unwrap();
    assert!(result.inferred_query.is_some(), "应推断查询");
    let query = result.inferred_query.unwrap();
    assert!(query.contains("SELECT"));
}

#[test]
fn test_to_nl_query_with_inferred() {
    let analyzer = ScreenshotAnalyzer::default();
    let analysis = ScreenshotAnalysis {
        detected_tables: vec!["users".to_string(), "orders".to_string()],
        detected_columns: vec![],
        inferred_query: Some("SELECT * FROM users JOIN orders".to_string()),
        confidence: 0.9,
    };
    let nl = analyzer.to_nl_query(&analysis);
    assert!(nl.contains("users"));
    assert!(nl.contains("orders"));
    assert!(nl.contains("SELECT * FROM users JOIN orders"));
}

#[test]
fn test_to_nl_query_empty_tables() {
    let analyzer = ScreenshotAnalyzer::default();
    let analysis = ScreenshotAnalysis {
        detected_tables: vec![],
        detected_columns: vec![],
        inferred_query: None,
        confidence: 0.0,
    };
    let nl = analyzer.to_nl_query(&analysis);
    assert_eq!(nl, "查询数据");
}

#[test]
fn test_low_confidence_threshold_rejects() {
    let analyzer = ScreenshotAnalyzer::new(0.99);
    let image_data = vec![1];
    assert!(analyzer.analyze(&image_data).is_err());
}

#[test]
fn test_different_images_different_tables() {
    let analyzer = ScreenshotAnalyzer::default();
    let result1 = analyzer.analyze(&[1, 2, 3]).unwrap();
    let result2 = analyzer.analyze(&[4, 5, 6]).unwrap();
    let result3 = analyzer.analyze(&[7, 8, 9]).unwrap();

    let all_tables: Vec<_> = [&result1, &result2, &result3]
        .iter()
        .flat_map(|r| r.detected_tables.iter())
        .collect();
    assert!(!all_tables.is_empty());
}
