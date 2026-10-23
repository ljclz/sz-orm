//! v10.12.0 T3.1 + T3.2 + T3.3: 亚百毫秒切换 + 半开连接探测 + 事务重试集成测试

#![cfg(any(
    feature = "auto-failover",
    feature = "pool-elastic",
    feature = "saga-tx"
))]

use std::time::Duration;

// ============================================================================
// T3.1: 亚百毫秒切换测试
// ============================================================================

#[cfg(feature = "auto-failover")]
mod fast_switch {
    use super::Duration;
    use sz_orm_core::failover_controller::{FastSwitchConfig, FastSwitchCoordinator};

    #[test]
    fn test_fast_switch_with_preselected_backup() {
        let coord = FastSwitchCoordinator::new(FastSwitchConfig::default());
        coord.parallel_heartbeat(&[("backup-1", true), ("backup-2", true), ("backup-3", false)]);
        coord.preselect_backups(
            &["backup-1".into(), "backup-2".into(), "backup-3".into()],
            "primary",
        );
        assert_eq!(coord.preselected_backups().len(), 2);

        let result = coord.fast_switch("primary");
        assert!(result.success);
        assert_eq!(result.new_primary, "backup-1");
        assert!(
            result.elapsed < Duration::from_millis(100),
            "Switch should be <100ms: {:?}",
            result.elapsed
        );
    }

    #[test]
    fn test_fast_switch_no_backups_fails() {
        let coord = FastSwitchCoordinator::new(FastSwitchConfig::default());
        let result = coord.fast_switch("primary");
        assert!(!result.success);
        assert!(result.new_primary.is_empty());
    }

    #[test]
    fn test_preselect_excludes_unhealthy() {
        let coord = FastSwitchCoordinator::new(FastSwitchConfig::default());
        coord.parallel_heartbeat(&[("a", true), ("b", false), ("c", true)]);
        coord.preselect_backups(&["a".into(), "b".into(), "c".into()], "primary");
        let backups = coord.preselected_backups();
        assert!(backups.contains(&"a".to_string()));
        assert!(backups.contains(&"c".to_string()));
        assert!(!backups.contains(&"b".to_string()));
    }

    #[test]
    fn test_preselect_excludes_failed_primary() {
        let coord = FastSwitchCoordinator::new(FastSwitchConfig::default());
        coord.parallel_heartbeat(&[("primary", true), ("backup", true)]);
        coord.preselect_backups(&["primary".into(), "backup".into()], "primary");
        let backups = coord.preselected_backups();
        assert!(!backups.contains(&"primary".to_string()));
        assert!(backups.contains(&"backup".to_string()));
    }

    #[test]
    fn test_preselect_respects_max_candidates() {
        let config = FastSwitchConfig {
            max_candidates: 2,
            ..FastSwitchConfig::default()
        };
        let coord = FastSwitchCoordinator::new(config);
        coord.parallel_heartbeat(&[("a", true), ("b", true), ("c", true), ("d", true)]);
        coord.preselect_backups(&["a".into(), "b".into(), "c".into(), "d".into()], "primary");
        assert_eq!(coord.preselected_backups().len(), 2);
    }

    #[test]
    fn test_switch_marks_primary_unhealthy() {
        let coord = FastSwitchCoordinator::new(FastSwitchConfig::default());
        coord.parallel_heartbeat(&[("backup", true), ("primary", true)]);
        coord.preselect_backups(&["backup".into(), "primary".into()], "primary");
        let _ = coord.fast_switch("primary");

        coord.preselect_backups(&["primary".into(), "backup".into()], "new_primary");
        let backups = coord.preselected_backups();
        assert!(
            !backups.contains(&"primary".to_string()),
            "primary should be unhealthy after switch, backups: {:?}",
            backups
        );
    }

    #[test]
    fn test_parallel_heartbeat_updates_health() {
        let coord = FastSwitchCoordinator::new(FastSwitchConfig::default());
        coord.parallel_heartbeat(&[("node-1", true), ("node-2", false)]);
        coord.preselect_backups(&["node-1".into(), "node-2".into()], "primary");
        let backups = coord.preselected_backups();
        assert!(backups.contains(&"node-1".to_string()));
        assert!(!backups.contains(&"node-2".to_string()));
    }

