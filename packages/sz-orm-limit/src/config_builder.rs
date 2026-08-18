//! 限流配置构建器（Rate Limit Config Builder）
//!
//! 提供链式 API 构建限流器配置，支持多种算法和策略组合。
//! 适用于从配置文件或环境变量构建限流器。

use std::time::Duration;

use crate::{
    FixedWindowRateLimiter, LeakyBucketLimiter, SlidingWindowLogLimiter, SlidingWindowRateLimiter,
    TokenBucketRateLimiter,
};

/// 限流算法类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LimitAlgorithm {
    /// 固定窗口
    FixedWindow,
    /// 滑动窗口（计数器）
    SlidingWindow,
    /// 滑动窗口（日志）
    SlidingWindowLog,
    /// 令牌桶
    TokenBucket,
    /// 漏桶
    LeakyBucket,
}

/// 限流配置
///
/// 描述一个限流器的完整配置，可序列化/反序列化。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RateLimitConfig {
    /// 算法类型
    pub algorithm: LimitAlgorithm,
    /// 容量/最大请求数
    pub capacity: u64,
    /// 补充速率（令牌桶）/ 漏出速率（漏桶），单位：请求/秒
    pub rate: f64,
    /// 窗口大小（毫秒），用于固定窗口和滑动窗口
    pub window_ms: u64,
    /// 最大 key 数量
    pub max_keys: usize,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            algorithm: LimitAlgorithm::TokenBucket,
            capacity: 100,
            rate: 10.0,
            window_ms: 1000,
            max_keys: crate::DEFAULT_MAX_KEYS,
        }
    }
}

impl RateLimitConfig {
    /// 创建默认配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 从 JSON 字符串解析配置
    pub fn from_json_str(s: &str) -> Result<Self, ConfigError> {
        serde_json::from_str(s).map_err(|e| ConfigError::Parse(e.to_string()))
    }

    /// 序列化为 JSON 字符串
    pub fn to_json_string(&self) -> Result<String, ConfigError> {
        serde_json::to_string(self).map_err(|e| ConfigError::Serialize(e.to_string()))
    }

    /// 校验配置合理性
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.capacity == 0 {
            return Err(ConfigError::InvalidCapacity);
        }
        match self.algorithm {
            LimitAlgorithm::TokenBucket | LimitAlgorithm::LeakyBucket => {
                if self.rate < 0.0 {
                    return Err(ConfigError::InvalidRate);
                }
            }
            _ => {}
        }
        match self.algorithm {
            LimitAlgorithm::FixedWindow
            | LimitAlgorithm::SlidingWindow
            | LimitAlgorithm::SlidingWindowLog => {
                if self.window_ms == 0 {
                    return Err(ConfigError::InvalidWindow);
                }
            }
            _ => {}
        }
        if self.max_keys == 0 {
            return Err(ConfigError::InvalidMaxKeys);
        }
        Ok(())
    }
}

