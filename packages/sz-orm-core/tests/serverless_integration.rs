//! Serverless 适配集成测试
//!
//! 验证 ColdStartOptimizer + GracefulShutdown + MeteringCollector 协同工作。

#![cfg(feature = "serverless-adapt")]

use std::time::Duration;

use sz_orm_core::{CdcCheckpoint, ColdStartOptimizer, GracefulShutdown, GracefulShutdownConfig};

#[test]
fn test_cold_start_to_graceful_shutdown() {
    let optimizer = ColdStartOptimizer::default();
    let stats = optimizer.on_cold_start();
    assert_eq!(stats.warmed_connections, 2);
    assert!(!stats.partial_available);

    let shutdown = GracefulShutdown::default();
    let checkpoints = vec![CdcCheckpoint {
        source: "mysql".into(),
        position: 42,
    }];
    let result = shutdown
        .on_scale_to_zero(stats.warmed_connections, &checkpoints)
        .unwrap();
    assert_eq!(result.released_connections, 2);
    assert_eq!(result.persisted_checkpoints, 1);
}

#[test]
fn test_cold_start_p95_tracking() {
    let optimizer = ColdStartOptimizer::default();
    for i in 1..=50 {
        optimizer.record_latency(Duration::from_millis(i));
    }
    let stats = optimizer.on_cold_start();
    assert!(stats.p95_latency >= Duration::from_millis(47));
}

#[test]
fn test_graceful_shutdown_scale_up_advice() {
    let shutdown = GracefulShutdown::default();
    let advice = shutdown.scale_up_advice(5, 15);
    assert!(advice.is_some());
    assert!(advice.unwrap() >= 15);
}

#[test]
fn test_graceful_shutdown_custom_config() {
    let shutdown = GracefulShutdown::new(GracefulShutdownConfig {
        checkpoint_timeout: Duration::from_secs(10),
        idle_release_threshold: Duration::from_secs(120),
    });
    assert_eq!(
        shutdown.config().checkpoint_timeout,
        Duration::from_secs(10)
    );
    assert!(!shutdown.should_release_idle());
}
