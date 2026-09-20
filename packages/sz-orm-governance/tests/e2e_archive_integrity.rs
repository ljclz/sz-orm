//! 端到端测试：归档完整性校验 + 源数据标记，归档前不删除源数据

use sz_orm_governance::lifecycle::{ArchiveExecutor, DataRange, DataTemperature, SourceStatus};

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

#[tokio::test]
async fn e2e_archive_integrity_verified() {
    let executor = ArchiveExecutor::new();
    let range = make_range();
    let record = executor.archive("orders", &range, "s3://archive").unwrap();

    assert!(record.integrity_verified);
    assert_eq!(record.source_status, SourceStatus::Archived);
}

#[tokio::test]
async fn e2e_archive_source_not_deleted() {
    let executor = ArchiveExecutor::new();
    let range = make_range();
    let record = executor.archive("orders", &range, "s3://archive").unwrap();

    assert_ne!(
        record.source_status,
        SourceStatus::Deleted,
        "归档后源数据不应被删除"
    );
    assert_eq!(record.source_status, SourceStatus::Archived);
}

#[tokio::test]
async fn e2e_archive_integrity_failure_rejected() {
    let executor = ArchiveExecutor::new();
    let range = DataRange {
        row_count: 0,
        ..make_range()
    };
    let result = executor.archive("orders", &range, "s3://archive");
    assert!(result.is_err(), "完整性校验失败应拒绝归档");
}

#[tokio::test]
async fn e2e_archive_batch_report() {
    let executor = ArchiveExecutor::new();
    let ranges = vec![make_range(), make_range(), make_range()];
    let report = executor
        .batch_archive("orders", &ranges, "s3://archive")
        .unwrap();

    assert_eq!(report.total_archived, 300);
    assert_eq!(report.failed, 0);
    assert_eq!(report.records.len(), 3);
    for record in &report.records {
        assert!(record.integrity_verified);
    }
}

#[tokio::test]
async fn e2e_archive_mark_deleted_after_verification() {
    let executor = ArchiveExecutor::new();
    let range = make_range();
    let record = executor.archive("orders", &range, "s3://archive").unwrap();

    executor.mark_source_deleted(&record.record_id).unwrap();
    let archives = executor.get_archives();
    assert_eq!(archives[0].source_status, SourceStatus::Deleted);
}
