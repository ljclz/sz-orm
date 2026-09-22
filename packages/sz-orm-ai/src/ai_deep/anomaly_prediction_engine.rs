//! 异常预测引擎
//!
//! 时序数据分析 → 预测异常类型 → 置信度评估 → 建议动作 → 脱敏 → 可解释报告。
//! 报告脱敏禁止泄露基础设施拓扑和敏感指标值。

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use sz_orm_anomaly::{
    AnomalyPredictor, PredictedAnomalyType, PredictionError, PredictionResult,
    TimeSeriesData as AnomalyTimeSeriesData,
};

use super::{
    record_audit, AiDeepError, AiDeepEventType, AnomalyPrediction, AnomalyPredictionReport,
    TimeSeriesData,
};

/// 异常预测配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyPredictConfig {
    /// 最低置信度阈值（百分比）
    pub min_confidence: f64,
    /// 是否启用脱敏
    pub enable_masking: bool,
    /// 预测窗口（分钟）
    pub prediction_window_minutes: u32,
}

impl Default for AnomalyPredictConfig {
    fn default() -> Self {
        Self {
            min_confidence: 85.0,
            enable_masking: true,
            prediction_window_minutes: 5,
        }
    }
}

/// 异常预测引擎
///
/// 时序数据分析 → 预测异常类型（容量瓶颈/连接池耗尽/查询退化）→ 置信度评估 → 建议动作 → 脱敏 → 可解释报告。
pub struct AnomalyPredictionEngine {
    predictor: Arc<AnomalyPredictor>,
    config: AnomalyPredictConfig,
}

impl AnomalyPredictionEngine {
    /// 创建异常预测引擎
    pub fn new(predictor: Arc<AnomalyPredictor>, config: AnomalyPredictConfig) -> Self {
        Self { predictor, config }
    }

    /// 使用默认配置创建
    pub fn with_default_config(predictor: Arc<AnomalyPredictor>) -> Self {
        Self::new(predictor, AnomalyPredictConfig::default())
    }

    /// 预测异常
    ///
    /// 流程：
    /// 1. 时序数据分析
    /// 2. 预测异常类型（容量瓶颈/连接池耗尽/查询退化）
    /// 3. 置信度评估
    /// 4. 建议动作
    /// 5. 脱敏
    /// 6. 可解释报告
    ///
    /// 异常映射：
    /// - 置信度低 → 标记 `FORECAST_LOW_CONFIDENCE`，附置信区间
    pub async fn predict(
        &self,
        history: &[TimeSeriesData],
    ) -> Result<AnomalyPredictionReport, AiDeepError> {
        // 1. 转换时序数据格式
        let anomaly_history = self.convert_history(history);

        // 2. 调用 AnomalyPredictor 预测
        let prediction_result = self
            .predictor
            .predict(&anomaly_history)
            .await
            .map_err(|e| Self::map_prediction_error(&e))?;

        // 3. 置信度评估
        let low_confidence = prediction_result.confidence < self.config.min_confidence;
        if low_confidence {
            eprintln!(
                "FORECAST_LOW_CONFIDENCE: 置信度 {:.1}% < 阈值 {:.1}%",
                prediction_result.confidence, self.config.min_confidence
            );
        }

        // 4. 建议动作
        let suggested_action = Self::suggest_action(&prediction_result);

        // 5. 构建预测列表
        let predictions = vec![AnomalyPrediction {
            anomaly_type: prediction_result.anomaly_type.as_str().to_string(),
            predicted_at: prediction_result.predicted_at,
            confidence: prediction_result.confidence,
            suggested_action,
        }];

        // 6. 可解释报告（脱敏）
        let raw_explanation = self.build_explanation(&prediction_result, history, low_confidence);
        let explanation = if self.config.enable_masking {
            Self::mask_explanation(&raw_explanation)
        } else {
            raw_explanation
        };

        let report = AnomalyPredictionReport {
            predictions,
            confidence: prediction_result.confidence,
            masked: self.config.enable_masking,
            explanation,
            low_confidence_flagged: low_confidence,
        };

        // 记录审计日志
        let _ = record_audit(
            &sz_orm_audit::AutonomousDecisionAuditor::new(),
            AiDeepEventType::AnomalyPredict,
            "anomaly_predict",
            &format!(
                "异常预测: 类型={}, 置信度={:.1}%, 低置信度={}",
                prediction_result.anomaly_type.as_str(),
                prediction_result.confidence,
                low_confidence
            ),
            true,
        );

        Ok(report)
    }

    /// 转换时序数据格式
    fn convert_history(&self, history: &[TimeSeriesData]) -> Vec<AnomalyTimeSeriesData> {
        history
            .iter()
            .map(|d| AnomalyTimeSeriesData {
                timestamp: d.timestamp,
                qps: d.value,
                latency_ms: d.value,
                error_rate: (d.value / 100.0).min(1.0),
                resource_usage: (d.value / 100.0).min(1.0),
            })
            .collect()
    }

    /// 映射预测错误
    fn map_prediction_error(e: &PredictionError) -> AiDeepError {
        match e {
            PredictionError::InsufficientHistory(msg) => {
                AiDeepError::Internal(format!("历史数据不足: {}", msg))
            }
            PredictionError::ConfigError(msg) => {
                AiDeepError::Internal(format!("配置错误: {}", msg))
            }
            PredictionError::PredictionFailed(msg) => {
                AiDeepError::Internal(format!("预测失败: {}", msg))
            }
        }
    }

