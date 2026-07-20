//! SLO 燃烧率监控
//!
//! 基于 Google SRE 实践（Google SRE Workbook 第 5 章），使用多窗口多燃烧率告警策略：
//!
//! v0.2.2 修复 P2-1：从 2 窗口扩展为 4 窗口（标准 Google SRE 多窗口燃烧率告警）：
//!
//! - **Page 告警**（短期，立即触发）：1h 长窗口 + 5m 短窗口，阈值 14.4x（2% 预算/小时）
//! - **Ticket 告警**（长期，工单系统）：6h 长窗口 + 30m 短窗口，阈值 6x（5% 预算/6 小时）
//!
//! 燃烧率 = 实际错误率 / 允许错误率
//!
//! 当燃烧率超过阈值时，表示 SLO 预算正在被快速消耗。
//!
//! # 多窗口告警原理
//!
//! 单一窗口告警会有误报或漏报：
//! - 仅长窗口：响应过慢，故障已影响用户才告警
//! - 仅短窗口：易因瞬时抖动误报
//!
//! 多窗口策略：短窗口和长窗口都超过阈值才告警，平衡灵敏度与准确度。
//!
//! # 示例
//!
//! ```
//! use sz_orm_observability::{SloConfig, SloMonitor};
//! use std::time::Duration;
//!
//! // 默认配置即为 Google SRE 推荐：4 窗口 + Page/Ticket 双告警
//! let config = SloConfig::default();
//! let monitor = SloMonitor::new(config);
//!
//! // 记录请求结果
//! monitor.record_success();
//! monitor.record_failure();
//!
//! // 获取燃烧率（包含 Page 和 Ticket 两级告警）
//! let rate = monitor.burn_rate();
//! println!("Page alerting: {}, Ticket alerting: {}",
//!     rate.page_alerting, rate.ticket_alerting);
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;

/// SLO 配置
///
/// v0.2.2 修复 P2-1：扩展为 4 窗口（Page + Ticket 两级告警）。
#[derive(Debug, Clone)]
pub struct SloConfig {
    /// 目标成功率（如 0.999 表示 99.9%）
    pub target_success_rate: f64,

    // ==================== Page 告警（短期，立即触发）====================
    /// Page 告警的长窗口时长（如 1 小时）
    ///
    /// 兼容旧字段：等价于 `page_long_window`。Google SRE 推荐 1 小时。
    pub long_window: Duration,
    /// Page 告警的短窗口时长（如 5 分钟）
    ///
    /// 兼容旧字段：等价于 `page_short_window`。Google SRE 推荐 5 分钟。
    pub short_window: Duration,
    /// Page 告警的燃烧率阈值（如 14.4 表示 1 小时消耗 2% 预算）
    ///
    /// 兼容旧字段：等价于 `page_burn_rate_threshold`。
    pub burn_rate_threshold: f64,

    // ==================== Ticket 告警（长期，工单系统）====================
    /// Ticket 告警的长窗口时长（如 6 小时）
    pub ticket_long_window: Duration,
    /// Ticket 告警的短窗口时长（如 30 分钟）
    pub ticket_short_window: Duration,
    /// Ticket 告警的燃烧率阈值（如 6.0 表示 6 小时消耗 5% 预算）
    pub ticket_burn_rate_threshold: f64,
}

impl Default for SloConfig {
    fn default() -> Self {
        // Google SRE Workbook 第 5 章推荐配置
        Self {
            target_success_rate: 0.999,
            // Page 告警：1h + 5m，14.4x（2% 预算/小时）
            long_window: Duration::from_secs(3600),
            short_window: Duration::from_secs(300),
            burn_rate_threshold: 14.4,
            // Ticket 告警：6h + 30m，6.0x（5% 预算/6 小时）
            ticket_long_window: Duration::from_secs(6 * 3600),
            ticket_short_window: Duration::from_secs(30 * 60),
            ticket_burn_rate_threshold: 6.0,
        }
    }
}

