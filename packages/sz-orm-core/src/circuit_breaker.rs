//! 断路器抽象（P1-4：抽象提升到核心层，供连接池执行路径集成）
//!
//! 连接池在 `circuit-breaker` feature 下通过 `configure_circuit_breaker()` 配置
//! [`DefaultCircuitBreaker`]，在获取连接/执行查询前调用 [`CircuitBreaker::can_execute`]
//! 拦截失败请求，成功/失败时记录反馈，防止故障级联。
//!
//! 本模块为自包含实现（不依赖 sz-orm-health），消除核心层对上层 crate 的反向依赖。

use std::time::{Duration, Instant};

/// 断路器状态机状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CircuitState {
    /// 正常运行：请求放行。
    Closed,
    /// 已熔断：在 `reset_timeout` 内拦截所有请求。
    Open,
    /// 试探：熔断超时后放行单个试探请求。
    HalfOpen,
}

/// 断路器抽象 trait。
///
/// 实现方需维护失败计数与状态机；连接池通过该 trait 统一驱动
/// （不依赖具体实现类型，便于测试替换）。
pub trait CircuitBreaker: Send + Sync {
    /// 请求是否可执行。`Open` 且熔断超时后自动转为 `HalfOpen` 并放行。
    fn can_execute(&mut self) -> bool;
    /// 记录一次成功（复位失败计数，回到 `Closed`）。
    fn record_success(&mut self);
    /// 记录一次失败（达到阈值后熔断为 `Open`）。
    fn record_failure(&mut self);
    /// 当前状态。
    fn state(&self) -> CircuitState;
    /// 手动重置到 `Closed`。返回是否实际发生了状态变更。
    fn reset(&mut self) -> bool;
}

/// 默认断路器实现：连续失败 `failure_threshold` 次后熔断，
/// `reset_timeout` 过后进入 `HalfOpen` 试探，成功恢复 `Closed`，失败回到 `Open`。
pub struct DefaultCircuitBreaker {
    failure_threshold: usize,
    reset_timeout: Duration,
    state: CircuitState,
    consecutive_failures: usize,
    last_failure_at: Option<Instant>,
    /// v3.8.0: 累计熔断次数
    total_trips: u64,
}

impl DefaultCircuitBreaker {
    /// 创建断路器。
    ///
    /// - `failure_threshold`：连续失败多少次后熔断（`Open`）；
    /// - `reset_timeout`：熔断后等待多久进入 `HalfOpen` 试探。
    pub fn new(failure_threshold: usize, reset_timeout: Duration) -> Self {
        Self {
            failure_threshold,
            reset_timeout,
            state: CircuitState::Closed,
            consecutive_failures: 0,
            last_failure_at: None,
            total_trips: 0,
        }
    }

    /// v3.8.0: 查询熔断器统计信息
    #[cfg(feature = "prod-circuit-tuning")]
    pub fn stats(&self) -> CircuitBreakerStats {
        CircuitBreakerStats {
            state: self.state,
            consecutive_failures: self.consecutive_failures,
            total_trips: self.total_trips,
        }
    }
}

impl CircuitBreaker for DefaultCircuitBreaker {
    fn can_execute(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::HalfOpen => true,
            CircuitState::Open => {
                let elapsed = self
                    .last_failure_at
                    .map(|t| t.elapsed())
                    .unwrap_or(Duration::ZERO);
                if elapsed >= self.reset_timeout {
                    self.state = CircuitState::HalfOpen;
                    true
                } else {
                    false
                }
            }
        }
    }

    fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.state = CircuitState::Closed;
        self.last_failure_at = None;
    }

    fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        self.last_failure_at = Some(Instant::now());
        if self.consecutive_failures >= self.failure_threshold && self.state != CircuitState::Open {
            self.state = CircuitState::Open;
            self.total_trips += 1;
        }
    }

    fn state(&self) -> CircuitState {
        self.state
    }

    fn reset(&mut self) -> bool {
        let changed = self.state != CircuitState::Closed || self.consecutive_failures != 0;
        self.state = CircuitState::Closed;
        self.consecutive_failures = 0;
        self.last_failure_at = None;
        changed
    }
}

// ============================================================================
// v3.8.0: 熔断器生产配置（prod-circuit-tuning feature）
// ============================================================================

#[cfg(feature = "prod-circuit-tuning")]
mod prod {
    use super::CircuitState;
    use serde::{Deserialize, Serialize};
    use std::time::Duration;

    /// 熔断器生产配置错误
    #[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
    pub enum CircuitBreakerProdError {
        /// 失败阈值非正
        #[error("circuit breaker failure_threshold must be positive")]
        FailureThresholdNotPositive,
        /// 重置超时非正
        #[error("circuit breaker reset_timeout must be positive")]
        ResetTimeoutNotPositive,
    }

    /// 熔断器生产配置
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CircuitBreakerProdConfig {
        /// 失败阈值
        pub failure_threshold: u32,
        /// 重置超时
        pub reset_timeout: Duration,
    }

