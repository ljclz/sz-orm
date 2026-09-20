//! 访问模式采集器

use super::types::{AccessPatternStats, LifecycleError};

/// 访问模式采集器：采集访问频率/时间维度数据
pub struct AccessPatternCollector {
    stats: std::collections::HashMap<String, AccessPatternStats>,
}

impl AccessPatternCollector {
    pub fn new() -> Self {
        Self {
            stats: std::collections::HashMap::new(),
        }
    }

    pub fn record_access(&mut self, table: &str) {
        let stat = self.stats.entry(table.to_string()).or_default();
        stat.table = table.to_string();
        stat.access_count_30d += 1;
        stat.avg_frequency_per_day = stat.access_count_30d as f64 / 30.0;
        stat.last_accessed_days_ago = 0;
    }

    pub fn set_total_rows(&mut self, table: &str, rows: u64) {
        let stat = self.stats.entry(table.to_string()).or_default();
        stat.table = table.to_string();
        stat.total_rows = rows;
    }

    pub fn set_last_accessed(&mut self, table: &str, days_ago: u32) {
        let stat = self.stats.entry(table.to_string()).or_default();
        stat.table = table.to_string();
        stat.last_accessed_days_ago = days_ago;
    }

    pub fn get_stats(&self, table: &str) -> Result<&AccessPatternStats, LifecycleError> {
        self.stats
            .get(table)
            .ok_or_else(|| LifecycleError::TableNotFound(table.to_string()))
    }

    pub fn all_stats(&self) -> Vec<&AccessPatternStats> {
        self.stats.values().collect()
    }

    pub fn record_batch_access(&mut self, table: &str, count: u64) {
        let stat = self.stats.entry(table.to_string()).or_default();
        stat.table = table.to_string();
        stat.access_count_30d += count;
        stat.avg_frequency_per_day = stat.access_count_30d as f64 / 30.0;
        stat.last_accessed_days_ago = 0;
    }
}

impl Default for AccessPatternCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_access() {
        let mut collector = AccessPatternCollector::new();
        collector.record_access("orders");
        collector.record_access("orders");

        let stats = collector.get_stats("orders").unwrap();
        assert_eq!(stats.access_count_30d, 2);
        assert!((stats.avg_frequency_per_day - 2.0 / 30.0).abs() < 0.001);
    }

    #[test]
    fn test_get_stats_not_found() {
        let collector = AccessPatternCollector::new();
        assert!(collector.get_stats("nonexistent").is_err());
    }

    #[test]
    fn test_set_total_rows() {
        let mut collector = AccessPatternCollector::new();
        collector.set_total_rows("orders", 10000);

        let stats = collector.get_stats("orders").unwrap();
        assert_eq!(stats.total_rows, 10000);
    }

    #[test]
    fn test_set_last_accessed() {
        let mut collector = AccessPatternCollector::new();
        collector.set_last_accessed("orders", 45);

        let stats = collector.get_stats("orders").unwrap();
        assert_eq!(stats.last_accessed_days_ago, 45);
    }

    #[test]
    fn test_batch_access() {
        let mut collector = AccessPatternCollector::new();
        collector.record_batch_access("orders", 100);

        let stats = collector.get_stats("orders").unwrap();
        assert_eq!(stats.access_count_30d, 100);
    }

    #[test]
    fn test_all_stats() {
        let mut collector = AccessPatternCollector::new();
        collector.record_access("orders");
        collector.record_access("users");

        assert_eq!(collector.all_stats().len(), 2);
    }
}