/// 配置错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    Parse(String),
    Serialize(String),
    InvalidCapacity,
    InvalidRate,
    InvalidWindow,
    InvalidMaxKeys,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Parse(msg) => write!(f, "config parse error: {}", msg),
            ConfigError::Serialize(msg) => write!(f, "config serialize error: {}", msg),
            ConfigError::InvalidCapacity => write!(f, "capacity must be positive"),
            ConfigError::InvalidRate => write!(f, "rate must be non-negative"),
            ConfigError::InvalidWindow => write!(f, "window size must be positive"),
            ConfigError::InvalidMaxKeys => write!(f, "max_keys must be positive"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// 限流配置构建器
///
/// 链式 API 构建限流配置。
///
/// # 示例
///
/// ```rust
/// use sz_orm_limit::config_builder::{RateLimitConfigBuilder, LimitAlgorithm};
///
/// let config = RateLimitConfigBuilder::new()
///     .algorithm(LimitAlgorithm::TokenBucket)
///     .capacity(100)
///     .rate(10.0)
///     .max_keys(5000)
///     .build();
/// ```
pub struct RateLimitConfigBuilder {
    config: RateLimitConfig,
}

impl Default for RateLimitConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimitConfigBuilder {
    /// 创建构建器
    pub fn new() -> Self {
        Self {
            config: RateLimitConfig::default(),
        }
    }

    /// 设置算法类型
    pub fn algorithm(mut self, algo: LimitAlgorithm) -> Self {
        self.config.algorithm = algo;
        self
    }

    /// 设置容量
    pub fn capacity(mut self, cap: u64) -> Self {
        self.config.capacity = cap;
        self
    }

    /// 设置速率
    pub fn rate(mut self, rate: f64) -> Self {
        self.config.rate = rate;
        self
    }

    /// 设置窗口大小（毫秒）
    pub fn window_ms(mut self, ms: u64) -> Self {
        self.config.window_ms = ms;
        self
    }

    /// 设置窗口大小（秒）
    pub fn window_secs(mut self, secs: u64) -> Self {
        self.config.window_ms = secs * 1000;
        self
    }

    /// 设置最大 key 数量
    pub fn max_keys(mut self, max: usize) -> Self {
        self.config.max_keys = max;
        self
    }

    /// 构建配置（不校验）
    pub fn build(self) -> RateLimitConfig {
        self.config
    }

    /// 构建配置并校验
    pub fn build_checked(self) -> Result<RateLimitConfig, ConfigError> {
        self.config.validate()?;
        Ok(self.config)
    }

    /// 从配置构建令牌桶限流器
    pub fn build_token_bucket(&self) -> TokenBucketRateLimiter {
        TokenBucketRateLimiter::new(self.config.capacity, self.config.rate)
            .with_max_keys(self.config.max_keys)
    }

    /// 从配置构建滑动窗口限流器
    pub fn build_sliding_window(&self) -> SlidingWindowRateLimiter {
        SlidingWindowRateLimiter::new(
            self.config.capacity,
            Duration::from_millis(self.config.window_ms),
        )
        .with_max_keys(self.config.max_keys)
    }

    /// 从配置构建固定窗口限流器
    pub fn build_fixed_window(&self) -> FixedWindowRateLimiter {
        FixedWindowRateLimiter::new(
            self.config.capacity,
            Duration::from_millis(self.config.window_ms),
        )
        .with_max_keys(self.config.max_keys)
    }

    /// 从配置构建漏桶限流器
    pub fn build_leaky_bucket(&self) -> LeakyBucketLimiter {
        LeakyBucketLimiter::new(self.config.capacity, self.config.rate)
            .with_max_keys(self.config.max_keys)
    }

    /// 从配置构建滑动窗口日志限流器
    pub fn build_sliding_window_log(&self) -> SlidingWindowLogLimiter {
        SlidingWindowLogLimiter::new(
            self.config.capacity,
            Duration::from_millis(self.config.window_ms),
        )
        .with_max_keys(self.config.max_keys)
    }
}

/// 多层级限流配置
///
/// 为不同维度（IP、用户、API）配置不同的限流策略。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TieredRateLimitConfig {
    /// IP 级别配置
    pub ip: RateLimitConfig,
    /// 用户级别配置
    pub user: RateLimitConfig,
    /// API 级别配置
    pub api: RateLimitConfig,
    /// 全局配置
    pub global: RateLimitConfig,
}

impl Default for TieredRateLimitConfig {
    fn default() -> Self {
        Self {
            ip: RateLimitConfig {
                algorithm: LimitAlgorithm::SlidingWindow,
                capacity: 1000,
                rate: 0.0,
                window_ms: 60_000,
                max_keys: 10_000,
            },
            user: RateLimitConfig {
                algorithm: LimitAlgorithm::TokenBucket,
                capacity: 100,
                rate: 10.0,
                window_ms: 1000,
                max_keys: 10_000,
            },
            api: RateLimitConfig {
                algorithm: LimitAlgorithm::FixedWindow,
                capacity: 500,
                rate: 0.0,
                window_ms: 60_000,
                max_keys: 1000,
            },
            global: RateLimitConfig {
                algorithm: LimitAlgorithm::TokenBucket,
                capacity: 10_000,
                rate: 100.0,
                window_ms: 1000,
                max_keys: 1,
            },
        }
    }
}