    impl Default for CircuitBreakerProdConfig {
        fn default() -> Self {
            Self {
                failure_threshold: 5,
                reset_timeout: Duration::from_secs(30),
            }
        }
    }

    impl CircuitBreakerProdConfig {
        /// 创建配置
        pub fn new(failure_threshold: u32, reset_timeout: Duration) -> Self {
            Self {
                failure_threshold,
                reset_timeout,
            }
        }

        /// 验证配置合法性
        pub fn validate(&self) -> Result<(), CircuitBreakerProdError> {
            if self.failure_threshold == 0 {
                return Err(CircuitBreakerProdError::FailureThresholdNotPositive);
            }
            if self.reset_timeout.is_zero() {
                return Err(CircuitBreakerProdError::ResetTimeoutNotPositive);
            }
            Ok(())
        }
    }

    /// 熔断器统计信息
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CircuitBreakerStats {
        /// 当前状态
        pub state: CircuitState,
        /// 连续失败次数
        pub consecutive_failures: usize,
        /// 总跳闸次数
        pub total_trips: u64,
    }
}

#[cfg(feature = "prod-circuit-tuning")]
pub use prod::{CircuitBreakerProdConfig, CircuitBreakerProdError, CircuitBreakerStats};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_starts_closed() {
        let mut cb = DefaultCircuitBreaker::new(3, Duration::from_secs(60));
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.can_execute());
    }

    #[test]
    fn test_circuit_breaker_trips_after_threshold() {
        let mut cb = DefaultCircuitBreaker::new(3, Duration::from_secs(60));
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.can_execute());
    }

    #[test]
    fn test_circuit_breaker_success_resets() {
        let mut cb = DefaultCircuitBreaker::new(2, Duration::from_secs(60));
        cb.record_failure();
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
        // 成功后失败计数已清零，需重新积累阈值
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn test_circuit_breaker_half_open_after_timeout() {
        let mut cb = DefaultCircuitBreaker::new(1, Duration::from_millis(10));
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        // 超时后 can_execute 自动进入 HalfOpen 并放行
        std::thread::sleep(Duration::from_millis(20));
        assert!(cb.can_execute());
        assert_eq!(cb.state(), CircuitState::HalfOpen);
    }

    #[test]
    fn test_circuit_breaker_half_open_success_closes() {
        let mut cb = DefaultCircuitBreaker::new(1, Duration::from_millis(10));
        cb.record_failure();
        std::thread::sleep(Duration::from_millis(20));
        assert!(cb.can_execute()); // HalfOpen
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_half_open_failure_reopens() {
        let mut cb = DefaultCircuitBreaker::new(1, Duration::from_millis(10));
        cb.record_failure();
        std::thread::sleep(Duration::from_millis(20));
        assert!(cb.can_execute()); // HalfOpen
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.can_execute());
    }

    #[test]
    fn test_circuit_breaker_reset() {
        let mut cb = DefaultCircuitBreaker::new(1, Duration::from_secs(60));
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(cb.reset());
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.can_execute());
        // 已关闭时 reset 返回 false
        assert!(!cb.reset());
    }
}

#[cfg(all(test, feature = "prod-circuit-tuning"))]
mod prod_tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_prod_config_validate_ok() {
        let config = CircuitBreakerProdConfig::new(10, Duration::from_secs(60));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_circuit_breaker_prod_config_threshold_zero_rejected() {
        let config = CircuitBreakerProdConfig::new(0, Duration::from_secs(60));
        let err = config.validate().unwrap_err();
        assert!(err
            .to_string()
            .contains("failure_threshold must be positive"));
    }

