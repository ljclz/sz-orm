//! AI 建议公共类型（审计记录 + 收益评估）
//!
//! 当 `ai-nl2sql-enhanced` / `ai-index-advisor` / `ai-rewrite-advisor` 任一 feature 启用时编译。
//! 为三个 AI 建议模块提供共享的数据结构，避免跨 feature 依赖。

use serde::{Deserialize, Serialize};

/// AI 建议来源引擎
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdviceSource {
    /// 规则型分析
    Rule,
    /// LLM 生成
    Llm,
}

/// AI 建议类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdviceType {
    /// NL2SQL 转换
    Nl2Sql,
    /// 查询意图分析
    Intent,
    /// 索引建议
    Index,
    /// 查询重写建议
    Rewrite,
}

/// AI 建议审计记录
///
/// 每条 AI 建议均生成审计记录，包含来源引擎、LLM 模型标识、置信度、建议类型和时间戳。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAdviceAuditRecord {
    /// 来源引擎
    pub source_engine: AdviceSource,
    /// LLM 模型标识（规则型为 None）
    pub llm_model: Option<String>,
    /// 置信度（0.0 ~ 1.0）
    pub confidence: f32,
    /// 建议类型
    pub advice_type: AdviceType,
    /// Unix 时间戳（秒）
    pub timestamp: i64,
}

/// 预期收益评估
///
/// 用于索引建议和重写建议的收益量化。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenefitEstimate {
    /// 预期加速比（> 1.0 表示有收益）
    pub speedup_ratio: f64,
    /// 置信度（0.0 ~ 1.0）
    pub confidence: f32,
    /// 收益不确定标注（当统计数据不足时为 true）
    pub uncertain: bool,
}

impl AiAdviceAuditRecord {
    /// 创建规则型建议审计记录
    pub fn from_rule(advice_type: AdviceType, confidence: f32) -> Self {
        Self {
            source_engine: AdviceSource::Rule,
            llm_model: None,
            confidence,
            advice_type,
            timestamp: current_timestamp(),
        }
    }

    /// 创建 LLM 建议审计记录
    pub fn from_llm(advice_type: AdviceType, confidence: f32, model: impl Into<String>) -> Self {
        Self {
            source_engine: AdviceSource::Llm,
            llm_model: Some(model.into()),
            confidence,
            advice_type,
            timestamp: current_timestamp(),
        }
    }
}

impl BenefitEstimate {
    /// 创建确定的收益评估
    pub fn certain(speedup_ratio: f64, confidence: f32) -> Self {
        Self {
            speedup_ratio,
            confidence,
            uncertain: false,
        }
    }

    /// 创建不确定的收益评估
    pub fn uncertain(speedup_ratio: f64, confidence: f32) -> Self {
        Self {
            speedup_ratio,
            confidence,
            uncertain: true,
        }
    }
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_advice_source_serde() {
        let s = AdviceSource::Rule;
        let json = serde_json::to_string(&s).unwrap();
        let de: AdviceSource = serde_json::from_str(&json).unwrap();
        assert_eq!(s, de);
    }

    #[test]
    fn test_advice_type_serde() {
        let t = AdviceType::Index;
        let json = serde_json::to_string(&t).unwrap();
        let de: AdviceType = serde_json::from_str(&json).unwrap();
        assert_eq!(t, de);
    }

    #[test]
    fn test_audit_record_from_rule() {
        let record = AiAdviceAuditRecord::from_rule(AdviceType::Intent, 0.8);
        assert_eq!(record.source_engine, AdviceSource::Rule);
        assert!(record.llm_model.is_none());
        assert!((record.confidence - 0.8).abs() < 1e-6);
        assert_eq!(record.advice_type, AdviceType::Intent);
        assert!(record.timestamp > 0);
    }

    #[test]
    fn test_audit_record_from_llm() {
        let record = AiAdviceAuditRecord::from_llm(AdviceType::Nl2Sql, 0.9, "gpt-4o-mini");
        assert_eq!(record.source_engine, AdviceSource::Llm);
        assert_eq!(record.llm_model.as_deref(), Some("gpt-4o-mini"));
        assert!((record.confidence - 0.9).abs() < 1e-6);
        assert_eq!(record.advice_type, AdviceType::Nl2Sql);
    }

    #[test]
    fn test_benefit_estimate_certain() {
        let be = BenefitEstimate::certain(3.5, 0.8);
        assert!((be.speedup_ratio - 3.5).abs() < 1e-6);
        assert!(!be.uncertain);
    }

    #[test]
    fn test_benefit_estimate_uncertain() {
        let be = BenefitEstimate::uncertain(2.0, 0.5);
        assert!(be.uncertain);
    }
}
