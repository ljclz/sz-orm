//! 并行查询配置与策略枚举

use std::time::Duration;

/// 合并策略
#[derive(Debug, Clone, PartialEq)]
pub enum MergeStrategy {
    /// 取首个成功结果
    First,
    /// 并集合并所有结果
    Union,
    /// 按 join_key 关联合并
    Join { join_key: String },
    /// 映射转换合并
    Map,
}

/// 降级策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureStrategy {
    /// 跳过失败查询，继续其他
    Skip,
    /// 全部中止
    Abort,
    /// 返回降级值
    Fallback,
}

/// 并行查询配置
#[derive(Debug, Clone)]
pub struct ParallelQueryConfig {
    /// 最大并发度（0 = 不限制）
    pub concurrency: usize,
    /// 整体超时（毫秒，0 = 不超时）
    pub overall_timeout_ms: u64,
    /// 单查询超时（毫秒，0 = 不超时）
    pub per_query_timeout_ms: u64,
    /// 失败降级策略
    pub failure_strategy: FailureStrategy,
    /// 结果合并策略
    pub merge_strategy: MergeStrategy,
}

impl Default for ParallelQueryConfig {
    fn default() -> Self {
        Self {
            concurrency: 0,
            overall_timeout_ms: 30_000,
            per_query_timeout_ms: 10_000,
            failure_strategy: FailureStrategy::Skip,
            merge_strategy: MergeStrategy::First,
        }
    }
}

impl ParallelQueryConfig {
    /// 创建默认配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置并发度
    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = concurrency;
        self
    }

    /// 设置整体超时
    pub fn with_overall_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.overall_timeout_ms = timeout_ms;
        self
    }

    /// 设置单查询超时
    pub fn with_per_query_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.per_query_timeout_ms = timeout_ms;
        self
    }

    /// 设置降级策略
    pub fn with_failure_strategy(mut self, strategy: FailureStrategy) -> Self {
        self.failure_strategy = strategy;
        self
    }

    /// 设置合并策略
    pub fn with_merge_strategy(mut self, strategy: MergeStrategy) -> Self {
        self.merge_strategy = strategy;
        self
    }

    /// 整体超时 Duration
    pub fn overall_timeout(&self) -> Option<Duration> {
        if self.overall_timeout_ms > 0 {
            Some(Duration::from_millis(self.overall_timeout_ms))
        } else {
            None
        }
    }

    /// 单查询超时 Duration
    pub fn per_query_timeout(&self) -> Option<Duration> {
        if self.per_query_timeout_ms > 0 {
            Some(Duration::from_millis(self.per_query_timeout_ms))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_values() {
        let config = ParallelQueryConfig::new();
        assert_eq!(config.concurrency, 0);
        assert_eq!(config.overall_timeout_ms, 30_000);
        assert_eq!(config.per_query_timeout_ms, 10_000);
        assert_eq!(config.failure_strategy, FailureStrategy::Skip);
        assert_eq!(config.merge_strategy, MergeStrategy::First);
    }

    #[test]
    fn config_builder_chain() {
        let config = ParallelQueryConfig::new()
            .with_concurrency(8)
            .with_overall_timeout_ms(60_000)
            .with_per_query_timeout_ms(5_000)
            .with_failure_strategy(FailureStrategy::Abort)
            .with_merge_strategy(MergeStrategy::Union);
        assert_eq!(config.concurrency, 8);
        assert_eq!(config.overall_timeout_ms, 60_000);
        assert_eq!(config.per_query_timeout_ms, 5_000);
        assert_eq!(config.failure_strategy, FailureStrategy::Abort);
        assert_eq!(config.merge_strategy, MergeStrategy::Union);
    }

    #[test]
    fn timeout_durations() {
        let config = ParallelQueryConfig::new();
        assert_eq!(
            config.overall_timeout(),
            Some(Duration::from_millis(30_000))
        );
        assert_eq!(
            config.per_query_timeout(),
            Some(Duration::from_millis(10_000))
        );

        let no_timeout = ParallelQueryConfig::new()
            .with_overall_timeout_ms(0)
            .with_per_query_timeout_ms(0);
        assert_eq!(no_timeout.overall_timeout(), None);
        assert_eq!(no_timeout.per_query_timeout(), None);
    }

    #[test]
    fn merge_strategy_variants() {
        let first = MergeStrategy::First;
        let union = MergeStrategy::Union;
        let join = MergeStrategy::Join {
            join_key: "id".into(),
        };
        let map = MergeStrategy::Map;
        assert_ne!(first, union);
        assert_ne!(join, map);
    }

    #[test]
    fn failure_strategy_variants() {
        assert_ne!(FailureStrategy::Skip, FailureStrategy::Abort);
        assert_ne!(FailureStrategy::Abort, FailureStrategy::Fallback);
        assert_ne!(FailureStrategy::Skip, FailureStrategy::Fallback);
    }
}
