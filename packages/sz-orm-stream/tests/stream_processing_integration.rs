//! 实时流处理集成测试

#![cfg(feature = "stream-processing")]

use std::time::Duration;

use sz_orm_stream::*;

#[test]
fn test_materialized_view_full_workflow() {
    let engine = ViewRefreshEngine::new();
    let def =
        MaterializedViewDef::new("user_summary", "public.users", "SELECT count(*) FROM users");
    let view_id = engine.define_view(def).unwrap();

    let stats = engine.start_incremental_refresh(&view_id).unwrap();
    assert!(stats.elapsed < Duration::from_secs(1));

    let result = engine.query_view(&view_id).unwrap();
    assert!(result.is_object());
}

#[test]
fn test_stream_batch_job_workflow() {
    let mut job = StreamBatchJob::new("etl_job", JobMode::StreamBatchUnified);
    job.dag.add_node("extract", vec![]);
    job.dag.add_node("transform", vec!["extract".to_string()]);
    job.dag.add_node("load", vec!["transform".to_string()]);

    let order = job.execution_order();
    assert_eq!(order, vec!["extract", "transform", "load"]);

    let handle = job.submit_stream().unwrap();
    assert_eq!(handle.mode, JobMode::Stream);
}

#[test]
fn test_window_assignment() {
    let assigner = WindowAssigner::new(WindowConfig::tumbling(Duration::from_millis(100)));
    let event = Event::new(150, serde_json::json!({"v": 1}));
    let windows = assigner.assign(&event);
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].start_ms, 100);
    assert_eq!(windows[0].end_ms, 200);
}

#[test]
fn test_backpressure_strategy() {
    let strategy = BackpressureStrategy::DropOldest;
    assert_eq!(strategy.apply(100, 80), 80);
    assert!(strategy.should_alert(100, 80));
}
