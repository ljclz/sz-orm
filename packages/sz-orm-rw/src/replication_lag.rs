//! 复制延迟监控
//!
//! 跟踪 master-slave 复制延迟，延迟过大时将读请求路由到 master（强一致读）。
//! 延迟数据由外部探针定期采集并上报。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// 单个 slave 的复制延迟快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationLagSnapshot {
    /// slave 地址
    pub slave: String,
    /// 复制延迟（秒）
    pub lag_seconds: u64,
    /// 采集时间戳（从实例创建起经过的秒数）
    pub collected_at_secs: u64,
}

impl ReplicationLagSnapshot {
    /// 创建快照
    pub fn new(slave: &str, lag_seconds: u64, collected_at_secs: u64) -> Self {
        Self {
            slave: slave.to_string(),
            lag_seconds,
            collected_at_secs,
        }
    }

    /// 延迟是否超过阈值
    pub fn exceeds(&self, threshold_secs: u64) -> bool {
        self.lag_seconds > threshold_secs
    }

    /// 延迟是否在可接受范围内
    pub fn is_acceptable(&self, threshold_secs: u64) -> bool {
        self.lag_seconds <= threshold_secs
    }
}

/// 复制延迟监控器
///
/// 维护每个 slave 的最新延迟快照，提供基于延迟的路由决策。
pub struct ReplicationLagMonitor {
    /// 延迟阈值（秒），超过此值的 slave 被视为"延迟过大"
    threshold_secs: u64,
    /// 各 slave 的延迟历史（保留最近 N 条）
    history: Mutex<HashMap<String, Vec<ReplicationLagSnapshot>>>,
    /// 最大历史保留条数
    max_history: usize,
    /// 实例创建时间
    started: Instant,
}

impl ReplicationLagMonitor {
    /// 创建监控器
    pub fn new(threshold_secs: u64) -> Self {
        Self {
            threshold_secs,
            history: Mutex::new(HashMap::new()),
            max_history: 10,
            started: Instant::now(),
        }
    }

    /// 设置最大历史保留条数
    pub fn with_max_history(mut self, max: usize) -> Self {
        self.max_history = max.max(1);
        self
    }

    /// 上报一次延迟采样
    pub fn report(&self, slave: &str, lag_seconds: u64) {
        let now = self.started.elapsed().as_secs();
        let snap = ReplicationLagSnapshot::new(slave, lag_seconds, now);
        if let Ok(mut history) = self.history.lock() {
            let entries = history.entry(slave.to_string()).or_default();
            entries.push(snap);
            if entries.len() > self.max_history {
                entries.remove(0);
            }
        }
    }

    /// 获取 slave 的最新延迟
    pub fn latest_lag(&self, slave: &str) -> Option<u64> {
        match self.history.lock() {
            Ok(history) => history
                .get(slave)
                .and_then(|v| v.last())
                .map(|s| s.lag_seconds),
            Err(_) => None,
        }
    }

    /// 判断 slave 是否延迟过大
    pub fn is_lagging(&self, slave: &str) -> bool {
        match self.latest_lag(slave) {
            Some(lag) => lag > self.threshold_secs,
            None => false,
        }
    }

    /// 判断 slave 是否可以安全读取
    pub fn is_safe_to_read(&self, slave: &str) -> bool {
        !self.is_lagging(slave)
    }

