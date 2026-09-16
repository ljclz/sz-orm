//! 反馈循环与自动回滚（`query-auto-tuning` feature）
//!
//! 监控已应用调优建议的性能表现，检测回归后自动触发回滚。

use std::collections::HashMap;
use std::time::Instant;

use crate::suggestion::OptimizationSuggestion;

/// 性能采样
#[derive(Debug, Clone)]
pub struct PerformanceSample {
    /// 调优前耗时（ms）
    pub before_ms: f64,
    /// 调优后耗时（ms）
    pub after_ms: f64,
    /// 采样时间
    pub sampled_at: Instant,
}

impl PerformanceSample {
    /// 变化百分比（正=劣化，负=改善）
    pub fn change_pct(&self) -> f64 {
        if self.before_ms <= 0.0 {
            return 0.0;
        }
        (self.after_ms - self.before_ms) / self.before_ms * 100.0
    }

    /// 是否为回归
    pub fn is_regression(&self) -> bool {
        self.change_pct() > 0.0
    }
}

/// 性能趋势
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PerformanceTrend {
    /// 改善
    Improving,
    /// 稳定
    Stable,
    /// 劣化
    Degrading,
}

/// 回滚回调类型
pub type RollbackFn = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// 反馈循环监控器
#[derive(Debug)]
pub struct FeedbackLoop {
    /// 回归阈值（百分比，默认 10.0）
    regression_threshold_pct: f64,
    /// 稳定阈值（百分比，变化绝对值小于此值视为稳定，默认 5.0）
    stable_threshold_pct: f64,
    /// 最小采样数（达到后才判断趋势，默认 3）
    min_samples: usize,
    /// 采样历史
    samples: HashMap<String, Vec<PerformanceSample>>,
    /// 已应用的调优建议（用于回滚）
    applied: HashMap<String, OptimizationSuggestion>,
    /// 回滚记录
    rollbacks: HashMap<String, String>,
}

impl FeedbackLoop {
    pub fn new() -> Self {
        Self {
            regression_threshold_pct: 10.0,
            stable_threshold_pct: 5.0,
            min_samples: 3,
            samples: HashMap::new(),
            applied: HashMap::new(),
            rollbacks: HashMap::new(),
        }
    }

    /// 配置回归阈值
    pub fn with_regression_threshold(mut self, pct: f64) -> Self {
        self.regression_threshold_pct = pct;
        self
    }

    /// 配置稳定阈值
    pub fn with_stable_threshold(mut self, pct: f64) -> Self {
        self.stable_threshold_pct = pct;
        self
    }

    /// 配置最小采样数
    pub fn with_min_samples(mut self, n: usize) -> Self {
        self.min_samples = n;
        self
    }

    /// 注册已应用的建议（供回滚使用）
    pub fn register_applied(&mut self, query_key: &str, suggestion: OptimizationSuggestion) {
        self.applied.insert(query_key.to_string(), suggestion);
    }

    /// 记录性能采样
    pub fn record_sample(&mut self, query_key: &str, before_ms: f64, after_ms: f64) {
        self.samples
            .entry(query_key.to_string())
            .or_default()
            .push(PerformanceSample {
                before_ms,
                after_ms,
                sampled_at: Instant::now(),
            });
    }

    /// 检测是否回归
    pub fn detect_regression(&self, query_key: &str) -> bool {
        let trend = self.get_trend(query_key);
        trend == PerformanceTrend::Degrading
    }

    /// 获取性能趋势
    pub fn get_trend(&self, query_key: &str) -> PerformanceTrend {
        let samples = self.samples.get(query_key);
        let Some(samples) = samples else {
            return PerformanceTrend::Stable;
        };
        if samples.len() < self.min_samples {
            return PerformanceTrend::Stable;
        }
        let avg_change: f64 =
            samples.iter().map(|s| s.change_pct()).sum::<f64>() / samples.len() as f64;
        if avg_change > self.regression_threshold_pct {
            PerformanceTrend::Degrading
        } else if avg_change < -self.stable_threshold_pct {
            PerformanceTrend::Improving
        } else {
            PerformanceTrend::Stable
        }
    }

