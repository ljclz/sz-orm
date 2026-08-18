//! 查询热度追踪器
//!
//! 提供 [`QueryHeatTracker`] 按时间窗口追踪查询热度，
//! 识别热点查询、冷门查询、突发流量等。

use std::collections::{HashMap, VecDeque};
use std::fmt;

/// 时间窗口
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeWindow {
    /// 1 分钟
    OneMinute,
    /// 5 分钟
    FiveMinutes,
    /// 15 分钟
    FifteenMinutes,
    /// 1 小时
    OneHour,
    /// 1 天
    OneDay,
}

impl TimeWindow {
    /// 返回窗口时长（毫秒）
    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        match self {
            TimeWindow::OneMinute => 60_000,
            TimeWindow::FiveMinutes => 300_000,
            TimeWindow::FifteenMinutes => 900_000,
            TimeWindow::OneHour => 3_600_000,
            TimeWindow::OneDay => 86_400_000,
        }
    }

    /// 返回描述
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            TimeWindow::OneMinute => "1min",
            TimeWindow::FiveMinutes => "5min",
            TimeWindow::FifteenMinutes => "15min",
            TimeWindow::OneHour => "1hour",
            TimeWindow::OneDay => "1day",
        }
    }
}

impl fmt::Display for TimeWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

/// 查询热度样本
#[derive(Debug, Clone)]
pub struct HeatSample {
    /// 窗口起始时间戳
    pub window_start: u64,
    /// 窗口结束时间戳
    pub window_end: u64,
    /// 查询次数
    pub count: u64,
    /// 总耗时
    pub total_time_ms: u64,
    /// 平均耗时
    pub avg_time_ms: f64,
    /// 错误次数
    pub error_count: u64,
}

impl HeatSample {
    /// 创建新样本
    #[must_use]
    pub fn new(window_start: u64, window_end: u64) -> Self {
        Self {
            window_start,
            window_end,
            count: 0,
            total_time_ms: 0,
            avg_time_ms: 0.0,
            error_count: 0,
        }
    }

    /// 记录一次查询
    pub fn record(&mut self, elapsed_ms: u64, is_error: bool) {
        self.count += 1;
        self.total_time_ms += elapsed_ms;
        self.avg_time_ms = self.total_time_ms as f64 / self.count as f64;
        if is_error {
            self.error_count += 1;
        }
    }

    /// 错误率
    #[must_use]
    pub fn error_rate(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        self.error_count as f64 / self.count as f64
    }

    /// 每秒查询率（QPS）
    #[must_use]
    pub fn qps(&self) -> f64 {
        let duration = self.window_end.saturating_sub(self.window_start);
        if duration == 0 {
            return 0.0;
        }
        self.count as f64 / (duration as f64 / 1000.0)
    }
}

impl fmt::Display for HeatSample {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "HeatSample(count={}, qps={:.1}, avg={:.1}ms, err_rate={:.2}%)",
            self.count,
            self.qps(),
            self.avg_time_ms,
            self.error_rate() * 100.0
        )
    }
}

/// 查询热度历史
#[derive(Debug, Clone)]
pub struct QueryHeatHistory {
    /// 查询指纹
    pub fingerprint: String,
    /// 历史样本（按时间顺序）
    pub samples: VecDeque<HeatSample>,
    /// 最大保留样本数
    max_samples: usize,
}

impl QueryHeatHistory {
    /// 创建新历史
    #[must_use]
    pub fn new(fingerprint: &str, max_samples: usize) -> Self {
        Self {
            fingerprint: fingerprint.to_string(),
            samples: VecDeque::with_capacity(max_samples),
            max_samples,
        }
    }

