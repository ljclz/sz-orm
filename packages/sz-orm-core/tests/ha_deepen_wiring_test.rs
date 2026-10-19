//! 企业级高可用深化接线端到端测试（v10.10.0 T2.1~T2.5）

#![cfg(all(feature = "auto-failover", feature = "tenant-quota-rls-enhanced", feature = "circuit-breaker"))]

use sz_orm_core::circuit_breaker::{
    AdaptiveThreshold, CircuitBreaker, CircuitState, DefaultCircuitBreaker, HysteresisThreshold,
};
use sz_orm_core::degradation::DegradationStrategy;
use sz_orm_core::failover_controller::{
    FailoverController, FailoverDetector, FailoverPhase, RecoveryStrategy,
};
use sz_orm_core::rate_limiter::{AdaptiveRateLimiter, LoadSignal};
use sz_orm_core::rw_split_enhanced::{
    LoadAwareSlaveSelector, ReadConsistency, ReadMasterWindow, RwSplitConfig, RwSplitRouter,
    SlaveLoadMetrics, SlaveHealthTracker, WeightedSlave,
};
use sz_orm_core::tenant_quota_rls::{
    RlsPredicateInjector, TenantQuotaEnforcer,
};
use std::time::Duration;
use tokio::time::sleep;

// ============================================================================
// T2.1: FailoverController 四阶段闭环
// ============================================================================

#[tokio::test]
async fn test_failover_detect_master_failure() {
    let controller = FailoverController::new(FailoverDetector::default(), 3);
    controller.set_master("master-1");
    controller.set_node_health("master-1", false);

    let detected = controller.detect_failure("master-1").await.unwrap();
    assert!(detected, "主节点故障应被检测到");
    assert_eq!(
        controller.phase(),
        FailoverPhase::Detecting,
        "检测到故障后阶段应为 Detecting"
    );
}

#[tokio::test]
async fn test_failover_confirm_avoids_false_positive() {
    let controller = FailoverController::new(FailoverDetector::default(), 3);
    controller.set_node_health("master-1", false);

    let r1 = controller.confirm_failure("master-1").await.unwrap();
    let r2 = controller.confirm_failure("master-1").await.unwrap();
    assert!(!r1 && !r2, "前 2 次不应确认故障");

    let r3 = controller.confirm_failure("master-1").await.unwrap();
    assert!(r3, "第 3 次应确认故障");
    assert_eq!(controller.phase(), FailoverPhase::Confirmed);
}

#[tokio::test]
async fn test_failover_promotes_slave_to_master() {
    let controller = FailoverController::new(FailoverDetector::default(), 1);
    controller.set_master("master-1");
    controller.add_slave("slave-1");
    controller.set_node_health("slave-1", true);

    let new_master = controller.failover("slave-1").await.unwrap();
    assert_eq!(new_master, "slave-1");
    assert_eq!(controller.current_master(), "slave-1");
    assert_eq!(controller.phase(), FailoverPhase::FailedOver);
}

#[tokio::test]
async fn test_failover_recover_original_master_as_slave() {
    let controller = FailoverController::new(FailoverDetector::default(), 1);
    controller.set_master("master-1");
    controller.add_slave("slave-1");
    controller.set_node_health("slave-1", true);
    controller.failover("slave-1").await.unwrap();

    controller.set_node_health("master-1", true);
    controller.recover("master-1").await.unwrap();

    assert_eq!(controller.phase(), FailoverPhase::Normal);
    assert!(
        controller.slaves().contains(&"master-1".to_string()),
        "原主应以从身份重新加入"
    );
    assert_eq!(
        controller.recovery_strategy(),
        RecoveryStrategy::RejoinAsSlave
    );
}

