//! TASK-014 验证测试：图表自动生成

use sz_orm_multimodal::chart::ChartGenerator;

#[test]
fn test_chart_generator_default() {
    let generator = ChartGenerator::new();
    let data = serde_json::json!([{"category": "A", "value": 100}]);
    let spec = generator.generate(&data).unwrap();
    assert!(!spec.chart_type.is_empty());
}

#[test]
fn test_chart_generator_fallback_table() {
    let generator = ChartGenerator;
    let data = serde_json::json!([]);
    let spec = generator.generate(&data).unwrap();
    assert_eq!(spec.chart_type, "table", "空数据降级为表格");
}
