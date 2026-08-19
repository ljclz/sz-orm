//! 异常检测配置 + 热更新
//!
//! 阈值配置使用 `Arc<RwLock<AnomalyConfig>>` 实现运行时热更新，不重启。
//! 非法配置自动回退默认值，记录配置错误。

use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// 异常检测配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyConfig {
    /// 慢查询阈值（毫秒），默认 100ms
    pub slow_query_threshold_ms: u64,
    /// 慢查询突增检测的 sigma 倍数，默认 3.0
    pub slow_query_sigma: f64,
    /// 错误率阈值（0.0 ~ 1.0），默认 0.05（5%）
    pub error_rate_threshold: f64,
    /// 错误率突增检测的 sigma 倍数，默认 3.0
    pub error_rate_sigma: f64,
    /// 连接池等待数阈值，默认 10
    pub pool_wait_count_threshold: u32,
    /// 连接池等待耗时阈值（毫秒），默认 1000ms
    pub pool_wait_time_threshold_ms: u64,
    /// 连接池上限，默认 50
    pub pool_max_connections: u32,
    /// 滑动窗口大小，默认 30 分钟
    pub window_size: Duration,
    /// 告警冷却期，默认 5 分钟
    pub alert_cooldown: Duration,
    /// 最小基线样本数，默认 100
    pub min_baseline_samples: usize,
    /// 偏离基线检测的连续窗口数，默认 3
    pub baseline_drift_windows: usize,
    /// 慢查询突增计数阈值（窗口内慢查询数超过此值触发），默认 10
    pub slow_query_spike_count: u64,
}

impl Default for AnomalyConfig {
    fn default() -> Self {
        Self {
            slow_query_threshold_ms: 100,
            slow_query_sigma: 3.0,
            error_rate_threshold: 0.05,
            error_rate_sigma: 3.0,
            pool_wait_count_threshold: 10,
            pool_wait_time_threshold_ms: 1000,
            pool_max_connections: 50,
            window_size: Duration::from_secs(30 * 60),
            alert_cooldown: Duration::from_secs(5 * 60),
            min_baseline_samples: 100,
            baseline_drift_windows: 3,
            slow_query_spike_count: 10,
        }
    }
}

impl AnomalyConfig {
    /// 创建默认配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置慢查询阈值（链式）
    pub fn with_slow_query_threshold_ms(mut self, ms: u64) -> Self {
        self.slow_query_threshold_ms = ms;
        self
    }

    /// 设置慢查询 sigma（链式）
    pub fn with_slow_query_sigma(mut self, sigma: f64) -> Self {
        self.slow_query_sigma = sigma;
        self
    }

    /// 设置错误率阈值（链式）
    pub fn with_error_rate_threshold(mut self, rate: f64) -> Self {
        self.error_rate_threshold = rate;
        self
    }

    /// 设置连接池等待数阈值（链式）
    pub fn with_pool_wait_count_threshold(mut self, count: u32) -> Self {
        self.pool_wait_count_threshold = count;
        self
    }

    /// 设置连接池等待耗时阈值（链式）
    pub fn with_pool_wait_time_threshold_ms(mut self, ms: u64) -> Self {
        self.pool_wait_time_threshold_ms = ms;
        self
    }

    /// 设置连接池上限（链式）
    pub fn with_pool_max_connections(mut self, max: u32) -> Self {
        self.pool_max_connections = max;
        self
    }

    /// 设置窗口大小（链式）
    pub fn with_window_size(mut self, size: Duration) -> Self {
        self.window_size = size;
        self
    }

    /// 设置告警冷却期（链式）
    pub fn with_alert_cooldown(mut self, cooldown: Duration) -> Self {
        self.alert_cooldown = cooldown;
        self
    }

    /// 设置最小基线样本数（链式）
    pub fn with_min_baseline_samples(mut self, samples: usize) -> Self {
        self.min_baseline_samples = samples;
        self
    }

    /// 设置慢查询突增计数阈值（链式）
    pub fn with_slow_query_spike_count(mut self, count: u64) -> Self {
        self.slow_query_spike_count = count;
        self
    }

    /// 设置偏离基线连续窗口数（链式）
    pub fn with_baseline_drift_windows(mut self, windows: usize) -> Self {
        self.baseline_drift_windows = windows;
        self
    }

    /// 验证配置合法性，非法字段回退默认值
    pub fn validated(self) -> Self {
        let default = Self::default();
        let mut config = self;
        if config.slow_query_threshold_ms == 0 {
            config.slow_query_threshold_ms = default.slow_query_threshold_ms;
        }
        if config.slow_query_sigma <= 0.0 || config.slow_query_sigma.is_nan() {
            config.slow_query_sigma = default.slow_query_sigma;
        }
        if config.error_rate_threshold <= 0.0
            || config.error_rate_threshold > 1.0
            || config.error_rate_threshold.is_nan()
        {
            config.error_rate_threshold = default.error_rate_threshold;
        }
        if config.error_rate_sigma <= 0.0 || config.error_rate_sigma.is_nan() {
            config.error_rate_sigma = default.error_rate_sigma;
        }
        if config.pool_wait_count_threshold == 0 {
            config.pool_wait_count_threshold = default.pool_wait_count_threshold;
        }
        if config.pool_wait_time_threshold_ms == 0 {
            config.pool_wait_time_threshold_ms = default.pool_wait_time_threshold_ms;
        }
        if config.pool_max_connections == 0 {
            config.pool_max_connections = default.pool_max_connections;
        }
        if config.window_size.is_zero() {
            config.window_size = default.window_size;
        }
        if config.alert_cooldown.is_zero() {
            config.alert_cooldown = default.alert_cooldown;
        }
        if config.min_baseline_samples == 0 {
            config.min_baseline_samples = default.min_baseline_samples;
        }
        if config.baseline_drift_windows == 0 {
            config.baseline_drift_windows = default.baseline_drift_windows;
        }
        if config.slow_query_spike_count == 0 {
            config.slow_query_spike_count = default.slow_query_spike_count;
        }
        config
    }