    #[test]
    fn test_fast_switch_elapsed_extremely_low() {
        let coord = FastSwitchCoordinator::new(FastSwitchConfig::default());
        coord.parallel_heartbeat(&[("backup", true)]);
        coord.preselect_backups(&["backup".into()], "primary");
        let result = coord.fast_switch("primary");
        assert!(
            result.elapsed < Duration::from_millis(1),
            "In-memory switch should be <1ms: {:?}",
            result.elapsed
        );
    }
}

// ============================================================================
// T3.2: 半开连接探测与自动剔除测试
// ============================================================================

#[cfg(feature = "pool-elastic")]
mod half_open_probe {
    use super::Duration;
    use sz_orm_core::pool_elastic::{
        AutoEvictor, ConnectionHealthChecker, HalfOpenProbe, ProbeAliveConfig, ProbeResult,
    };

    #[test]
    fn test_half_open_probe_alive() {
        let probe = HalfOpenProbe::new(ProbeAliveConfig::default());
        let result = probe.probe_alive("conn-1", true);
        assert_eq!(result, ProbeResult::Alive);
        assert_eq!(probe.fail_count("conn-1"), 0);
    }

    #[test]
    fn test_half_open_probe_dead_single_fail() {
        let probe = HalfOpenProbe::new(ProbeAliveConfig::default());
        let result = probe.probe_alive("conn-1", false);
        assert_eq!(result, ProbeResult::HalfOpen { fail_count: 1 });
        assert!(!probe.should_evict("conn-1"));
    }

    #[test]
    fn test_half_open_probe_evict_after_threshold() {
        let config = ProbeAliveConfig {
            fail_threshold: 3,
            ..ProbeAliveConfig::default()
        };
        let probe = HalfOpenProbe::new(config);
        assert_eq!(
            probe.probe_alive("conn-1", false),
            ProbeResult::HalfOpen { fail_count: 1 }
        );
        assert_eq!(
            probe.probe_alive("conn-1", false),
            ProbeResult::HalfOpen { fail_count: 2 }
        );
        assert_eq!(probe.probe_alive("conn-1", false), ProbeResult::Evicted);
        assert_eq!(probe.auto_evictor().evicted_count(), 1);
        assert_eq!(probe.auto_evictor().compensated_count(), 1);
    }

    #[test]
    fn test_half_open_probe_keep_alive_resets_count() {
        let config = ProbeAliveConfig {
            fail_threshold: 3,
            ..ProbeAliveConfig::default()
        };
        let probe = HalfOpenProbe::new(config);
        probe.probe_alive("conn-1", false);
        probe.probe_alive("conn-1", false);
        assert_eq!(probe.fail_count("conn-1"), 2);
        probe.probe_alive("conn-1", true);
        assert_eq!(probe.fail_count("conn-1"), 0);
        assert_eq!(
            probe.probe_alive("conn-1", false),
            ProbeResult::HalfOpen { fail_count: 1 }
        );
    }

