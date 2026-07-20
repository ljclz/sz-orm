//! SLO 燃烧率监控
//!
//! 基于 Google SRE 实践，使用多窗口多燃烧率告警策略：
//!
//! - 5 分钟窗口 + 1 小时窗口（短期燃烧）
//! - 1 小时窗口 + 6 小时窗口（长期燃烧）
//!
//! 燃烧率 = 实际错误率 / 允许错误率
//!
//! 当燃烧率超过阈值时，表示 SLO 预算正在被快速消耗。
//!
//! # 示例
//!
//! ```
//! use sz_orm_observability::{SloConfig, SloMonitor};
//! use std::time::Duration;
//!
//! let config = SloConfig {
//!     target_success_rate: 0.999, // 99.9% SLO
//!     short_window: Duration::from_secs(300), // 5 分钟
//!     long_window: Duration::from_secs(3600), // 1 小时
//!     burn_rate_threshold: 14.4, // 2% 预算消耗/小时
//! };
//! let monitor = SloMonitor::new(config);
//!
//! // 记录请求结果
//! monitor.record_success();
//! monitor.record_failure();
//!
//! // 获取燃烧率
//! let rate = monitor.burn_rate();
//! println!("Current burn rate: {}", rate);
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// SLO 配置
#[derive(Debug, Clone)]
pub struct SloConfig {
    /// 目标成功率（如 0.999 表示 99.9%）
    pub target_success_rate: f64,
    /// 短窗口时长（如 5 分钟）
    pub short_window: Duration,
    /// 长窗口时长（如 1 小时）
    pub long_window: Duration,
    /// 燃烧率告警阈值（如 14.4 表示 1 小时消耗 2% 预算）
    pub burn_rate_threshold: f64,
}

impl Default for SloConfig {
    fn default() -> Self {
        Self {
            target_success_rate: 0.999,
            short_window: Duration::from_secs(300),
            long_window: Duration::from_secs(3600),
            burn_rate_threshold: 14.4,
        }
    }
}

/// SLO 燃烧率快照
#[derive(Debug, Clone)]
pub struct SloBurnRate {
    /// 短窗口实际成功率
    pub short_success_rate: f64,
    /// 长窗口实际成功率
    pub long_success_rate: f64,
    /// 短窗口燃烧率（实际错误率 / 允许错误率）
    pub short_burn_rate: f64,
    /// 长窗口燃烧率
    pub long_burn_rate: f64,
    /// 剩余 SLO 预算（0.0-1.0）
    pub error_budget_remaining: f64,
    /// 是否触发告警
    pub alerting: bool,
}

impl Default for SloBurnRate {
    fn default() -> Self {
        Self {
            short_success_rate: 1.0,
            long_success_rate: 1.0,
            short_burn_rate: 0.0,
            long_burn_rate: 0.0,
            error_budget_remaining: 1.0,
            alerting: false,
        }
    }
}

impl std::fmt::Display for SloBurnRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SloBurnRate{{short_success={:.4}, long_success={:.4}, short_burn={:.2}, long_burn={:.2}, budget_remaining={:.2}%, alerting={}}}",
            self.short_success_rate,
            self.long_success_rate,
            self.short_burn_rate,
            self.long_burn_rate,
            self.error_budget_remaining * 100.0,
            self.alerting
        )
    }
}

/// 基于时间窗口的请求计数器
struct WindowedCounter {
    window: Duration,
    success: Arc<AtomicU64>,
    failure: Arc<AtomicU64>,
    window_start: Arc<RwLock<Instant>>,
}

use parking_lot::RwLock;

impl WindowedCounter {
    fn new(window: Duration) -> Self {
        Self {
            window,
            success: Arc::new(AtomicU64::new(0)),
            failure: Arc::new(AtomicU64::new(0)),
            window_start: Arc::new(RwLock::new(Instant::now())),
        }
    }

    fn record_success(&self) {
        self.rotate_if_needed();
        self.success.fetch_add(1, Ordering::Relaxed);
    }

    fn record_failure(&self) {
        self.rotate_if_needed();
        self.failure.fetch_add(1, Ordering::Relaxed);
    }

    fn rotate_if_needed(&self) {
        let start = self.window_start.read();
        if start.elapsed() >= self.window {
            drop(start);
            let mut start = self.window_start.write();
            // 双重检查
            if start.elapsed() >= self.window {
                // 重置计数器
                self.success.store(0, Ordering::Relaxed);
                self.failure.store(0, Ordering::Relaxed);
                *start = Instant::now();
            }
        }
    }

    fn success_rate(&self) -> f64 {
        let s = self.success.load(Ordering::Relaxed);
        let f = self.failure.load(Ordering::Relaxed);
        let total = s + f;
        if total == 0 {
            return 1.0;
        }
        s as f64 / total as f64
    }
}

/// SLO 监控器
pub struct SloMonitor {
    config: SloConfig,
    short_counter: WindowedCounter,
    long_counter: WindowedCounter,
    /// 累计成功数（自启动以来）
    total_success: AtomicU64,
    /// 累计失败数（自启动以来）
    total_failure: AtomicU64,
}

