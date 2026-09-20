//! SLI 指标保留期清理器

use super::types::SliMetrics;

/// SLI 指标保留期清理器
pub struct SliRetentionCleaner {
    retention_days: u32,
}

impl SliRetentionCleaner {
    pub fn new(retention_days: u32) -> Self {
        Self { retention_days }
    }

    pub fn cleanup(&self, metrics: &mut Vec<SliMetrics>, current_timestamp: i64) -> usize {
        let cutoff = current_timestamp - (self.retention_days as i64 * 86400 * 1000);
        let before = metrics.len();
        metrics.retain(|m| m.timestamp >= cutoff);
        before - metrics.len()
    }

    pub fn retention_days(&self) -> u32 {
        self.retention_days
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_metrics(ts: i64) -> SliMetrics {
        SliMetrics {
            timestamp: ts,
            availability: 0.999,
            latency_ms: 10.0,
            throughput: 100.0,
            correctness: 0.999,
        }
    }

    #[test]
    fn test_cleanup_expired() {
        let cleaner = SliRetentionCleaner::new(30);
        let mut metrics = vec![make_metrics(0), make_metrics(1000 * 86400 * 40)];
        let removed = cleaner.cleanup(&mut metrics, 1000 * 86400 * 40);
        assert_eq!(removed, 1);
        assert_eq!(metrics.len(), 1);
    }

    #[test]
    fn test_cleanup_none_expired() {
        let cleaner = SliRetentionCleaner::new(30);
        let mut metrics = vec![make_metrics(1000 * 86400 * 10)];
        let removed = cleaner.cleanup(&mut metrics, 1000 * 86400 * 15);
        assert_eq!(removed, 0);
    }
}
