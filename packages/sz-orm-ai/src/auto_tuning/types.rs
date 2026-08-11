//! AI 自动调优报告类型定义

use super::TuningSuggestion;
use std::time::Duration;

/// 慢查询信息
#[derive(Debug, Clone)]
pub struct SlowQueryInfo {
    /// SQL 文本
    pub sql: String,
    /// 执行耗时
    pub elapsed: Duration,
    /// EXPLAIN 解析信号（全表扫描/索引缺失等）
    pub signals: Vec<String>,
}

/// Detect 阶段报告
#[derive(Debug, Clone)]
pub struct DetectReport {
    /// 检测到的慢查询列表
    pub slow_queries: Vec<SlowQueryInfo>,
    /// 慢查询阈值
    pub threshold: Duration,
}

/// Advise 阶段报告
#[derive(Debug, Clone)]
pub struct AdviseReport {
    /// 生成的调优建议列表
    pub suggestions: Vec<TuningSuggestion>,
}

/// 已执行的建议
#[derive(Debug, Clone)]
pub struct AppliedSuggestion {
    /// 建议内容
    pub suggestion: TuningSuggestion,
    /// 执行时间
    pub apply_time: std::time::Instant,
}

/// 跳过的建议
#[derive(Debug, Clone)]
pub struct SkippedSuggestion {
    /// 建议内容
    pub suggestion: TuningSuggestion,
    /// 跳过原因
    pub reason: String,
}

/// Apply 阶段报告
#[derive(Debug, Clone)]
pub struct ApplyReport {
    /// 已执行的建议列表
    pub applied: Vec<AppliedSuggestion>,
    /// 跳过的建议列表
    pub skipped: Vec<SkippedSuggestion>,
}

/// 单条建议验证结果
#[derive(Debug, Clone)]
pub struct VerifyResult {
    /// 建议索引
    pub suggestion_id: usize,
    /// 调优前耗时（ms）
    pub before_ms: f64,
    /// 调优后耗时（ms）
    pub after_ms: f64,
    /// 提升百分比（正=提升，负=回退）
    pub gain_pct: f64,
    /// 是否回归
    pub is_regression: bool,
}

/// 回归记录
#[derive(Debug, Clone)]
pub struct RegressionRecord {
    /// 导致回归的建议
    pub suggestion: TuningSuggestion,
    /// 调优前耗时（ms）
    pub before_ms: f64,
    /// 调优后耗时（ms）
    pub after_ms: f64,
    /// 回滚是否成功
    pub rollback_succeeded: bool,
}

/// Verify 阶段报告
#[derive(Debug, Clone)]
pub struct VerifyReport {
    /// 验证结果列表
    pub results: Vec<VerifyResult>,
    /// 回归记录列表
    pub regressions: Vec<RegressionRecord>,
}
