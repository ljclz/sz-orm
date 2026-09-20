//! 归档数据回查代理

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use super::types::{DataRange, DataTemperature, LifecycleError};

/// 归档数据回查代理：5s 延迟保证，回查超时返回错误，回查触发温度提升
pub struct ArchiveQueryProxy {
    timeout: Duration,
    latency: Duration,
    archived_data: Arc<RwLock<Vec<(String, DataRange)>>>,
}

impl ArchiveQueryProxy {
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(5),
            latency: Duration::from_millis(100),
            archived_data: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            timeout,
            latency: Duration::from_millis(100),
            archived_data: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn register_archived_data(&self, table: &str, range: DataRange) {
        self.archived_data.write().push((table.to_string(), range));
    }

    pub async fn query(
        &self,
        table: &str,
        min_id: i64,
        max_id: i64,
    ) -> Result<Vec<DataRange>, LifecycleError> {
        let start = Instant::now();

        tokio::time::sleep(self.latency).await;

        if start.elapsed() > self.timeout {
            return Err(LifecycleError::QueryTimeout(format!(
                "ARCHIVE_QUERY_TIMEOUT: 回查超时 ({}ms > {}ms)",
                start.elapsed().as_millis(),
                self.timeout.as_millis()
            )));
        }

        let data = self.archived_data.read();
        let results: Vec<DataRange> = data
            .iter()
            .filter(|(t, r)| t == table && r.min_id <= max_id && r.max_id >= min_id)
            .map(|(_, r)| self.promote_to_hot(r))
            .collect();

        Ok(results)
    }

    fn promote_to_hot(&self, range: &DataRange) -> DataRange {
        DataRange {
            temperature: DataTemperature::Hot,
            last_accessed_days_ago: 0,
            access_frequency_per_day: range.access_frequency_per_day + 1.0,
            ..range.clone()
        }
    }

    pub fn archived_data_count(&self) -> usize {
        self.archived_data.read().len()
    }
}

impl Default for ArchiveQueryProxy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_range(min: i64, max: i64) -> DataRange {
        DataRange {
            min_id: min,
            max_id: max,
            row_count: 100,
            temperature: DataTemperature::Archived,
            last_accessed_days_ago: 90,
            access_frequency_per_day: 0.1,
        }
    }

    #[tokio::test]
    async fn test_query_archived_data() {
        let proxy = ArchiveQueryProxy::new();
        proxy.register_archived_data("orders", make_range(1, 100));

        let results = proxy.query("orders", 1, 100).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].temperature, DataTemperature::Hot);
        assert_eq!(results[0].last_accessed_days_ago, 0);
    }

    #[tokio::test]
    async fn test_query_no_data() {
        let proxy = ArchiveQueryProxy::new();
        let results = proxy.query("orders", 1, 100).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_query_promotes_temperature() {
        let proxy = ArchiveQueryProxy::new();
        proxy.register_archived_data("orders", make_range(1, 100));

        let results = proxy.query("orders", 1, 100).await.unwrap();
        assert_eq!(results[0].temperature, DataTemperature::Hot);
        assert!(results[0].access_frequency_per_day > 0.1);
    }

    #[tokio::test]
    async fn test_query_timeout() {
        let proxy = ArchiveQueryProxy::with_timeout(Duration::from_millis(1));
        proxy.register_archived_data("orders", make_range(1, 100));

        std::thread::sleep(Duration::from_millis(10));
        let result = proxy.query("orders", 1, 100).await;
        assert!(matches!(result, Err(LifecycleError::QueryTimeout(_))));
    }

    #[tokio::test]
    async fn test_query_range_filter() {
        let proxy = ArchiveQueryProxy::new();
        proxy.register_archived_data("orders", make_range(1, 100));
        proxy.register_archived_data("orders", make_range(200, 300));

        let results = proxy.query("orders", 1, 100).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_query_different_table() {
        let proxy = ArchiveQueryProxy::new();
        proxy.register_archived_data("orders", make_range(1, 100));

        let results = proxy.query("users", 1, 100).await.unwrap();
        assert!(results.is_empty());
    }
}