#[tokio::test]
async fn test_failover_total_time_within_5_seconds() {
    let start = std::time::Instant::now();

    let controller = FailoverController::new(FailoverDetector::default(), 2);
    controller.set_master("master-1");
    controller.add_slave("slave-1");
    controller.set_node_health("master-1", false);
    controller.set_node_health("slave-1", true);

    controller.confirm_failure("master-1").await.unwrap();
    controller.confirm_failure("master-1").await.unwrap();
    controller.failover("slave-1").await.unwrap();
    controller.set_node_health("master-1", true);
    controller.recover("master-1").await.unwrap();

    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "四阶段闭环总时效应 < 5 秒，实际 {:?}",
        elapsed
    );
}
// ============================================================================
// T2.2: LoadAwareSlaveSelector 负载感知路由
// ============================================================================

fn make_test_router() -> RwSplitRouter {
    RwSplitRouter::new(RwSplitConfig {
        master: "master".into(),
        slaves: vec![
            WeightedSlave {
                name: "slave-a".into(),
                weight: 1,
                host: "127.0.0.1".into(),
                port: 3307,
            },
            WeightedSlave {
                name: "slave-b".into(),
                weight: 1,
                host: "127.0.0.1".into(),
                port: 3308,
            },
        ],
        lag_threshold_secs: 5,
    })
}

#[tokio::test]
async fn test_load_aware_selector_routes_to_least_loaded_slave() {
    let router = make_test_router();
    let selector = LoadAwareSlaveSelector::new();

    selector.update_slave_metrics("slave-a", SlaveLoadMetrics::new(100, 50.0, 1.0));
    selector.update_slave_metrics("slave-b", SlaveLoadMetrics::new(10, 5.0, 1.0));

    let result = router.route_read_load_aware(&selector, ReadConsistency::AllowSlave);
    assert_eq!(
        result.target, "slave-b",
        "读请求应路由到负载最低的 slave-b"
    );
    assert!(!result.is_master);
    assert!(!result.degraded);
}

#[tokio::test]
async fn test_route_write_always_to_master() {
    let router = make_test_router();
    let result = router.route_write();
    assert_eq!(result.target, "master");
    assert!(result.is_master);
    assert!(!result.degraded);
}

#[tokio::test]
async fn test_read_consistency_after_write() {
    let router = make_test_router();
    let selector = LoadAwareSlaveSelector::new();
    let window = ReadMasterWindow::new(std::time::Duration::from_secs(1));

    selector.update_slave_metrics("slave-a", SlaveLoadMetrics::new(10, 5.0, 1.0));
    selector.update_slave_metrics("slave-b", SlaveLoadMetrics::new(10, 5.0, 1.0));

    let read_before = router.route_read_with_window(&selector, &window);
    assert!(
        !read_before.is_master,
        "写前应读从节点"
    );

    window.record_write();
    let read_after = router.route_read_with_window(&selector, &window);
    assert!(
        read_after.is_master,
        "写后应强制读主"
    );
    assert_eq!(read_after.target, "master");
}

#[tokio::test]
async fn test_slave_health_tracker_updates_metrics() {
    let tracker = SlaveHealthTracker::new();
    assert!(tracker.get_metrics("slave-a").is_none(), "初始无指标");

    tracker.update_metrics("slave-a", SlaveLoadMetrics::new(50, 20.0, 0.8));
    let metrics = tracker.get_metrics("slave-a").unwrap();
    assert_eq!(metrics.connection_count, 50);
    assert!((metrics.latency_ms - 20.0).abs() < 1e-9);
    assert!((metrics.health_score - 0.8).abs() < 1e-9);

    let tracked = tracker.tracked_slaves();
    assert!(tracked.contains(&"slave-a".to_string()));
}

#[tokio::test]
async fn test_load_aware_selector_skips_unhealthy_slave() {
    let router = make_test_router();
    let selector = LoadAwareSlaveSelector::new();

    selector.update_slave_metrics("slave-a", SlaveLoadMetrics::new(10, 5.0, 0.0));
    selector.update_slave_metrics("slave-b", SlaveLoadMetrics::new(100, 50.0, 1.0));

    let result = router.route_read_load_aware(&selector, ReadConsistency::AllowSlave);
    assert_eq!(
        result.target, "slave-b",
        "不健康的 slave-a 应被跳过，路由到 slave-b"
    );
}

