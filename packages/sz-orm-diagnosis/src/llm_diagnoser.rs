//! LLM 驱动故障诊断模块
//!
//! 规则引擎返回 MixedCause 时，将故障上下文发送 LLM 请求根因判断 + 修复建议。
//! 启用 `llm-diagnosis` feature 后可用（需传入 LlmRouter）。
//! 未启用时零编译开销。

use serde::{Deserialize, Serialize};

use crate::root_cause::{RootCause, Severity};

/// 修复建议
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixSuggestion {
    /// 建议描述
    pub description: String,
    /// 预期收益
    pub expected_benefit: String,
    /// 建议来源（Rule / Llm）
    pub source: DiagnosisSource,
}

/// 诊断来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosisSource {
    /// 规则引擎
    Rule,
    /// LLM 生成
    Llm,
}

/// 诊断结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosisResult {
    /// 根因
    pub root_cause: RootCause,
    /// 置信度（0.0 ~ 1.0）
    pub confidence: f32,
    /// 修复建议列表
    pub fix_suggestions: Vec<FixSuggestion>,
    /// 诊断来源
    pub source: DiagnosisSource,
    /// 是否低置信度（< 0.7，建议人工复核）
    pub low_confidence: bool,
}

/// LLM 诊断错误
#[derive(Debug, thiserror::Error)]
pub enum LlmDiagnosisError {
    #[error("Diagnosis error: {0}")]
    DiagnosisFailed(String),
    #[error("LLM service unavailable: {0}")]
    LlmUnavailable(String),
}

/// LLM 驱动故障诊断器
///
/// 规则引擎返回 MixedCause 时调用 LLM 请求根因判断 + 修复建议。
/// 传入 LlmRouter 后启用（需 `llm-diagnosis` feature）。
pub struct LlmDiagnoser {
    llm_router: std::sync::Arc<sz_orm_ai::llm_provider::LlmRouter>,
}

impl LlmDiagnoser {
    /// 创建 LLM 诊断器
    pub fn new(llm_router: std::sync::Arc<sz_orm_ai::llm_provider::LlmRouter>) -> Self {
        Self { llm_router }
    }

    /// LLM 驱动诊断
    ///
    /// 将故障上下文（慢查询 + 执行计划 + 池状态）发送 LLM 请求根因判断 + 修复建议。
    /// 置信度 < 0.7 时标注"低置信度，建议人工复核"警告。
    pub async fn diagnose(
        &self,
        slow_sql: &str,
        execution_plan: &str,
        pool_status: &str,
    ) -> Result<DiagnosisResult, LlmDiagnosisError> {
        let prompt = format!(
            "Diagnose the following slow query issue:\n\
             SQL: {}\n\
             Execution Plan: {}\n\
             Pool Status: {}\n\
             Return JSON with: root_cause (one of: pool-exhaustion, sql-inefficiency, large-result-set, build-overhead, mixed-cause, unknown), \
             confidence (0.0-1.0), fix_suggestions (array of {{description, expected_benefit}})",
            slow_sql, execution_plan, pool_status
        );

        let config = sz_orm_ai::llm_provider::LlmRequestConfig::default();
        let resp = self
            .llm_router
            .complete(&prompt, &config)
            .await
            .map_err(|e| LlmDiagnosisError::LlmUnavailable(e.to_string()))?;

        self.parse_llm_response(&resp.text)
    }

    /// 解析 LLM 响应为 DiagnosisResult
    fn parse_llm_response(
        &self,
        response_text: &str,
    ) -> Result<DiagnosisResult, LlmDiagnosisError> {
        let resp = response_text.trim();

        // 尝试解析 JSON 响应
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(resp) {
            let root_cause = json
                .get("root_cause")
                .and_then(|v| v.as_str())
                .map(parse_root_cause)
                .unwrap_or(RootCause::Unknown);

            let confidence = json
                .get("confidence")
                .and_then(|v| v.as_f64())
                .map(|f| f as f32)
                .unwrap_or(0.5);

            let fix_suggestions = json
                .get("fix_suggestions")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| {
                            let description = item
                                .get("description")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let expected_benefit = item
                                .get("expected_benefit")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if description.is_empty() {
                                None
                            } else {
                                Some(FixSuggestion {
                                    description: description.to_string(),
                                    expected_benefit: expected_benefit.to_string(),
                                    source: DiagnosisSource::Llm,
                                })
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();

            let low_confidence = confidence < 0.7;
            if low_confidence {
                tracing::warn!("LLM diagnosis low confidence: {},建议人工复核", confidence);
            }

            return Ok(DiagnosisResult {
                root_cause,
                confidence,
                fix_suggestions,
                source: DiagnosisSource::Llm,
                low_confidence,
            });
        }

        // JSON 解析失败，返回 Unknown + 低置信度
        Ok(DiagnosisResult {
            root_cause: RootCause::Unknown,
            confidence: 0.3,
            fix_suggestions: vec![],
            source: DiagnosisSource::Llm,
            low_confidence: true,
        })
    }

    /// 规则引擎诊断结果转 DiagnosisResult
    pub fn from_rule_result(root_cause: RootCause, severity: Severity) -> DiagnosisResult {
        let confidence = match severity {
            Severity::Critical => 0.9,
            Severity::Warning => 0.75,
            Severity::Info => 0.6,
        };
        let fix_suggestions = match root_cause {
            RootCause::PoolExhaustion => vec![FixSuggestion {
                description: "增加连接池大小或优化连接获取逻辑".to_string(),
                expected_benefit: "降低 PoolAcquire 耗时".to_string(),
                source: DiagnosisSource::Rule,
            }],
            RootCause::SqlInefficiency => vec![FixSuggestion {
                description: "优化 SQL 查询或添加索引".to_string(),
                expected_benefit: "降低 SqlExecute 耗时".to_string(),
                source: DiagnosisSource::Rule,
            }],
            RootCause::LargeResultSet => vec![FixSuggestion {
                description: "添加分页或减少返回列".to_string(),
                expected_benefit: "降低 ResultMap 耗时".to_string(),
                source: DiagnosisSource::Rule,
            }],
            RootCause::BuildOverhead => vec![FixSuggestion {
                description: "简化查询构造逻辑".to_string(),
                expected_benefit: "降低 Build 耗时".to_string(),
                source: DiagnosisSource::Rule,
            }],
            _ => vec![],
        };

        DiagnosisResult {
            root_cause,
            confidence,
            fix_suggestions,
            source: DiagnosisSource::Rule,
            low_confidence: confidence < 0.7,
        }
    }
}

fn parse_root_cause(s: &str) -> RootCause {
    match s {
        "pool-exhaustion" => RootCause::PoolExhaustion,
        "sql-inefficiency" => RootCause::SqlInefficiency,
        "large-result-set" => RootCause::LargeResultSet,
        "build-overhead" => RootCause::BuildOverhead,
        "mixed-cause" => RootCause::MixedCause,
        _ => RootCause::Unknown,
    }
}
