//! 端到端测试：TTL 清理先日志后删除，清理日志记录删除时间和数据摘要

use sz_orm_governance::lifecycle::{DataRange, DataTemperature, TtlCleanupExecutor};

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

#[tokio::test]
async fn e2e_ttl_cleanup_logs_before_delete() {
    let executor = TtlCleanupExecutor::new();
    let ranges = vec![make_range(400, DataTemperature::Cold)];

    let report = executor.cleanup("orders", &ranges, "r1", 365).unwrap();
    assert_eq!(report.total_deleted, 100);

    let logs = executor.get_cleanup_logs();
    assert_eq!(logs.len(), 1, "清理日志应在删除前写入");
    assert_eq!(logs[0].table, "orders");
    assert_eq!(logs[0].row_count, 100);
    assert!(logs[0].data_summary.contains("range="));
}

#[tokio::test]
async fn e2e_ttl_cleanup_skips_recently_accessed() {
    let executor = TtlCleanupExecutor::new();
    let ranges = vec![make_range(100, DataTemperature::Cold)];

    let report = executor.cleanup("orders", &ranges, "r1", 365).unwrap();
    assert_eq!(report.total_deleted, 0);
    assert_eq!(report.total_skipped, 100);
    assert!(report
        .skipped_reasons
        .contains_key("TTL_SKIP_RECENTLY_ACCESSED"));
}

#[tokio::test]
async fn e2e_ttl_cleanup_skips_hot_data() {
    let executor = TtlCleanupExecutor::new();
    let ranges = vec![make_range(400, DataTemperature::Hot)];

    let report = executor.cleanup("orders", &ranges, "r1", 365).unwrap();
    assert_eq!(report.total_deleted, 0);
    assert!(report.skipped_reasons.contains_key("TTL_SKIP_HOT_DATA"));
}

#[tokio::test]
async fn e2e_ttl_cleanup_mixed() {
    let executor = TtlCleanupExecutor::new();
    let ranges = vec![
        make_range(400, DataTemperature::Cold),
        make_range(100, DataTemperature::Cold),
        make_range(400, DataTemperature::Warm),
    ];

    let report = executor.cleanup("orders", &ranges, "r1", 365).unwrap();
    assert_eq!(report.total_scanned, 300);
    assert_eq!(report.total_deleted, 200);
    assert_eq!(report.total_skipped, 100);
}

#[tokio::test]
async fn e2e_ttl_cleanup_log_has_timestamp() {
    let executor = TtlCleanupExecutor::new();
    let ranges = vec![make_range(400, DataTemperature::Cold)];

    executor.cleanup("orders", &ranges, "r1", 365).unwrap();
    let logs = executor.get_cleanup_logs();
    assert!(logs[0].deleted_at > 0, "清理日志应记录删除时间戳");
}
