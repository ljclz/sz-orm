//! 图表自动生成（TASK-014 占位）
#![allow(dead_code)]

use crate::types::{ChartSpec, MultimodalError};

pub struct ChartGenerator;

impl ChartGenerator {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(&self, _data: &serde_json::Value) -> Result<ChartSpec, MultimodalError> {
        Ok(ChartSpec {
            chart_type: "table".to_string(),
            data: serde_json::json!([]),
        })
    }
}

impl Default for ChartGenerator {
    fn default() -> Self {
        Self::new()
    }
}
