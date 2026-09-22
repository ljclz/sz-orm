//! v8.0.0 AI 能力深化模块
//!
//! 提供 NL 查询重写闭环、schema 设计建议、异常预测、等价性验证四大能力。
//! 子能力通过 feature gate 门控：
//! - `ai-deep`：基础聚合（等价性验证器 + 审计 + 脱敏）
//! - `ai-nl-rewrite`：NL 查询重写闭环
//! - `ai-schema-design`：schema 设计建议器
//! - `ai-anomaly-predict`：异常预测引擎

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// AI 能力深化错误类型
#[derive(Debug, Error)]
pub enum AiDeepError {
    /// DB 连接失败
    #[error("DB 连接失败: {0}")]
    DbConnectionFailed(String),
    /// NL2SQL 解析失败（错误码 NL2SQL_PARSE_FAILED）
    #[error("NL2SQL 解析失败: {0}")]
    Nl2SqlParseFailed(String),
    /// 重写等价性验证失败（告警 REWRITE_EQUIVALENCE_FAILED）
    #[error("重写等价性验证失败: {0}")]
    RewriteEquivalenceFailed(String),
    /// Schema 设计数据不足（告警 SCHEMA_DESIGN_DATA_INSUFFICIENT）
    #[error("Schema 设计数据不足: {0}")]
    SchemaDesignDataInsufficient(String),
    /// 异常预测置信度低（告警 FORECAST_LOW_CONFIDENCE）
    #[error("异常预测置信度低: {0}")]
    ForecastLowConfidence(String),
    /// 注入检测命中
    #[error("注入检测命中: {0}")]
    InjectionDetected(String),
    /// 内部错误
    #[error("内部错误: {0}")]
    Internal(String),
}

impl AiDeepError {
    /// 错误码
    pub fn error_code(&self) -> &str {
        match self {
            AiDeepError::DbConnectionFailed(_) => "DB_CONNECTION_FAILED",
            AiDeepError::Nl2SqlParseFailed(_) => "NL2SQL_PARSE_FAILED",
            AiDeepError::RewriteEquivalenceFailed(_) => "REWRITE_EQUIVALENCE_FAILED",
            AiDeepError::SchemaDesignDataInsufficient(_) => "SCHEMA_DESIGN_DATA_INSUFFICIENT",
            AiDeepError::ForecastLowConfidence(_) => "FORECAST_LOW_CONFIDENCE",
            AiDeepError::InjectionDetected(_) => "INJECTION_DETECTED",
            AiDeepError::Internal(_) => "AI_DEEP_INTERNAL",
        }
    }
}

/// 等价性验证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquivalenceResult {
    /// 是否等价
    pub is_equivalent: bool,
    /// 原 SQL 行数
    pub original_row_count: usize,
    /// 改写 SQL 行数
    pub rewritten_row_count: usize,
    /// 差异详情（不等价时记录）
    pub diff_details: Option<String>,
}

impl EquivalenceResult {
    /// 等价
    pub fn equivalent(row_count: usize) -> Self {
        Self {
            is_equivalent: true,
            original_row_count: row_count,
            rewritten_row_count: row_count,
            diff_details: None,
        }
    }

    /// 不等价
    pub fn not_equivalent(
        original_row_count: usize,
        rewritten_row_count: usize,
        diff_details: String,
    ) -> Self {
        Self {
            is_equivalent: false,
            original_row_count,
            rewritten_row_count,
            diff_details: Some(diff_details),
        }
    }
}

/// NL 查询重写结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NlRewriteResult {
    /// 原始 SQL
    pub original_sql: String,
    /// 重写后 SQL
    pub rewritten_sql: String,
    /// 等价性是否已验证
    pub equivalence_verified: bool,
    /// 注入是否被过滤
    pub injection_filtered: bool,
    /// 可解释报告
    pub explanation: String,
    /// 应用的变换列表
    pub transforms_applied: Vec<String>,
}

/// 查询模式（schema 设计建议器输入）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPattern {
    /// SQL 模板（参数化后）
    pub sql_template: String,
    /// 查询频率
    pub frequency: u64,
    /// 访问的列
    pub columns_accessed: Vec<String>,
    /// 涉及的表
    pub tables: Vec<String>,
    /// 数据天数（用于判断数据是否充足）
    pub data_days: u32,
}

/// 表建议
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSuggestion {
    pub table_name: String,
    pub suggestion: String,
    pub rationale: String,
}

/// 索引建议
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexSuggestion {
    pub table_name: String,
    pub index_columns: Vec<String>,
    pub ddl_text: String,
    pub expected_benefit: f64,
}

/// 反范式建议
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DenormSuggestion {
    pub target_table: String,
    pub redundant_column: String,
    pub source_table: String,
    pub source_column: String,
    pub rationale: String,
}