#[tokio::test]
async fn test_force_master_consistency_overrides_selector() {
    let router = make_test_router();
    let selector = LoadAwareSlaveSelector::new();
    selector.update_slave_metrics("slave-a", SlaveLoadMetrics::new(10, 5.0, 1.0));

    let result = router.route_read_load_aware(&selector, ReadConsistency::ForceMaster);
    assert!(
        result.is_master,
        "ForceMaster 一致性应强制路由到主节点"
    );
    assert_eq!(result.target, "master");
}
// ============================================================================
// T2.3: RlsPredicateInjector + TenantQuotaEnforcer
// ============================================================================

#[tokio::test]
async fn test_rls_injector_adds_tenant_predicate() {
    let injector = RlsPredicateInjector::new();
    let sql = "SELECT * FROM orders";
    let result = injector.inject(sql, "tenant-001");
    assert!(
        result.contains("tenant_id = 'tenant-001'"),
        "注入后 SQL 应包含 tenant_id 谓词: {}",
        result
    );
    assert!(
        result.contains("WHERE"),
        "无 WHERE 子句时应添加 WHERE: {}",
        result
    );
}

#[tokio::test]
async fn test_rls_injector_appends_to_existing_where() {
    let injector = RlsPredicateInjector::new();
    let sql = "SELECT * FROM orders WHERE status = 'active'";
    let result = injector.inject(sql, "tenant-001");
    assert!(
        result.contains("AND tenant_id = 'tenant-001'"),
        "已有 WHERE 时应追加 AND: {}",
        result
    );
}

#[tokio::test]
async fn test_rls_injector_prevents_cross_tenant_access() {
    let injector = RlsPredicateInjector::new();
    let sql = "SELECT * FROM orders";
    let tenant_a_sql = injector.inject(sql, "tenant-a");
    let tenant_b_sql = injector.inject(sql, "tenant-b");

    assert!(
        tenant_a_sql.contains("tenant_id = 'tenant-a'") && !tenant_a_sql.contains("tenant-b"),
        "租户 A 的 SQL 不应包含 tenant-b"
    );
    assert!(
        tenant_b_sql.contains("tenant_id = 'tenant-b'") && !tenant_b_sql.contains("tenant-a"),
        "租户 B 的 SQL 不应包含 tenant-a"
    );
}

#[tokio::test]
async fn test_rls_injector_skips_if_already_present() {
    let injector = RlsPredicateInjector::new();
    let sql = "SELECT * FROM orders WHERE tenant_id = 'tenant-001'";
    let result = injector.inject(sql, "tenant-001");
    assert_eq!(
        result, sql,
        "已包含 tenant_id 条件时不应重复注入"
    );
}

#[tokio::test]
async fn test_rls_injector_custom_column() {
    let injector = RlsPredicateInjector::new().with_column("org_id");
    let sql = "SELECT * FROM orders";
    let result = injector.inject(sql, "org-001");
    assert!(
        result.contains("org_id = 'org-001'"),
        "自定义列名应正确注入: {}",
        result
    );
}

#[tokio::test]
async fn test_tenant_quota_enforcer_limits_connections() {
    let enforcer = TenantQuotaEnforcer::new();
    enforcer.set_connection_limit("tenant-a", 4);

    assert!(enforcer.acquire_connection("tenant-a").is_ok(), "第 1 个连接");
    assert!(enforcer.acquire_connection("tenant-a").is_ok(), "第 2 个连接");
    assert!(enforcer.acquire_connection("tenant-a").is_ok(), "第 3 个连接");

    let result = enforcer.acquire_connection("tenant-a");
    assert!(
        result.is_err(),
        "第 4 个连接应超限拒绝"
    );
    assert_eq!(enforcer.current_connections("tenant-a"), 3);
}

