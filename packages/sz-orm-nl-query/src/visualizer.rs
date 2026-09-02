//! 可视化推荐与渲染（TASK-023）

use crate::types::{NlQueryError, VisualizationSpec};
use serde::{Deserialize, Serialize};

/// 可视化器 trait
pub trait Visualizer: Send + Sync {
    fn name(&self) -> &str;
    fn can_handle(&self, data: &serde_json::Value, intent: &VisualizationIntent) -> bool;
    fn render(&self, data: &serde_json::Value) -> Result<VisualizationSpec, NlQueryError>;
}

/// 可视化意图（来自 NL 分析）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualizationIntent {
    pub primary_type: ChartType,
    pub has_time_axis: bool,
    pub has_category: bool,
    pub has_measure: bool,
    pub row_count: usize,
}

/// 图表类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChartType {
    Bar,
    Line,
    Pie,
    Scatter,
    Table,
}

impl Default for VisualizationIntent {
    fn default() -> Self {
        Self {
            primary_type: ChartType::Table,
            has_time_axis: false,
            has_category: false,
            has_measure: false,
            row_count: 0,
        }
    }
}

/// 柱状图可视化器
pub struct BarVisualizer;

impl Visualizer for BarVisualizer {
    fn name(&self) -> &str {
        "bar"
    }

    fn can_handle(&self, _data: &serde_json::Value, intent: &VisualizationIntent) -> bool {
        intent.has_category && intent.has_measure && !intent.has_time_axis
    }

    fn render(&self, data: &serde_json::Value) -> Result<VisualizationSpec, NlQueryError> {
        Ok(VisualizationSpec {
            chart_type: "bar".to_string(),
            data: data.clone(),
        })
    }
}

/// 折线图可视化器
pub struct LineVisualizer;

impl Visualizer for LineVisualizer {
    fn name(&self) -> &str {
        "line"
    }

    fn can_handle(&self, _data: &serde_json::Value, intent: &VisualizationIntent) -> bool {
        intent.has_time_axis && intent.has_measure
    }

    fn render(&self, data: &serde_json::Value) -> Result<VisualizationSpec, NlQueryError> {
        Ok(VisualizationSpec {
            chart_type: "line".to_string(),
            data: data.clone(),
        })
    }
}

/// 饼图可视化器
pub struct PieVisualizer;

impl Visualizer for PieVisualizer {
    fn name(&self) -> &str {
        "pie"
    }

    fn can_handle(&self, _data: &serde_json::Value, intent: &VisualizationIntent) -> bool {
        intent.has_category && intent.has_measure && intent.row_count <= 10
    }

    fn render(&self, data: &serde_json::Value) -> Result<VisualizationSpec, NlQueryError> {
        Ok(VisualizationSpec {
            chart_type: "pie".to_string(),
            data: data.clone(),
        })
    }
}

/// 散点图可视化器
pub struct ScatterVisualizer;

impl Visualizer for ScatterVisualizer {
    fn name(&self) -> &str {
        "scatter"
    }

    fn can_handle(&self, _data: &serde_json::Value, intent: &VisualizationIntent) -> bool {
        intent.has_measure && intent.row_count > 10 && !intent.has_time_axis
    }

    fn render(&self, data: &serde_json::Value) -> Result<VisualizationSpec, NlQueryError> {
        Ok(VisualizationSpec {
            chart_type: "scatter".to_string(),
            data: data.clone(),
        })
    }
}

/// 表格可视化器（兜底）
pub struct TableVisualizer;

impl Visualizer for TableVisualizer {
    fn name(&self) -> &str {
        "table"
    }

    fn can_handle(&self, _data: &serde_json::Value, _intent: &VisualizationIntent) -> bool {
        true
    }

    fn render(&self, data: &serde_json::Value) -> Result<VisualizationSpec, NlQueryError> {
        Ok(VisualizationSpec {
            chart_type: "table".to_string(),
            data: data.clone(),
        })
    }
}

/// 可视化选择器
pub struct VisualizerSelector {
    visualizers: Vec<Box<dyn Visualizer>>,
}

impl VisualizerSelector {
    pub fn new() -> Self {
        Self {
            visualizers: vec![
                Box::new(LineVisualizer),
                Box::new(PieVisualizer),
                Box::new(BarVisualizer),
                Box::new(ScatterVisualizer),
                Box::new(TableVisualizer),
            ],
        }
    }

    pub fn select(
        &self,
        data: &serde_json::Value,
        intent: &VisualizationIntent,
    ) -> Result<VisualizationSpec, NlQueryError> {
        for v in &self.visualizers {
            if v.can_handle(data, intent) {
                return v.render(data);
            }
        }
        Err(NlQueryError::Nl2SqlFailed("无可用的可视化器".to_string()))
    }

    pub fn with_visualizer(mut self, visualizer: Box<dyn Visualizer>) -> Self {
        self.visualizers.insert(0, visualizer);
        self
    }
}

impl Default for VisualizerSelector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_line_for_time_series() {
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
    fn test_select_bar_for_category() {
        let selector = VisualizerSelector::new();
        let intent = VisualizationIntent {
            primary_type: ChartType::Bar,
            has_time_axis: false,
            has_category: true,
            has_measure: true,
            row_count: 20,
        };
        let data = serde_json::json!([{"category": "A", "value": 10}]);
        let spec = selector.select(&data, &intent).unwrap();
        assert_eq!(spec.chart_type, "bar");
    }

    #[test]
    fn test_select_pie_for_small_category() {
        let selector = VisualizerSelector::new();
        let intent = VisualizationIntent {
            primary_type: ChartType::Pie,
            has_time_axis: false,
            has_category: true,
            has_measure: true,
            row_count: 5,
        };
        let data = serde_json::json!([{"category": "A", "value": 10}]);
        let spec = selector.select(&data, &intent).unwrap();
        assert_eq!(spec.chart_type, "pie");
    }

    #[test]
    fn test_select_table_as_fallback() {
        let selector = VisualizerSelector::new();
        let intent = VisualizationIntent::default();
        let data = serde_json::json!([]);
        let spec = selector.select(&data, &intent).unwrap();
        assert_eq!(spec.chart_type, "table");
    }
}