impl SloMonitor {
    /// 创建新监控器
    pub fn new(config: SloConfig) -> Self {
        let short_counter = WindowedCounter::new(config.short_window);
        let long_counter = WindowedCounter::new(config.long_window);
        Self {
            config,
            short_counter,
            long_counter,
            total_success: AtomicU64::new(0),
            total_failure: AtomicU64::new(0),
        }
    }

    /// 记录一次成功
    pub fn record_success(&self) {
        self.short_counter.record_success();
        self.long_counter.record_success();
        self.total_success.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录一次失败
    pub fn record_failure(&self) {
        self.short_counter.record_failure();
        self.long_counter.record_failure();
        self.total_failure.fetch_add(1, Ordering::Relaxed);
    }

    /// 当前燃烧率
    pub fn burn_rate(&self) -> SloBurnRate {
        // 触发窗口轮转检查（即使无新请求也能轮转）
        self.short_counter.rotate_if_needed();
        self.long_counter.rotate_if_needed();

        let short_rate = self.short_counter.success_rate();
        let long_rate = self.long_counter.success_rate();
        let allowed_error_rate = 1.0 - self.config.target_success_rate;

        let short_error = 1.0 - short_rate;
        let long_error = 1.0 - long_rate;

        let short_burn = if allowed_error_rate > 0.0 {
            short_error / allowed_error_rate
        } else {
            0.0
        };
        let long_burn = if allowed_error_rate > 0.0 {
            long_error / allowed_error_rate
        } else {
            0.0
        };

        // 剩余预算
        let total_success = self.total_success.load(Ordering::Relaxed);
        let total_failure = self.total_failure.load(Ordering::Relaxed);
        let total = total_success + total_failure;
        let error_budget_remaining = if total == 0 {
            1.0
        } else {
            let actual_error_rate = total_failure as f64 / total as f64;
            let consumed = (actual_error_rate / allowed_error_rate).min(1.0);
            1.0 - consumed
        };

        // 触发告警：短窗口和长窗口都超过阈值（多窗口告警策略）
        let alerting = short_burn > self.config.burn_rate_threshold
            && long_burn > self.config.burn_rate_threshold;

        SloBurnRate {
            short_success_rate: short_rate,
            long_success_rate: long_rate,
            short_burn_rate: short_burn,
            long_burn_rate: long_burn,
            error_budget_remaining,
            alerting,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_slo_no_data() {
        let monitor = SloMonitor::new(SloConfig::default());
        let rate = monitor.burn_rate();
        assert_eq!(rate.short_success_rate, 1.0);
        assert_eq!(rate.long_success_rate, 1.0);
        assert!(!rate.alerting);
    }

    #[test]
    fn test_slo_all_success() {
        let monitor = SloMonitor::new(SloConfig::default());
        for _ in 0..1000 {
            monitor.record_success();
        }
        let rate = monitor.burn_rate();
        assert_eq!(rate.short_success_rate, 1.0);
        assert!(!rate.alerting);
    }

    #[test]
    fn test_slo_with_failures() {
        let monitor = SloMonitor::new(SloConfig {
            target_success_rate: 0.99,
            short_window: Duration::from_millis(100),
            long_window: Duration::from_millis(200),
            burn_rate_threshold: 1.0,
        });
        // 1000 次成功 + 100 次失败 = 90.9% 成功率（低于 99% SLO）
        for _ in 0..1000 {
            monitor.record_success();
        }
        for _ in 0..100 {
            monitor.record_failure();
        }
        let rate = monitor.burn_rate();
        assert!(rate.short_success_rate < 0.99);
        assert!(rate.short_burn_rate > 1.0);
        assert!(rate.error_budget_remaining < 1.0);
    }

    #[test]
    fn test_slo_alerting() {
        let monitor = SloMonitor::new(SloConfig {
            target_success_rate: 0.999,
            short_window: Duration::from_millis(100),
            long_window: Duration::from_millis(200),
            burn_rate_threshold: 1.0,
        });
        // 100 次成功 + 10 次失败 = 90.9% 成功率（远低于 99.9% SLO）
        // 燃烧率 = (0.091 / 0.001) = 91x > 1.0 阈值
        for _ in 0..100 {
            monitor.record_success();
        }
        for _ in 0..10 {
            monitor.record_failure();
        }
        let rate = monitor.burn_rate();
        assert!(rate.alerting, "Should be alerting: {:?}", rate);
    }

    #[test]
    fn test_window_rotation() {
        let monitor = SloMonitor::new(SloConfig {
            target_success_rate: 0.99,
            short_window: Duration::from_millis(50),
            long_window: Duration::from_millis(100),
            burn_rate_threshold: 1.0,
        });
        // 第一批请求
        for _ in 0..10 {
            monitor.record_failure();
        }
        let rate1 = monitor.burn_rate();
        assert!(rate1.short_burn_rate > 0.0);

        // 等待窗口过期
        sleep(Duration::from_millis(150));

        // 窗口应已轮转
        let rate2 = monitor.burn_rate();
        assert_eq!(rate2.short_success_rate, 1.0); // 无数据视为 1.0
        assert_eq!(rate2.long_success_rate, 1.0);
    }
}
