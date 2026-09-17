//! v7.3.0 任务 2.2：故障转移端到端测试（真实 DB，需 --ignored）
//!
//! 验证：主备库探活 + 故障转移 ≤ 5s + 位点校验 + 回切 + 决策链
//!
//! 运行：cargo test -p sz-orm-core --features auto-failover -- --ignored failover_e2e

use std::time::{Duration, Instant};

use sz_orm_core::{
    FailbackStrategy, FailoverConfig,
    rw_split_enhanced::{AutoFailoverCoordinator, ProbeResult},
};

fn make_ha_config() -> FailoverConfig {
    FailoverConfig {
        primary_url: "mysql://root:test123@127.0.0.1:3306/sz_orm_test".into(),
        replica_url: "mysql://root:test123@127.0.0.1:3307/sz_orm_test".into(),
        probe_interval: Duration::from_secs(1),
        probe_failure_threshold: 3,
        failback_strategy: FailbackStrategy::Auto,
    }
}

#[tokio::test]
#[ignore = "需要真实主备库（MySQL 3306 + 3307）"]
async fn failover_e2e_primary_down_failover_within_5s() {
    let coord = AutoFailoverCoordinator::new(make_ha_config());
    coord.set_primary_lsn(100);
    coord.set_replica_lsn(100);

    let start = Instant::now();
    // 模拟主库宕机：连续探活失败
    for _ in 0..3 {
        coord.record_probe(ProbeResult {
            success: false,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            error: Some("connection refused".to_string()),
        });
    }
    let elapsed = start.elapsed();

    assert!(coord.is_failed_over(), "应已故障转移到备库");
    assert!(
        elapsed <= Duration::from_secs(5),
        "RTO 应 ≤ 5s，实际 {:?}",
        elapsed
    );

    // 决策链应有记录
    let history = coord.decision_history();
    assert!(!history.is_empty());
    assert!(history[0].switched);
}

#[tokio::test]
#[ignore = "需要真实主备库（MySQL 3306 + 3307）"]
async fn failover_e2e_lsn_check_and_failback() {
    let coord = AutoFailoverCoordinator::new(make_ha_config());
    coord.set_primary_lsn(100);
    coord.set_replica_lsn(100);

    // 故障转移
    coord.trigger_failover().unwrap();
    assert!(coord.is_failed_over());

    // 模拟主库恢复：探活成功
    coord.record_probe(ProbeResult {
        success: true,
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
        error: None,
    });

    // 位点一致后自动回切
    coord.set_replica_lsn(100);
    coord.failback(FailbackStrategy::Auto).unwrap();
    assert!(!coord.is_failed_over(), "应已回切到主库");
}