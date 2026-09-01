//! LLM 驱动参数调优模块
//!
//! 规则引擎无法确定最优参数时，将历史性能数据发送 LLM 请求参数建议。
//! LLM 建议附理由（如"工作负载呈双峰分布，建议连接池大小 25"）。
//!
//! 启用 `llm-tuning` feature 后可用。

use crate::param_tuner::{
    AdaptiveParameterTuner, PerformanceMetrics, SuggestionSeverity, TunableParam, TuningAdvisor,
    TuningSuggestion,
};
use thiserror::Error;

/// LLM 调优错误
#[derive(Debug, Error)]
pub enum LlmTuningError {
    /// LLM 调用错误
    #[error("LLM error: {0}")]
    Llm(String),
    /// 参数无效
    #[error("Invalid parameter: {0}")]
    InvalidParam(String),
}

/// LLM 参数建议
#[derive(Debug, Clone)]
pub struct LlmParameterAdvice {
    /// 参数
    pub param: TunableParam,
    /// 建议值
    pub suggested_value: u64,
    /// 建议理由
    pub reason: String,
    /// 置信度（0.0 ~ 1.0）
    pub confidence: f32,
}

/// LLM 调优 Provider trait
///
/// 抽象 LLM 调用，测试中用 Mock 实现。
#[async_trait::async_trait]
pub trait LlmTuningProvider: Send + Sync {
    /// 请求参数建议
    async fn request_advice(
        &self,
        metrics: &PerformanceMetrics,
        current_params: &[(TunableParam, u64)],
        rule_suggestions: &[TuningSuggestion],
    ) -> Result<Vec<LlmParameterAdvice>, LlmTuningError>;
}

/// LLM 驱动参数调优器
///
/// 包装规则调优器，规则引擎无法确定最优参数时 fallback 到 LLM。
pub struct LlmParameterTuner {
    /// 规则调优器
    rule_tuner: AdaptiveParameterTuner,
    /// LLM Provider
    llm_provider: Option<Box<dyn LlmTuningProvider>>,
}

impl LlmParameterTuner {
    /// 创建仅规则引擎的调优器
    pub fn rule_only() -> Self {
        Self {
            rule_tuner: AdaptiveParameterTuner::default(),
            llm_provider: None,
        }
    }

    /// 创建带 LLM 的调优器
    pub fn with_llm(llm_provider: Box<dyn LlmTuningProvider>) -> Self {
        Self {
            rule_tuner: AdaptiveParameterTuner::default(),
            llm_provider: Some(llm_provider),
        }
    }

    /// 获取规则调优器引用
    pub fn rule_tuner(&self) -> &AdaptiveParameterTuner {
        &self.rule_tuner
    }

    /// 获取规则调优器可变引用
    pub fn rule_tuner_mut(&mut self) -> &mut AdaptiveParameterTuner {
        &mut self.rule_tuner
    }

    /// 带规则的调优
    ///
    /// 先用规则引擎调优，规则无法确定时 fallback 到 LLM。
    pub async fn tune_with_llm(
        &mut self,
        metrics: &PerformanceMetrics,
    ) -> Result<TuningResult, LlmTuningError> {
        // 1. 规则引擎建议
        let advisor = TuningAdvisor::from_tuner(&self.rule_tuner);
        let rule_suggestions = advisor.suggestions();

        // 2. 收集当前参数值
        let current_params: Vec<(TunableParam, u64)> = TunableParam::all()
            .iter()
            .map(|&p| (p, self.rule_tuner.get(p)))
            .collect();

        // 3. 判断规则是否充分
        // 规则不充分：有 Critical 建议，或偏离 >= 30%，或性能指标差
        let has_critical = rule_suggestions
            .iter()
            .any(|s| s.severity == SuggestionSeverity::Critical || s.deviation_pct.abs() >= 30.0);
        let metrics_bad = metrics.avg_query_ms > 100.0 || metrics.cache_hit_rate < 0.3;
        let rule_confident = !has_critical && !metrics_bad;

        if rule_confident {
            return Ok(TuningResult {
                rule_suggestions,
                llm_advice: Vec::new(),
                applied_from: AppliedFrom::Rule,
            });
        }

        // 4. LLM fallback
        if let Some(llm) = &self.llm_provider {
            let llm_advice = llm
                .request_advice(metrics, &current_params, &rule_suggestions)
                .await?;

            // 应用 LLM 建议
            for advice in &llm_advice {
                if advice.confidence > 0.5 {
                    self.rule_tuner.set(advice.param, advice.suggested_value);
                }
            }

            return Ok(TuningResult {
                rule_suggestions,
                llm_advice,
                applied_from: AppliedFrom::Llm,
            });
        }

        // 5. 无 LLM，仅返回规则建议
        Ok(TuningResult {
            rule_suggestions,
            llm_advice: Vec::new(),
            applied_from: AppliedFrom::Rule,
        })
    }
}

/// 调优结果
#[derive(Debug, Clone)]
pub struct TuningResult {
    /// 规则引擎建议
    pub rule_suggestions: Vec<TuningSuggestion>,
    /// LLM 建议
    pub llm_advice: Vec<LlmParameterAdvice>,
    /// 应用来源
    pub applied_from: AppliedFrom,
}

/// 应用来源
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppliedFrom {
    /// 规则引擎
    Rule,
    /// LLM
    Llm,
}
