//! 截图输入分析（TASK-034）

use crate::types::MultimodalError;
use serde::{Deserialize, Serialize};

/// 截图分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotAnalysis {
    pub detected_tables: Vec<String>,
    pub detected_columns: Vec<DetectedColumn>,
    pub inferred_query: Option<String>,
    pub confidence: f64,
}

/// 检测到的列
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedColumn {
    pub table: String,
    pub column: String,
    pub data_type: String,
    pub confidence: f64,
}

/// 截图分析器
pub struct ScreenshotAnalyzer {
    min_confidence: f64,
}

impl ScreenshotAnalyzer {
    pub fn new(min_confidence: f64) -> Self {
        Self { min_confidence }
    }

    /// 分析截图（模拟 OCR + 表结构识别）
    pub fn analyze(&self, image_data: &[u8]) -> Result<ScreenshotAnalysis, MultimodalError> {
        if image_data.is_empty() {
            return Err(MultimodalError::ScreenshotRecognizeFailed);
        }

        let detected_tables = self.detect_tables(image_data);
        let detected_columns = self.detect_columns(&detected_tables);
        let inferred_query = self.infer_query(&detected_tables, &detected_columns);
        let confidence = self.compute_confidence(&detected_tables, &detected_columns);

        if confidence < self.min_confidence {
            return Err(MultimodalError::ScreenshotRecognizeFailed);
        }

        Ok(ScreenshotAnalysis {
            detected_tables,
            detected_columns,
            inferred_query,
            confidence,
        })
    }

    /// 从分析结果生成 NL 查询
    pub fn to_nl_query(&self, analysis: &ScreenshotAnalysis) -> String {
        if analysis.detected_tables.is_empty() {
            return "查询数据".to_string();
        }

        let tables = analysis.detected_tables.join(", ");
        if let Some(query) = &analysis.inferred_query {
            format!("查询 {}（推断: {}）", tables, query)
        } else {
            format!("查询 {}", tables)
        }
    }

    fn detect_tables(&self, image_data: &[u8]) -> Vec<String> {
        let mut tables = Vec::new();
        let hash = image_data
            .iter()
            .fold(0u64, |acc, b| acc.wrapping_add(*b as u64));

        match hash % 3 {
            0 => {
                tables.push("users".to_string());
            }
            1 => {
                tables.push("users".to_string());
                tables.push("orders".to_string());
            }
            _ => {
                tables.push("products".to_string());
            }
        }

        tables
    }

    fn detect_columns(&self, tables: &[String]) -> Vec<DetectedColumn> {
        let mut columns = Vec::new();

        for table in tables {
            match table.as_str() {
                "users" => {
                    columns.push(DetectedColumn {
                        table: table.clone(),
                        column: "id".to_string(),
                        data_type: "BIGINT".to_string(),
                        confidence: 0.95,
                    });
                    columns.push(DetectedColumn {
                        table: table.clone(),
                        column: "name".to_string(),
                        data_type: "VARCHAR".to_string(),
                        confidence: 0.85,
                    });
                }
                "orders" => {
                    columns.push(DetectedColumn {
                        table: table.clone(),
                        column: "id".to_string(),
                        data_type: "BIGINT".to_string(),
                        confidence: 0.95,
                    });
                    columns.push(DetectedColumn {
                        table: table.clone(),
                        column: "amount".to_string(),
                        data_type: "DECIMAL".to_string(),
                        confidence: 0.80,
                    });
                }
                "products" => {
                    columns.push(DetectedColumn {
                        table: table.clone(),
                        column: "id".to_string(),
                        data_type: "BIGINT".to_string(),
                        confidence: 0.95,
                    });
                    columns.push(DetectedColumn {
                        table: table.clone(),
                        column: "price".to_string(),
                        data_type: "DECIMAL".to_string(),
                        confidence: 0.80,
                    });
                }
                _ => {}
            }
        }

        columns
    }

    fn infer_query(&self, tables: &[String], columns: &[DetectedColumn]) -> Option<String> {
        if tables.is_empty() {
            return None;
        }

        let col_names: Vec<_> = columns.iter().map(|c| c.column.as_str()).collect();
        Some(format!(
            "SELECT {} FROM {}",
            col_names.join(", "),
            tables.join(", ")
        ))
    }

    fn compute_confidence(&self, _tables: &[String], columns: &[DetectedColumn]) -> f64 {
        if columns.is_empty() {
            return 0.0;
        }
        columns.iter().map(|c| c.confidence).sum::<f64>() / columns.len() as f64
    }
}

impl Default for ScreenshotAnalyzer {
    fn default() -> Self {
        Self::new(0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_screenshot() {
        let analyzer = ScreenshotAnalyzer::default();
        let image_data = vec![1, 2, 3, 4, 5];
        let result = analyzer.analyze(&image_data).unwrap();

        assert!(!result.detected_tables.is_empty());
        assert!(!result.detected_columns.is_empty());
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn test_empty_image_fails() {
        let analyzer = ScreenshotAnalyzer::default();
        assert!(analyzer.analyze(&[]).is_err());
    }

    #[test]
    fn test_to_nl_query() {
        let analyzer = ScreenshotAnalyzer::default();
        let analysis = ScreenshotAnalysis {
            detected_tables: vec!["users".to_string()],
            detected_columns: vec![],
            inferred_query: Some("SELECT * FROM users".to_string()),
            confidence: 0.9,
        };
        let nl = analyzer.to_nl_query(&analysis);
        assert!(nl.contains("users"));
        assert!(nl.contains("SELECT * FROM users"));
    }

    #[test]
    fn test_to_nl_query_empty() {
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
    fn test_low_confidence_fails() {
        let analyzer = ScreenshotAnalyzer::new(0.99);
        let image_data = vec![1];
        assert!(analyzer.analyze(&image_data).is_err());
    }
}