    #[test]
    fn test_circuit_breaker_prod_config_timeout_zero_rejected() {
        let config = CircuitBreakerProdConfig::new(10, Duration::ZERO);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_circuit_breaker_stats_after_trips() {
        let mut cb = DefaultCircuitBreaker::new(3, Duration::from_secs(60));
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        let stats = cb.stats();
        assert_eq!(stats.state, CircuitState::Open);
        assert_eq!(stats.consecutive_failures, 3);
        assert_eq!(stats.total_trips, 1);
    }

    #[test]
    fn test_circuit_breaker_stats_no_trips() {
        let cb = DefaultCircuitBreaker::new(5, Duration::from_secs(60));
        let stats = cb.stats();
        assert_eq!(stats.state, CircuitState::Closed);
        assert_eq!(stats.total_trips, 0);
    }

    #[test]
    fn test_circuit_breaker_total_trips_increments() {
        let mut cb = DefaultCircuitBreaker::new(1, Duration::from_secs(60));
        cb.record_failure();
        assert_eq!(cb.stats().total_trips, 1);
        cb.record_success();
        cb.record_failure();
        assert_eq!(cb.stats().total_trips, 2);
    }
}
// ============================================================================
// v7.3.0 错误率熔断器（基于滑动窗口错误率阈值 + 可配半开探测）
// ============================================================================

/// 错误率熔断器（v7.3.0）
///
/// 基于滑动窗口错误率阈值（而非连续失败数）触发熔断。
/// 可配半开探测数 `half_open_probes`：`HalfOpen` 状态下仅放行 N 个探测请求。
///
/// 状态机：Closed → Open（错误率超阈值）→ HalfOpen（reset_timeout 后，仅放行 N 个探测）
/// → Closed（探测全部成功）/ Open（任一探测失败）
pub struct ErrorRateCircuitBreaker {
    /// 错误率阈值 ∈ (0,1)
    error_threshold: f64,
    /// 熔断后等待多久进入 HalfOpen
    reset_timeout: Duration,
    /// 半开探测请求数（≥ 1）
    half_open_probes: u32,
    /// 当前状态
    state: CircuitState,
    /// 滑动窗口：成功/失败记录（true=成功，false=失败）
    window: std::collections::VecDeque<bool>,
    /// 滑动窗口大小
    window_size: usize,
    /// HalfOpen 状态下已放行的探测数
    probes_in_half_open: u32,
    /// HalfOpen 状态下探测成功数
    probe_successes: u32,
    /// 上次失败时间（用于 reset_timeout 判断）
    last_failure_at: Option<Instant>,
    /// 总跳闸次数
    total_trips: u64,
}

impl ErrorRateCircuitBreaker {
    /// 创建错误率熔断器
    ///
    /// - `error_threshold`：错误率阈值 ∈ (0,1)，默认 0.5
    /// - `reset_timeout`：熔断后等待多久进入 HalfOpen
    /// - `half_open_probes`：半开探测请求数（≥ 1）
    /// - `window_size`：滑动窗口大小（用于错误率统计）
    pub fn new(
        error_threshold: f64,
        reset_timeout: Duration,
        half_open_probes: u32,
        window_size: usize,
    ) -> Self {
        Self {
            error_threshold,
            reset_timeout,
            half_open_probes: half_open_probes.max(1),
            state: CircuitState::Closed,
            window: std::collections::VecDeque::with_capacity(window_size),
            window_size,
            probes_in_half_open: 0,
            probe_successes: 0,
            last_failure_at: None,
            total_trips: 0,
        }
    }

    /// 当前错误率（窗口内失败数 / 窗口大小）
    pub fn error_rate(&self) -> f64 {
        if self.window.is_empty() {
            return 0.0;
        }
        let failures = self.window.iter().filter(|&&s| !s).count() as f64;
        failures / self.window.len() as f64
    }

    /// 总跳闸次数
    pub fn total_trips(&self) -> u64 {
        self.total_trips
    }

    /// 滑动窗口当前样本数
    pub fn window_samples(&self) -> usize {
        self.window.len()
    }

    /// 记录一次请求结果（true=成功，false=失败）
    fn record_result(&mut self, success: bool) {
        // 滑动窗口维护
        if self.window.len() >= self.window_size {
            self.window.pop_front();
        }
        self.window.push_back(success);

        if !success {
            self.last_failure_at = Some(Instant::now());
        }

        match self.state {
            CircuitState::Closed => {
                // 检查错误率是否超阈值
                if self.window.len() >= self.window_size && self.error_rate() > self.error_threshold
                {
                    self.state = CircuitState::Open;
                    self.total_trips += 1;
                }
            }
            CircuitState::HalfOpen => {
                // can_execute 已计数 probes_in_half_open，这里只检查结果
                if success {
                    self.probe_successes += 1;
                }
                // 任一探测失败立即重回 Open
                if !success {
                    self.state = CircuitState::Open;
                    self.probes_in_half_open = 0;
                    self.probe_successes = 0;
                } else if self.probes_in_half_open >= self.half_open_probes {
                    // 所有探测成功，恢复 Closed
                    self.state = CircuitState::Closed;
                    self.probes_in_half_open = 0;
                    self.probe_successes = 0;
                    self.window.clear();
                }
            }
            CircuitState::Open => {}
        }
    }
}

impl CircuitBreaker for ErrorRateCircuitBreaker {
    fn can_execute(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                let elapsed = self
                    .last_failure_at
                    .map(|t| t.elapsed())
                    .unwrap_or(Duration::ZERO);
                if elapsed >= self.reset_timeout {
                    self.state = CircuitState::HalfOpen;
                    // Open → HalfOpen 转换时放行第一个探测请求
                    self.probes_in_half_open = 1;
                    self.probe_successes = 0;
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => {
                // 仅放行 N 个探测请求，其余快速失败
                if self.probes_in_half_open < self.half_open_probes {
                    self.probes_in_half_open += 1;
                    true
                } else {
                    false
                }
            }
        }
    }

    fn record_success(&mut self) {
        self.record_result(true);
    }

    fn record_failure(&mut self) {
        self.record_result(false);
    }

    fn state(&self) -> CircuitState {
        self.state
    }

    fn reset(&mut self) -> bool {
        let changed = self.state != CircuitState::Closed || !self.window.is_empty();
        self.state = CircuitState::Closed;
        self.window.clear();
        self.probes_in_half_open = 0;
        self.probe_successes = 0;
        self.last_failure_at = None;
        changed
    }
}