/// SLO 燃烧率快照
#[derive(Debug, Clone)]
pub struct SloBurnRate {
    /// Page 短窗口实际成功率
    pub short_success_rate: f64,
    /// Page 长窗口实际成功率
    pub long_success_rate: f64,
    /// Page 短窗口燃烧率
    pub short_burn_rate: f64,
    /// Page 长窗口燃烧率
    pub long_burn_rate: f64,
    /// 剩余 SLO 预算（0.0-1.0）
    pub error_budget_remaining: f64,
    /// Page 告警是否触发（兼容旧字段，等价于 `page_alerting`）
    pub alerting: bool,

    // ==================== v0.2.2 修复 P2-1：Ticket 告警字段 ====================
    /// Ticket 短窗口实际成功率
    pub ticket_short_success_rate: f64,
    /// Ticket 长窗口实际成功率
    pub ticket_long_success_rate: f64,
    /// Ticket 短窗口燃烧率
    pub ticket_short_burn_rate: f64,
    /// Ticket 长窗口燃烧率
    pub ticket_long_burn_rate: f64,
    /// Page 告警是否触发（短期故障，立即通知）
    pub page_alerting: bool,
    /// Ticket 告警是否触发（长期故障，工单系统）
    pub ticket_alerting: bool,
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
            ticket_short_success_rate: 1.0,
            ticket_long_success_rate: 1.0,
            ticket_short_burn_rate: 0.0,
            ticket_long_burn_rate: 0.0,
            page_alerting: false,
            ticket_alerting: false,
        }
    }
}

impl std::fmt::Display for SloBurnRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SloBurnRate{{page[short={:.4}/{:.2}x, long={:.4}/{:.2}x, alert={}], ticket[short={:.4}/{:.2}x, long={:.4}/{:.2}x, alert={}], budget={:.2}%}}",
            self.short_success_rate,
            self.short_burn_rate,
            self.long_success_rate,
            self.long_burn_rate,
            self.page_alerting,
            self.ticket_short_success_rate,
            self.ticket_short_burn_rate,
            self.ticket_long_success_rate,
            self.ticket_long_burn_rate,
            self.ticket_alerting,
            self.error_budget_remaining * 100.0,
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
///
/// v0.2.2 修复 P2-1：扩展为 4 窗口（Page + Ticket 两级告警）。
pub struct SloMonitor {
    config: SloConfig,
    // Page 告警窗口（短期）
    page_short_counter: WindowedCounter,
    page_long_counter: WindowedCounter,
    // Ticket 告警窗口（长期）
    ticket_short_counter: WindowedCounter,
    ticket_long_counter: WindowedCounter,
    /// 累计成功数（自启动以来）
    total_success: AtomicU64,
    /// 累计失败数（自启动以来）
    total_failure: AtomicU64,
}

impl SloMonitor {
    /// 创建新监控器
    pub fn new(config: SloConfig) -> Self {
        let page_short_counter = WindowedCounter::new(config.short_window);
        let page_long_counter = WindowedCounter::new(config.long_window);
        let ticket_short_counter = WindowedCounter::new(config.ticket_short_window);
        let ticket_long_counter = WindowedCounter::new(config.ticket_long_window);
        Self {
            config,
            page_short_counter,
            page_long_counter,
            ticket_short_counter,
            ticket_long_counter,
            total_success: AtomicU64::new(0),
            total_failure: AtomicU64::new(0),
        }
    }