    /// 判断是否应该回滚
    pub fn should_rollback(&self, query_key: &str) -> bool {
        self.detect_regression(query_key) && self.applied.contains_key(query_key)
    }

    /// 执行自动回滚
    ///
    /// 返回 `Ok(reason)` 表示已回滚，`Err(reason)` 表示无法回滚。
    pub fn auto_rollback(&mut self, query_key: &str) -> Result<String, String> {
        if !self.should_rollback(query_key) {
            return Err("no regression detected or no applied suggestion".into());
        }
        let suggestion = self
            .applied
            .remove(query_key)
            .ok_or_else(|| "no applied suggestion to rollback".to_string())?;
        let reason = format!(
            "auto-rollback: {} (suggestion: {})",
            suggestion.description, suggestion.action
        );
        self.rollbacks.insert(query_key.to_string(), reason.clone());
        self.samples.remove(query_key);
        Ok(reason)
    }

    /// 获取回滚记录
    pub fn rollback_reason(&self, query_key: &str) -> Option<&str> {
        self.rollbacks.get(query_key).map(|s| s.as_str())
    }

    /// 采样数
    pub fn sample_count(&self, query_key: &str) -> usize {
        self.samples.get(query_key).map(|v| v.len()).unwrap_or(0)
    }
}

impl Default for FeedbackLoop {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suggestion::SuggestionType;

    fn suggestion() -> OptimizationSuggestion {
        OptimizationSuggestion::new(
            SuggestionType::AddIndex,
            "q1",
            "test",
            "CREATE INDEX idx ON t(a)",
            0.9,
        )
    }

    #[test]
    fn record_sample_and_detect_improvement() {
        let mut loop_ = FeedbackLoop::new().with_min_samples(1);
        loop_.record_sample("q1", 200.0, 100.0);
        assert_eq!(loop_.get_trend("q1"), PerformanceTrend::Improving);
        assert!(!loop_.detect_regression("q1"));
    }

    #[test]
    fn detect_regression_after_degradation() {
        let mut loop_ = FeedbackLoop::new().with_min_samples(2);
        loop_.register_applied("q1", suggestion());
        loop_.record_sample("q1", 100.0, 150.0);
        loop_.record_sample("q1", 100.0, 160.0);
        assert_eq!(loop_.get_trend("q1"), PerformanceTrend::Degrading);
        assert!(loop_.detect_regression("q1"));
        assert!(loop_.should_rollback("q1"));
    }

    #[test]
    fn auto_rollback_removes_applied() {
        let mut loop_ = FeedbackLoop::new().with_min_samples(1);
        loop_.register_applied("q1", suggestion());
        loop_.record_sample("q1", 100.0, 200.0);
        let result = loop_.auto_rollback("q1");
        assert!(result.is_ok());
        assert!(loop_.rollback_reason("q1").is_some());
    }

    #[test]
    fn no_rollback_without_applied() {
        let mut loop_ = FeedbackLoop::new().with_min_samples(1);
        loop_.record_sample("q1", 100.0, 200.0);
        assert!(!loop_.should_rollback("q1"));
        let result = loop_.auto_rollback("q1");
        assert!(result.is_err());
    }

    #[test]
    fn stable_trend_within_threshold() {
        let mut loop_ = FeedbackLoop::new().with_min_samples(1);
        loop_.record_sample("q1", 100.0, 103.0);
        assert_eq!(loop_.get_trend("q1"), PerformanceTrend::Stable);
    }

    #[test]
    fn insufficient_samples_returns_stable() {
        let mut loop_ = FeedbackLoop::new().with_min_samples(5);
        loop_.record_sample("q1", 100.0, 200.0);
        loop_.record_sample("q1", 100.0, 200.0);
        assert_eq!(loop_.get_trend("q1"), PerformanceTrend::Stable);
    }
}
