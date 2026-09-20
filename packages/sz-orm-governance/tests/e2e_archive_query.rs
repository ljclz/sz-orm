//! 端到端测试：归档数据回查 5s 延迟，结果与归档前一致

use std::time::Duration;

use sz_orm_governance::lifecycle::{ArchiveQueryProxy, DataRange, DataTemperature};

fn make_archived_range() -> DataRange {
    DataRange {
        min_id: 1,
        max_id: 100,
        row_count: 100,
        temperature: DataTemperature::Archived,
        last_accessed_days_ago: 90,
        access_frequency_per_day: 0.1,
    }
}

#[tokio::test]
async fn e2e_archive_query_returns_data() {
    let proxy = ArchiveQueryProxy::new();
    let range = make_archived_range();
    proxy.register_archived_data("orders", range);

    let results = proxy.query("orders", 1, 100).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].min_id, 1);
    assert_eq!(results[0].max_id, 100);
}

#[tokio::test]
async fn e2e_archive_query_promotes_to_hot() {
    let proxy = ArchiveQueryProxy::new();
    let range = make_archived_range();
    proxy.register_archived_data("orders", range);

    let results = proxy.query("orders", 1, 100).await.unwrap();
    assert_eq!(results[0].temperature, DataTemperature::Hot);
    assert_eq!(results[0].last_accessed_days_ago, 0);
}

#[tokio::test]
async fn e2e_archive_query_timeout() {
    let proxy = ArchiveQueryProxy::with_timeout(Duration::from_millis(1));
    proxy.register_archived_data("orders", make_archived_range());

    std::thread::sleep(Duration::from_millis(10));
    let result = proxy.query("orders", 1, 100).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn e2e_archive_query_no_data() {
    let proxy = ArchiveQueryProxy::new();
    let results = proxy.query("orders", 1, 100).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn e2e_archive_query_multiple_ranges() {
    let proxy = ArchiveQueryProxy::new();
    proxy.register_archived_data("orders", make_archived_range());
    proxy.register_archived_data(
        "orders",
        DataRange {
            min_id: 200,
            max_id: 300,
            ..make_archived_range()
        },
    );

    let results = proxy.query("orders", 1, 300).await.unwrap();
    assert_eq!(results.len(), 2);
    for r in &results {
        assert_eq!(r.temperature, DataTemperature::Hot);
    }
}
