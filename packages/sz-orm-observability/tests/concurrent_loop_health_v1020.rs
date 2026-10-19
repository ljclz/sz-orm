//! v10.2.0 闭环健康度并发安全性测试

use std::sync::Arc;
use sz_orm_observability::loop_health::{LoopHealthConfig, LoopHealthMonitor};

#[tokio::test]
async fn test_concurrent_loop_health_metrics_100_threads() {
    let config = LoopHealthConfig {
        loop_health_enabled: true,
        ..Default::default()
    };
    let monitor = Arc::new(LoopHealthMonitor::new(config));
    let mut handles = Vec::new();
    for _ in 0..100 {
        let m = monitor.clone();
        handles.push(tokio::spawn(async move {
            m.record_enqueue();
            m.record_process(1);
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
    let snap = monitor.metrics();
    assert_eq!(snap.enqueued_count, 100, "100 enqueued across threads");
    assert_eq!(snap.processed_count, 100, "100 processed across threads");
    let score = monitor.compute_health_score();
    assert!(score >= 80.0, "healthy concurrent operation: {score}");
}