impl TieredRateLimitConfig {
    /// 创建默认多层级配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 校验所有层级配置
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.ip.validate()?;
        self.user.validate()?;
        self.api.validate()?;
        self.global.validate()?;
        Ok(())
    }

    /// 从 JSON 字符串解析
    pub fn from_json_str(s: &str) -> Result<Self, ConfigError> {
        serde_json::from_str(s).map_err(|e| ConfigError::Parse(e.to_string()))
    }

    /// 序列化为 JSON 字符串
    pub fn to_json_string(&self) -> Result<String, ConfigError> {
        serde_json::to_string(self).map_err(|e| ConfigError::Serialize(e.to_string()))
    }
}

/// 多层级配置构建器
pub struct TieredConfigBuilder {
    config: TieredRateLimitConfig,
}

impl Default for TieredConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TieredConfigBuilder {
    /// 创建构建器
    pub fn new() -> Self {
        Self {
            config: TieredRateLimitConfig::default(),
        }
    }

    /// 设置 IP 级别配置
    pub fn ip(mut self, config: RateLimitConfig) -> Self {
        self.config.ip = config;
        self
    }

    /// 设置用户级别配置
    pub fn user(mut self, config: RateLimitConfig) -> Self {
        self.config.user = config;
        self
    }

    /// 设置 API 级别配置
    pub fn api(mut self, config: RateLimitConfig) -> Self {
        self.config.api = config;
        self
    }

    /// 设置全局配置
    pub fn global(mut self, config: RateLimitConfig) -> Self {
        self.config.global = config;
        self
    }

    /// 构建
    pub fn build(self) -> TieredRateLimitConfig {
        self.config
    }

