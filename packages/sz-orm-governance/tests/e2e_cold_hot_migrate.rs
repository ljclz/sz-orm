//! 端到端测试：冷数据自动迁移到冷存储，在线查询不阻塞

use sz_orm_governance::lifecycle::{
    AccessPatternCollector, ColdHotClassification, ColdHotClassifier, ColdHotMigrationScheduler,
    ColdHotThreshold, DataRange, DataTemperature, MigrationWindow,
};

fn make_cold_range() -> DataRange {
    DataRange {
        min_id: 1,
        max_id: 1000,
        row_count: 1000,
        temperature: DataTemperature::Cold,
        last_accessed_days_ago: 90,
        access_frequency_per_day: 0.1,
    }
}

fn make_hot_range() -> DataRange {
    DataRange {
        min_id: 1001,
        max_id: 2000,
        row_count: 1000,
        temperature: DataTemperature::Hot,
        last_accessed_days_ago: 1,
        access_frequency_per_day: 10.0,
    }
}

#[tokio::test]
#[ignore = "需真实 DB 环境验证迁移不阻塞在线查询"]
async fn e2e_cold_hot_migrate_full_chain() {
    let mut collector = AccessPatternCollector::new();
    collector.record_batch_access("orders", 10);
    collector.set_last_accessed("orders", 90);
    collector.set_total_rows("orders", 2000);

    let classifier = ColdHotClassifier::with_default_threshold();
    let stats = collector.get_stats("orders").unwrap();
    let ranges = vec![make_hot_range(), make_cold_range()];
    let classification = classifier.classify("orders", stats, &ranges).unwrap();

    assert_eq!(classification.hot_data.len(), 1);
    assert_eq!(classification.cold_data.len(), 1);

    let scheduler = ColdHotMigrationScheduler::new();
    let window = MigrationWindow::default();
    let cold_data = scheduler
        .schedule_migration(&classification, &window, 3)
        .unwrap();

    assert_eq!(cold_data.len(), 1);
    assert_eq!(cold_data[0].temperature, DataTemperature::Cold);
    scheduler.complete_migration("orders");
}

#[tokio::test]
async fn e2e_cold_hot_migrate_out_of_window() {
    let classification = ColdHotClassification {
        table: "orders".to_string(),
        hot_data: vec![],
        warm_data: vec![],
        cold_data: vec![make_cold_range()],
    };

    let scheduler = ColdHotMigrationScheduler::new();
    let window = MigrationWindow::default();
    let result = scheduler.schedule_migration(&classification, &window, 10);
    assert!(result.is_err());
}

#[tokio::test]
async fn e2e_cold_hot_migrate_cold_storage_unavailable() {
    let classification = ColdHotClassification {
        table: "orders".to_string(),
        hot_data: vec![],
        warm_data: vec![],
        cold_data: vec![make_cold_range()],
    };

    let scheduler = ColdHotMigrationScheduler::new();
    scheduler.set_cold_storage_available(false);
    let window = MigrationWindow::default();
    let result = scheduler.schedule_migration(&classification, &window, 3);
    assert!(result.is_err());
}

#[tokio::test]
async fn e2e_cold_hot_classify_threshold_boundary() {
    let classifier = ColdHotClassifier::new(ColdHotThreshold {
        access_frequency_per_day: 1.0,
        age_days: 30,
    });

    let range = DataRange {
        min_id: 0,
        max_id: 100,
        row_count: 100,
        temperature: DataTemperature::Hot,
        last_accessed_days_ago: 30,
        access_frequency_per_day: 1.0,
    };
    assert_eq!(classifier.classify_range(&range), DataTemperature::Hot);
}