/// 分区建议
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionSuggestion {
    pub table_name: String,
    pub partition_column: String,
    pub partition_strategy: String,
    pub rationale: String,
}

/// Schema 设计报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaDesignReport {
    /// 表建议
    pub table_suggestions: Vec<TableSuggestion>,
    /// 索引建议
    pub index_suggestions: Vec<IndexSuggestion>,
    /// 反范式建议
    pub denorm_suggestions: Vec<DenormSuggestion>,
    /// 分区建议
    pub partition_suggestions: Vec<PartitionSuggestion>,
    /// 预期收益（0.0 ~ 1.0）
    pub expected_benefit: f64,
    /// 可解释报告
    pub explanation: String,
}

/// 时序数据点（异常预测引擎输入）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesData {
    /// Unix 时间戳（秒）
    pub timestamp: i64,
    /// 指标值
    pub value: f64,
    /// 指标名称
    pub metric_name: String,
}

/// 异常预测
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyPrediction {
    /// 预测的异常类型
    pub anomaly_type: String,
    /// 预测发生时间（Unix 秒）
    pub predicted_at: i64,
    /// 置信度（0.0 ~ 100.0）
    pub confidence: f64,
    /// 建议动作
    pub suggested_action: String,
}

/// 异常预测报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyPredictionReport {
    /// 预测列表
    pub predictions: Vec<AnomalyPrediction>,
    /// 整体置信度
    pub confidence: f64,
    /// 是否已脱敏
    pub masked: bool,
    /// 可解释报告（已脱敏）
    pub explanation: String,
    /// 低置信度标注
    pub low_confidence_flagged: bool,
}

/// 审计事件类型
#[derive(Debug, Clone, Copy)]
pub enum AiDeepEventType {
    /// NL 重写
    NlRewrite,
    /// Schema 设计
    SchemaDesign,
    /// 异常预测
    AnomalyPredict,
}

impl AiDeepEventType {
    /// 事件类型字符串
    pub fn as_str(&self) -> &str {
        match self {
            AiDeepEventType::NlRewrite => "nl_rewrite",
            AiDeepEventType::SchemaDesign => "schema_design",
            AiDeepEventType::AnomalyPredict => "anomaly_predict",
        }
    }
}

/// 记录审计日志（复用 AutonomousDecisionAuditor）
pub fn record_audit(
    auditor: &sz_orm_audit::AutonomousDecisionAuditor,
    event_type: AiDeepEventType,
    action_type: &str,
    reasoning: &str,
    success: bool,
) -> Result<String, String> {
    let entry =
        sz_orm_audit::AuditEntryBuilder::new(event_type.as_str(), "ai_deep_policy", action_type)
            .reasoning(reasoning)
            .execution(success, if success { "成功" } else { "失败" })
            .verification(success)
            .build();
    auditor.record(entry)
}

/// 脱敏可解释报告中的敏感值
///
/// 使用 `ContextAwareMasker` 对报告中的敏感字段值进行脱敏。
/// 扫描报告中的敏感关键词（password/token/secret/key/credential），
/// 对其后的值进行脱敏替换。
pub fn mask_explanation(
    explanation: &str,
    masker: &sz_orm_masking::dynamic_masking::ContextAwareMasker,
) -> String {
    use sz_orm_masking::dynamic_masking::{DataFlow, MaskingContext};

    let context = MaskingContext::new("analyst", "ai_deep_report", DataFlow::Outbound);
    let sensitive_fields = [
        "password",
        "token",
        "secret",
        "key",
        "credential",
        "ip",
        "host",
    ];

    let mut result = explanation.to_string();
    for field in sensitive_fields {
        // 使用 ContextAwareMasker 对字段值脱敏
        let masked_value = masker.mask_with_context("placeholder", field, &context);
        if masked_value != "placeholder" {
            // 规则匹配，替换文本中该字段后的值
            let field_eq = format!("{}=", field);
            if result.contains(&field_eq) {
                let parts: Vec<&str> = result.splitn(2, &field_eq).collect();
                if parts.len() == 2 {
                    let after = parts[1];
                    let value_end = after
                        .find(|c: char| c.is_whitespace() || c == '\n' || c == ',')
                        .unwrap_or(after.len());
                    let original_value = &after[..value_end];
                    let masked = masker.mask_with_context(original_value, field, &context);
                    result = format!("{}{}={}", parts[0], field, masked) + &after[value_end..];
                }
            }
        }
    }
    result
}

// 子模块导出（feature gate 门控）
pub mod equivalence_verifier;

#[cfg(feature = "ai-nl-rewrite")]
pub mod nl_rewrite_pipeline;
#[cfg(feature = "ai-nl-rewrite")]
pub use nl_rewrite_pipeline::{NlRewriteConfig, NlRewritePipeline};