    /// 返回所有延迟过大的 slave
    pub fn lagging_slaves(&self) -> Vec<String> {
        match self.history.lock() {
            Ok(history) => history
                .iter()
                .filter(|(_, v)| {
                    v.last()
                        .map(|s| s.lag_seconds > self.threshold_secs)
                        .unwrap_or(false)
                })
                .map(|(k, _)| k.clone())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 返回所有安全可读的 slave
    pub fn safe_slaves(&self) -> Vec<String> {
        match self.history.lock() {
            Ok(history) => history
                .iter()
                .filter(|(_, v)| {
                    v.last()
                        .map(|s| s.lag_seconds <= self.threshold_secs)
                        .unwrap_or(true)
                })
                .map(|(k, _)| k.clone())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 获取 slave 的延迟历史
    pub fn history(&self, slave: &str) -> Vec<ReplicationLagSnapshot> {
        match self.history.lock() {
            Ok(history) => history.get(slave).cloned().unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    /// 计算 slave 的平均延迟（基于历史）
    pub fn avg_lag(&self, slave: &str) -> Option<u64> {
        match self.history.lock() {
            Ok(history) => {
                let entries = history.get(slave)?;
                if entries.is_empty() {
                    return None;
                }
                let total: u64 = entries.iter().map(|s| s.lag_seconds).sum();
                Some(total / entries.len() as u64)
            }
            Err(_) => None,
        }
    }

    /// 计算 slave 的最大延迟（基于历史）
    pub fn max_lag(&self, slave: &str) -> Option<u64> {
        match self.history.lock() {
            Ok(history) => history
                .get(slave)
                .and_then(|v| v.iter().map(|s| s.lag_seconds).max()),
            Err(_) => None,
        }
    }

    /// 获取延迟阈值
    pub fn threshold(&self) -> u64 {
        self.threshold_secs
    }

    /// 重置 slave 的历史
    pub fn reset(&self, slave: &str) {
        if let Ok(mut history) = self.history.lock() {
            history.remove(slave);
        }
    }

    /// 重置所有 slave 的历史
    pub fn reset_all(&self) {
        if let Ok(mut history) = self.history.lock() {
            history.clear();
        }
    }

    /// 从候选列表中选择延迟最小的 slave
    ///
    /// 返回 None 表示候选列表为空或无延迟数据。
    pub fn select_least_lag<'a>(&self, candidates: &'a [String]) -> Option<&'a str> {
        let history = self.history.lock().ok()?;
        let mut best: Option<(&str, u64)> = None;
        for slave in candidates {
            if let Some(entries) = history.get(slave) {
                if let Some(latest) = entries.last() {
                    match best {
                        None => best = Some((slave.as_str(), latest.lag_seconds)),
                        Some((_, b_lag)) if latest.lag_seconds < b_lag => {
                            best = Some((slave.as_str(), latest.lag_seconds))
                        }
                        _ => {}
                    }
                }
            }
        }
        best.map(|(s, _)| s)
    }

    /// 计算 slave 的最小延迟（基于历史）
    pub fn min_lag(&self, slave: &str) -> Option<u64> {
        match self.history.lock() {
            Ok(history) => history
                .get(slave)
                .and_then(|v| v.iter().map(|s| s.lag_seconds).min()),
            Err(_) => None,
        }
    }

    /// 返回所有已知 slave 列表
    pub fn all_slaves(&self) -> Vec<String> {
        match self.history.lock() {
            Ok(history) => history.keys().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 已知 slave 数量
    pub fn slave_count(&self) -> usize {
        match self.history.lock() {
            Ok(history) => history.len(),
            Err(_) => 0,
        }
    }

    /// 延迟趋势：比较最近两次采样
    ///
    /// 返回正数表示延迟上升，负数表示下降，0 表示稳定或数据不足。
    pub fn lag_trend(&self, slave: &str) -> i64 {
        match self.history.lock() {
            Ok(history) => {
                if let Some(entries) = history.get(slave) {
                    if entries.len() < 2 {
                        return 0;
                    }
                    let last = entries.last().unwrap().lag_seconds as i64;
                    let prev = entries[entries.len() - 2].lag_seconds as i64;
                    last - prev
                } else {
                    0
                }
            }
            Err(_) => 0,
        }
    }

    /// 生成汇总报告字符串
    pub fn summary(&self) -> String {
        let history = match self.history.lock() {
            Ok(h) => h,
            Err(_) => return "ReplicationLagMonitor: lock poisoned".to_string(),
        };
        let mut out = format!(
            "ReplicationLagMonitor: {} slave(s), threshold={}s\n",
            history.len(),
            self.threshold_secs
        );
        for (slave, entries) in history.iter() {
            let latest = entries.last().map(|s| s.lag_seconds).unwrap_or(0);
            let status = if latest > self.threshold_secs {
                "LAGGING"
            } else {
                "OK"
            };
            out.push_str(&format!("  {} : lag={}s [{}]\n", slave, latest, status));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_exceeds() {
        let snap = ReplicationLagSnapshot::new("s1", 10, 0);
        assert!(snap.exceeds(5));
        assert!(!snap.exceeds(10));
        assert!(!snap.exceeds(15));
    }

    #[test]
    fn test_snapshot_is_acceptable() {
        let snap = ReplicationLagSnapshot::new("s1", 5, 0);
        assert!(snap.is_acceptable(5));
        assert!(snap.is_acceptable(10));
        assert!(!snap.is_acceptable(3));
    }

    #[test]
    fn test_monitor_new_has_threshold() {
        let m = ReplicationLagMonitor::new(30);
        assert_eq!(m.threshold(), 30);
    }

    #[test]
    fn test_report_and_latest_lag() {
        let m = ReplicationLagMonitor::new(10);
        m.report("s1", 5);
        assert_eq!(m.latest_lag("s1"), Some(5));
        m.report("s1", 8);
        assert_eq!(m.latest_lag("s1"), Some(8));
    }

    #[test]
    fn test_latest_lag_unknown_slave() {
        let m = ReplicationLagMonitor::new(10);
        assert_eq!(m.latest_lag("ghost"), None);
    }

    #[test]
    fn test_is_lagging() {
        let m = ReplicationLagMonitor::new(10);
        m.report("s1", 5);
        assert!(!m.is_lagging("s1"));
        m.report("s1", 15);
        assert!(m.is_lagging("s1"));
    }

    #[test]
    fn test_is_safe_to_read() {
        let m = ReplicationLagMonitor::new(10);
        m.report("s1", 5);
        assert!(m.is_safe_to_read("s1"));
        m.report("s1", 20);
        assert!(!m.is_safe_to_read("s1"));
    }

    #[test]
    fn test_is_safe_to_read_no_data() {
        let m = ReplicationLagMonitor::new(10);
        assert!(m.is_safe_to_read("unknown"), "no data should be safe");
    }

    #[test]
    fn test_lagging_slaves() {
        let m = ReplicationLagMonitor::new(10);
        m.report("s1", 5);
        m.report("s2", 15);
        m.report("s3", 20);
        let mut lagging = m.lagging_slaves();
        lagging.sort();
        assert_eq!(lagging, vec!["s2".to_string(), "s3".to_string()]);
    }

    #[test]
    fn test_safe_slaves() {
        let m = ReplicationLagMonitor::new(10);
        m.report("s1", 5);
        m.report("s2", 15);
        let safe = m.safe_slaves();
        assert_eq!(safe, vec!["s1".to_string()]);
    }

    #[test]
    fn test_history_retention() {
        let m = ReplicationLagMonitor::new(10).with_max_history(3);
        m.report("s1", 1);
        m.report("s1", 2);
        m.report("s1", 3);
        m.report("s1", 4);
        assert_eq!(m.history("s1").len(), 3);
        assert_eq!(m.latest_lag("s1"), Some(4));
    }

    #[test]
    fn test_avg_lag() {
        let m = ReplicationLagMonitor::new(100);
        m.report("s1", 10);
        m.report("s1", 20);
        m.report("s1", 30);
        assert_eq!(m.avg_lag("s1"), Some(20));
    }

    #[test]
    fn test_avg_lag_no_data() {
        let m = ReplicationLagMonitor::new(100);
        assert_eq!(m.avg_lag("ghost"), None);
    }

    #[test]
    fn test_max_lag() {
        let m = ReplicationLagMonitor::new(100);
        m.report("s1", 10);
        m.report("s1", 50);
        m.report("s1", 20);
        assert_eq!(m.max_lag("s1"), Some(50));
    }

    #[test]
    fn test_reset() {
        let m = ReplicationLagMonitor::new(10);
        m.report("s1", 5);
        m.reset("s1");
        assert_eq!(m.latest_lag("s1"), None);
    }

    #[test]
    fn test_reset_all() {
        let m = ReplicationLagMonitor::new(10);
        m.report("s1", 5);
        m.report("s2", 10);
        m.reset_all();
        assert_eq!(m.latest_lag("s1"), None);
        assert_eq!(m.latest_lag("s2"), None);
    }

    #[test]
    fn test_select_least_lag() {
        let m = ReplicationLagMonitor::new(100);
        m.report("s1", 20);
        m.report("s2", 5);
        m.report("s3", 15);
        let candidates = vec!["s1".to_string(), "s2".to_string(), "s3".to_string()];
        let selected = m.select_least_lag(&candidates);
        assert_eq!(selected, Some("s2"));
    }

    #[test]
    fn test_select_least_lag_no_data() {
        let m = ReplicationLagMonitor::new(100);
        let candidates = vec!["s1".to_string()];
        assert_eq!(m.select_least_lag(&candidates), None);
    }

    #[test]
    fn test_select_least_lag_empty_candidates() {
        let m = ReplicationLagMonitor::new(100);
        let candidates: Vec<String> = vec![];
        assert_eq!(m.select_least_lag(&candidates), None);
    }

    #[test]
    fn test_with_max_history_clamped_to_one() {
        let m = ReplicationLagMonitor::new(10).with_max_history(0);
        m.report("s1", 5);
        assert_eq!(m.history("s1").len(), 1);
    }

    #[test]
    fn test_min_lag() {
        let m = ReplicationLagMonitor::new(100);
        m.report("s1", 30);
        m.report("s1", 10);
        m.report("s1", 50);
        assert_eq!(m.min_lag("s1"), Some(10));
    }

    #[test]
    fn test_min_lag_no_data() {
        let m = ReplicationLagMonitor::new(100);
        assert_eq!(m.min_lag("ghost"), None);
    }

    #[test]
    fn test_all_slaves() {
        let m = ReplicationLagMonitor::new(100);
        m.report("s1", 5);
        m.report("s2", 10);
        let mut slaves = m.all_slaves();
        slaves.sort();
        assert_eq!(slaves, vec!["s1".to_string(), "s2".to_string()]);
    }

    #[test]
    fn test_slave_count() {
        let m = ReplicationLagMonitor::new(100);
        m.report("s1", 5);
        m.report("s2", 10);
        assert_eq!(m.slave_count(), 2);
    }

    #[test]
    fn test_lag_trend_rising() {
        let m = ReplicationLagMonitor::new(100);
        m.report("s1", 5);
        m.report("s1", 10);
        assert_eq!(m.lag_trend("s1"), 5);
    }

    #[test]
    fn test_lag_trend_falling() {
        let m = ReplicationLagMonitor::new(100);
        m.report("s1", 20);
        m.report("s1", 10);
        assert_eq!(m.lag_trend("s1"), -10);
    }

    #[test]
    fn test_lag_trend_stable() {
        let m = ReplicationLagMonitor::new(100);
        m.report("s1", 10);
        m.report("s1", 10);
        assert_eq!(m.lag_trend("s1"), 0);
    }

    #[test]
    fn test_lag_trend_single_sample() {
        let m = ReplicationLagMonitor::new(100);
        m.report("s1", 10);
        assert_eq!(m.lag_trend("s1"), 0);
    }

    #[test]
    fn test_summary_contains_slave() {
        let m = ReplicationLagMonitor::new(10);
        m.report("s1", 5);
        m.report("s2", 20);
        let s = m.summary();
        assert!(s.contains("s1"));
        assert!(s.contains("s2"));
        assert!(s.contains("OK"));
        assert!(s.contains("LAGGING"));
    }
}