    /// 检查配置是否合法
    pub fn is_valid(&self) -> bool {
        self.slow_query_threshold_ms > 0
            && self.slow_query_sigma > 0.0
            && !self.slow_query_sigma.is_nan()
            && self.error_rate_threshold > 0.0
            && self.error_rate_threshold <= 1.0
            && !self.error_rate_threshold.is_nan()
            && self.error_rate_sigma > 0.0
            && self.pool_wait_count_threshold > 0
            && self.pool_wait_time_threshold_ms > 0
            && self.pool_max_connections > 0
            && !self.window_size.is_zero()
            && !self.alert_cooldown.is_zero()
            && self.min_baseline_samples > 0
            && self.baseline_drift_windows > 0
            && self.slow_query_spike_count > 0
    }
}

impl Default for ConfigStore {
    fn default() -> Self {
        Self::new(AnomalyConfig::default())
    }
}

/// 配置存储（支持热更新）
#[derive(Debug, Clone)]
pub struct ConfigStore {
    config: Arc<RwLock<AnomalyConfig>>,
}

impl ConfigStore {
    /// 创建配置存储
    pub fn new(config: AnomalyConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config.validated())),
        }
    }

    /// 使用默认配置创建
    pub fn with_defaults() -> Self {
        Self::default()
    }

    /// 获取当前配置快照
    pub fn get(&self) -> AnomalyConfig {
        self.config.read().clone()
    }

    /// 热更新配置（自动校验合法性）
    pub fn update(&self, new_config: AnomalyConfig) {
        let validated = new_config.validated();
        *self.config.write() = validated;
    }

    /// 获取配置 Arc 引用（用于内部共享）
    pub fn shared(&self) -> Arc<RwLock<AnomalyConfig>> {
        Arc::clone(&self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AnomalyConfig::default();
        assert_eq!(config.slow_query_threshold_ms, 100);
        assert!((config.slow_query_sigma - 3.0).abs() < 1e-9);
        assert!((config.error_rate_threshold - 0.05).abs() < 1e-9);
        assert_eq!(config.pool_wait_count_threshold, 10);
        assert_eq!(config.pool_wait_time_threshold_ms, 1000);
        assert_eq!(config.pool_max_connections, 50);
        assert_eq!(config.window_size, Duration::from_secs(30 * 60));
        assert_eq!(config.alert_cooldown, Duration::from_secs(5 * 60));
        assert_eq!(config.min_baseline_samples, 100);
    }

    #[test]
    fn test_config_validated() {
        let config = AnomalyConfig {
            slow_query_threshold_ms: 0,
            slow_query_sigma: -1.0,
            error_rate_threshold: 2.0,
            error_rate_sigma: 0.0,
            pool_wait_count_threshold: 0,
            pool_wait_time_threshold_ms: 0,
            pool_max_connections: 0,
            window_size: Duration::ZERO,
            alert_cooldown: Duration::ZERO,
            min_baseline_samples: 0,
            baseline_drift_windows: 0,
            slow_query_spike_count: 0,
        };
        let validated = config.validated();
        assert!(validated.is_valid());
    }

    #[test]
    fn test_config_builder() {
        let config = AnomalyConfig::new()
            .with_slow_query_threshold_ms(200)
            .with_slow_query_sigma(2.5)
            .with_error_rate_threshold(0.1)
            .with_pool_wait_count_threshold(20)
            .with_pool_wait_time_threshold_ms(2000)
            .with_pool_max_connections(100)
            .with_window_size(Duration::from_secs(60))
            .with_alert_cooldown(Duration::from_secs(30))
            .with_min_baseline_samples(50);
        assert_eq!(config.slow_query_threshold_ms, 200);
        assert!((config.slow_query_sigma - 2.5).abs() < 1e-9);
        assert!((config.error_rate_threshold - 0.1).abs() < 1e-9);
        assert_eq!(config.pool_wait_count_threshold, 20);
        assert_eq!(config.pool_wait_time_threshold_ms, 2000);
        assert_eq!(config.pool_max_connections, 100);
        assert_eq!(config.window_size, Duration::from_secs(60));
        assert_eq!(config.alert_cooldown, Duration::from_secs(30));
        assert_eq!(config.min_baseline_samples, 50);
    }

    #[test]
    fn test_config_store_update() {
        let store = ConfigStore::with_defaults();
        let initial = store.get();
        assert_eq!(initial.slow_query_threshold_ms, 100);

        store.update(AnomalyConfig::new().with_slow_query_threshold_ms(200));
        let updated = store.get();
        assert_eq!(updated.slow_query_threshold_ms, 200);
    }

    #[test]
    fn test_config_store_invalid_fallback() {
        let store = ConfigStore::with_defaults();
        store.update(AnomalyConfig {
            slow_query_threshold_ms: 0,
            ..AnomalyConfig::default()
        });
        let config = store.get();
        // 非法值应回退默认
        assert_eq!(config.slow_query_threshold_ms, 100);
    }

    #[test]
    fn test_config_is_valid() {
        assert!(AnomalyConfig::default().is_valid());
        assert!(!AnomalyConfig {
            slow_query_threshold_ms: 0,
            ..AnomalyConfig::default()
        }
        .is_valid());
    }
}