#[cfg(feature = "ai-schema-design")]
pub mod schema_design_advisor;
#[cfg(feature = "ai-schema-design")]
pub use schema_design_advisor::{SchemaDesignAdvisor, SchemaDesignConfig};

#[cfg(feature = "ai-anomaly-predict")]
pub mod anomaly_prediction_engine;
#[cfg(feature = "ai-anomaly-predict")]
pub use anomaly_prediction_engine::{AnomalyPredictConfig, AnomalyPredictionEngine};

pub use equivalence_verifier::{EquivalenceConfig, RewriteEquivalenceVerifier};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_mapping() {
        assert_eq!(
            AiDeepError::DbConnectionFailed("test".to_string()).error_code(),
            "DB_CONNECTION_FAILED"
        );
        assert_eq!(
            AiDeepError::Nl2SqlParseFailed("test".to_string()).error_code(),
            "NL2SQL_PARSE_FAILED"
        );
        assert_eq!(
            AiDeepError::RewriteEquivalenceFailed("test".to_string()).error_code(),
            "REWRITE_EQUIVALENCE_FAILED"
        );
        assert_eq!(
            AiDeepError::SchemaDesignDataInsufficient("test".to_string()).error_code(),
            "SCHEMA_DESIGN_DATA_INSUFFICIENT"
        );
        assert_eq!(
            AiDeepError::ForecastLowConfidence("test".to_string()).error_code(),
            "FORECAST_LOW_CONFIDENCE"
        );
        assert_eq!(
            AiDeepError::InjectionDetected("test".to_string()).error_code(),
            "INJECTION_DETECTED"
        );
        assert_eq!(
            AiDeepError::Internal("test".to_string()).error_code(),
            "AI_DEEP_INTERNAL"
        );
    }

    #[test]
    fn test_equivalence_result_equivalent() {
        let result = EquivalenceResult::equivalent(42);
        assert!(result.is_equivalent);
        assert_eq!(result.original_row_count, 42);
        assert_eq!(result.rewritten_row_count, 42);
        assert!(result.diff_details.is_none());
    }

    #[test]
    fn test_equivalence_result_not_equivalent() {
        let result = EquivalenceResult::not_equivalent(10, 8, "行数不匹配".to_string());
        assert!(!result.is_equivalent);
        assert_eq!(result.original_row_count, 10);
        assert_eq!(result.rewritten_row_count, 8);
        assert_eq!(result.diff_details.as_deref(), Some("行数不匹配"));
    }

    #[test]
    fn test_event_type_as_str() {
        assert_eq!(AiDeepEventType::NlRewrite.as_str(), "nl_rewrite");
        assert_eq!(AiDeepEventType::SchemaDesign.as_str(), "schema_design");
        assert_eq!(AiDeepEventType::AnomalyPredict.as_str(), "anomaly_predict");
    }

    #[test]
    fn test_record_audit_success() {
        let auditor = sz_orm_audit::AutonomousDecisionAuditor::new();
        let result = record_audit(
            &auditor,
            AiDeepEventType::NlRewrite,
            "rewrite_query",
            "NL 重写成功",
            true,
        );
        assert!(result.is_ok());
        assert_eq!(auditor.entry_count(), 1);
        let entries = auditor.get_entries();
        assert_eq!(entries[0].event_type, "nl_rewrite");
    }

    #[test]
    fn test_record_audit_different_event_types() {
        let auditor = sz_orm_audit::AutonomousDecisionAuditor::new();
        record_audit(&auditor, AiDeepEventType::NlRewrite, "rewrite", "r1", true).unwrap();
        record_audit(
            &auditor,
            AiDeepEventType::SchemaDesign,
            "advise",
            "r2",
            true,
        )
        .unwrap();
        record_audit(
            &auditor,
            AiDeepEventType::AnomalyPredict,
            "predict",
            "r3",
            true,
        )
        .unwrap();
        let entries = auditor.get_entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].event_type, "nl_rewrite");
        assert_eq!(entries[1].event_type, "schema_design");
        assert_eq!(entries[2].event_type, "anomaly_predict");
    }

    #[test]
    fn test_mask_explanation_redacts_sensitive() {
        use sz_orm_masking::dynamic_masking::{
            ContextAwareMasker, MaskingRuleConfig, MaskingStrategy,
        };
        let rules = vec![MaskingRuleConfig::new(
            "password",
            MaskingStrategy::Replace("***".to_string()),
        )];
        let masker = ContextAwareMasker::new(rules);
        let masked = mask_explanation("password=s3cr3t", &masker);
        // 脱敏后不应包含原始敏感值
        assert!(!masked.contains("s3cr3t"));
    }
}