    /// 添加样本
    pub fn push_sample(&mut self, sample: HeatSample) {
        if self.samples.len() >= self.max_samples {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    /// 当前样本
    #[must_use]
    pub fn current(&self) -> Option<&HeatSample> {
        self.samples.back()
    }

    /// 总查询次数
    #[must_use]
    pub fn total_count(&self) -> u64 {
        self.samples.iter().map(|s| s.count).sum()
    }

    /// 总耗时
    #[must_use]
    pub fn total_time_ms(&self) -> u64 {
        self.samples.iter().map(|s| s.total_time_ms).sum()
    }

    /// 平均QPS
    #[must_use]
    pub fn avg_qps(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let total_qps: f64 = self.samples.iter().map(|s| s.qps()).sum();
        total_qps / self.samples.len() as f64
    }

    /// 峰值QPS
    #[must_use]
    pub fn peak_qps(&self) -> f64 {
        self.samples.iter().map(|s| s.qps()).fold(0.0_f64, f64::max)
    }

    /// 是否为热点查询（QPS超过阈值）
    #[must_use]
    pub fn is_hot(&self, qps_threshold: f64) -> bool {
        self.current()
            .map(|s| s.qps() > qps_threshold)
            .unwrap_or(false)
    }

    /// 是否为冷门查询（总次数低于阈值）
    #[must_use]
    pub fn is_cold(&self, count_threshold: u64) -> bool {
        self.total_count() < count_threshold
    }

    /// 是否有突发流量（当前QPS > 2 * 平均QPS）
    #[must_use]
    pub fn has_burst(&self) -> bool {
        let Some(current) = self.current() else {
            return false;
        };
        let avg = self.avg_qps();
        avg > 0.0 && current.qps() > 2.0 * avg
    }

    /// 错误率趋势（最近样本 vs 之前样本）
    #[must_use]
    pub fn error_trend(&self) -> ErrorTrend {
        if self.samples.len() < 2 {
            return ErrorTrend::Stable;
        }
        let recent = self.samples.back().unwrap().error_rate();
        let older: f64 = {
            let n = self.samples.len();
            let prev: f64 = self
                .samples
                .iter()
                .take(n - 1)
                .map(|s| s.error_rate())
                .sum::<f64>()
                / (n - 1) as f64;
            prev
        };
        let diff = recent - older;
        if diff > 0.05 {
            ErrorTrend::Increasing
        } else if diff < -0.05 {
            ErrorTrend::Decreasing
        } else {
            ErrorTrend::Stable
        }
    }
}

/// 错误率趋势
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorTrend {
    /// 上升
    Increasing,
    /// 下降
    Decreasing,
    /// 稳定
    Stable,
}

impl ErrorTrend {
    /// 返回描述
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            ErrorTrend::Increasing => "increasing",
            ErrorTrend::Decreasing => "decreasing",
            ErrorTrend::Stable => "stable",
        }
    }
}

/// 查询热度追踪器
#[derive(Debug)]
pub struct QueryHeatTracker {
    /// 查询热度历史映射
    histories: HashMap<String, QueryHeatHistory>,
    /// 时间窗口
    window: TimeWindow,
    /// 最大保留样本数
    max_samples: usize,
}

impl QueryHeatTracker {
    /// 创建新追踪器
    #[must_use]
    pub fn new(window: TimeWindow) -> Self {
        Self {
            histories: HashMap::new(),
            window,
            max_samples: 60,
        }
    }

    /// 设置最大样本数
    #[must_use]
    pub fn with_max_samples(mut self, max: usize) -> Self {
        self.max_samples = max;
        self
    }

    /// 记录查询
    pub fn record(&mut self, fingerprint: &str, timestamp: u64, elapsed_ms: u64, is_error: bool) {
        let window_start = (timestamp / self.window.duration_ms()) * self.window.duration_ms();
        let window_end = window_start + self.window.duration_ms();
        let history = self
            .histories
            .entry(fingerprint.to_string())
            .or_insert_with(|| QueryHeatHistory::new(fingerprint, self.max_samples));
        if let Some(sample) = history.samples.back_mut() {
            if sample.window_start == window_start {
                sample.record(elapsed_ms, is_error);
                return;
            }
        }
        let mut sample = HeatSample::new(window_start, window_end);
        sample.record(elapsed_ms, is_error);
        history.push_sample(sample);
    }

