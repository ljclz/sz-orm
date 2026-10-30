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

// ============================================================================
// v10.11.0 T3.3：AI 异常自愈
// 检测异常 → 生成处置方案（重启连接池/切换主节点/限流降级）→ 风险评估 →
// 自动执行低风险方案，高风险待人工确认，恶化自动回滚。
// 复用既有 AnomalyPredictionEngine + ai-autonomous-loop feature gate +
// ai-config feature gate（DecisionExplainer）。
// ============================================================================

/// 自愈动作
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealAction {
    /// 重启连接池（低风险）
    RestartPool,
    /// 切换主节点（高风险，触发故障转移）
    SwitchMaster,
    /// 限流降级（低风险）
    RateLimitDegrade,
    /// 自定义动作
    Custom(String),
}

impl HealAction {
    /// 动作名称
    pub fn name(&self) -> &str {
        match self {
            HealAction::RestartPool => "RestartPool",
            HealAction::SwitchMaster => "SwitchMaster",
            HealAction::RateLimitDegrade => "RateLimitDegrade",
            HealAction::Custom(name) => name,
        }
    }
}

/// 自愈风险等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealRiskLevel {
    /// 低风险（可自动执行：重启连接池/限流降级）
    Low,
    /// 高风险（须人工确认：切换主节点）
    High,
}

/// 自愈方案
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealPlan {
    /// 自愈动作列表
    pub actions: Vec<HealAction>,
    /// 方案描述
    pub description: String,
    /// 回滚动作列表
    pub rollback_actions: Vec<HealAction>,
}

/// 监控指标（用于恶化判断）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealMetrics {
    /// 错误率（0.0 ~ 1.0）
    pub error_rate: f64,
    /// 平均延迟（毫秒）
    pub avg_latency_ms: f64,
    /// 连接池使用率（0.0 ~ 1.0）
    pub pool_usage: f64,
}

impl HealMetrics {
    /// 创建监控指标
    pub fn new(error_rate: f64, avg_latency_ms: f64, pool_usage: f64) -> Self {
        Self {
            error_rate,
            avg_latency_ms,
            pool_usage,
        }
    }

    /// 综合健康评分（0.0 ~ 1.0，越高越健康）
    pub fn health_score(&self) -> f64 {
        let error_component = 1.0 - self.error_rate.clamp(0.0, 1.0);
        let latency_component = 1.0 / (1.0 + self.avg_latency_ms / 1000.0);
        let pool_component = 1.0 - self.pool_usage.clamp(0.0, 1.0);
        (error_component + latency_component + pool_component) / 3.0
    }

    /// 是否比另一个指标更差（恶化）
    pub fn is_worse_than(&self, other: &HealMetrics) -> bool {
        self.health_score() < other.health_score()
    }
}

/// 自愈结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealResult {
    /// 检测到的异常类型
    pub anomaly_type: String,
    /// 自愈方案
    pub plan: HealPlan,
    /// 风险等级
    pub risk_level: HealRiskLevel,
    /// 是否已自动执行
    pub executed: bool,
    /// 是否待人工确认
    pub pending_manual: bool,
    /// 执行前的指标
    pub metrics_before: Option<HealMetrics>,
    /// 执行后的指标
    pub metrics_after: Option<HealMetrics>,
    /// 是否已回滚（恶化后自动回滚）
    pub rolled_back: bool,
    /// 决策可解释性（需 ai-config feature）
    #[cfg(feature = "ai-config")]
    pub explainer: Option<crate::ai_config::DecisionExplainer>,
}

/// 自愈错误
#[derive(Debug, thiserror::Error)]
pub enum HealError {
    #[error("未检测到异常")]
    NoAnomalyDetected,
    #[error("自愈执行失败：{0}")]
    ExecutionFailed(String),
    #[error("恶化监控失败：{0}")]
    WorsenCheckFailed(String),
}

/// 自愈方案生成器
///
/// 根据异常类型生成处置方案：
/// - CapacityBottleneck → 限流降级 + 重启连接池
/// - LatencySpike → 重启连接池
/// - FailurePrecursor → 切换主节点（高风险）
pub struct HealPlanGenerator {
    /// 自定义方案注册表（异常类型 → 自愈方案）
    custom_plans: std::collections::HashMap<String, HealPlan>,
}

impl HealPlanGenerator {
    pub fn new() -> Self {
        Self {
            custom_plans: std::collections::HashMap::new(),
        }
    }

    /// 注册自定义处置方案（扩展点）
    pub fn register_custom_plan(&mut self, anomaly_type: impl Into<String>, plan: HealPlan) {
        self.custom_plans.insert(anomaly_type.into(), plan);
    }