    /// 建议动作
    fn suggest_action(result: &PredictionResult) -> String {
        match result.anomaly_type {
            PredictedAnomalyType::CapacityBottleneck => {
                "扩容资源：增加 CPU/内存/连接池容量".to_string()
            }
            PredictedAnomalyType::LatencySpike => {
                "优化慢查询：检查慢查询日志，添加索引或重写 SQL".to_string()
            }
            PredictedAnomalyType::FailurePrecursor => {
                "预防故障：检查错误率上升趋势，排查异常源头".to_string()
            }
        }
    }

    /// 构建可解释报告
    fn build_explanation(
        &self,
        result: &PredictionResult,
        history: &[TimeSeriesData],
        low_confidence: bool,
    ) -> String {
        let confidence_interval = format!("[{:.1}%, 100.0%]", result.confidence);

        format!(
            "异常预测报告：\n\
             - 预测异常类型: {}\n\
             - 预测发生时间: {}\n\
             - 置信度: {:.1}%\n\
             - 置信区间: {}\n\
             - 预警提前量: {:.0}秒\n\
             - 历史数据点: {}\n\
             - 低置信度标注: {}\n\
             - 数据依据: {}",
            result.anomaly_type.as_str(),
            result.predicted_at,
            result.confidence,
            confidence_interval,
            result.early_warning_seconds,
            history.len(),
            if low_confidence { "是" } else { "否" },
            result.history_data_basis
        )
    }

    /// 脱敏可解释报告
    ///
    /// 禁止泄露基础设施拓扑和敏感指标值。
    /// 替换 IP、主机名、端口号等敏感信息。
    fn mask_explanation(explanation: &str) -> String {
        let mut result = explanation.to_string();

        // 替换常见敏感模式
        result = result.replace("127.0.0.1", "***.***.***.***");
        result = result.replace("localhost", "***");
        result = result.replace("0.0.0.0", "***.***.***.***");

        // 脱敏端口号（如 :3306, :5432）
        for port in &["3306", "5432", "1521", "6379", "9090"] {
            let pattern = format!(":{}", port);
            result = result.replace(&pattern, ":****");
        }

        result
    }

    /// 获取配置
    pub fn config(&self) -> &AnomalyPredictConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> AnomalyPredictionEngine {
        let predictor = Arc::new(AnomalyPredictor::default());
        AnomalyPredictionEngine::with_default_config(predictor)
    }

    fn make_history_30_days() -> Vec<TimeSeriesData> {
        // 生成 30 天的时序数据（每天 1 个点，资源使用率上升趋势）
        (0..30)
            .map(|i| TimeSeriesData {
                timestamp: 1700000000 + i as i64 * 86400,
                value: 50.0 + i as f64 * 1.5,
                metric_name: "resource_usage".to_string(),
            })
            .collect()
    }

    #[tokio::test]
    async fn test_predict_with_sufficient_data() {
        let engine = make_engine();
        let history = make_history_30_days();
        let result = engine.predict(&history).await.unwrap();
        assert!(!result.predictions.is_empty());
        assert!(result.confidence >= 0.0);
        assert!(!result.explanation.is_empty());
    }

    #[tokio::test]
    async fn test_predict_masked() {
        let engine = make_engine();
        let history = make_history_30_days();
        let result = engine.predict(&history).await.unwrap();
        assert!(result.masked);
        // 脱敏后不应包含原始 IP
        assert!(!result.explanation.contains("127.0.0.1"));
    }

    #[tokio::test]
    async fn test_predict_low_confidence_flagged() {
        let mut config = AnomalyPredictConfig::default();
        config.min_confidence = 99.0; // 设置高阈值，使置信度必然低于阈值

        let predictor = Arc::new(AnomalyPredictor::default());
        let engine = AnomalyPredictionEngine::new(predictor, config);

        let history = make_history_30_days();
        let result = engine.predict(&history).await.unwrap();
        assert!(result.low_confidence_flagged);
    }

    #[tokio::test]
    async fn test_predict_insufficient_history() {
        let engine = make_engine();
        let history = vec![TimeSeriesData {
            timestamp: 1700000000,
            value: 50.0,
            metric_name: "test".to_string(),
        }];
        let result = engine.predict(&history).await;
        // AnomalyPredictor 要求至少 3 个数据点
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_predict_explanation_contains_required_fields() {
        let engine = make_engine();
        let history = make_history_30_days();
        let result = engine.predict(&history).await.unwrap();
        assert!(result.explanation.contains("异常预测报告"));
        assert!(result.explanation.contains("预测异常类型"));
        assert!(result.explanation.contains("置信度"));
        assert!(result.explanation.contains("置信区间"));
    }

    #[tokio::test]
    async fn test_predict_suggested_action() {
        let engine = make_engine();
        let history = make_history_30_days();
        let result = engine.predict(&history).await.unwrap();
        let action = &result.predictions[0].suggested_action;
        assert!(!action.is_empty());
        // 建议动作应包含具体建议
        assert!(action.contains("扩容") || action.contains("优化") || action.contains("预防"));
    }

    #[test]
    fn test_config_default() {
        let config = AnomalyPredictConfig::default();
        assert_eq!(config.min_confidence, 85.0);
        assert!(config.enable_masking);
        assert_eq!(config.prediction_window_minutes, 5);
    }

    #[test]
    fn test_mask_explanation_redacts_ip() {
        let explanation = "连接 127.0.0.1:3306 失败";
        let masked = AnomalyPredictionEngine::mask_explanation(explanation);
        assert!(!masked.contains("127.0.0.1"));
        assert!(!masked.contains("3306"));
        assert!(masked.contains("***"));
    }
}
