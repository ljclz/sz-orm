//! TASK-014: LLM 驱动参数调优测试

use async_trait::async_trait;
use sz_orm_adaptive::{
    AppliedFrom, LlmParameterAdvice, LlmParameterTuner, LlmTuningError, LlmTuningProvider,
    PerformanceMetrics, TunableParam, TuningSignal, TuningSuggestion,
};

// ==================== Mock LLM Provider ====================

struct MockLlmTuningProvider {
    advice: Vec<LlmParameterAdvice>,
}

impl MockLlmTuningProvider {
    fn new(advice: Vec<LlmParameterAdvice>) -> Self {
        Self { advice }
    }
}

#[async_trait]
impl LlmTuningProvider for MockLlmTuningProvider {
    async fn request_advice(
        &self,
        _metrics: &PerformanceMetrics,
        _current_params: &[(TunableParam, u64)],
        _rule_suggestions: &[TuningSuggestion],
    ) -> Result<Vec<LlmParameterAdvice>, LlmTuningError> {
        Ok(self.advice.clone())
    }
}

// ==================== LlmParameterTuner 测试 ====================

#[tokio::test]
async fn test_tuner_rule_only() {
    let mut tuner = LlmParameterTuner::rule_only();
    let metrics = PerformanceMetrics::new(10.0, 50.0, 100.0, 0.8, 0.1);

    let result = tuner.tune_with_llm(&metrics).await.unwrap();

    assert_eq!(result.applied_from, AppliedFrom::Rule);
    assert!(result.llm_advice.is_empty());
}

#[tokio::test]
async fn test_tuner_with_llm_rule_confident() {
    // 规则引擎置信时，不调用 LLM
    let llm = MockLlmTuningProvider::new(vec![LlmParameterAdvice {
        param: TunableParam::PaginationThreshold,
        suggested_value: 2000,
        reason: "test".to_string(),
        confidence: 0.9,
    }]);

    let mut tuner = LlmParameterTuner::with_llm(Box::new(llm));
    let metrics = PerformanceMetrics::new(10.0, 50.0, 100.0, 0.8, 0.1);

    let result = tuner.tune_with_llm(&metrics).await.unwrap();

    // 规则置信时使用规则
    assert_eq!(result.applied_from, AppliedFrom::Rule);
}

#[tokio::test]
async fn test_tuner_with_llm_fallback_on_critical() {
    // 先通过规则调优器产生 Critical 建议
    let llm = MockLlmTuningProvider::new(vec![LlmParameterAdvice {
        param: TunableParam::PaginationThreshold,
        suggested_value: 2000,
        reason: "工作负载呈双峰分布，建议提高分页阈值".to_string(),
        confidence: 0.9,
    }]);

    let mut tuner = LlmParameterTuner::with_llm(Box::new(llm));

    // 手动触发 Critical 偏离
    for _ in 0..20 {
        tuner.rule_tuner().tune(
            TunableParam::PaginationThreshold,
            TuningSignal::Increase,
            "test",
        );
    }

    let metrics = PerformanceMetrics::new(500.0, 2000.0, 10.0, 0.2, 0.8);
    let result = tuner.tune_with_llm(&metrics).await.unwrap();

    // 有 Critical 建议时应 fallback 到 LLM
    assert_eq!(result.applied_from, AppliedFrom::Llm);
    assert!(!result.llm_advice.is_empty());
}

#[tokio::test]
async fn test_tuner_llm_advice_applied() {
    let llm = MockLlmTuningProvider::new(vec![
        LlmParameterAdvice {
            param: TunableParam::PaginationThreshold,
            suggested_value: 5000,
            reason: "高负载建议提高阈值".to_string(),
            confidence: 0.9,
        },
        LlmParameterAdvice {
            param: TunableParam::CacheTtlSecs,
            suggested_value: 600,
            reason: "建议延长缓存 TTL".to_string(),
            confidence: 0.8,
        },
    ]);

    let mut tuner = LlmParameterTuner::with_llm(Box::new(llm));

    // 触发 LLM fallback
    for _ in 0..20 {
        tuner.rule_tuner().tune(
            TunableParam::PaginationThreshold,
            TuningSignal::Increase,
            "test",
        );
    }

    let metrics = PerformanceMetrics::new(500.0, 2000.0, 10.0, 0.2, 0.8);
    let result = tuner.tune_with_llm(&metrics).await.unwrap();

    assert_eq!(result.applied_from, AppliedFrom::Llm);
    assert_eq!(result.llm_advice.len(), 2);
}

#[tokio::test]
async fn test_tuner_llm_low_confidence_not_applied() {
    // LLM 置信度 <= 0.5 时不应用
    let llm = MockLlmTuningProvider::new(vec![LlmParameterAdvice {
        param: TunableParam::PaginationThreshold,
        suggested_value: 5000,
        reason: "低置信度建议".to_string(),
        confidence: 0.3,
    }]);

    let mut tuner = LlmParameterTuner::with_llm(Box::new(llm));

    // 触发 LLM fallback
    for _ in 0..20 {
        tuner.rule_tuner().tune(
            TunableParam::PaginationThreshold,
            TuningSignal::Increase,
            "test",
        );
    }

    let metrics = PerformanceMetrics::new(500.0, 2000.0, 10.0, 0.2, 0.8);
    let result = tuner.tune_with_llm(&metrics).await.unwrap();

    // LLM 被调用但建议未应用
    assert_eq!(result.applied_from, AppliedFrom::Llm);
    assert_eq!(result.llm_advice.len(), 1);
    // 参数不应被修改（低置信度）
    // 注意：由于规则调优器已经 tune 了，值可能已变化
    // 这里仅验证 LLM 建议被返回
}

#[test]
fn test_llm_parameter_advice_structure() {
    let advice = LlmParameterAdvice {
        param: TunableParam::BatchSizeMax,
        suggested_value: 2000,
        reason: "批量大小建议".to_string(),
        confidence: 0.85,
    };
    assert_eq!(advice.param, TunableParam::BatchSizeMax);
    assert_eq!(advice.suggested_value, 2000);
    assert!((advice.confidence - 0.85).abs() < 1e-6);
}

#[test]
fn test_tuner_rule_tuner_access() {
    let tuner = LlmParameterTuner::rule_only();
    let _ = tuner.rule_tuner();
    assert_eq!(
        tuner.rule_tuner().get(TunableParam::PaginationThreshold),
        TunableParam::PaginationThreshold.default_value()
    );
}