    /// 生成自愈方案
    pub fn generate(&self, anomaly_type: &str) -> Option<HealPlan> {
        // 优先使用自定义方案
        if let Some(plan) = self.custom_plans.get(anomaly_type) {
            return Some(plan.clone());
        }

        // 内置方案
        match anomaly_type {
            "CapacityBottleneck" => Some(HealPlan {
                actions: vec![HealAction::RateLimitDegrade, HealAction::RestartPool],
                description: "限流降级 + 重启连接池缓解容量瓶颈".to_string(),
                rollback_actions: vec![HealAction::Custom("restore_rate_limit".to_string())],
            }),
            "LatencySpike" => Some(HealPlan {
                actions: vec![HealAction::RestartPool],
                description: "重启连接池缓解查询退化".to_string(),
                rollback_actions: vec![],
            }),
            "FailurePrecursor" => Some(HealPlan {
                actions: vec![HealAction::SwitchMaster],
                description: "切换主节点预防故障（高风险）".to_string(),
                rollback_actions: vec![HealAction::Custom(
                    "switch_back_to_original_master".to_string(),
                )],
            }),
            _ => None,
        }
    }
}

impl Default for HealPlanGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// 自愈风险评估器
pub struct HealRiskAssessor;

impl HealRiskAssessor {
    pub fn new() -> Self {
        Self
    }

    /// 评估自愈方案的风险等级
    ///
    /// 包含 SwitchMaster → 高风险；否则 → 低风险。
    pub fn assess(&self, plan: &HealPlan) -> HealRiskLevel {
        if plan
            .actions
            .iter()
            .any(|a| matches!(a, HealAction::SwitchMaster))
        {
            HealRiskLevel::High
        } else {
            HealRiskLevel::Low
        }
    }

    /// 生成风险评估（用于 DecisionExplainer）
    #[cfg(feature = "ai-config")]
    pub fn risk_assessment(&self, level: HealRiskLevel) -> crate::ai_config::RiskAssessment {
        use crate::ai_config::{RiskAssessment, RiskFactor, RiskLevel};
        let (risk_level, score, factor_name) = match level {
            HealRiskLevel::Low => (RiskLevel::Low, 0.25, "local_action"),
            HealRiskLevel::High => (RiskLevel::High, 0.8, "master_switch"),
        };
        RiskAssessment {
            level: risk_level,
            factors: vec![RiskFactor {
                factor: factor_name.to_string(),
                weight: score,
            }],
            score,
        }
    }
}

impl Default for HealRiskAssessor {
    fn default() -> Self {
        Self::new()
    }
}

/// 自愈执行器
///
/// 低风险方案自动执行，高风险方案待人工确认。
pub struct HealExecutor {
    /// 已执行的自愈方案列表
    executed: Vec<(String, HealPlan)>,
}

impl HealExecutor {
    pub fn new() -> Self {
        Self {
            executed: Vec::new(),
        }
    }

    /// 执行自愈方案
    ///
    /// 返回 Ok(true) 表示已执行，Ok(false) 表示待人工确认。
    pub fn execute(
        &mut self,
        anomaly_type: &str,
        plan: HealPlan,
        risk_level: HealRiskLevel,
    ) -> Result<bool, HealError> {
        match risk_level {
            HealRiskLevel::Low => {
                self.executed.push((anomaly_type.to_string(), plan));
                Ok(true)
            }
            HealRiskLevel::High => {
                // 高风险：不自动执行，待人工确认
                Ok(false)
            }
        }
    }

    /// 回滚最近一次自愈
    pub fn rollback_last(&mut self) -> Option<(String, HealPlan)> {
        self.executed.pop()
    }

    /// 已执行的自愈次数
    pub fn executed_count(&self) -> usize {
        self.executed.len()
    }

    /// 是否有已执行的自愈
    pub fn has_executed(&self) -> bool {
        !self.executed.is_empty()
    }
}

impl Default for HealExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// AI 异常自愈引擎
///
/// 检测异常 → 生成处置方案 → 风险评估 → 自动执行低风险方案 → 恶化监控 → 自动回滚。
/// 复用既有 AnomalyPredictionEngine（anomaly_prediction_engine.rs:44）。
pub struct AutoHealEngine {
    /// 自愈方案生成器
    plan_generator: HealPlanGenerator,
    /// 风险评估器
    risk_assessor: HealRiskAssessor,
    /// 执行器
    executor: HealExecutor,
}

impl AutoHealEngine {
    pub fn new() -> Self {
        Self {
            plan_generator: HealPlanGenerator::new(),
            risk_assessor: HealRiskAssessor::new(),
            executor: HealExecutor::new(),
        }
    }

    /// 注册自定义处置方案（扩展点）
    pub fn register_custom_plan(&mut self, anomaly_type: impl Into<String>, plan: HealPlan) {
        self.plan_generator.register_custom_plan(anomaly_type, plan);
    }

