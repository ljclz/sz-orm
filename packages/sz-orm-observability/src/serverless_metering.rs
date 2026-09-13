//! v7.0.0 Serverless 按需计费度量收集器
//!
//! 收集请求计数、执行时长、连接驻留时长，周期上报至 Serverless 平台。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 计费度量报告
#[derive(Debug, Clone)]
pub struct MeteringReport {
    /// 请求总数
    pub request_count: u64,
    /// 总执行时长（毫秒）
    pub total_duration_ms: u64,
    /// 总连接驻留时长（毫秒）
    pub total_connection_resident_ms: u64,
    /// 导出时间戳（Unix 毫秒）
    pub export_time_ms: u64,
}

/// 上报状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportStatus {
    /// 上报成功
    Success,
    /// 上报失败，已缓冲
    Buffered,
}

/// 按需计费度量收集器（v7.0.0）
///
/// 无锁原子计数器收集请求事件，周期上报至 Serverless 平台。
/// 上报失败时本地缓冲并周期重试。
pub struct MeteringCollector {
    request_count: AtomicU64,
    total_duration_ns: AtomicU64,
    total_connection_resident_ns: AtomicU64,
    /// 上报间隔（默认 60s）
    report_interval: Duration,
    /// 缓冲失败次数
    buffered_failures: AtomicU64,
    /// 上报成功次数
    report_successes: AtomicU64,
}

impl Default for MeteringCollector {
    fn default() -> Self {
        Self::new(Duration::from_secs(60))
    }
}

impl MeteringCollector {
    /// 创建度量收集器
    pub fn new(report_interval: Duration) -> Self {
        Self {
            request_count: AtomicU64::new(0),
            total_duration_ns: AtomicU64::new(0),
            total_connection_resident_ns: AtomicU64::new(0),
            report_interval,
            buffered_failures: AtomicU64::new(0),
            report_successes: AtomicU64::new(0),
        }
    }

    /// 上报间隔
    pub fn report_interval(&self) -> Duration {
        self.report_interval
    }

    /// 记录请求事件
    pub fn record_request(&self, duration: Duration, connection_resident: Duration) {
        self.request_count.fetch_add(1, Ordering::Relaxed);
        self.total_duration_ns
            .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
        self.total_connection_resident_ns
            .fetch_add(connection_resident.as_nanos() as u64, Ordering::Relaxed);
    }

    /// 导出计费度量报告
    pub fn export(&self) -> MeteringReport {
        MeteringReport {
            request_count: self.request_count.load(Ordering::Relaxed),
            total_duration_ms: self.total_duration_ns.load(Ordering::Relaxed) / 1_000_000,
            total_connection_resident_ms: self.total_connection_resident_ns.load(Ordering::Relaxed)
                / 1_000_000,
            export_time_ms: now_ms(),
        }
    }

    /// 模拟上报（成功时清零计数，失败时缓冲）
    pub fn try_report(&self, success: bool) -> ReportStatus {
        if success {
            self.report_successes.fetch_add(1, Ordering::Relaxed);
            self.request_count.store(0, Ordering::Relaxed);
            self.total_duration_ns.store(0, Ordering::Relaxed);
            self.total_connection_resident_ns
                .store(0, Ordering::Relaxed);
            ReportStatus::Success
        } else {
            self.buffered_failures.fetch_add(1, Ordering::Relaxed);
            ReportStatus::Buffered
        }
    }

    /// 缓冲失败次数
    pub fn buffered_failures(&self) -> u64 {
        self.buffered_failures.load(Ordering::Relaxed)
    }

    /// 上报成功次数
    pub fn report_successes(&self) -> u64 {
        self.report_successes.load(Ordering::Relaxed)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metering_record_request() {
        let collector = MeteringCollector::default();
        collector.record_request(Duration::from_millis(50), Duration::from_millis(100));
        collector.record_request(Duration::from_millis(30), Duration::from_millis(80));
        let report = collector.export();
        assert_eq!(report.request_count, 2);
        assert_eq!(report.total_duration_ms, 80);
        assert_eq!(report.total_connection_resident_ms, 180);
    }

    #[test]
    fn test_metering_export_empty() {
        let collector = MeteringCollector::default();
        let report = collector.export();
        assert_eq!(report.request_count, 0);
        assert_eq!(report.total_duration_ms, 0);
    }

    #[test]
    fn test_metering_report_success_clears_counters() {
        let collector = MeteringCollector::default();
        collector.record_request(Duration::from_millis(50), Duration::from_millis(100));
        let status = collector.try_report(true);
        assert_eq!(status, ReportStatus::Success);
        assert_eq!(collector.report_successes(), 1);
        let report = collector.export();
        assert_eq!(report.request_count, 0);
    }

    #[test]
    fn test_metering_report_failure_buffers() {
        let collector = MeteringCollector::default();
        collector.record_request(Duration::from_millis(50), Duration::from_millis(100));
        let status = collector.try_report(false);
        assert_eq!(status, ReportStatus::Buffered);
        assert_eq!(collector.buffered_failures(), 1);
        let report = collector.export();
        assert_eq!(report.request_count, 1);
    }

    #[test]
    fn test_metering_custom_interval() {
        let collector = MeteringCollector::new(Duration::from_secs(30));
        assert_eq!(collector.report_interval(), Duration::from_secs(30));
    }

    #[test]
    fn test_metering_retry_after_failure() {
        let collector = MeteringCollector::default();
        collector.record_request(Duration::from_millis(50), Duration::from_millis(100));
        collector.try_report(false);
        collector.try_report(true);
        assert_eq!(collector.buffered_failures(), 1);
        assert_eq!(collector.report_successes(), 1);
    }
}