    /// 获取查询热度历史
    #[must_use]
    pub fn history(&self, fingerprint: &str) -> Option<&QueryHeatHistory> {
        self.histories.get(fingerprint)
    }

    /// 所有指纹
    #[must_use]
    pub fn fingerprints(&self) -> Vec<String> {
        self.histories.keys().cloned().collect()
    }

    /// 获取热点查询（按当前QPS排序）
    #[must_use]
    pub fn hot_queries(&self, qps_threshold: f64, limit: usize) -> Vec<(&str, &QueryHeatHistory)> {
        let mut hot: Vec<(&str, &QueryHeatHistory)> = self
            .histories
            .iter()
            .filter(|(_, h)| h.is_hot(qps_threshold))
            .map(|(k, v)| (k.as_str(), v))
            .collect();
        hot.sort_by(|a, b| {
            let qps_a = a.1.current().map(|s| s.qps()).unwrap_or(0.0);
            let qps_b = b.1.current().map(|s| s.qps()).unwrap_or(0.0);
            qps_b
                .partial_cmp(&qps_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hot.into_iter().take(limit).collect()
    }

    /// 获取冷门查询
    #[must_use]
    pub fn cold_queries(&self, count_threshold: u64) -> Vec<(&str, &QueryHeatHistory)> {
        self.histories
            .iter()
            .filter(|(_, h)| h.is_cold(count_threshold))
            .map(|(k, v)| (k.as_str(), v))
            .collect()
    }

    /// 获取突发流量查询
    #[must_use]
    pub fn burst_queries(&self) -> Vec<(&str, &QueryHeatHistory)> {
        self.histories
            .iter()
            .filter(|(_, h)| h.has_burst())
            .map(|(k, v)| (k.as_str(), v))
            .collect()
    }

    /// 获取错误率上升的查询
    #[must_use]
    pub fn queries_with_increasing_errors(&self) -> Vec<(&str, &QueryHeatHistory)> {
        self.histories
            .iter()
            .filter(|(_, h)| h.error_trend() == ErrorTrend::Increasing)
            .map(|(k, v)| (k.as_str(), v))
            .collect()
    }

    /// 查询数
    #[must_use]
    pub fn query_count(&self) -> usize {
        self.histories.len()
    }

    /// 时间窗口
    #[must_use]
    pub fn window(&self) -> TimeWindow {
        self.window
    }

    /// 生成热度报告
    #[must_use]
    pub fn report(&self, qps_threshold: f64) -> HeatReport {
        let hot = self.hot_queries(qps_threshold, 10);
        let burst = self.burst_queries();
        let error_rising = self.queries_with_increasing_errors();
        let total_count: u64 = self.histories.values().map(|h| h.total_count()).sum();
        HeatReport {
            total_queries: self.query_count(),
            total_count,
            window: self.window,
            hot_count: hot.len(),
            burst_count: burst.len(),
            error_rising_count: error_rising.len(),
            hot_queries: hot
                .iter()
                .map(|(fp, h)| HeatQuerySummary {
                    fingerprint: fp.to_string(),
                    current_qps: h.current().map(|s| s.qps()).unwrap_or(0.0),
                    peak_qps: h.peak_qps(),
                    total_count: h.total_count(),
                })
                .collect(),
        }
    }
}

impl fmt::Display for QueryHeatTracker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "QueryHeatTracker(queries={}, window={})",
            self.query_count(),
            self.window
        )
    }
}

/// 热度报告
#[derive(Debug, Clone)]
pub struct HeatReport {
    /// 查询总数
    pub total_queries: usize,
    /// 总查询次数
    pub total_count: u64,
    /// 时间窗口
    pub window: TimeWindow,
    /// 热点查询数
    pub hot_count: usize,
    /// 突发流量查询数
    pub burst_count: usize,
    /// 错误率上升查询数
    pub error_rising_count: usize,
    /// 热点查询摘要
    pub hot_queries: Vec<HeatQuerySummary>,
}

