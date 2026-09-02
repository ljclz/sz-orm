//! 查询结果洞察提取（TASK-024）

use serde::{Deserialize, Serialize};

/// 洞类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum InsightType {
    Trend,
    Anomaly,
    TopN,
    Proportion,
}

/// 洞结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Insight {
    pub insight_type: InsightType,
    pub description: String,
    pub confidence: f64,
    pub evidence: serde_json::Value,
}

/// 洞提取器
pub struct InsightExtractor;

impl InsightExtractor {
    pub fn new() -> Self {
        Self
    }

    /// 从查询结果提取洞察
    pub fn extract(&self, rows: &[serde_json::Value]) -> Vec<Insight> {
        let mut insights = Vec::new();

        if let Some(trend) = self.extract_trend(rows) {
            insights.push(trend);
        }
        if let Some(anomaly) = self.extract_anomaly(rows) {
            insights.push(anomaly);
        }
        if let Some(topn) = self.extract_topn(rows) {
            insights.push(topn);
        }
        if let Some(prop) = self.extract_proportion(rows) {
            insights.push(prop);
        }

        insights
    }

    /// 趋势分析：检测数值列的上升/下降趋势
    fn extract_trend(&self, rows: &[serde_json::Value]) -> Option<Insight> {
        let values = Self::extract_numeric_values(rows);
        if values.len() < 3 {
            return None;
        }

        let mut increasing = 0;
        let mut decreasing = 0;
        for i in 1..values.len() {
            if values[i] > values[i - 1] {
                increasing += 1;
            } else if values[i] < values[i - 1] {
                decreasing += 1;
            }
        }

        let total = values.len() - 1;
        if increasing > decreasing && increasing as f64 / total as f64 > 0.6 {
            let confidence = increasing as f64 / total as f64;
            Some(Insight {
                insight_type: InsightType::Trend,
                description: format!("数据呈上升趋势（{}/{} 递增）", increasing, total),
                confidence,
                evidence: serde_json::json!({"direction": "up", "increasing": increasing, "total": total}),
            })
        } else if decreasing > increasing && decreasing as f64 / total as f64 > 0.6 {
            let confidence = decreasing as f64 / total as f64;
            Some(Insight {
                insight_type: InsightType::Trend,
                description: format!("数据呈下降趋势（{}/{} 递减）", decreasing, total),
                confidence,
                evidence: serde_json::json!({"direction": "down", "decreasing": decreasing, "total": total}),
            })
        } else {
            None
        }
    }

    /// 异常检测：基于 Z-score 检测离群点
    fn extract_anomaly(&self, rows: &[serde_json::Value]) -> Option<Insight> {
        let values = Self::extract_numeric_values(rows);
        if values.len() < 5 {
            return None;
        }

        let mean: f64 = values.iter().sum::<f64>() / values.len() as f64;
        let variance: f64 =
            values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
        let std = variance.sqrt();

        if std < 1e-10 {
            return None;
        }

        let anomalies: Vec<_> = values
            .iter()
            .enumerate()
            .filter(|(_, v)| ((**v - mean) / std).abs() > 2.0)
            .collect();

        if !anomalies.is_empty() {
            let confidence = 1.0 - anomalies.len() as f64 / values.len() as f64;
            Some(Insight {
                insight_type: InsightType::Anomaly,
                description: format!("检测到 {} 个异常值（Z-score > 2）", anomalies.len()),
                confidence,
                evidence: serde_json::json!({
                    "mean": mean,
                    "std": std,
                    "anomaly_indices": anomalies.iter().map(|(i, _)| i).collect::<Vec<_>>(),
                }),
            })
        } else {
            None
        }
    }

    /// Top-N 分析：找出最大值
    fn extract_topn(&self, rows: &[serde_json::Value]) -> Option<Insight> {
        let values = Self::extract_numeric_values(rows);
        if values.is_empty() {
            return None;
        }

        let max_val = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let max_idx = values.iter().position(|v| (*v - max_val).abs() < 1e-10)?;

        Some(Insight {
            insight_type: InsightType::TopN,
            description: format!("最大值为 {:.2}（第 {} 行）", max_val, max_idx + 1),
            confidence: 1.0,
            evidence: serde_json::json!({"max": max_val, "index": max_idx}),
        })
    }

    /// 占比分析：计算各值占总和的比例
    fn extract_proportion(&self, rows: &[serde_json::Value]) -> Option<Insight> {
        let values = Self::extract_numeric_values(rows);
        if values.len() < 2 {
            return None;
        }

        let total: f64 = values.iter().sum();
        if total.abs() < 1e-10 {
            return None;
        }

        let proportions: Vec<f64> = values.iter().map(|v| v / total).collect();
        let max_prop = proportions.iter().cloned().fold(0.0, f64::max);

        if max_prop > 0.5 {
            let dominant_idx = proportions
                .iter()
                .position(|p| (*p - max_prop).abs() < 1e-10)?;
            Some(Insight {
                insight_type: InsightType::Proportion,
                description: format!(
                    "第 {} 行占总量的 {:.1}%，占据主导地位",
                    dominant_idx + 1,
                    max_prop * 100.0
                ),
                confidence: max_prop,
                evidence: serde_json::json!({"proportions": proportions, "dominant_index": dominant_idx}),
            })
        } else {
            None
        }
    }

    fn extract_numeric_values(rows: &[serde_json::Value]) -> Vec<f64> {
        let mut values = Vec::new();
        for row in rows {
            if let Some(obj) = row.as_object() {
                for (_, v) in obj {
                    if let Some(n) = v.as_f64() {
                        values.push(n);
                    }
                }
            }
        }
        values
    }
}

impl Default for InsightExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_trend_up() {
        let extractor = InsightExtractor::new();
        let rows = vec![
            serde_json::json!({"v": 1.0}),
            serde_json::json!({"v": 2.0}),
            serde_json::json!({"v": 3.0}),
            serde_json::json!({"v": 4.0}),
            serde_json::json!({"v": 5.0}),
        ];
        let insights = extractor.extract(&rows);
        let trend = insights
            .iter()
            .find(|i| i.insight_type == InsightType::Trend)
            .expect("应检测到上升趋势");
        assert!(trend.description.contains("上升"));
    }

    #[test]
    fn test_extract_anomaly() {
        let extractor = InsightExtractor::new();
        let rows = vec![
            serde_json::json!({"v": 10.0}),
            serde_json::json!({"v": 11.0}),
            serde_json::json!({"v": 10.5}),
            serde_json::json!({"v": 10.2}),
            serde_json::json!({"v": 100.0}),
            serde_json::json!({"v": 9.8}),
        ];
        let insights = extractor.extract(&rows);
        assert!(
            insights
                .iter()
                .any(|i| i.insight_type == InsightType::Anomaly),
            "应检测到异常值"
        );
    }

    #[test]
    fn test_extract_topn() {
        let extractor = InsightExtractor::new();
        let rows = vec![
            serde_json::json!({"v": 10.0}),
            serde_json::json!({"v": 50.0}),
            serde_json::json!({"v": 30.0}),
        ];
        let insights = extractor.extract(&rows);
        let topn = insights
            .iter()
            .find(|i| i.insight_type == InsightType::TopN)
            .expect("应提取 TopN");
        assert!(topn.description.contains("50.00"));
    }

    #[test]
    fn test_empty_rows() {
        let extractor = InsightExtractor::new();
        let insights = extractor.extract(&[]);
        assert!(insights.is_empty());
    }
}
