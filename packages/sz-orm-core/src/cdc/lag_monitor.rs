//! CDC 延迟监控
//!
//! 监控 CDC 事件从源端到 Sink 的处理延迟。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// CDC 延迟监控器
pub struct CdcLagMonitor {
    /// 最大延迟（毫秒）
    max_lag_ms: AtomicU64,
    /// 总延迟（毫秒）
    total_lag_ms: AtomicU64,
    /// 事件计数
    event_count: AtomicU64,
    /// 最后事件时间戳（毫秒）
    last_event_ts: AtomicU64,
}

impl CdcLagMonitor {
    /// 创建监控器
    pub fn new() -> Self {
        Self {
            max_lag_ms: AtomicU64::new(0),
            total_lag_ms: AtomicU64::new(0),
            event_count: AtomicU64::new(0),
            last_event_ts: AtomicU64::new(0),
        }
    }

    /// 记录事件处理延迟
    pub fn record_event(&self, event_timestamp_ms: u64) {
        let now = current_time_ms();
        let lag = now.saturating_sub(event_timestamp_ms);

        self.max_lag_ms.fetch_max(lag, Ordering::Relaxed);
        self.total_lag_ms.fetch_add(lag, Ordering::Relaxed);
        self.event_count.fetch_add(1, Ordering::Relaxed);
        self.last_event_ts
            .store(event_timestamp_ms, Ordering::Relaxed);
    }

    /// 最大延迟（毫秒）
    pub fn max_lag_ms(&self) -> u64 {
        self.max_lag_ms.load(Ordering::Relaxed)
    }

    /// 平均延迟（毫秒）
    pub fn avg_lag_ms(&self) -> u64 {
        let count = self.event_count.load(Ordering::Relaxed);
        self.total_lag_ms
            .load(Ordering::Relaxed)
            .checked_div(count)
            .unwrap_or(0)
    }

    /// 已处理事件数
    pub fn event_count(&self) -> u64 {
        self.event_count.load(Ordering::Relaxed)
    }

    /// 最后事件时间戳
    pub fn last_event_ts(&self) -> u64 {
        self.last_event_ts.load(Ordering::Relaxed)
    }

    /// 检查是否超出延迟阈值
    pub fn is_lag_exceeded(&self, threshold_ms: u64) -> bool {
        self.max_lag_ms() > threshold_ms
    }
}

impl Default for CdcLagMonitor {
    fn default() -> Self {
        Self::new()
    }
}

fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_max_lag() {
        let monitor = CdcLagMonitor::new();
        let now = current_time_ms();
        monitor.record_event(now - 100);
        monitor.record_event(now - 200);
        assert!(monitor.max_lag_ms() >= 200);
    }

    #[test]
    fn test_avg_lag() {
        let monitor = CdcLagMonitor::new();
        let now = current_time_ms();
        monitor.record_event(now - 100);
        monitor.record_event(now - 300);
        let avg = monitor.avg_lag_ms();
        assert!((100..=300).contains(&avg));
    }

    #[test]
    fn test_event_count() {
        let monitor = CdcLagMonitor::new();
        let now = current_time_ms();
        monitor.record_event(now);
        monitor.record_event(now);
        monitor.record_event(now);
        assert_eq!(monitor.event_count(), 3);
    }

    #[test]
    fn test_lag_threshold_check() {
        let monitor = CdcLagMonitor::new();
        let now = current_time_ms();
        monitor.record_event(now - 500);
        assert!(monitor.is_lag_exceeded(400));
        assert!(!monitor.is_lag_exceeded(600));
    }
}
