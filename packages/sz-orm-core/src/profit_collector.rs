//! 收益持续采集配置与快照（v10.3.0）
//!
//! 基于 AtomicU64 无锁采集器，持续累积联动命中/跳过计数，
//! 定期生成收益快照，支持趋势查询。独立于既有 ProfitQuantEngine 的 A/B 基准模式。

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// 收益持续采集配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfitCollectorConfig {
    /// 是否启用持续采集，默认 false
    pub continuous_collect_enabled: bool,
    /// 快照间隔，默认 60s
    pub snapshot_interval: Duration,
    /// 最大快照保留数，默认 1440（24h / 60s）
    pub max_snapshots: usize,
    /// 趋势查询最大时间范围，默认 168h（7天）
    pub trend_query_max_range: Duration,
}

impl Default for ProfitCollectorConfig {
    fn default() -> Self {
        Self {
            continuous_collect_enabled: false,
            snapshot_interval: Duration::from_secs(60),
            max_snapshots: 1440,
            trend_query_max_range: Duration::from_secs(168 * 3600),
        }
    }
}

impl ProfitCollectorConfig {
    /// 校验配置有效性
    pub fn validate(&self) -> Result<(), String> {
        if self.snapshot_interval < Duration::from_secs(1) {
            return Err(format!(
                "snapshot_interval must be >= 1s, got {:?}",
                self.snapshot_interval
            ));
        }
        if self.snapshot_interval > Duration::from_secs(3600) {
            return Err(format!(
                "snapshot_interval must be <= 3600s, got {:?}",
                self.snapshot_interval
            ));
        }
        if self.max_snapshots == 0 {
            return Err("max_snapshots must be >= 1".to_string());
        }
        if self.max_snapshots > 10000 {
            return Err(format!(
                "max_snapshots must be <= 10000, got {}",
                self.max_snapshots
            ));
        }
        if self.trend_query_max_range < Duration::from_secs(3600) {
            return Err(format!(
                "trend_query_max_range must be >= 1h, got {:?}",
                self.trend_query_max_range
            ));
        }
        if self.trend_query_max_range > Duration::from_secs(720 * 3600) {
            return Err(format!(
                "trend_query_max_range must be <= 720h, got {:?}",
                self.trend_query_max_range
            ));
        }
        Ok(())
    }
}

/// 收益快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfitSnapshot {
    /// 命中计数
    pub hit_count: u64,
    /// 跳过计数
    pub skip_count: u64,
    /// 命中比率 [0.0, 1.0]
    pub hit_ratio: f64,
    /// 采集窗口起始时间（UNIX 毫秒）
    pub collection_window_start: u64,
    /// 采集窗口结束时间（UNIX 毫秒）
    pub collection_window_end: u64,
    /// 快照时间（UNIX 毫秒）
    pub snapshot_time: u64,
}

/// 收益持续采集器（AtomicU64 无锁）
pub struct ProfitCollector {
    hit_counter: AtomicU64,
    skip_counter: AtomicU64,
    window_start: AtomicU64,
    snapshots: Mutex<VecDeque<ProfitSnapshot>>,
    config: ProfitCollectorConfig,
}

impl ProfitCollector {
    /// 创建采集器
    pub fn new(config: ProfitCollectorConfig) -> Self {
        let now = current_millis();
        Self {
            hit_counter: AtomicU64::new(0),
            skip_counter: AtomicU64::new(0),
            window_start: AtomicU64::new(now),
            snapshots: Mutex::new(VecDeque::new()),
            config,
        }
    }