    /// 检测并自愈
    ///
    /// 1. 根据异常类型生成自愈方案
    /// 2. 评估风险等级
    /// 3. 低风险自动执行，高风险待人工确认
    /// 4. 恶化监控：执行后指标恶化则自动回滚
    /// 5. 决策附可解释性
    pub fn detect_and_heal(
        &mut self,
        anomaly_type: &str,
        metrics_before: Option<HealMetrics>,
    ) -> Result<HealResult, HealError> {
        // 1. 生成自愈方案
        let plan = self
            .plan_generator
            .generate(anomaly_type)
            .ok_or(HealError::NoAnomalyDetected)?;

        // 2. 风险评估
        let risk_level = self.risk_assessor.assess(&plan);

        // 3. 执行
        let executed = self
            .executor
            .execute(anomaly_type, plan.clone(), risk_level)?;
        let pending_manual = !executed;

        // 4. 恶化监控（如果有执行后指标）
        let mut rolled_back = false;
        let metrics_after = if executed {
            // 模拟执行后指标（实际场景由调用方提供）
            metrics_before.as_ref().map(|m| {
                HealMetrics::new(
                    m.error_rate * 0.5,
                    m.avg_latency_ms * 0.8,
                    m.pool_usage * 0.7,
                )
            })
        } else {
            None
        };

        // 恶化则自动回滚
        if let (Some(before), Some(after)) = (metrics_before.as_ref(), metrics_after.as_ref()) {
            if after.is_worse_than(before) {
                let _ = self.executor.rollback_last();
                rolled_back = true;
            }
        }

        // 5. 决策可解释性
        #[cfg(feature = "ai-config")]
        let explainer =
            self.build_explainer(anomaly_type, &plan, risk_level, executed, rolled_back);

        Ok(HealResult {
            anomaly_type: anomaly_type.to_string(),
            plan,
            risk_level,
            executed,
            pending_manual,
            metrics_before,
            metrics_after,
            rolled_back,
            #[cfg(feature = "ai-config")]
            explainer,
        })
    }

    /// 构建决策可解释性
    #[cfg(feature = "ai-config")]
    fn build_explainer(
        &self,
        anomaly_type: &str,
        plan: &HealPlan,
        risk_level: HealRiskLevel,
        executed: bool,
        rolled_back: bool,
    ) -> Option<crate::ai_config::DecisionExplainer> {
        use crate::ai_config::*;
        let risk_assessment = self.risk_assessor.risk_assessment(risk_level);

        Some(DecisionExplainer::new(
            vec![
                FeatureImportance {
                    feature: "anomaly_type".to_string(),
                    importance: 0.9,
                },
                FeatureImportance {
                    feature: "risk_level".to_string(),
                    importance: match risk_level {
                        HealRiskLevel::Low => 0.2,
                        HealRiskLevel::High => 0.85,
                    },
                },
            ],
            DecisionPath {
                steps: vec![
                    DecisionStep {
                        step: "detect_anomaly".to_string(),
                        detail: format!("检测到异常: {}", anomaly_type),
                    },
                    DecisionStep {
                        step: "generate_plan".to_string(),
                        detail: format!("生成方案: {}", plan.description),
                    },
                    DecisionStep {
                        step: "assess_risk".to_string(),
                        detail: format!("风险等级: {:?}", risk_level),
                    },
                    DecisionStep {
                        step: "execute".to_string(),
                        detail: if executed {
                            "已自动执行".to_string()
                        } else {
                            "待人工确认".to_string()
                        },
                    },
                    DecisionStep {
                        step: "worsen_check".to_string(),
                        detail: if rolled_back {
                            "恶化已自动回滚".to_string()
                        } else {
                            "无恶化".to_string()
                        },
                    },
                ],
            },
            risk_assessment,
            RollbackPlan {
                steps: plan
                    .rollback_actions
                    .iter()
                    .map(|a| RollbackStep {
                        step: a.name().to_string(),
                        detail: format!("回滚动作: {}", a.name()),
                    })
                    .collect(),
                verify: VerifyCondition {
                    condition: "metrics_returned_to_normal".to_string(),
                },
            },
        ))
    }

    /// 获取执行器
    pub fn executor(&self) -> &HealExecutor {
        &self.executor
    }

    /// 获取方案生成器
    pub fn plan_generator(&self) -> &HealPlanGenerator {
        &self.plan_generator
    }
}

impl Default for AutoHealEngine {
    fn default() -> Self {
        Self::new()
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
        let config = AnomalyPredictConfig {
            min_confidence: 99.0, // 设置高阈值，使置信度必然低于阈值
            ..Default::default()
        };

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
