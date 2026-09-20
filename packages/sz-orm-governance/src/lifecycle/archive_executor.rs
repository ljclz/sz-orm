//! 归档执行器

use std::sync::Arc;

use parking_lot::RwLock;

use super::types::{ArchiveRecord, ArchiveReport, DataRange, LifecycleError, SourceStatus};

/// 归档执行器：执行归档迁移，归档前验证数据完整性，归档后标记源数据
pub struct ArchiveExecutor {
    archives: Arc<RwLock<Vec<ArchiveRecord>>>,
    integrity_check_enabled: bool,
}

impl ArchiveExecutor {
    pub fn new() -> Self {
        Self {
            archives: Arc::new(RwLock::new(Vec::new())),
            integrity_check_enabled: true,
        }
    }

    pub fn with_integrity_check(enabled: bool) -> Self {
        Self {
            archives: Arc::new(RwLock::new(Vec::new())),
            integrity_check_enabled: enabled,
        }
    }

    pub fn archive(
        &self,
        table: &str,
        data_range: &DataRange,
        archive_target: &str,
    ) -> Result<ArchiveRecord, LifecycleError> {
        if archive_target.is_empty() {
            return Err(LifecycleError::ArchiveTargetUnavailable(
                "归档目标为空".to_string(),
            ));
        }

        if self.integrity_check_enabled {
            self.verify_integrity(table, data_range)?;
        }

        let record = ArchiveRecord {
            record_id: format!("archive-{}-{}", table, data_range.min_id),
            table: table.to_string(),
            data_range: data_range.clone(),
            archive_target: archive_target.to_string(),
            archived_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
            source_status: SourceStatus::Archived,
            integrity_verified: self.integrity_check_enabled,
        };

        self.archives.write().push(record.clone());
        Ok(record)
    }

    pub fn batch_archive(
        &self,
        table: &str,
        ranges: &[DataRange],
        archive_target: &str,
    ) -> Result<ArchiveReport, LifecycleError> {
        let start = std::time::Instant::now();
        let mut records = Vec::new();
        let mut failed = 0u64;
        let mut total_archived = 0u64;

        for range in ranges {
            match self.archive(table, range, archive_target) {
                Ok(record) => {
                    total_archived += range.row_count;
                    records.push(record);
                }
                Err(_) => {
                    failed += 1;
                }
            }
        }

        Ok(ArchiveReport {
            table: table.to_string(),
            total_archived,
            failed,
            records,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    fn verify_integrity(&self, table: &str, range: &DataRange) -> Result<(), LifecycleError> {
        if range.row_count == 0 {
            return Err(LifecycleError::IntegrityCheckFailed(format!(
                "表 {} 数据范围为空",
                table
            )));
        }
        if range.min_id > range.max_id {
            return Err(LifecycleError::IntegrityCheckFailed(format!(
                "表 {} 数据范围无效: min_id {} > max_id {}",
                table, range.min_id, range.max_id
            )));
        }
        Ok(())
    }

    pub fn get_archives(&self) -> Vec<ArchiveRecord> {
        self.archives.read().clone()
    }

    pub fn mark_source_deleted(&self, record_id: &str) -> Result<(), LifecycleError> {
        let mut archives = self.archives.write();
        let record = archives
            .iter_mut()
            .find(|r| r.record_id == record_id)
            .ok_or_else(|| {
                LifecycleError::TableNotFound(format!("归档记录 {} 不存在", record_id))
            })?;
        record.source_status = SourceStatus::Deleted;
        Ok(())
    }
}

impl Default for ArchiveExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::DataTemperature;
    use super::*;

    fn make_range() -> DataRange {
        DataRange {
            min_id: 1,
            max_id: 100,
            row_count: 100,
            temperature: DataTemperature::Cold,
            last_accessed_days_ago: 90,
            access_frequency_per_day: 0.1,
        }
    }

    #[test]
    fn test_archive_success() {
        let executor = ArchiveExecutor::new();
        let range = make_range();
        let result = executor.archive("orders", &range, "s3://archive");
        assert!(result.is_ok());
        let record = result.unwrap();
        assert_eq!(record.table, "orders");
        assert!(record.integrity_verified);
        assert_eq!(record.source_status, SourceStatus::Archived);
    }

    #[test]
    fn test_archive_empty_target() {
        let executor = ArchiveExecutor::new();
        let range = make_range();
        let result = executor.archive("orders", &range, "");
        assert!(matches!(
            result,
            Err(LifecycleError::ArchiveTargetUnavailable(_))
        ));
    }

    #[test]
    fn test_archive_integrity_check_empty_range() {
        let executor = ArchiveExecutor::new();
        let range = DataRange {
            row_count: 0,
            ..make_range()
        };
        let result = executor.archive("orders", &range, "s3://archive");
        assert!(matches!(
            result,
            Err(LifecycleError::IntegrityCheckFailed(_))
        ));
    }

    #[test]
    fn test_archive_integrity_check_invalid_range() {
        let executor = ArchiveExecutor::new();
        let range = DataRange {
            min_id: 100,
            max_id: 1,
            ..make_range()
        };
        let result = executor.archive("orders", &range, "s3://archive");
        assert!(matches!(
            result,
            Err(LifecycleError::IntegrityCheckFailed(_))
        ));
    }

    #[test]
    fn test_batch_archive() {
        let executor = ArchiveExecutor::new();
        let ranges = vec![make_range(), make_range(), make_range()];
        let report = executor
            .batch_archive("orders", &ranges, "s3://archive")
            .unwrap();
        assert_eq!(report.total_archived, 300);
        assert_eq!(report.failed, 0);
        assert_eq!(report.records.len(), 3);
    }

    #[test]
    fn test_mark_source_deleted() {
        let executor = ArchiveExecutor::new();
        let range = make_range();
        let record = executor.archive("orders", &range, "s3://archive").unwrap();

        executor.mark_source_deleted(&record.record_id).unwrap();
        let archives = executor.get_archives();
        assert_eq!(archives[0].source_status, SourceStatus::Deleted);
    }

    #[test]
    fn test_archive_without_integrity_check() {
        let executor = ArchiveExecutor::with_integrity_check(false);
        let range = DataRange {
            row_count: 0,
            ..make_range()
        };
        let result = executor.archive("orders", &range, "s3://archive");
        assert!(result.is_ok());
        assert!(!result.unwrap().integrity_verified);
    }
}
