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
    /// 写入开销系数（>= 0.0，0 表示无额外写入开销，1 表示写入开销翻倍）
    /// v5.1.0 新增，向后兼容（旧代码不感知，默认 0.0）
    pub write_overhead: f64,
    /// 存储开销（MB，>= 0.0）
    /// v5.1.0 新增，向后兼容（旧代码不感知，默认 0.0）
    pub storage_cost_mb: f64,
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
            write_overhead: 0.0,
            storage_cost_mb: 0.0,
        }
    }

    /// 创建不确定的收益评估
    pub fn uncertain(speedup_ratio: f64, confidence: f32) -> Self {
        Self {
            speedup_ratio,
            confidence,
            uncertain: true,
            write_overhead: 0.0,
            storage_cost_mb: 0.0,
        }
    }

    /// 设置写入开销
    pub fn with_write_overhead(mut self, overhead: f64) -> Self {
        self.write_overhead = overhead;
        self
    }

    /// 设置存储开销
    pub fn with_storage_cost(mut self, cost_mb: f64) -> Self {
        self.storage_cost_mb = cost_mb;
        self
    }

    /// 综合评分（加速比 - 写入开销惩罚 - 存储开销惩罚）
    ///
    /// 用于索引组合优化时排序候选索引。
    pub fn composite_score(&self) -> f64 {
        self.speedup_ratio - self.write_overhead * 0.5 - self.storage_cost_mb * 0.01
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
        assert!((be.write_overhead - 0.0).abs() < 1e-6);
        assert!((be.storage_cost_mb - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_benefit_estimate_uncertain() {
        let be = BenefitEstimate::uncertain(2.0, 0.5);
        assert!(be.uncertain);
    }

    #[test]
    fn test_benefit_estimate_with_overhead_and_storage() {
        let be = BenefitEstimate::certain(5.0, 0.9)
            .with_write_overhead(0.3)
            .with_storage_cost(10.0);
        assert!((be.write_overhead - 0.3).abs() < 1e-6);
        assert!((be.storage_cost_mb - 10.0).abs() < 1e-6);
        // composite_score = 5.0 - 0.3*0.5 - 10.0*0.01 = 5.0 - 0.15 - 0.1 = 4.75
        assert!((be.composite_score() - 4.75).abs() < 1e-6);
    }
}