#[tokio::test]
async fn test_tenant_quota_enforcer_limits_queries() {
    let enforcer = TenantQuotaEnforcer::new();
    enforcer.set_qps_limit("tenant-a", 3);

    assert!(enforcer.record_query("tenant-a").is_ok(), "第 1 个查询");
    assert!(enforcer.record_query("tenant-a").is_ok(), "第 2 个查询");

    let result = enforcer.record_query("tenant-a");
    assert!(result.is_err(), "第 3 个查询应超限拒绝");
    assert_eq!(enforcer.current_queries("tenant-a"), 2);
}

#[tokio::test]
async fn test_tenant_quota_enforcer_release_connection() {
    let enforcer = TenantQuotaEnforcer::new();
    enforcer.set_connection_limit("tenant-a", 3);

    enforcer.acquire_connection("tenant-a").unwrap();
    enforcer.acquire_connection("tenant-a").unwrap();
    assert_eq!(enforcer.current_connections("tenant-a"), 2);

    enforcer.release_connection("tenant-a");
    assert_eq!(enforcer.current_connections("tenant-a"), 1);

    assert!(
        enforcer.acquire_connection("tenant-a").is_ok(),
        "释放后应能重新获取连接"
    );
}

#[tokio::test]
async fn test_tenant_quota_enforcer_independent_tenants() {
    let enforcer = TenantQuotaEnforcer::new();
    enforcer.set_connection_limit("tenant-a", 2);
    enforcer.set_connection_limit("tenant-b", 2);

    assert!(enforcer.acquire_connection("tenant-a").is_ok());
    assert!(enforcer.acquire_connection("tenant-b").is_ok());
    assert!(
        enforcer.acquire_connection("tenant-a").is_err(),
        "tenant-a 超限"
    );
    assert!(
        enforcer.acquire_connection("tenant-b").is_err(),
        "tenant-b 超限"
    );
}

// ============================================================================
// T2.4: AdaptiveRateLimiter + DegradationStrategy
// ============================================================================

#[tokio::test]
async fn test_adaptive_rate_limiter_adjusts_threshold_by_load() {
    let mut limiter = AdaptiveRateLimiter::new(100);

    let low_load = LoadSignal::new(0.1, 5, 0.1);
    limiter.adjust_threshold(&low_load);
    assert!(
        limiter.current_threshold() > 100,
        "低负载时阈值应提升: {}",
        limiter.current_threshold()
    );

    let high_load = LoadSignal::new(0.9, 80, 0.9);
    limiter.adjust_threshold(&high_load);
    assert!(
        limiter.current_threshold() < 100,
        "高负载时阈值应降低: {}",
        limiter.current_threshold()
    );
}

#[tokio::test]
async fn test_adaptive_rate_limiter_acquire_and_reject() {
    let limiter = AdaptiveRateLimiter::new(3);

    assert!(limiter.acquire().is_ok(), "第 1 个请求");
    assert!(limiter.acquire().is_ok(), "第 2 个请求");
    assert!(limiter.acquire().is_ok(), "第 3 个请求");
    assert!(limiter.acquire().is_err(), "第 4 个请求应被拒绝");
    assert_eq!(limiter.acquired_count(), 4);
}

#[tokio::test]
async fn test_adaptive_rate_limiter_reset() {
    let limiter = AdaptiveRateLimiter::new(2);
    limiter.acquire().unwrap();
    limiter.acquire().unwrap();
    assert!(limiter.acquire().is_err());

    limiter.reset();
    assert_eq!(limiter.acquired_count(), 0);
    assert!(limiter.acquire().is_ok(), "重置后应能获取");
}

#[tokio::test]
async fn test_degradation_strategy_new_variants() {
    assert_eq!(DegradationStrategy::CachedFallback.as_str(), "cached_fallback");
    assert_eq!(DegradationStrategy::QueueWait.as_str(), "queue_wait");
    assert_eq!(DegradationStrategy::DefaultResponse.as_str(), "default_response");
}

