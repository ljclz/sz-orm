//! TASK-023 集成测试：可视化选择器端到端验证

use sz_orm_nl_query::visualizer::{
    BarVisualizer, ChartType, LineVisualizer, PieVisualizer, ScatterVisualizer, TableVisualizer,
    VisualizationIntent, Visualizer, VisualizerSelector,
};

#[test]
fn test_selector_picks_line_for_time_series() {
    let selector = VisualizerSelector::new();
    let intent = VisualizationIntent {
        primary_type: ChartType::Line,
        has_time_axis: true,
        has_category: false,
        has_measure: true,
        row_count: 100,
    };
    let data = serde_json::json!([{"date": "2024-01", "value": 100}]);
    let spec = selector.select(&data, &intent).unwrap();
    assert_eq!(spec.chart_type, "line");
}

#[test]
fn test_selector_picks_bar_for_category_measure() {
    let selector = VisualizerSelector::new();
    let intent = VisualizationIntent {
        primary_type: ChartType::Bar,
        has_time_axis: false,
        has_category: true,
        has_measure: true,
        row_count: 20,
    };
    let data = serde_json::json!([{"cat": "A", "val": 10}]);
    let spec = selector.select(&data, &intent).unwrap();
    assert_eq!(spec.chart_type, "bar");
}

#[test]
fn test_selector_picks_pie_for_small_dataset() {
    let selector = VisualizerSelector::new();
    let intent = VisualizationIntent {
        primary_type: ChartType::Pie,
        has_time_axis: false,
        has_category: true,
        has_measure: true,
        row_count: 5,
    };
    let data = serde_json::json!([{"cat": "A", "val": 10}]);
    let spec = selector.select(&data, &intent).unwrap();
    assert_eq!(spec.chart_type, "pie");
}

#[test]
fn test_selector_falls_back_to_table() {
    let selector = VisualizerSelector::new();
    let intent = VisualizationIntent::default();
    let data = serde_json::json!([]);
    let spec = selector.select(&data, &intent).unwrap();
    assert_eq!(spec.chart_type, "table");
}

#[test]
fn test_individual_visualizers() {
    let data = serde_json::json!([{"x": 1}]);
    let intent = VisualizationIntent::default();

    let bar = BarVisualizer;
    let line = LineVisualizer;
    let pie = PieVisualizer;
    let scatter = ScatterVisualizer;
    let table = TableVisualizer;

    assert_eq!(bar.name(), "bar");
    assert_eq!(line.name(), "line");
    assert_eq!(pie.name(), "pie");
    assert_eq!(scatter.name(), "scatter");
    assert_eq!(table.name(), "table");

    assert!(table.can_handle(&data, &intent));
}