    /// 记录一次成功
    pub fn record_success(&self) {
        self.page_short_counter.record_success();
        self.page_long_counter.record_success();
        self.ticket_short_counter.record_success();
        self.ticket_long_counter.record_success();
        self.total_success.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录一次失败
    pub fn record_failure(&self) {
        self.page_short_counter.record_failure();
        self.page_long_counter.record_failure();
        self.ticket_short_counter.record_failure();
        self.ticket_long_counter.record_failure();
        self.total_failure.fetch_add(1, Ordering::Relaxed);
    }

    /// 当前燃烧率
    pub fn burn_rate(&self) -> SloBurnRate {
        // 触发窗口轮转检查（即使无新请求也能轮转）
        self.page_short_counter.rotate_if_needed();
        self.page_long_counter.rotate_if_needed();
        self.ticket_short_counter.rotate_if_needed();
        self.ticket_long_counter.rotate_if_needed();

        // Page 窗口
        let page_short_rate = self.page_short_counter.success_rate();
        let page_long_rate = self.page_long_counter.success_rate();

        // Ticket 窗口
        let ticket_short_rate = self.ticket_short_counter.success_rate();
        let ticket_long_rate = self.ticket_long_counter.success_rate();

        let allowed_error_rate = 1.0 - self.config.target_success_rate;

        let page_short_error = 1.0 - page_short_rate;
        let page_long_error = 1.0 - page_long_rate;
        let ticket_short_error = 1.0 - ticket_short_rate;
        let ticket_long_error = 1.0 - ticket_long_rate;

        let page_short_burn = if allowed_error_rate > 0.0 {
            page_short_error / allowed_error_rate
        } else {
            0.0
        };
        let page_long_burn = if allowed_error_rate > 0.0 {
            page_long_error / allowed_error_rate
        } else {
            0.0
        };
        let ticket_short_burn = if allowed_error_rate > 0.0 {
            ticket_short_error / allowed_error_rate
        } else {
            0.0
        };
        let ticket_long_burn = if allowed_error_rate > 0.0 {
            ticket_long_error / allowed_error_rate
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

        // v0.2.2 修复 P2-1：多窗口告警策略
        //
        // Page 告警：page_short 和 page_long 都超过 page_threshold
        // Ticket 告警：ticket_short 和 ticket_long 都超过 ticket_threshold
        //
        // 两级告警互相独立，避免短期故障误触发工单系统。
        let page_alerting = page_short_burn > self.config.burn_rate_threshold
            && page_long_burn > self.config.burn_rate_threshold;
        let ticket_alerting = ticket_short_burn > self.config.ticket_burn_rate_threshold
            && ticket_long_burn > self.config.ticket_burn_rate_threshold;

        SloBurnRate {
            short_success_rate: page_short_rate,
            long_success_rate: page_long_rate,
            short_burn_rate: page_short_burn,
            long_burn_rate: page_long_burn,
            error_budget_remaining,
            // 兼容旧字段：alerting 等价于 page_alerting
            alerting: page_alerting,
            ticket_short_success_rate: ticket_short_rate,
            ticket_long_success_rate: ticket_long_rate,
            ticket_short_burn_rate: ticket_short_burn,
            ticket_long_burn_rate: ticket_long_burn,
            page_alerting,
            ticket_alerting,
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
        assert!(!rate.page_alerting);
        assert!(!rate.ticket_alerting);
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
        assert!(!rate.page_alerting);
        assert!(!rate.ticket_alerting);
    }

    #[test]
    fn test_slo_with_failures() {
        let monitor = SloMonitor::new(SloConfig {
            target_success_rate: 0.99,
            short_window: Duration::from_millis(100),
            long_window: Duration::from_millis(200),
            burn_rate_threshold: 1.0,
            ticket_short_window: Duration::from_millis(300),
            ticket_long_window: Duration::from_millis(500),
            ticket_burn_rate_threshold: 1.0,
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
        // Ticket 窗口也应看到失败
        assert!(rate.ticket_short_burn_rate > 1.0);
    }

    #[test]
    fn test_slo_page_alerting() {
        let monitor = SloMonitor::new(SloConfig {
            target_success_rate: 0.999,
            short_window: Duration::from_millis(100),
            long_window: Duration::from_millis(200),
            burn_rate_threshold: 1.0,
            ticket_short_window: Duration::from_millis(300),
            ticket_long_window: Duration::from_millis(500),
            ticket_burn_rate_threshold: 100.0, // 设置高阈值，确保 ticket 不触发
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
        assert!(rate.page_alerting, "Page should be alerting: {:?}", rate);
        // 兼容字段
        assert!(rate.alerting);
        // Ticket 不应触发（阈值很高）
        assert!(
            !rate.ticket_alerting,
            "Ticket should NOT alerting: {:?}",
            rate
        );
    }

    #[test]
    fn test_slo_ticket_alerting_independent() {
        // 验证 Ticket 告警独立于 Page 告警
        let monitor = SloMonitor::new(SloConfig {
            target_success_rate: 0.999,
            // Page 窗口很短，容易过期 → Page 不告警
            short_window: Duration::from_millis(50),
            long_window: Duration::from_millis(80),
            burn_rate_threshold: 1.0,
            // Ticket 窗口较长 → Ticket 告警
            ticket_short_window: Duration::from_millis(500),
            ticket_long_window: Duration::from_millis(1000),
            ticket_burn_rate_threshold: 1.0,
        });
        for _ in 0..100 {
            monitor.record_success();
        }
        for _ in 0..10 {
            monitor.record_failure();
        }
        let rate = monitor.burn_rate();
        // 此时 Page 和 Ticket 都应告警（数据在两个窗口内）
        assert!(
            rate.ticket_alerting,
            "Ticket should be alerting: {:?}",
            rate
        );

        // 等待 Page 窗口过期但 Ticket 窗口未过期
        sleep(Duration::from_millis(200));

        let rate2 = monitor.burn_rate();
        // Page 窗口已过期 → Page 不告警
        assert!(
            !rate2.page_alerting,
            "Page should NOT alerting after window rotation: {:?}",
            rate2
        );
        // Ticket 窗口未过期 → Ticket 仍告警
        assert!(
            rate2.ticket_alerting,
            "Ticket should still alerting: {:?}",
            rate2
        );
    }

    #[test]
    fn test_window_rotation() {
        let monitor = SloMonitor::new(SloConfig {
            target_success_rate: 0.99,
            short_window: Duration::from_millis(50),
            long_window: Duration::from_millis(100),
            burn_rate_threshold: 1.0,
            ticket_short_window: Duration::from_millis(150),
            ticket_long_window: Duration::from_millis(200),
            ticket_burn_rate_threshold: 1.0,
        });
        // 第一批请求
        for _ in 0..10 {
            monitor.record_failure();
        }
        let rate1 = monitor.burn_rate();
        assert!(rate1.short_burn_rate > 0.0);

        // 等待所有窗口过期
        sleep(Duration::from_millis(250));

        // 所有窗口应已轮转
        let rate2 = monitor.burn_rate();
        assert_eq!(rate2.short_success_rate, 1.0); // 无数据视为 1.0
        assert_eq!(rate2.long_success_rate, 1.0);
        assert_eq!(rate2.ticket_short_success_rate, 1.0);
        assert_eq!(rate2.ticket_long_success_rate, 1.0);
    }

    #[test]
    fn test_default_config_is_google_sre_recommended() {
        // v0.2.2 修复 P2-1：默认配置应符合 Google SRE Workbook 第 5 章推荐
        let config = SloConfig::default();
        // Page: 5m + 1h，14.4x
        assert_eq!(config.short_window, Duration::from_secs(300)); // 5 分钟
        assert_eq!(config.long_window, Duration::from_secs(3600)); // 1 小时
        assert!((config.burn_rate_threshold - 14.4).abs() < 0.01);
        // Ticket: 30m + 6h，6.0x
        assert_eq!(config.ticket_short_window, Duration::from_secs(30 * 60)); // 30 分钟
        assert_eq!(config.ticket_long_window, Duration::from_secs(6 * 3600)); // 6 小时
        assert!((config.ticket_burn_rate_threshold - 6.0).abs() < 0.01);
    }

    #[test]
    fn test_display_includes_both_alerts() {
        let monitor = SloMonitor::new(SloConfig::default());
        let rate = monitor.burn_rate();
        let s = format!("{}", rate);
        // Display 输出应包含 page 和 ticket 两级告警信息
        assert!(s.contains("page["), "display should include page: {}", s);
        assert!(
            s.contains("ticket["),
            "display should include ticket: {}",
            s
        );
    }
}