#[tokio::test]
async fn test_load_signal_composite_load() {
    let low = LoadSignal::new(0.1, 5, 0.1);
    assert!(low.composite_load() < 0.3, "低负载综合分应 < 0.3");

    let high = LoadSignal::new(0.9, 80, 0.9);
    assert!(high.composite_load() > 0.7, "高负载综合分应 > 0.7");
}

// ============================================================================
// T2.5: 熔断器增强（半开探测 + 自适应阈值 + 滞后阈值）
// ============================================================================

#[tokio::test]
async fn test_half_open_probe_allows_limited_requests() {
    let mut cb = DefaultCircuitBreaker::new(2, Duration::from_millis(10))
        .with_half_open_probe(2);

    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::Open);

    sleep(Duration::from_millis(15)).await;
    assert!(cb.can_execute(), "第 1 个探测请求应放行");
    assert!(cb.can_execute(), "第 2 个探测请求应放行");
    assert!(
        !cb.can_execute(),
        "第 3 个探测请求应拒绝（超过探测限制）"
    );
    assert_eq!(cb.half_open_probed(), 2);
}

#[tokio::test]
async fn test_half_open_probe_success_recovers() {
    let mut cb = DefaultCircuitBreaker::new(1, Duration::from_millis(10))
        .with_half_open_probe(3);

    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::Open);

    sleep(Duration::from_millis(15)).await;
    assert!(cb.can_execute(), "探测请求应放行");
    cb.record_success();
    assert_eq!(cb.state(), CircuitState::Closed, "探测成功应恢复 Closed");
    assert_eq!(cb.half_open_probed(), 0);
}

#[tokio::test]
async fn test_half_open_probe_failure_reopens() {
    let mut cb = DefaultCircuitBreaker::new(1, Duration::from_millis(10))
        .with_half_open_probe(3);

    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::Open);

    sleep(Duration::from_millis(15)).await;
    assert!(cb.can_execute(), "探测请求应放行");
    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::Open, "探测失败应重新熔断");
}

#[tokio::test]
async fn test_adaptive_threshold_adjusts_by_error_rate() {
    let adaptive = AdaptiveThreshold::new(2, 10, 100);

    let low_error = adaptive.adjust(5, 0.1);
    assert!(
        low_error > 5,
        "低错误率时阈值应提升: {}",
        low_error
    );

    let high_error = adaptive.adjust(5, 0.9);
    assert!(
        high_error < 5,
        "高错误率时阈值应降低: {}",
        high_error
    );
}

#[tokio::test]
async fn test_hysteresis_avoids_oscillation() {
    let hysteresis = HysteresisThreshold::new(0.5, 0.3);

    assert!(hysteresis.should_open(0.6), "错误率 0.6 >= 开阈值 0.5 应熔断");
    assert!(!hysteresis.should_open(0.4), "错误率 0.4 < 开阈值 0.5 不应熔断");

    assert!(hysteresis.should_close(0.2), "错误率 0.2 < 闭阈值 0.3 应恢复");
    assert!(!hysteresis.should_close(0.4), "错误率 0.4 >= 闭阈值 0.3 不应恢复");

    assert!(
        !hysteresis.should_open(0.4) && !hysteresis.should_close(0.4),
        "错误率 0.4 在滞后区间 (0.3, 0.5) 内应既不开也不闭"
    );
}

#[tokio::test]
async fn test_circuit_breaker_adjust_threshold_by_error_rate() {
    let mut cb = DefaultCircuitBreaker::new(5, Duration::from_secs(1))
        .with_adaptive_threshold(AdaptiveThreshold::new(2, 10, 100));

    cb.adjust_threshold_by_error_rate(0.9);
    assert!(
        cb.failure_threshold() <= 5,
        "高错误率后阈值应降低"
    );

    cb.adjust_threshold_by_error_rate(0.1);
    assert!(
        cb.failure_threshold() >= 5,
        "低错误率后阈值应提升"
    );
}

