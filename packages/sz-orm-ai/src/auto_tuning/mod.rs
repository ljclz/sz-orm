//! AI 自动调优闭环（v4.0.0 M2）
//!
//! 提供 `AutoTuningPipeline` 编排四阶段闭环：Detect→Advise→Apply→Verify，
//! 复用既有 `UnifiedQueryOptimizer`/`IndexAdvisor`/`RewriteAdvisor`/`ExplainPlanParser`，
//! 低风险自动执行，回归自动回滚，输出调优报告。

pub mod detector;
pub mod pipeline;
pub mod types;

pub use detector::{SlowQueryDetector, TuningConnection};
pub use pipeline::AutoTuningPipeline;
pub use types::{
    AdviseReport, AppliedSuggestion, ApplyReport, DetectReport, RegressionRecord,
    SkippedSuggestion, SlowQueryInfo, VerifyReport, VerifyResult,
};

use std::time::Duration;

/// 调优风险等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    /// 低风险（自动执行）
    Low,
    /// 中风险（需确认）
    Medium,
    /// 高风险（不自动执行）
    High,
}

/// 建议类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionType {
    /// 添加索引
    Index,
    /// SQL 重写
    Rewrite,
    /// Schema 变更
    Schema,
}

/// 自动调优配置
#[derive(Debug, Clone)]
pub struct AutoTuningConfig {
    /// 慢查询阈值（默认 1s）
    pub slow_query_threshold: Duration,
    /// 风险阈值（仅自动执行低于等于此风险的建议，默认 Low）
    pub risk_threshold: RiskLevel,
    /// 最大建议数（默认 10）
    pub max_suggestions: usize,
    /// 回归阈值（回退百分比，默认 0.1 = 10%）
    pub regression_threshold: f64,
    /// 验证采样次数（默认 3）
    pub verify_samples: u32,
}

impl Default for AutoTuningConfig {
    fn default() -> Self {
        Self {
            slow_query_threshold: Duration::from_secs(1),
            risk_threshold: RiskLevel::Low,
            max_suggestions: 10,
            regression_threshold: 0.1,
            verify_samples: 3,
        }
    }
}

/// 调优建议
#[derive(Debug, Clone)]
pub struct TuningSuggestion {
    /// 建议类型
    pub suggestion_type: SuggestionType,
    /// 调优前 SQL
    pub sql_before: String,
    /// 调优后 SQL（或 DDL，如 CREATE INDEX）
    pub sql_after: String,
    /// 预期收益（百分比）
    pub expected_gain: Option<f32>,
    /// 风险等级
    pub risk: RiskLevel,
    /// 建议原因
    pub reason: String,
}

/// 自动调优报告（四阶段完整报告）
#[derive(Debug, Clone)]
pub struct AutoTuningReport {
    /// Detect 阶段报告
    pub detect: DetectReport,
    /// Advise 阶段报告
    pub advise: AdviseReport,
    /// Apply 阶段报告
    pub apply: ApplyReport,
    /// Verify 阶段报告
    pub verify: VerifyReport,
    /// 采纳率（applied / total_suggestions）
    pub adoption_rate: f64,
    /// 回归记录
    pub regressions: Vec<RegressionRecord>,
}

impl AutoTuningReport {
    /// 计算采纳率
    pub fn compute_adoption_rate(applied: usize, total: usize) -> f64 {
        if total == 0 {
            0.0
        } else {
            applied as f64 / total as f64
        }
    }
}

/// 调优错误类型
#[derive(Debug, thiserror::Error)]
pub enum TuningError {
    /// 检测错误
    #[error("detect error: {0}")]
    Detect(String),

    /// 建议生成错误
    #[error("advise error: {0}")]
    Advise(String),

    /// 执行错误
    #[error("apply error: {0}")]
    Apply(String),

    /// 验证错误
    #[error("verify error: {0}")]
    Verify(String),

    /// 回滚错误
    #[error("rollback error: {0}")]
    Rollback(String),

    /// 连接错误
    #[error("connection error: {0}")]
    Connection(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_tuning_config_default() {
        let config = AutoTuningConfig::default();
        assert_eq!(config.slow_query_threshold, Duration::from_secs(1));
        assert_eq!(config.risk_threshold, RiskLevel::Low);
        assert_eq!(config.max_suggestions, 10);
        assert_eq!(config.regression_threshold, 0.1);
        assert_eq!(config.verify_samples, 3);
    }

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
    }

    #[test]
    fn test_adoption_rate_calculation() {
        assert_eq!(AutoTuningReport::compute_adoption_rate(8, 10), 0.8);
        assert_eq!(AutoTuningReport::compute_adoption_rate(0, 0), 0.0);
        assert_eq!(AutoTuningReport::compute_adoption_rate(10, 10), 1.0);
    }

    #[test]
    fn test_tuning_suggestion_construction() {
        let suggestion = TuningSuggestion {
            suggestion_type: SuggestionType::Index,
            sql_before: "SELECT * FROM users WHERE name LIKE '%foo%'".to_string(),
            sql_after: "CREATE INDEX idx_users_name ON users(name)".to_string(),
            expected_gain: Some(50.0),
            risk: RiskLevel::Low,
            reason: "Full table scan detected, add index on name column".to_string(),
        };
        assert_eq!(suggestion.suggestion_type, SuggestionType::Index);
        assert_eq!(suggestion.risk, RiskLevel::Low);
        assert!(suggestion.expected_gain.is_some());
    }

    #[test]
    fn test_auto_tuning_report_construction() {
        let report = AutoTuningReport {
            detect: DetectReport {
                slow_queries: vec![],
                threshold: Duration::from_secs(1),
            },
            advise: AdviseReport {
                suggestions: vec![],
            },
            apply: ApplyReport {
                applied: vec![],
                skipped: vec![],
            },
            verify: VerifyReport {
                results: vec![],
                regressions: vec![],
            },
            adoption_rate: 0.0,
            regressions: vec![],
        };
        assert_eq!(report.adoption_rate, 0.0);
        assert!(report.detect.slow_queries.is_empty());
    }

    #[test]
    fn test_tuning_error_display() {
        let err = TuningError::Detect("no slow queries".to_string());
        assert!(err.to_string().contains("detect error"));

        let err = TuningError::Advise("LLM unavailable".to_string());
        assert!(err.to_string().contains("advise error"));

        let err = TuningError::Apply("permission denied".to_string());
        assert!(err.to_string().contains("apply error"));

        let err = TuningError::Verify("timeout".to_string());
        assert!(err.to_string().contains("verify error"));

        let err = TuningError::Rollback("DROP INDEX failed".to_string());
        assert!(err.to_string().contains("rollback error"));
    }
}
