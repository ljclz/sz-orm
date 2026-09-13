//! 复制延迟追踪器
//!
//! 记录区域间复制延迟采样，P99 滑动窗口计算，阈值告警。

use std::collections::VecDeque;
use std::time::Duration;

use parking_lot::RwLock;

/// 同城延迟阈值：200ms
pub const SAME_CITY_THRESHOLD: Duration = Duration::from_millis(200);
/// 跨洲延迟阈值：2s
pub const CROSS_CONTINENT_THRESHOLD: Duration = Duration::from_secs(2);

/// 滑动窗口大小
const WINDOW_SIZE: usize = 100;

/// 延迟采样窗口
struct LagWindow {
    samples: VecDeque<Duration>,
}

impl Default for LagWindow {
    fn default() -> Self {
        Self {
            samples: VecDeque::with_capacity(WINDOW_SIZE),
        }
    }
}

impl LagWindow {
    fn record(&mut self, lag: Duration) {
        if self.samples.len() >= WINDOW_SIZE {
            self.samples.pop_front();
        }
        self.samples.push_back(lag);
    }

    fn p99(&self) -> Duration {
        if self.samples.is_empty() {
            return Duration::ZERO;
        }
        let mut sorted: Vec<Duration> = self.samples.iter().copied().collect();
        sorted.sort();
        let idx = ((sorted.len() as f64) * 0.99) as usize;
        let idx = idx.min(sorted.len() - 1);
        sorted[idx]
    }

    fn count(&self) -> usize {
        self.samples.len()
    }
}

/// 区域间链接类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkType {
    /// 同城
    SameCity,
    /// 跨洲
    CrossContinent,
}

/// 复制延迟追踪器
pub struct ReplicationLagTracker {
    lags: RwLock<std::collections::HashMap<(String, String), LagWindow>>,
    thresholds: RwLock<std::collections::HashMap<(String, String), Duration>>,
}

impl Default for ReplicationLagTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplicationLagTracker {
    /// 创建追踪器
    pub fn new() -> Self {
        Self {
            lags: RwLock::new(std::collections::HashMap::new()),
            thresholds: RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// 记录延迟采样
    pub fn record_lag(&self, source: &str, target: &str, lag: Duration) {
        let key = (source.to_string(), target.to_string());
        self.lags.write().entry(key).or_default().record(lag);
    }

    /// 计算 P99 延迟
    pub fn p99_lag(&self, source: &str, target: &str) -> Duration {
        let key = (source.to_string(), target.to_string());
        self.lags
            .read()
            .get(&key)
            .map(|w| w.p99())
            .unwrap_or_default()
    }

    /// 设置链接阈值
    pub fn set_threshold(&self, source: &str, target: &str, threshold: Duration) {
        self.thresholds
            .write()
            .insert((source.to_string(), target.to_string()), threshold);
    }

    /// 设置链接类型（自动设置阈值）
    pub fn set_link_type(&self, source: &str, target: &str, link_type: LinkType) {
        let threshold = match link_type {
            LinkType::SameCity => SAME_CITY_THRESHOLD,
            LinkType::CrossContinent => CROSS_CONTINENT_THRESHOLD,
        };
        self.set_threshold(source, target, threshold);
    }

    /// 检查是否超阈值
    pub fn check_threshold(&self, source: &str, target: &str) -> Option<bool> {
        let key = (source.to_string(), target.to_string());
        let thresholds = self.thresholds.read();
        let threshold = thresholds.get(&key)?;
        let p99 = self.p99_lag(source, target);
        Some(p99 > *threshold)
    }

    /// 采样数量
    pub fn sample_count(&self, source: &str, target: &str) -> usize {
        let key = (source.to_string(), target.to_string());
        self.lags.read().get(&key).map(|w| w.count()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_p99() {
        let tracker = ReplicationLagTracker::new();
        for i in 1..=100 {
            tracker.record_lag("a", "b", Duration::from_millis(i));
        }
        let p99 = tracker.p99_lag("a", "b");
        assert!(p99 >= Duration::from_millis(99));
    }

    #[test]
    fn p99_empty_returns_zero() {
        let tracker = ReplicationLagTracker::new();
        assert_eq!(tracker.p99_lag("a", "b"), Duration::ZERO);
    }

    #[test]
    fn same_city_threshold_alert() {
        let tracker = ReplicationLagTracker::new();
        tracker.set_link_type("a", "b", LinkType::SameCity);
        tracker.record_lag("a", "b", Duration::from_millis(300));
        assert_eq!(tracker.check_threshold("a", "b"), Some(true));
    }

    #[test]
    fn same_city_threshold_ok() {
        let tracker = ReplicationLagTracker::new();
        tracker.set_link_type("a", "b", LinkType::SameCity);
        tracker.record_lag("a", "b", Duration::from_millis(100));
        assert_eq!(tracker.check_threshold("a", "b"), Some(false));
    }

    #[test]
    fn cross_continent_threshold_alert() {
        let tracker = ReplicationLagTracker::new();
        tracker.set_link_type("a", "b", LinkType::CrossContinent);
        tracker.record_lag("a", "b", Duration::from_secs(3));
        assert_eq!(tracker.check_threshold("a", "b"), Some(true));
    }

    #[test]
    fn cross_continent_threshold_ok() {
        let tracker = ReplicationLagTracker::new();
        tracker.set_link_type("a", "b", LinkType::CrossContinent);
        tracker.record_lag("a", "b", Duration::from_millis(500));
        assert_eq!(tracker.check_threshold("a", "b"), Some(false));
    }

    #[test]
    fn sliding_window_evicts_old() {
        let tracker = ReplicationLagTracker::new();
        for _ in 0..150 {
            tracker.record_lag("a", "b", Duration::from_millis(1));
        }
        tracker.record_lag("a", "b", Duration::from_millis(1000));
        assert_eq!(tracker.sample_count("a", "b"), 100);
    }
}
