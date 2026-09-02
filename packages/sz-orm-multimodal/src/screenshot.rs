//! 截图输入分析（TASK-034）

use crate::types::MultimodalError;
use serde::{Deserialize, Serialize};

/// OCR 服务响应
#[derive(Debug, Clone, Deserialize)]
struct OcrResponse {
    tables: Vec<String>,
    #[allow(dead_code)]
    confidence: f64,
}

/// OCR HTTP 客户端（接入真实 OCR/CV 服务）
pub struct OcrClient {
    client: reqwest::Client,
    endpoint: String,
}

impl OcrClient {
    pub fn new(endpoint: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoint: endpoint.to_string(),
        }
    }

    async fn recognize(&self, image_data: &[u8]) -> Result<OcrResponse, MultimodalError> {
        let base64 = base64_encode(image_data);
        let body = serde_json::json!({ "image": base64 });

        let resp = self
            .client
            .post(&self.endpoint)
            .json(&body)
            .send()
            .await
            .map_err(|_e| MultimodalError::ScreenshotRecognizeFailed)?;

        resp.json::<OcrResponse>()
            .await
            .map_err(|_e| MultimodalError::ScreenshotRecognizeFailed)
    }
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((n >> 18) & 63) as usize] as char);
        result.push(CHARS[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((n >> 6) & 63) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(n & 63) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

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
    ocr: Option<OcrClient>,
}

impl ScreenshotAnalyzer {
    pub fn new(min_confidence: f64) -> Self {
        Self {
            min_confidence,
            ocr: None,
        }
    }

    /// 注入 OCR 服务端点（如 "http://localhost:8080/ocr"）
    pub fn with_ocr_endpoint(mut self, endpoint: &str) -> Self {
        self.ocr = Some(OcrClient::new(endpoint));
        self
    }

    /// 分析截图（同步伪检测模式）
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

    /// 分析截图（异步真实 OCR 模式）
    ///
    /// 注入 OCR 端点时通过 HTTP 调用真实 OCR/CV 服务识别表结构。
    /// 未注入时退回到同步伪检测。
    pub async fn analyze_async(
        &self,
        image_data: &[u8],
    ) -> Result<ScreenshotAnalysis, MultimodalError> {
        if image_data.is_empty() {
            return Err(MultimodalError::ScreenshotRecognizeFailed);
        }

        let detected_tables = if let Some(ocr) = &self.ocr {
            let resp = ocr.recognize(image_data).await?;
            resp.tables
        } else {
            self.detect_tables(image_data)
        };

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

    /// 检测截图中的表名（演示用伪检测）
    ///
    /// **注意**：基于数据哈希取模选择表名，非真实 OCR。
    /// 生产环境应通过 `with_ocr_endpoint` 注入 OCR 服务。
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
        Self {
            min_confidence: 0.5,
            ocr: None,
        }
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

    #[tokio::test]
    async fn test_analyze_async_without_ocr() {
        let analyzer = ScreenshotAnalyzer::default();
        let image_data = vec![1, 2, 3, 4, 5];
        let result = analyzer.analyze_async(&image_data).await.unwrap();
        assert!(!result.detected_tables.is_empty());
    }

    #[tokio::test]
    async fn test_analyze_async_empty_fails() {
        let analyzer = ScreenshotAnalyzer::default();
        assert!(analyzer.analyze_async(&[]).await.is_err());
    }

    #[tokio::test]
    async fn test_analyze_async_with_ocr_endpoint_unreachable() {
        let analyzer = ScreenshotAnalyzer::default()
            .with_ocr_endpoint("http://localhost:9999/ocr");
        let image_data = vec![1, 2, 3];
        // OCR 服务不可达时应返回错误
        assert!(analyzer.analyze_async(&image_data).await.is_err());
    }
}