/// 热点查询摘要
#[derive(Debug, Clone)]
pub struct HeatQuerySummary {
    /// 查询指纹
    pub fingerprint: String,
    /// 当前QPS
    pub current_qps: f64,
    /// 峰值QPS
    pub peak_qps: f64,
    /// 总查询次数
    pub total_count: u64,
}

impl fmt::Display for HeatReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "HeatReport(queries={}, count={}, hot={}, burst={})",
            self.total_queries, self.total_count, self.hot_count, self.burst_count
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_window_duration() {
        assert_eq!(TimeWindow::OneMinute.duration_ms(), 60_000);
        assert_eq!(TimeWindow::FiveMinutes.duration_ms(), 300_000);
        assert_eq!(TimeWindow::OneHour.duration_ms(), 3_600_000);
        assert_eq!(TimeWindow::OneDay.duration_ms(), 86_400_000);
    }

    #[test]
    fn test_time_window_description() {
        assert_eq!(TimeWindow::OneMinute.description(), "1min");
        assert_eq!(TimeWindow::OneHour.description(), "1hour");
    }

    #[test]
    fn test_time_window_display() {
        let s = format!("{}", TimeWindow::FiveMinutes);
        assert!(s.contains("5min"));
    }

    #[test]
    fn test_heat_sample_new() {
        let s = HeatSample::new(0, 60_000);
        assert_eq!(s.count, 0);
        assert_eq!(s.window_start, 0);
        assert_eq!(s.window_end, 60_000);
    }

    #[test]
    fn test_heat_sample_record() {
        let mut s = HeatSample::new(0, 60_000);
        s.record(10, false);
        s.record(20, true);
        assert_eq!(s.count, 2);
        assert_eq!(s.total_time_ms, 30);
        assert_eq!(s.error_count, 1);
        assert!((s.avg_time_ms - 15.0).abs() < 1e-10);
    }

    #[test]
    fn test_heat_sample_error_rate() {
        let mut s = HeatSample::new(0, 60_000);
        s.record(10, false);
        s.record(10, true);
        assert!((s.error_rate() - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_heat_sample_error_rate_no_data() {
        let s = HeatSample::new(0, 60_000);
        assert!((s.error_rate() - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_heat_sample_qps() {
        let mut s = HeatSample::new(0, 60_000);
        for _ in 0..120 {
            s.record(10, false);
        }
        assert!((s.qps() - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_heat_sample_qps_zero_duration() {
        let s = HeatSample::new(0, 0);
        assert!((s.qps() - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_heat_sample_display() {
        let mut s = HeatSample::new(0, 60_000);
        s.record(10, false);
        let str = format!("{}", s);
        assert!(str.contains("HeatSample"));
    }

    #[test]
    fn test_query_heat_history_new() {
        let h = QueryHeatHistory::new("test", 10);
        assert_eq!(h.fingerprint, "test");
        assert!(h.samples.is_empty());
    }

    #[test]
    fn test_query_heat_history_push_sample() {
        let mut h = QueryHeatHistory::new("test", 10);
        h.push_sample(HeatSample::new(0, 60_000));
        assert_eq!(h.samples.len(), 1);
    }

    #[test]
    fn test_query_heat_history_eviction() {
        let mut h = QueryHeatHistory::new("test", 2);
        h.push_sample(HeatSample::new(0, 60_000));
        h.push_sample(HeatSample::new(60_000, 120_000));
        h.push_sample(HeatSample::new(120_000, 180_000));
        assert_eq!(h.samples.len(), 2);
    }

    #[test]
    fn test_query_heat_history_current() {
        let mut h = QueryHeatHistory::new("test", 10);
        h.push_sample(HeatSample::new(0, 60_000));
        assert!(h.current().is_some());
    }

    #[test]
    fn test_query_heat_history_total_count() {
        let mut h = QueryHeatHistory::new("test", 10);
        let mut s1 = HeatSample::new(0, 60_000);
        s1.record(10, false);
        s1.record(10, false);
        h.push_sample(s1);
        let mut s2 = HeatSample::new(60_000, 120_000);
        s2.record(10, false);
        h.push_sample(s2);
        assert_eq!(h.total_count(), 3);
    }

    #[test]
    fn test_query_heat_history_avg_qps() {
        let mut h = QueryHeatHistory::new("test", 10);
        let mut s = HeatSample::new(0, 60_000);
        s.record(10, false);
        h.push_sample(s);
        assert!(h.avg_qps() > 0.0);
    }

    #[test]
    fn test_query_heat_history_avg_qps_empty() {
        let h = QueryHeatHistory::new("test", 10);
        assert!((h.avg_qps() - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_query_heat_history_peak_qps() {
        let mut h = QueryHeatHistory::new("test", 10);
        let mut s1 = HeatSample::new(0, 60_000);
        s1.record(10, false);
        h.push_sample(s1);
        let mut s2 = HeatSample::new(60_000, 120_000);
        for _ in 0..120 {
            s2.record(10, false);
        }
        h.push_sample(s2);
        assert!(h.peak_qps() > 1.0);
    }

    #[test]
    fn test_query_heat_history_is_hot() {
        let mut h = QueryHeatHistory::new("test", 10);
        let mut s = HeatSample::new(0, 60_000);
        for _ in 0..120 {
            s.record(10, false);
        }
        h.push_sample(s);
        assert!(h.is_hot(1.0));
        assert!(!h.is_hot(10.0));
    }

    #[test]
    fn test_query_heat_history_is_cold() {
        let h = QueryHeatHistory::new("test", 10);
        assert!(h.is_cold(10));
    }

    #[test]
    fn test_query_heat_history_has_burst() {
        let mut h = QueryHeatHistory::new("test", 10);
        let mut s1 = HeatSample::new(0, 60_000);
        s1.record(10, false);
        h.push_sample(s1);
        let mut s2 = HeatSample::new(60_000, 120_000);
        s2.record(10, false);
        h.push_sample(s2);
        let mut s3 = HeatSample::new(120_000, 180_000);
        for _ in 0..600 {
            s3.record(10, false);
        }
        h.push_sample(s3);
        assert!(h.has_burst());
    }

    #[test]
    fn test_query_heat_history_no_burst() {
        let mut h = QueryHeatHistory::new("test", 10);
        let mut s1 = HeatSample::new(0, 60_000);
        s1.record(10, false);
        h.push_sample(s1);
        let mut s2 = HeatSample::new(60_000, 120_000);
        s2.record(10, false);
        h.push_sample(s2);
        assert!(!h.has_burst());
    }

    #[test]
    fn test_error_trend_stable() {
        let h = QueryHeatHistory::new("test", 10);
        assert_eq!(h.error_trend(), ErrorTrend::Stable);
    }

    #[test]
    fn test_error_trend_increasing() {
        let mut h = QueryHeatHistory::new("test", 10);
        let mut s1 = HeatSample::new(0, 60_000);
        s1.record(10, false);
        h.push_sample(s1);
        let mut s2 = HeatSample::new(60_000, 120_000);
        s2.record(10, true);
        h.push_sample(s2);
        assert_eq!(h.error_trend(), ErrorTrend::Increasing);
    }

    #[test]
    fn test_error_trend_description() {
        assert_eq!(ErrorTrend::Increasing.description(), "increasing");
        assert_eq!(ErrorTrend::Decreasing.description(), "decreasing");
        assert_eq!(ErrorTrend::Stable.description(), "stable");
    }

    #[test]
    fn test_query_heat_tracker_new() {
        let t = QueryHeatTracker::new(TimeWindow::OneMinute);
        assert_eq!(t.query_count(), 0);
        assert_eq!(t.window(), TimeWindow::OneMinute);
    }

    #[test]
    fn test_query_heat_tracker_with_max_samples() {
        let t = QueryHeatTracker::new(TimeWindow::OneMinute).with_max_samples(100);
        assert_eq!(t.query_count(), 0);
    }

    #[test]
    fn test_query_heat_tracker_record() {
        let mut t = QueryHeatTracker::new(TimeWindow::OneMinute);
        t.record("q1", 1000, 10, false);
        t.record("q1", 2000, 20, false);
        assert_eq!(t.query_count(), 1);
        let h = t.history("q1").unwrap();
        assert_eq!(h.total_count(), 2);
    }

    #[test]
    fn test_query_heat_tracker_record_multiple_queries() {
        let mut t = QueryHeatTracker::new(TimeWindow::OneMinute);
        t.record("q1", 1000, 10, false);
        t.record("q2", 1000, 10, false);
        assert_eq!(t.query_count(), 2);
    }

    #[test]
    fn test_query_heat_tracker_record_different_windows() {
        let mut t = QueryHeatTracker::new(TimeWindow::OneMinute);
        t.record("q1", 1000, 10, false);
        t.record("q1", 70_000, 20, false);
        let h = t.history("q1").unwrap();
        assert_eq!(h.samples.len(), 2);
    }

    #[test]
    fn test_query_heat_tracker_history_nonexistent() {
        let t = QueryHeatTracker::new(TimeWindow::OneMinute);
        assert!(t.history("q1").is_none());
    }

    #[test]
    fn test_query_heat_tracker_fingerprints() {
        let mut t = QueryHeatTracker::new(TimeWindow::OneMinute);
        t.record("q1", 1000, 10, false);
        t.record("q2", 1000, 10, false);
        let fps = t.fingerprints();
        assert_eq!(fps.len(), 2);
    }

    #[test]
    fn test_query_heat_tracker_hot_queries() {
        let mut t = QueryHeatTracker::new(TimeWindow::OneMinute);
        for i in 0..120 {
            t.record("q1", i, 10, false);
        }
        t.record("q2", 1000, 10, false);
        let hot = t.hot_queries(1.0, 10);
        assert!(!hot.is_empty());
    }

    #[test]
    fn test_query_heat_tracker_cold_queries() {
        let mut t = QueryHeatTracker::new(TimeWindow::OneMinute);
        t.record("q1", 1000, 10, false);
        let cold = t.cold_queries(10);
        assert_eq!(cold.len(), 1);
    }

    #[test]
    fn test_query_heat_tracker_burst_queries() {
        let mut t = QueryHeatTracker::new(TimeWindow::OneMinute);
        t.record("q1", 1000, 10, false);
        t.record("q1", 70_000, 10, false);
        for i in 0..600 {
            t.record("q1", 130_000 + i, 10, false);
        }
        let burst = t.burst_queries();
        assert!(!burst.is_empty());
    }

    #[test]
    fn test_query_heat_tracker_report() {
        let mut t = QueryHeatTracker::new(TimeWindow::OneMinute);
        t.record("q1", 1000, 10, false);
        t.record("q2", 1000, 10, false);
        let report = t.report(1.0);
        assert_eq!(report.total_queries, 2);
    }

    #[test]
    fn test_query_heat_tracker_display() {
        let t = QueryHeatTracker::new(TimeWindow::OneMinute);
        let s = format!("{}", t);
        assert!(s.contains("QueryHeatTracker"));
    }

    #[test]
    fn test_heat_report_display() {
        let t = QueryHeatTracker::new(TimeWindow::OneMinute);
        let report = t.report(1.0);
        let s = format!("{}", report);
        assert!(s.contains("HeatReport"));
    }

    #[test]
    fn test_queries_with_increasing_errors() {
        let mut t = QueryHeatTracker::new(TimeWindow::OneMinute);
        t.record("q1", 1000, 10, false);
        t.record("q1", 70_000, 10, true);
        let rising = t.queries_with_increasing_errors();
        assert!(!rising.is_empty());
    }
}
