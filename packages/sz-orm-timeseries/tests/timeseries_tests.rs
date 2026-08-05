//! 集成测试：验证 TimeseriesExt 的端到端行为
//!
//! 使用内存实现（不需要 TimescaleDB 连接）。

use chrono::{Duration, Utc};
use sz_orm_timeseries::{
    Aggregation, Metric, TimeseriesBuilder, TimeseriesExt, TimeseriesProvider,
};

fn wrapper() -> impl TimeseriesExt {
    TimeseriesBuilder::new(TimeseriesProvider::Memory)
        .build()
        .expect("build memory provider")
}

#[tokio::test]
async fn integration_create_hypertable_and_insert() {
    let w = wrapper();
    w.create_hypertable("cpu_usage", "ts").await.unwrap();
    let now = Utc::now();
    w.insert_metric(&Metric::new("cpu_usage", now, 0.75))
        .await
        .unwrap();
    let rows = w
        .query_range(
            "cpu_usage",
            now - Duration::minutes(1),
            now + Duration::minutes(1),
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, 0.75);
}

#[tokio::test]
async fn integration_query_range_filters_by_time() {
    let w = wrapper();
    w.create_hypertable("req_latency", "ts").await.unwrap();
    let now = Utc::now();
    w.insert_metric(&Metric::new(
        "req_latency",
        now - Duration::minutes(5),
        10.0,
    ))
    .await
    .unwrap();
    w.insert_metric(&Metric::new("req_latency", now, 20.0))
        .await
        .unwrap();
    w.insert_metric(&Metric::new(
        "req_latency",
        now + Duration::minutes(5),
        30.0,
    ))
    .await
    .unwrap();

    let rows = w
        .query_range(
            "req_latency",
            now - Duration::minutes(1),
            now + Duration::minutes(1),
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "仅时间窗口内的数据点应返回");
    assert_eq!(rows[0].value, 20.0);
}

#[tokio::test]
async fn integration_time_bucket_aggregate() {
    let w = wrapper();
    w.create_hypertable("temp", "ts").await.unwrap();
    let now = Utc::now();
    for (i, v) in [1.0, 2.0, 3.0].iter().enumerate() {
        w.insert_metric(&Metric::new("temp", now + Duration::seconds(i as i64), *v))
            .await
            .unwrap();
    }

    let buckets = w
        .time_bucket_aggregate(
            "temp",
            "1m",
            Aggregation::Avg,
            now,
            now + Duration::minutes(1),
        )
        .await
        .unwrap();
    assert_eq!(buckets.len(), 1, "1 分钟窗口内应聚合成 1 个桶");
    assert_eq!(buckets[0].count, 3);
    assert!(
        (buckets[0].avg - 2.0).abs() < 1e-6,
        "平均值为 2.0，实际 {}",
        buckets[0].avg
    );
}

#[tokio::test]
async fn integration_drop_metric() {
    let w = wrapper();
    w.create_hypertable("ephemeral", "ts").await.unwrap();
    let now = Utc::now();
    w.insert_metric(&Metric::new("ephemeral", now, 1.0))
        .await
        .unwrap();
    w.drop_metric("ephemeral").await.unwrap();

    let rows = w
        .query_range(
            "ephemeral",
            now - Duration::minutes(1),
            now + Duration::minutes(1),
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 0, "删除后应无数据");
}

#[tokio::test]
async fn integration_metric_tags_roundtrip() {
    let w = wrapper();
    w.create_hypertable("tagged", "ts").await.unwrap();
    let now = Utc::now();
    let metric = Metric::new("tagged", now, 42.0)
        .with_tag("host", "node-1")
        .with_tag("region", "cn-north");
    w.insert_metric(&metric).await.unwrap();

    let rows = w
        .query_range(
            "tagged",
            now - Duration::minutes(1),
            now + Duration::minutes(1),
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tags.get("host").map(|s| s.as_str()), Some("node-1"));
}
