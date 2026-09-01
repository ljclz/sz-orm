//! LlmDiagnoser 单元测试

#![cfg(feature = "llm-diagnosis")]

use sz_orm_diagnosis::llm_diagnoser::{
    DiagnosisResult, DiagnosisSource, FixSuggestion, LlmDiagnoser,
};
use sz_orm_diagnosis::root_cause::{RootCause, Severity};

#[test]
fn test_from_rule_result_pool_exhaustion() {
    let result = LlmDiagnoser::from_rule_result(RootCause::PoolExhaustion, Severity::Critical);
    assert_eq!(result.root_cause, RootCause::PoolExhaustion);
    assert_eq!(result.source, DiagnosisSource::Rule);
    assert!(!result.low_confidence);
    assert!(!result.fix_suggestions.is_empty());
    assert!(result.confidence >= 0.7);
}

#[test]
fn test_from_rule_result_info_low_confidence() {
    let result = LlmDiagnoser::from_rule_result(RootCause::Unknown, Severity::Info);
    assert_eq!(result.root_cause, RootCause::Unknown);
    assert!(result.low_confidence);
    assert!(result.confidence < 0.7);
}

#[test]
fn test_from_rule_result_all_causes() {
    let causes = [
        RootCause::PoolExhaustion,
        RootCause::SqlInefficiency,
        RootCause::LargeResultSet,
        RootCause::BuildOverhead,
        RootCause::MixedCause,
        RootCause::Unknown,
    ];
    for cause in causes {
        let result = LlmDiagnoser::from_rule_result(cause, Severity::Warning);
        assert_eq!(result.root_cause, cause);
        assert_eq!(result.source, DiagnosisSource::Rule);
    }
}

#[test]
fn test_diagnosis_result_serialization() {
    let result = DiagnosisResult {
        root_cause: RootCause::SqlInefficiency,
        confidence: 0.85,
        fix_suggestions: vec![FixSuggestion {
            description: "添加索引".to_string(),
            expected_benefit: "降低查询耗时".to_string(),
            source: DiagnosisSource::Llm,
        }],
        source: DiagnosisSource::Llm,
        low_confidence: false,
    };
    let json = serde_json::to_string(&result).unwrap();
    let deserialized: DiagnosisResult = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.root_cause, RootCause::SqlInefficiency);
    assert_eq!(deserialized.confidence, 0.85);
    assert_eq!(deserialized.fix_suggestions.len(), 1);
}

#[test]
fn test_fix_suggestion_source_distinction() {
    let rule_suggestion = FixSuggestion {
        description: "规则建议".to_string(),
        expected_benefit: "规则收益".to_string(),
        source: DiagnosisSource::Rule,
    };
    let llm_suggestion = FixSuggestion {
        description: "LLM 建议".to_string(),
        expected_benefit: "LLM 收益".to_string(),
        source: DiagnosisSource::Llm,
    };
    assert_ne!(rule_suggestion.source, llm_suggestion.source);
}
