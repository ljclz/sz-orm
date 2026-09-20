//! TTL 清理执行器

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use super::types::{
    DataRange, DataTemperature, LifecycleError, TtlCleanupLogEntry, TtlCleanupReport,
};

/// TTL 清理执行器：扫描超销毁期数据，先写清理日志再物理删除
pub struct TtlCleanupExecutor {
    cleanup_logs: Arc<RwLock<Vec<TtlCleanupLogEntry>>>,
    deleted_data: Arc<RwLock<Vec<String>>>,
}

impl TtlCleanupExecutor {
    pub fn new() -> Self {
        Self {
            cleanup_logs: Arc::new(RwLock::new(Vec::new())),
            deleted_data: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn cleanup(
        &self,
        table: &str,
        ranges: &[DataRange],
        rule_name: &str,
        destruction_days: u32,
    ) -> Result<TtlCleanupReport, LifecycleError> {
        let mut total_scanned = 0u64;
        let mut total_deleted = 0u64;
        let mut total_skipped = 0u64;
        let mut skipped_reasons: HashMap<String, u64> = HashMap::new();

        for range in ranges {
            total_scanned += range.row_count;

            if range.last_accessed_days_ago < destruction_days {
                total_skipped += range.row_count;
                *skipped_reasons
                    .entry("TTL_SKIP_RECENTLY_ACCESSED".to_string())
                    .or_insert(0) += range.row_count;
                continue;
            }

            if range.temperature == DataTemperature::Hot {
                total_skipped += range.row_count;
                *skipped_reasons
                    .entry("TTL_SKIP_HOT_DATA".to_string())
                    .or_insert(0) += range.row_count;
                continue;
            }

            let log_entry = TtlCleanupLogEntry {
                table: table.to_string(),
                deleted_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0),
                row_count: range.row_count,
                data_summary: format!(
                    "range=[{}, {}], temperature={:?}",
                    range.min_id, range.max_id, range.temperature
                ),
                rule_name: rule_name.to_string(),
            };
            self.cleanup_logs.write().push(log_entry);

            self.deleted_data
                .write()
                .push(format!("{}:[{}-{}]", table, range.min_id, range.max_id));

            total_deleted += range.row_count;
        }

        Ok(TtlCleanupReport {
            table: table.to_string(),
            total_scanned,
            total_deleted,
            total_skipped,
            cleanup_log_path: format!("/var/log/ttl-cleanup/{}.json", table),
            skipped_reasons,
        })
    }

    pub fn get_cleanup_logs(&self) -> Vec<TtlCleanupLogEntry> {
        self.cleanup_logs.read().clone()
    }

    pub fn deleted_count(&self) -> usize {
        self.deleted_data.read().len()
    }

    pub fn is_deleted(&self, table: &str, min_id: i64, max_id: i64) -> bool {
        let key = format!("{}:[{}-{}]", table, min_id, max_id);
        self.deleted_data.read().iter().any(|d| d == &key)
    }
}

impl Default for TtlCleanupExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_range(days_ago: u32, temp: DataTemperature) -> DataRange {
        DataRange {
            min_id: 1,
            max_id: 100,
            row_count: 100,
            temperature: temp,
            last_accessed_days_ago: days_ago,
            access_frequency_per_day: 0.1,
        }
    }

    #[test]
    fn test_cleanup_deletes_old_cold_data() {
        let executor = TtlCleanupExecutor::new();
        let ranges = vec![make_range(400, DataTemperature::Cold)];

        let report = executor.cleanup("orders", &ranges, "r1", 365).unwrap();
        assert_eq!(report.total_scanned, 100);
        assert_eq!(report.total_deleted, 100);
        assert_eq!(report.total_skipped, 0);
        assert_eq!(executor.deleted_count(), 1);
    }

    #[test]
    fn test_cleanup_skips_recently_accessed() {
        let executor = TtlCleanupExecutor::new();
        let ranges = vec![make_range(100, DataTemperature::Cold)];

        let report = executor.cleanup("orders", &ranges, "r1", 365).unwrap();
        assert_eq!(report.total_deleted, 0);
        assert_eq!(report.total_skipped, 100);
        assert!(report
            .skipped_reasons
            .contains_key("TTL_SKIP_RECENTLY_ACCESSED"));
    }

    #[test]
    fn test_cleanup_skips_hot_data() {
        let executor = TtlCleanupExecutor::new();
        let ranges = vec![make_range(400, DataTemperature::Hot)];

        let report = executor.cleanup("orders", &ranges, "r1", 365).unwrap();
        assert_eq!(report.total_deleted, 0);
        assert_eq!(report.total_skipped, 100);
        assert!(report.skipped_reasons.contains_key("TTL_SKIP_HOT_DATA"));
    }

    #[test]
    fn test_cleanup_mixed_data() {
        let executor = TtlCleanupExecutor::new();
        let ranges = vec![
            make_range(400, DataTemperature::Cold),
            make_range(100, DataTemperature::Cold),
            make_range(400, DataTemperature::Hot),
            make_range(400, DataTemperature::Warm),
        ];

        let report = executor.cleanup("orders", &ranges, "r1", 365).unwrap();
        assert_eq!(report.total_scanned, 400);
        assert_eq!(report.total_deleted, 200);
        assert_eq!(report.total_skipped, 200);
    }

    #[test]
    fn test_cleanup_logs_written() {
        let executor = TtlCleanupExecutor::new();
        let ranges = vec![make_range(400, DataTemperature::Cold)];

        executor.cleanup("orders", &ranges, "r1", 365).unwrap();
        let logs = executor.get_cleanup_logs();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].table, "orders");
        assert_eq!(logs[0].rule_name, "r1");
        assert!(logs[0].data_summary.contains("range="));
    }

    #[test]
    fn test_is_deleted() {
        let executor = TtlCleanupExecutor::new();
        let ranges = vec![make_range(400, DataTemperature::Cold)];

        executor.cleanup("orders", &ranges, "r1", 365).unwrap();
        assert!(executor.is_deleted("orders", 1, 100));
        assert!(!executor.is_deleted("orders", 200, 300));
    }
}