    /// 构建并校验
    pub fn build_checked(self) -> Result<TieredRateLimitConfig, ConfigError> {
        self.config.validate()?;
        Ok(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RateLimiter;

    #[test]
    fn test_limit_algorithm_serde() {
        let algo = LimitAlgorithm::TokenBucket;
        let json = serde_json::to_string(&algo).unwrap();
        let back: LimitAlgorithm = serde_json::from_str(&json).unwrap();
        assert_eq!(algo, back);
    }

    #[test]
    fn test_rate_limit_config_default() {
        let config = RateLimitConfig::default();
        assert_eq!(config.algorithm, LimitAlgorithm::TokenBucket);
        assert_eq!(config.capacity, 100);
        assert_eq!(config.rate, 10.0);
    }

    #[test]
    fn test_rate_limit_config_validate_ok() {
        let config = RateLimitConfig {
            algorithm: LimitAlgorithm::TokenBucket,
            capacity: 100,
            rate: 10.0,
            window_ms: 1000,
            max_keys: 1000,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_rate_limit_config_validate_zero_capacity() {
        let config = RateLimitConfig {
            capacity: 0,
            ..Default::default()
        };
        assert_eq!(config.validate(), Err(ConfigError::InvalidCapacity));
    }

    #[test]
    fn test_rate_limit_config_validate_negative_rate() {
        let config = RateLimitConfig {
            algorithm: LimitAlgorithm::TokenBucket,
            rate: -1.0,
            ..Default::default()
        };
        assert_eq!(config.validate(), Err(ConfigError::InvalidRate));
    }

    #[test]
    fn test_rate_limit_config_validate_zero_window() {
        let config = RateLimitConfig {
            algorithm: LimitAlgorithm::FixedWindow,
            window_ms: 0,
            ..Default::default()
        };
        assert_eq!(config.validate(), Err(ConfigError::InvalidWindow));
    }

    #[test]
    fn test_rate_limit_config_validate_zero_max_keys() {
        let config = RateLimitConfig {
            max_keys: 0,
            ..Default::default()
        };
        assert_eq!(config.validate(), Err(ConfigError::InvalidMaxKeys));
    }

    #[test]
    fn test_rate_limit_config_json_roundtrip() {
        let config = RateLimitConfig::default();
        let json = config.to_json_string().unwrap();
        let back = RateLimitConfig::from_json_str(&json).unwrap();
        assert_eq!(config.algorithm, back.algorithm);
        assert_eq!(config.capacity, back.capacity);
    }

    #[test]
    fn test_config_builder_basic() {
        let config = RateLimitConfigBuilder::new()
            .algorithm(LimitAlgorithm::SlidingWindow)
            .capacity(200)
            .window_secs(60)
            .max_keys(5000)
            .build();
        assert_eq!(config.algorithm, LimitAlgorithm::SlidingWindow);
        assert_eq!(config.capacity, 200);
        assert_eq!(config.window_ms, 60_000);
        assert_eq!(config.max_keys, 5000);
    }

    #[test]
    fn test_config_builder_checked_ok() {
        let config = RateLimitConfigBuilder::new()
            .algorithm(LimitAlgorithm::TokenBucket)
            .capacity(100)
            .rate(10.0)
            .build_checked();
        assert!(config.is_ok());
    }

    #[test]
    fn test_config_builder_checked_fail() {
        let config = RateLimitConfigBuilder::new().capacity(0).build_checked();
        assert!(config.is_err());
    }

    #[test]
    fn test_config_builder_build_token_bucket() {
        let builder = RateLimitConfigBuilder::new()
            .algorithm(LimitAlgorithm::TokenBucket)
            .capacity(10)
            .rate(1.0)
            .max_keys(100);
        let limiter = builder.build_token_bucket();
        assert_eq!(limiter.capacity(), 10);
        let r = limiter.acquire("k").unwrap();
        assert!(r.allowed);
    }

    #[test]
    fn test_config_builder_build_sliding_window() {
        let builder = RateLimitConfigBuilder::new()
            .algorithm(LimitAlgorithm::SlidingWindow)
            .capacity(10)
            .window_secs(60);
        let limiter = builder.build_sliding_window();
        assert_eq!(limiter.max_requests(), 10);
        let r = limiter.acquire("k").unwrap();
        assert!(r.allowed);
    }

    #[test]
    fn test_config_builder_build_fixed_window() {
        let builder = RateLimitConfigBuilder::new()
            .algorithm(LimitAlgorithm::FixedWindow)
            .capacity(10)
            .window_secs(60);
        let limiter = builder.build_fixed_window();
        assert_eq!(limiter.max_requests(), 10);
        let r = limiter.acquire("k").unwrap();
        assert!(r.allowed);
    }

    #[test]
    fn test_config_builder_build_leaky_bucket() {
        let builder = RateLimitConfigBuilder::new()
            .algorithm(LimitAlgorithm::LeakyBucket)
            .capacity(10)
            .rate(1.0);
        let limiter = builder.build_leaky_bucket();
        assert_eq!(limiter.capacity(), 10);
        let r = limiter.acquire("k").unwrap();
        assert!(r.allowed);
    }

    #[test]
    fn test_config_builder_build_sliding_window_log() {
        let builder = RateLimitConfigBuilder::new()
            .algorithm(LimitAlgorithm::SlidingWindowLog)
            .capacity(10)
            .window_secs(60);
        let limiter = builder.build_sliding_window_log();
        assert_eq!(limiter.max_requests(), 10);
        let r = limiter.acquire("k").unwrap();
        assert!(r.allowed);
    }

    #[test]
    fn test_tiered_config_default() {
        let config = TieredRateLimitConfig::default();
        assert_eq!(config.ip.capacity, 1000);
        assert_eq!(config.user.capacity, 100);
        assert_eq!(config.api.capacity, 500);
        assert_eq!(config.global.capacity, 10_000);
    }

    #[test]
    fn test_tiered_config_validate() {
        let config = TieredRateLimitConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_tiered_config_json_roundtrip() {
        let config = TieredRateLimitConfig::default();
        let json = config.to_json_string().unwrap();
        let back = TieredRateLimitConfig::from_json_str(&json).unwrap();
        assert_eq!(back.ip.capacity, config.ip.capacity);
    }

    #[test]
    fn test_tiered_config_builder() {
        let config = TieredConfigBuilder::new()
            .ip(RateLimitConfig {
                capacity: 2000,
                ..Default::default()
            })
            .user(RateLimitConfig {
                capacity: 200,
                ..Default::default()
            })
            .build();
        assert_eq!(config.ip.capacity, 2000);
        assert_eq!(config.user.capacity, 200);
    }

    #[test]
    fn test_tiered_config_builder_checked() {
        let config = TieredConfigBuilder::new().build_checked();
        assert!(config.is_ok());
    }

    #[test]
    fn test_config_builder_window_ms() {
        let config = RateLimitConfigBuilder::new().window_ms(500).build();
        assert_eq!(config.window_ms, 500);
    }

    #[test]
    fn test_config_builder_rate() {
        let config = RateLimitConfigBuilder::new().rate(5.0).build();
        assert_eq!(config.rate, 5.0);
    }
}