    /// 记录一次命中
    pub fn record_hit(&self) {
        self.hit_counter.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录一次跳过
    pub fn record_skip(&self) {
        self.skip_counter.fetch_add(1, Ordering::Relaxed);
    }

    /// 生成快照
    pub fn snapshot(&self) -> ProfitSnapshot {
        let hit_count = self.hit_counter.load(Ordering::Relaxed);
        let skip_count = self.skip_counter.load(Ordering::Relaxed);
        let window_start = self.window_start.load(Ordering::Relaxed);
        let window_end = current_millis();
        let total = hit_count + skip_count;
        let hit_ratio = if total == 0 {
            0.0
        } else {
            hit_count as f64 / total as f64
        };
        let snapshot = ProfitSnapshot {
            hit_count,
            skip_count,
            hit_ratio,
            collection_window_start: window_start,
            collection_window_end: window_end,
            snapshot_time: window_end,
        };
        let mut snapshots = self.snapshots.lock().unwrap();
        if snapshots.len() >= self.config.max_snapshots {
            snapshots.pop_front();
        }
        snapshots.push_back(snapshot.clone());
        snapshot
    }

    /// 趋势查询：返回 [start, end] 时间范围内的快照
    pub fn trend(&self, start: u64, end: u64) -> Vec<ProfitSnapshot> {
        let snapshots = self.snapshots.lock().unwrap();
        snapshots
            .iter()
            .filter(|s| s.snapshot_time >= start && s.snapshot_time <= end)
            .cloned()
            .collect()
    }

    /// 返回配置引用
    pub fn config(&self) -> &ProfitCollectorConfig {
        &self.config
    }
}

fn current_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default_disabled() {
        let config = ProfitCollectorConfig::default();
        assert!(!config.continuous_collect_enabled);
    }

    #[test]
    fn test_config_validate_valid() {
        let config = ProfitCollectorConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validate_invalid_interval() {
        let config = ProfitCollectorConfig {
            snapshot_interval: Duration::from_secs(0),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_invalid_max_snapshots() {
        let config = ProfitCollectorConfig {
            max_snapshots: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_invalid_trend_range() {
        let config = ProfitCollectorConfig {
            trend_query_max_range: Duration::from_secs(1800),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_atomic_count() {
        let collector = ProfitCollector::new(ProfitCollectorConfig::default());
        collector.record_hit();
        collector.record_hit();
        collector.record_hit();
        collector.record_skip();
        let snap = collector.snapshot();
        assert_eq!(snap.hit_count, 3);
        assert_eq!(snap.skip_count, 1);
        assert!((snap.hit_ratio - 0.75).abs() < 1e-9);
    }

    #[test]
    fn test_snapshot_stored_in_deque() {
        let collector = ProfitCollector::new(ProfitCollectorConfig::default());
        collector.snapshot();
        collector.snapshot();
        let trend = collector.trend(0, u64::MAX);
        assert_eq!(trend.len(), 2);
    }

    #[test]
    fn test_max_snapshots_eviction() {
        let config = ProfitCollectorConfig {
            max_snapshots: 2,
            ..Default::default()
        };
        let collector = ProfitCollector::new(config);
        collector.snapshot();
        collector.snapshot();
        collector.snapshot();
        let trend = collector.trend(0, u64::MAX);
        assert_eq!(trend.len(), 2);
    }

    #[test]
    fn test_trend_time_range_filter() {
        use std::thread;
        let collector = ProfitCollector::new(ProfitCollectorConfig::default());
        let s1 = collector.snapshot();
        thread::sleep(Duration::from_millis(10));
        collector.record_hit();
        let s2 = collector.snapshot();
        thread::sleep(Duration::from_millis(10));
        collector.record_hit();
        let s3 = collector.snapshot();
        let trend = collector.trend(s1.snapshot_time, s2.snapshot_time);
        assert!(trend.iter().any(|s| s.snapshot_time == s1.snapshot_time));
        assert!(trend.iter().any(|s| s.snapshot_time == s2.snapshot_time));
        assert!(!trend.iter().any(|s| s.snapshot_time == s3.snapshot_time));
    }

    #[test]
    fn test_trend_empty_when_no_snapshot() {
        let collector = ProfitCollector::new(ProfitCollectorConfig::default());
        let trend = collector.trend(0, u64::MAX);
        assert!(trend.is_empty());
    }

    #[test]
    fn test_snapshot_no_sensitive_data() {
        let collector = ProfitCollector::new(ProfitCollectorConfig::default());
        collector.record_hit();
        let snap = collector.snapshot();
        let json = serde_json::to_string(&snap).unwrap();
        assert!(!json.contains("password"));
        assert!(!json.contains("credential"));
        assert!(!json.contains("secret"));
    }
}
