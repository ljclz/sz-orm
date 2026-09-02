//! 图表自动生成（TASK-014）
//!
//! 根据数据特征自动选择最合适的图表类型：
//! - 时间序列数据 → 折线图（line）
//! - 分类数据 → 柱状图（bar）
//! - x/y 坐标数据 → 散点图（scatter）
//! - 空数据或其他 → 表格（table）降级

use crate::types::{ChartSpec, MultimodalError};

/// 图表自动生成器
///
/// 分析数据结构特征，推荐最合适的可视化图表类型。
pub struct ChartGenerator;

impl ChartGenerator {
    pub fn new() -> Self {
        Self
    }

    /// 根据数据特征生成图表规格
    ///
    /// 自动检测数据中的字段名，推荐合适的图表类型：
    /// - 含 time/date/timestamp 字段 → line（折线图）
    /// - 含 category/type/group 字段 → bar（柱状图）
    /// - 含 x 和 y 字段 → scatter（散点图）
    /// - 空数组或其他 → table（表格降级）
    pub fn generate(&self, data: &serde_json::Value) -> Result<ChartSpec, MultimodalError> {
        let chart_type = Self::select_chart_type(data);
        Ok(ChartSpec {
            chart_type,
            data: data.clone(),
        })
    }

    /// 根据数据特征选择图表类型
    fn select_chart_type(data: &serde_json::Value) -> String {
        let arr = match data.as_array() {
            Some(a) if !a.is_empty() => a,
            _ => return "table".into(),
        };

        let first = &arr[0];
        let keys: Vec<&str> = match first.as_object() {
            Some(obj) => obj.keys().map(|k| k.as_str()).collect(),
            None => return "table".into(),
        };

        let has_field = |names: &[&str]| keys.iter().any(|k| names.contains(k));

        if has_field(&["time", "date", "timestamp", "datetime"]) {
            return "line".into();
        }
        if has_field(&["x"]) && has_field(&["y"]) {
            return "scatter".into();
        }
        if has_field(&["category", "type", "group", "label"]) {
            return "bar".into();
        }

        "table".into()
    }
}

impl Default for ChartGenerator {
    fn default() -> Self {
        Self::new()
    }
}