    #[test]
    fn test_half_open_probe_concurrent() {
        use std::sync::Arc;
        use std::thread;
        let probe = Arc::new(HalfOpenProbe::new(ProbeAliveConfig {
            fail_threshold: 1000,
            ..ProbeAliveConfig::default()
        }));
        let mut handles = Vec::new();
        for i in 0..4 {
            let p = Arc::clone(&probe);
            handles.push(thread::spawn(move || {
                let conn_id = format!("conn-{}", i);
                for _ in 0..10 {
                    p.probe_alive(&conn_id, false);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        for i in 0..4 {
            let conn_id = format!("conn-{}", i);
            assert_eq!(probe.fail_count(&conn_id), 10);
        }
    }

    #[test]
    fn test_half_open_probe_batch() {
        let probe = HalfOpenProbe::new(ProbeAliveConfig::default());
        for i in 0..10 {
            let conn_id = format!("conn-{}", i);
            probe.probe_alive(&conn_id, i % 2 == 0);
        }
        assert_eq!(probe.alive_count(), 5);
        assert_eq!(probe.half_open_count(), 5);
    }

    #[test]
    fn test_half_open_probe_config_bounds() {
        let config = ProbeAliveConfig {
            fail_threshold: 1,
            ..ProbeAliveConfig::default()
        };
        let probe = HalfOpenProbe::new(config);
        let result = probe.probe_alive("conn-1", false);
        assert_eq!(result, ProbeResult::Evicted);
    }

    #[test]
    fn test_half_open_probe_all_dead_clear() {
        let config = ProbeAliveConfig {
            fail_threshold: 2,
            ..ProbeAliveConfig::default()
        };
        let probe = HalfOpenProbe::new(config);
        probe.probe_alive("a", false);
        probe.probe_alive("b", false);
        assert_eq!(probe.half_open_count(), 2);
        probe.clear();
        assert_eq!(probe.half_open_count(), 0);
        assert_eq!(probe.alive_count(), 0);
    }

    #[test]
    fn test_half_open_probe_get_alive_count() {
        let probe = HalfOpenProbe::new(ProbeAliveConfig::default());
        probe.probe_alive("alive-1", true);
        probe.probe_alive("alive-2", true);
        probe.probe_alive("dead-1", false);
        assert_eq!(probe.alive_count(), 2);
        assert_eq!(probe.half_open_count(), 1);
    }

    #[test]
    fn test_auto_evictor_counts() {
        let evictor = AutoEvictor::new();
        evictor.record_evict();
        evictor.record_evict();
        evictor.record_compensate();
        assert_eq!(evictor.evicted_count(), 2);
        assert_eq!(evictor.compensated_count(), 1);
    }

    #[test]
    fn test_connection_health_checker_with_probe() {
        let checker = ConnectionHealthChecker::new_with_probe(
            3,
            ProbeAliveConfig {
                fail_threshold: 2,
                ..ProbeAliveConfig::default()
            },
        );
        assert!(checker.half_open_probe().is_some());
        let result = checker.record_probe("conn-1", false);
        assert_eq!(result, ProbeResult::HalfOpen { fail_count: 1 });
        let result = checker.record_probe("conn-1", false);
        assert_eq!(result, ProbeResult::Evicted);
    }

    #[test]
    fn test_connection_health_checker_without_probe() {
        let checker = ConnectionHealthChecker::new(3);
        assert!(checker.half_open_probe().is_none());
        let result = checker.record_probe("conn-1", false);
        assert_eq!(result, ProbeResult::Alive);
    }

    #[test]
    fn test_half_open_probe_should_probe_first_time() {
        let probe = HalfOpenProbe::new(ProbeAliveConfig::default());
        assert!(probe.should_probe("conn-1"));
    }

    #[test]
    fn test_half_open_probe_should_probe_after_interval() {
        let config = ProbeAliveConfig {
            probe_interval: Duration::from_millis(1),
            ..ProbeAliveConfig::default()
        };
        let probe = HalfOpenProbe::new(config);
        probe.probe_alive("conn-1", true);
        assert!(!probe.should_probe("conn-1"));
        std::thread::sleep(Duration::from_millis(2));
        assert!(probe.should_probe("conn-1"));
    }
}
// ============================================================================
// T3.3: 事务补偿与自动重试策略增强测试
// ============================================================================

#[cfg(feature = "saga-tx")]
mod tx_retry {
    use super::Duration;
    use sz_orm_core::retry::RetryPolicy;
    use sz_orm_core::saga::{RetryStormGuard, TxRetryError, TxRetryPolicy};
    use sz_orm_core::DbError;

    fn fast_policy() -> RetryPolicy {
        RetryPolicy {
            max_retries: 3,
            initial_delay: Duration::from_micros(1),
            max_delay: Duration::from_micros(10),
            backoff_factor: 2.0,
            jitter: false,
        }
    }

    #[test]
    fn test_tx_retry_idempotent() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;
        let policy = TxRetryPolicy::new(fast_policy(), 10, true);
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let result: Result<u32, TxRetryError> = policy.execute_with_retry(|| {
            let n = c.fetch_add(1, Ordering::SeqCst);
            if n < 2 {
                Err(DbError::ConnectionError("fail".into()))
            } else {
                Ok(42)
            }
        });
        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_tx_retry_non_idempotent_skip() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;
        let policy = TxRetryPolicy::new(fast_policy(), 10, false);
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let result: Result<u32, TxRetryError> = policy.execute_with_retry(|| {
            c.fetch_add(1, Ordering::SeqCst);
            Err(DbError::ConnectionError("fail".into()))
        });
        assert!(matches!(result, Err(TxRetryError::NonRetryable(_))));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_tx_retry_compensation_linkage() {
        let policy = TxRetryPolicy::new(fast_policy(), 10, true);
        let result: Result<u32, TxRetryError> =
            policy.execute_with_retry(|| Err(DbError::ConnectionError("permanent fail".into())));
        assert!(matches!(result, Err(TxRetryError::RetryExhausted { .. })));
        assert_eq!(policy.compensate_trigger().compensate_count(), 1);
    }

    #[test]
    fn test_tx_retry_backoff_inherited() {
        let retry_policy = RetryPolicy {
            max_retries: 2,
            initial_delay: Duration::from_micros(1),
            max_delay: Duration::from_micros(100),
            backoff_factor: 3.0,
            jitter: false,
        };
        let policy = TxRetryPolicy::new(retry_policy, 10, true);
        assert_eq!(policy.retry_policy().max_retries, 2);
        assert!((policy.retry_policy().backoff_factor - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_tx_retry_max_attempts() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;
        let policy = TxRetryPolicy::new(fast_policy(), 10, true);
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let result: Result<u32, TxRetryError> = policy.execute_with_retry(|| {
            c.fetch_add(1, Ordering::SeqCst);
            Err(DbError::ConnectionError("fail".into()))
        });
        assert!(matches!(
            result,
            Err(TxRetryError::RetryExhausted { attempts: 4, .. })
        ));
        assert_eq!(counter.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn test_tx_retry_on_connection_error() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;
        let policy = TxRetryPolicy::new(fast_policy(), 10, true);
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let result: Result<u32, TxRetryError> = policy.execute_with_retry(|| {
            let n = c.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                Err(DbError::ConnectionTimeout("timeout".into()))
            } else {
                Ok(100)
            }
        });
        assert_eq!(result.unwrap(), 100);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_tx_retry_query_error_skip() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;
        let policy = TxRetryPolicy::new(fast_policy(), 10, true);
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let result: Result<u32, TxRetryError> = policy.execute_with_retry(|| {
            c.fetch_add(1, Ordering::SeqCst);
            Err(DbError::QueryError("bad query".into()))
        });
        assert!(matches!(result, Err(TxRetryError::NonRetryable(_))));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_tx_retry_permanent_failure() {
        let policy = TxRetryPolicy::new(fast_policy(), 10, true);
        let result: Result<u32, TxRetryError> =
            policy.execute_with_retry(|| Err(DbError::ConfigError("permanent".into())));
        assert!(matches!(result, Err(TxRetryError::NonRetryable(_))));
    }

    #[test]
    fn test_retry_storm_guard() {
        let guard = RetryStormGuard::new(2);
        assert!(guard.try_acquire());
        assert!(guard.try_acquire());
        assert!(!guard.try_acquire());
        guard.release();
        assert!(guard.try_acquire());
    }

    #[test]
    fn test_saga_coordinator_with_retry_policy() {
        use std::sync::Arc;
        use sz_orm_core::saga::{SagaCoordinator, SagaStore, TimeoutConfig};
        let store = Arc::new(SagaStore::new());
        let timeout = TimeoutConfig::default();
        let policy = TxRetryPolicy::new(fast_policy(), 10, true);
        let coord = SagaCoordinator::new(store, timeout).with_retry_policy(policy);
        assert!(coord.retry_policy().is_some());
        assert!(coord.retry_policy().unwrap().is_idempotent());
    }

    #[test]
    fn test_saga_coordinator_without_retry_policy() {
        use std::sync::Arc;
        use sz_orm_core::saga::{SagaCoordinator, SagaStore, TimeoutConfig};
        let store = Arc::new(SagaStore::new());
        let timeout = TimeoutConfig::default();
        let coord = SagaCoordinator::new(store, timeout);
        assert!(coord.retry_policy().is_none());
    }
}
// ============================================================================
// T3.4: 多活 RTO 优化测试
// ============================================================================

#[cfg(feature = "auto-failover")]
mod rto_optimizer {
    use super::Duration;
    use sz_orm_core::failover_controller::{
        CrossRegionSwitcher, FastVerifyAfterSwitch, ParallelSwitchCoordinator, PriorityStrategy,
        RegionConfig, RegionTopology, ReplicationPosition, ReplicationPositionComparator, RtoError,
        RtoOptimizer, SwitchVerifyChecker,
    };

    fn make_topology() -> RegionTopology {
        RegionTopology::new(
            vec![
                RegionConfig {
                    region_id: "region-a".into(),
                    geo_location: "us-east-1".into(),
                    capacity_weight: 80,
                    latency_ms: 10,
                },
                RegionConfig {
                    region_id: "region-b".into(),
                    geo_location: "us-west-1".into(),
                    capacity_weight: 90,
                    latency_ms: 50,
                },
                RegionConfig {
                    region_id: "region-c".into(),
                    geo_location: "eu-west-1".into(),
                    capacity_weight: 70,
                    latency_ms: 100,
                },
            ],
            "region-a",
        )
    }

    #[tokio::test]
    async fn test_rto_optimizer_multi_active() {
        let optimizer = RtoOptimizer::new(Duration::from_millis(200));
        let topology = make_topology();
        let healthy = ["region-b", "region-c"];

        let result = optimizer
            .optimize_rto("region-a", &topology, &healthy)
            .await
            .unwrap();

        assert_eq!(result.new_region, "region-b");
        assert!(result.elapsed < Duration::from_millis(100));
        assert!(result.verify_passed);
    }

    #[tokio::test]
    async fn test_rto_optimizer_preselected() {
        let optimizer = RtoOptimizer::new(Duration::from_millis(200));
        optimizer.set_preselected_region("region-c");
        let topology = make_topology();
        let healthy = ["region-b", "region-c"];

        let result = optimizer
            .optimize_rto("region-a", &topology, &healthy)
            .await
            .unwrap();

        assert_eq!(result.new_region, "region-c");
        assert!(result.used_preselected, "应使用预选区域");
    }

    #[tokio::test]
    async fn test_rto_optimizer_preselected_unhealthy_fallback() {
        let optimizer = RtoOptimizer::new(Duration::from_millis(200));
        optimizer.set_preselected_region("region-c");
        let topology = make_topology();
        let healthy = ["region-b"];

        let result = optimizer
            .optimize_rto("region-a", &topology, &healthy)
            .await
            .unwrap();

        assert_eq!(result.new_region, "region-b");
        assert!(!result.used_preselected, "预选不健康时回退常规选择");
    }

    #[tokio::test]
    async fn test_rto_optimizer_skip_unhealthy() {
        let optimizer = RtoOptimizer::new(Duration::from_millis(200));
        let topology = make_topology();
        let healthy = ["region-c"];

        let result = optimizer
            .optimize_rto("region-a", &topology, &healthy)
            .await
            .unwrap();

        assert_eq!(result.new_region, "region-c");
    }

    #[tokio::test]
    async fn test_rto_optimizer_no_healthy_region() {
        let optimizer = RtoOptimizer::new(Duration::from_millis(200));
        let topology = make_topology();
        let healthy: [&str; 0] = [];

        let result = optimizer
            .optimize_rto("region-a", &topology, &healthy)
            .await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), RtoError::NoHealthyRegion);
    }

    #[tokio::test]
    async fn test_rto_optimizer_best_region() {
        let optimizer = RtoOptimizer::new(Duration::from_millis(200));
        let topology = make_topology();
        let healthy = ["region-b", "region-c"];

        let result = optimizer
            .optimize_rto("region-a", &topology, &healthy)
            .await
            .unwrap();

        assert_eq!(result.new_region, "region-b", "应选第一个健康区域");
    }

    #[tokio::test]
    async fn test_rto_optimizer_get_rto() {
        let optimizer = RtoOptimizer::new(Duration::from_millis(200));
        let topology = make_topology();
        let healthy = ["region-b"];

        optimizer
            .optimize_rto("region-a", &topology, &healthy)
            .await
            .unwrap();

        let last = optimizer.last_result().unwrap();
        assert_eq!(last.new_region, "region-b");
        assert!(last.elapsed < Duration::from_millis(100));
        assert!(last.verify_passed);
    }

    #[tokio::test]
    async fn test_rto_optimizer_with_verify_pass() {
        let optimizer = RtoOptimizer::new(Duration::from_millis(200));
        let topology = make_topology();
        let healthy = ["region-b"];
        let checker = SwitchVerifyChecker::new(ReplicationPositionComparator::Gtid);
        let master_pos = ReplicationPosition::empty().with_gtid("uuid:1-100");
        let slave_pos = ReplicationPosition::empty().with_gtid("uuid:1-100");

        let result = optimizer
            .optimize_rto_with_verify(
                "region-a",
                &topology,
                &healthy,
                &checker,
                &master_pos,
                &slave_pos,
            )
            .await
            .unwrap();

        assert!(result.verify_passed);
        assert_eq!(optimizer.fast_verify().verify_successes(), 1);
    }

    #[tokio::test]
    async fn test_rto_optimizer_with_verify_fail() {
        let optimizer = RtoOptimizer::new(Duration::from_millis(200));
        let topology = make_topology();
        let healthy = ["region-b"];
        let checker = SwitchVerifyChecker::new(ReplicationPositionComparator::Gtid);
        let master_pos = ReplicationPosition::empty().with_gtid("uuid:1-100");
        let slave_pos = ReplicationPosition::empty().with_gtid("uuid:1-50");

        let result = optimizer
            .optimize_rto_with_verify(
                "region-a",
                &topology,
                &healthy,
                &checker,
                &master_pos,
                &slave_pos,
            )
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), RtoError::VerifyFailed);
        assert_eq!(optimizer.fast_verify().verify_failures(), 1);
    }

    #[tokio::test]
    async fn test_parallel_switch_coordinator() {
        let coord = ParallelSwitchCoordinator::new();
        let (v, r) = coord.parallel_switch(async { 1 }, async { 2 }).await;
        assert_eq!(v, 1);
        assert_eq!(r, 2);
        assert_eq!(coord.parallel_switches(), 1);
        assert_eq!(coord.serial_switches(), 0);

        let (v, r) = coord.serial_switch(async { 3 }, async { 4 }).await;
        assert_eq!(v, 3);
        assert_eq!(r, 4);
        assert_eq!(coord.parallel_switches(), 1);
        assert_eq!(coord.serial_switches(), 1);
    }

    #[test]
    fn test_fast_verify_timeout_config() {
        let verify = FastVerifyAfterSwitch::new(Duration::from_millis(50));
        assert_eq!(verify.verify_timeout(), Duration::from_millis(50));
    }

    #[tokio::test]
    async fn test_cross_region_switcher_with_rto() {
        let mut switcher = CrossRegionSwitcher::new_with_rto_optimizer(
            PriorityStrategy::GeoNear,
            Duration::from_millis(200),
        );
        let topology = make_topology();
        let healthy = ["region-b", "region-c"];

        let rto = switcher.rto_optimizer().unwrap();
        rto.set_preselected_region("region-c");

        let result = switcher.switch(&topology, &healthy).unwrap();
        assert_eq!(result, "region-c", "应使用预选区域直接切换");
    }

    #[tokio::test]
    async fn test_cross_region_switcher_without_rto() {
        let mut switcher = CrossRegionSwitcher::new(PriorityStrategy::GeoNear);
        let topology = make_topology();
        let healthy = ["region-b", "region-c"];

        assert!(switcher.rto_optimizer().is_none());

        let result = switcher.switch(&topology, &healthy).unwrap();
        assert_eq!(result, "region-b", "无 RTO 优化器时按优先级排序选择");
    }

    #[tokio::test]
    async fn test_rto_optimizer_preselected_region_getter() {
        let optimizer = RtoOptimizer::new(Duration::from_millis(200));
        assert!(optimizer.preselected_region().is_none());

        optimizer.set_preselected_region("region-b");
        assert_eq!(optimizer.preselected_region().unwrap(), "region-b");
    }
}
// ============================================================================
// T3.5: 故障恢复联动协调测试
// ============================================================================

#[cfg(feature = "auto-failover")]
mod recovery_coordinator {
    use super::Duration;
    use sz_orm_core::failover_controller::{
        FailoverController, FailoverDetector, FailoverRecoveryCoordinator, LinkageError,
        LinkageSequencer,
    };

    // --- LinkageSequencer 单元测试 ---

    #[test]
    fn test_linkage_sequencer_new_all_false() {
        let seq = LinkageSequencer::new();
        assert!(!seq.is_switch_completed());
        assert!(!seq.is_old_master_evicted());
        assert!(!seq.is_retry_routed());
    }

    #[test]
    fn test_linkage_sequencer_mark_switch_completed() {
        let seq = LinkageSequencer::new();
        seq.mark_switch_completed();
        assert!(seq.is_switch_completed());
        assert!(!seq.is_old_master_evicted());
    }

    #[test]
    fn test_linkage_sequencer_mark_old_master_evicted() {
        let seq = LinkageSequencer::new();
        seq.mark_old_master_evicted();
        assert!(seq.is_old_master_evicted());
        assert!(!seq.is_switch_completed());
    }

    #[test]
    fn test_linkage_sequencer_mark_retry_routed() {
        let seq = LinkageSequencer::new();
        seq.mark_retry_routed();
        assert!(seq.is_retry_routed());
    }

    #[test]
    fn test_linkage_sequencer_reset() {
        let seq = LinkageSequencer::new();
        seq.mark_switch_completed();
        seq.mark_old_master_evicted();
        seq.mark_retry_routed();
        assert!(seq.is_switch_completed());
        assert!(seq.is_old_master_evicted());
        assert!(seq.is_retry_routed());

        seq.reset();
        assert!(!seq.is_switch_completed());
        assert!(!seq.is_old_master_evicted());
        assert!(!seq.is_retry_routed());
    }

    #[test]
    fn test_linkage_sequencer_default() {
        let seq = LinkageSequencer::default();
        assert!(!seq.is_switch_completed());
    }

    #[test]
    fn test_linkage_sequencer_debug_format() {
        let seq = LinkageSequencer::new();
        let debug_str = format!("{:?}", seq);
        assert!(debug_str.contains("switch_completed"));
        assert!(debug_str.contains("old_master_evicted"));
        assert!(debug_str.contains("retry_routed"));
    }

    // --- FailoverRecoveryCoordinator 测试 ---

    #[tokio::test]
    async fn test_recovery_coordinator_new() {
        let coord = FailoverRecoveryCoordinator::new();
        assert_eq!(coord.recovery_count(), 0);
        assert!(!coord.sequencer().is_switch_completed());
    }

    #[tokio::test]
    async fn test_recovery_coordinator_default() {
        let coord = FailoverRecoveryCoordinator::default();
        assert_eq!(coord.recovery_count(), 0);
    }

    #[tokio::test]
    async fn test_on_switch_completed_marks_both() {
        let coord = FailoverRecoveryCoordinator::new();
        coord
            .on_switch_completed("new-master", "old-master")
            .await
            .unwrap();
        assert!(coord.sequencer().is_switch_completed());
        assert!(coord.sequencer().is_old_master_evicted());
        assert_eq!(coord.recovery_count(), 1);
    }

    #[tokio::test]
    async fn test_retry_route_before_switch_fails() {
        let coord = FailoverRecoveryCoordinator::new();
        let result = coord.retry_route_to_new_master("new-master").await;
        assert_eq!(result, Err(LinkageError::SwitchNotCompleted));
    }

    #[tokio::test]
    async fn test_retry_route_after_switch_succeeds() {
        let coord = FailoverRecoveryCoordinator::new();
        coord
            .on_switch_completed("new-master", "old-master")
            .await
            .unwrap();
        coord.retry_route_to_new_master("new-master").await.unwrap();
        assert!(coord.sequencer().is_retry_routed());
    }

    #[tokio::test]
    async fn test_full_recovery_sequence() {
        let coord = FailoverRecoveryCoordinator::new();

        // 阶段 1: 切换完成
        coord
            .on_switch_completed("new-master", "old-master")
            .await
            .unwrap();
        assert!(coord.sequencer().is_switch_completed());
        assert!(coord.sequencer().is_old_master_evicted());

        // 阶段 2: 重试路由到新主
        coord.retry_route_to_new_master("new-master").await.unwrap();
        assert!(coord.sequencer().is_retry_routed());

        assert_eq!(coord.recovery_count(), 1);
    }

    #[tokio::test]
    async fn test_recovery_count_increments() {
        let coord = FailoverRecoveryCoordinator::new();
        assert_eq!(coord.recovery_count(), 0);

        coord.on_switch_completed("m1", "old1").await.unwrap();
        assert_eq!(coord.recovery_count(), 1);

        coord.on_switch_completed("m2", "old2").await.unwrap();
        assert_eq!(coord.recovery_count(), 2);

        coord.on_switch_completed("m3", "old3").await.unwrap();
        assert_eq!(coord.recovery_count(), 3);
    }

    #[tokio::test]
    async fn test_reset_clears_sequencer() {
        let coord = FailoverRecoveryCoordinator::new();
        coord
            .on_switch_completed("new-master", "old-master")
            .await
            .unwrap();
        coord.retry_route_to_new_master("new-master").await.unwrap();
        assert!(coord.sequencer().is_switch_completed());
        assert!(coord.sequencer().is_retry_routed());

        coord.reset();
        assert!(!coord.sequencer().is_switch_completed());
        assert!(!coord.sequencer().is_old_master_evicted());
        assert!(!coord.sequencer().is_retry_routed());
    }

    #[tokio::test]
    async fn test_sequencer_getter() {
        let coord = FailoverRecoveryCoordinator::new();
        let seq = coord.sequencer();
        assert!(!seq.is_switch_completed());

        coord.on_switch_completed("new", "old").await.unwrap();
        assert!(seq.is_switch_completed());
    }

    #[tokio::test]
    async fn test_multiple_recovery_cycles() {
        let coord = FailoverRecoveryCoordinator::new();

        for i in 0..5 {
            coord.reset();
            coord
                .on_switch_completed(&format!("new-{i}"), &format!("old-{i}"))
                .await
                .unwrap();
            coord
                .retry_route_to_new_master(&format!("new-{i}"))
                .await
                .unwrap();
        }

        assert_eq!(coord.recovery_count(), 5);
    }

    #[tokio::test]
    async fn test_retry_route_only_switch_no_evict_fails() {
        let coord = FailoverRecoveryCoordinator::new();
        // 手动只标记切换完成，不标记剔除
        coord.sequencer().mark_switch_completed();
        let result = coord.retry_route_to_new_master("new-master").await;
        assert_eq!(result, Err(LinkageError::OldMasterNotEvicted));
    }

    // --- FailoverController 集成测试 ---

    fn make_controller() -> FailoverController {
        let detector = FailoverDetector::new(Duration::from_secs(5), 0.5);
        FailoverController::new_with_recovery_coordinator(detector, 3)
    }

    #[tokio::test]
    async fn test_controller_with_recovery_coordinator_recover_triggers_coordination() {
        let controller = make_controller();
        controller.set_master("master-a");
        controller.add_slave("slave-b");
        controller.set_node_health("master-a", true);
        controller.set_node_health("slave-b", true);

        // 执行故障转移
        controller.failover("slave-b").await.unwrap();

        // 恢复原主
        controller.recover("master-a").await.unwrap();

        // 验证恢复协调器被触发
        let coord = controller.recovery_coordinator().unwrap();
        assert!(coord.sequencer().is_switch_completed());
        assert!(coord.sequencer().is_old_master_evicted());
        assert_eq!(coord.recovery_count(), 1);
    }

    #[tokio::test]
    async fn test_controller_without_recovery_coordinator_backward_compat() {
        let detector = FailoverDetector::new(Duration::from_secs(5), 0.5);
        let controller = FailoverController::new(detector, 3);
        controller.set_master("master-a");
        controller.add_slave("slave-b");
        controller.set_node_health("master-a", true);
        controller.set_node_health("slave-b", true);

        controller.failover("slave-b").await.unwrap();
        controller.recover("master-a").await.unwrap();

        assert!(controller.recovery_coordinator().is_none());
    }

    #[tokio::test]
    async fn test_controller_recovery_coordinator_multiple_recovers() {
        let controller = make_controller();
        controller.set_master("master-a");
        controller.add_slave("slave-b");
        controller.set_node_health("master-a", true);
        controller.set_node_health("slave-b", true);

        // 第一次故障转移 + 恢复
        controller.failover("slave-b").await.unwrap();
        controller.recover("master-a").await.unwrap();

        // 第二次故障转移 + 恢复
        controller.failover("slave-b").await.unwrap();
        controller.recover("master-a").await.unwrap();

        let coord = controller.recovery_coordinator().unwrap();
        assert_eq!(coord.recovery_count(), 2);
    }

    #[tokio::test]
    async fn test_controller_recover_unhealthy_master_fails_no_coordination() {
        let controller = make_controller();
        controller.set_master("master-a");
        controller.add_slave("slave-b");
        controller.set_node_health("master-a", false);
        controller.set_node_health("slave-b", true);

        controller.failover("slave-b").await.unwrap();

        // 原主不健康，恢复应失败
        let result = controller.recover("master-a").await;
        assert!(result.is_err());

        // 恢复协调器不应被触发
        let coord = controller.recovery_coordinator().unwrap();
        assert_eq!(coord.recovery_count(), 0);
    }

    #[tokio::test]
    async fn test_recovery_coordinator_debug_format() {
        let coord = FailoverRecoveryCoordinator::new();
        let debug_str = format!("{:?}", coord);
        assert!(debug_str.contains("FailoverRecoveryCoordinator"));
        assert!(debug_str.contains("recovery_count"));
    }

    #[tokio::test]
    async fn test_linkage_error_equality() {
        assert_eq!(
            LinkageError::SwitchNotCompleted,
            LinkageError::SwitchNotCompleted
        );
        assert_eq!(
            LinkageError::OldMasterNotEvicted,
            LinkageError::OldMasterNotEvicted
        );
        assert_ne!(
            LinkageError::SwitchNotCompleted,
            LinkageError::OldMasterNotEvicted
        );
        assert_ne!(
            LinkageError::SwitchNotCompleted,
            LinkageError::SequenceViolation
        );
    }
}
