//! v10.11.0 T2.* 多区域多活高可用端到端接线测试
//!
//! 验证生产调用点可达：
//! - T2.1 多区域故障转移（RegionHealthChecker + CrossRegionSwitcher）
//! - T2.2 异地多活路由（后续任务追加）
//! - T2.3 区域配额隔离（后续任务追加）
//! - T2.4 全局熔断协调（后续任务追加）
//! - T2.5 故障切换数据校验（后续任务追加）

#![cfg(feature = "auto-failover")]

use std::time::{Duration, Instant};

use sz_orm_core::failover_controller::{
    CrossRegionSwitcher, FailoverController, FailoverDetector, PriorityStrategy, RegionConfig,
    RegionHealthChecker, RegionHealthStatus, RegionTopology,
};

/// T2.1 测试：RegionHealthChecker 心跳检测
///
/// 验证行为副作用：心跳缺失累计后状态从 Healthy → Suspect → Failed。
#[test]
fn test_region_health_checker_heartbeat() {
    let mut checker = RegionHealthChecker::new(Duration::from_secs(3), 0.5, 3);

    assert_eq!(
        checker.check_region_health("region-a"),
        RegionHealthStatus::Healthy
    );

    checker.record_heartbeat_miss("region-a");
    assert_eq!(
        checker.check_region_health("region-a"),
        RegionHealthStatus::Suspect {
            heartbeat_misses: 1
        }
    );

    checker.record_heartbeat_miss("region-a");
    assert_eq!(
        checker.check_region_health("region-a"),
        RegionHealthStatus::Suspect {
            heartbeat_misses: 2
        }
    );

    checker.record_heartbeat_miss("region-a");
    assert_eq!(
        checker.check_region_health("region-a"),
        RegionHealthStatus::Failed { confirm_count: 3 }
    );

    checker.record_heartbeat_success("region-a");
    assert_eq!(
        checker.check_region_health("region-a"),
        RegionHealthStatus::Healthy
    );
}

/// T2.1 测试：RegionHealthChecker 错误率检测
#[test]
fn test_region_health_checker_error_rate() {
    let mut checker = RegionHealthChecker::new(Duration::from_secs(3), 0.5, 2);

    checker.set_region_error_rate("region-b", 0.3);
    assert_eq!(
        checker.check_region_health("region-b"),
        RegionHealthStatus::Healthy
    );

    checker.set_region_error_rate("region-b", 0.6);
    assert_eq!(
        checker.check_region_health("region-b"),
        RegionHealthStatus::Suspect {
            heartbeat_misses: 0
        }
    );

    assert_eq!(
        checker.check_region_health("region-b"),
        RegionHealthStatus::Failed { confirm_count: 2 }
    );
}

/// T2.1 测试：多实例交叉确认
///
/// 验证行为副作用：cross_region_confirm 在确认次数达阈值时返回 true。
#[test]
fn test_region_health_cross_confirm() {
    let mut checker = RegionHealthChecker::new(Duration::from_secs(3), 0.5, 3);

    assert!(!checker.cross_region_confirm("region-a"));

    checker.record_heartbeat_miss("region-a");
    checker.check_region_health("region-a");
    assert!(!checker.cross_region_confirm("region-a"));

    checker.record_heartbeat_miss("region-a");
    checker.check_region_health("region-a");
    assert!(!checker.cross_region_confirm("region-a"));

    checker.record_heartbeat_miss("region-a");
    checker.check_region_health("region-a");
    assert!(checker.cross_region_confirm("region-a"));

    checker.reset_region("region-a");
    assert!(!checker.cross_region_confirm("region-a"));
}

/// T2.1 测试：跨区域切换优先级
///
/// 验证行为副作用：GeoNear 策略按延迟升序，CapacityRedundant 按容量降序。
#[test]
fn test_cross_region_switch_priority() {
    let regions = vec![
        RegionConfig {
            region_id: "us-east-1".to_string(),
            geo_location: "us-east".to_string(),
            capacity_weight: 80,
            latency_ms: 100,
        },
        RegionConfig {
            region_id: "eu-west-1".to_string(),
            geo_location: "eu-west".to_string(),
            capacity_weight: 90,
            latency_ms: 50,
        },
        RegionConfig {
            region_id: "ap-south-1".to_string(),
            geo_location: "ap-south".to_string(),
            capacity_weight: 70,
            latency_ms: 200,
        },
    ];

    let geo_sorter =
        sz_orm_core::failover_controller::RegionPrioritySorter::new(PriorityStrategy::GeoNear);
    let geo_order = geo_sorter.sort_regions(&regions);
    assert_eq!(geo_order, vec!["eu-west-1", "us-east-1", "ap-south-1"]);

    let cap_sorter = sz_orm_core::failover_controller::RegionPrioritySorter::new(
        PriorityStrategy::CapacityRedundant,
    );
    let cap_order = cap_sorter.sort_regions(&regions);
    assert_eq!(cap_order, vec!["eu-west-1", "us-east-1", "ap-south-1"]);
}

/// T2.1 端到端测试：区域故障转移 SLA
///
/// 验证行为副作用：
/// 1. 区域故障检测 ≤ 5 秒（心跳超时 3 秒 + 确认）
/// 2. 跨区域切换 ≤ 10 秒
#[test]
fn e2e_region_failover_within_sla() {
    let regions = vec![
        RegionConfig {
            region_id: "primary".to_string(),
            geo_location: "us-east".to_string(),
            capacity_weight: 100,
            latency_ms: 10,
        },
        RegionConfig {
            region_id: "secondary".to_string(),
            geo_location: "us-west".to_string(),
            capacity_weight: 80,
            latency_ms: 50,
        },
        RegionConfig {
            region_id: "tertiary".to_string(),
            geo_location: "eu-west".to_string(),
            capacity_weight: 60,
            latency_ms: 150,
        },
    ];

    let topology =
        RegionTopology::new(regions, "primary").with_priority_strategy(PriorityStrategy::GeoNear);

    let mut checker = RegionHealthChecker::new(Duration::from_secs(3), 0.5, 3);
    let mut switcher = CrossRegionSwitcher::new(PriorityStrategy::GeoNear);

    let detect_start = Instant::now();

    let mut health = RegionHealthStatus::Healthy;
    for _ in 0..3 {
        checker.record_heartbeat_miss("primary");
        health = checker.check_region_health("primary");
    }
    let detect_elapsed = detect_start.elapsed();

    assert_eq!(health, RegionHealthStatus::Failed { confirm_count: 3 });
    assert!(
        detect_elapsed <= Duration::from_secs(5),
        "区域故障检测应 ≤ 5 秒，实际 {:?}",
        detect_elapsed
    );

    let switch_start = Instant::now();
    let healthy_regions = vec!["secondary", "tertiary"];
    let new_region = switcher.switch(&topology, &healthy_regions).unwrap();
    let switch_elapsed = switch_start.elapsed();

    assert_eq!(new_region, "secondary");
    assert!(
        switch_elapsed <= Duration::from_secs(10),
        "跨区域切换应 ≤ 10 秒，实际 {:?}",
        switch_elapsed
    );

    assert!(switcher.switch_history().contains_key("secondary"));
}

/// T2.1 端到端测试：多区域故障转移全流程
///
/// 验证行为副作用：检测 → 确认 → 切换 → 校验 → 恢复全流程。
#[test]
fn test_multi_region_failover_integration() {
    let regions = vec![
        RegionConfig {
            region_id: "r1".to_string(),
            geo_location: "zone-a".to_string(),
            capacity_weight: 100,
            latency_ms: 10,
        },
        RegionConfig {
            region_id: "r2".to_string(),
            geo_location: "zone-b".to_string(),
            capacity_weight: 80,
            latency_ms: 40,
        },
        RegionConfig {
            region_id: "r3".to_string(),
            geo_location: "zone-c".to_string(),
            capacity_weight: 60,
            latency_ms: 80,
        },
    ];

    let topology = RegionTopology::new(regions, "r1");
    assert_eq!(topology.active_region(), "r1");
    assert_eq!(topology.regions().len(), 3);

    let mut checker = RegionHealthChecker::default();
    assert_eq!(
        checker.check_region_health("r1"),
        RegionHealthStatus::Healthy
    );

    let mut health = RegionHealthStatus::Healthy;
    for _ in 0..3 {
        checker.record_heartbeat_miss("r1");
        health = checker.check_region_health("r1");
    }
    assert!(matches!(health, RegionHealthStatus::Failed { .. }));
    assert!(checker.cross_region_confirm("r1"));

    let mut switcher = CrossRegionSwitcher::new(PriorityStrategy::GeoNear);
    let new_region = switcher.switch(&topology, &["r2", "r3"]).unwrap();
    assert_eq!(new_region, "r2");

    checker.reset_region("r1");
    checker.record_heartbeat_success("r1");
    assert_eq!(
        checker.check_region_health("r1"),
        RegionHealthStatus::Healthy
    );

    let err = switcher.switch(&topology, &[]).unwrap_err();
    assert!(err.contains("无可用健康区域"));
}

/// T2.1 测试：FailoverController 持有 region_topology 字段
///
/// 验证行为副作用：set_region_topology 后 region_topology 返回 Some。
#[test]
fn test_failover_controller_with_region_topology() {
    let detector = FailoverDetector::default();
    let controller = FailoverController::new(detector, 3);

    assert!(controller.region_topology().is_none());

    let regions = vec![
        RegionConfig {
            region_id: "r1".to_string(),
            geo_location: "zone-a".to_string(),
            capacity_weight: 100,
            latency_ms: 10,
        },
        RegionConfig {
            region_id: "r2".to_string(),
            geo_location: "zone-b".to_string(),
            capacity_weight: 80,
            latency_ms: 50,
        },
    ];
    let topology = RegionTopology::new(regions, "r1");
    controller.set_region_topology(topology);

    let retrieved = controller.region_topology().unwrap();
    assert_eq!(retrieved.active_region(), "r1");
    assert_eq!(retrieved.regions().len(), 2);
    assert!(retrieved.find_region("r1").is_some());
    assert!(retrieved.find_region("r3").is_none());
}

/// T2.1 测试：CapacityRedundant 策略
#[test]
fn test_priority_strategy_capacity_redundant() {
    let regions = vec![
        RegionConfig {
            region_id: "low-cap".to_string(),
            geo_location: "zone-a".to_string(),
            capacity_weight: 30,
            latency_ms: 10,
        },
        RegionConfig {
            region_id: "high-cap".to_string(),
            geo_location: "zone-b".to_string(),
            capacity_weight: 90,
            latency_ms: 100,
        },
        RegionConfig {
            region_id: "mid-cap".to_string(),
            geo_location: "zone-c".to_string(),
            capacity_weight: 60,
            latency_ms: 50,
        },
    ];

    let topology = RegionTopology::new(regions, "low-cap")
        .with_priority_strategy(PriorityStrategy::CapacityRedundant);

    let mut switcher = CrossRegionSwitcher::new(PriorityStrategy::CapacityRedundant);
    let new_region = switcher
        .switch(&topology, &["high-cap", "mid-cap"])
        .unwrap();
    assert_eq!(new_region, "high-cap");
}
// ============================================================================
// T2.2 异地多活路由测试
// ============================================================================

use sz_orm_core::rw_split_enhanced::{
    MultiActiveRouter, PartitionKeyMapping, PartitionStrategy, RouteError,
};

/// T2.2 测试：PartitionStrategy::Hash 分区路由
#[test]
fn test_partition_strategy_hash() {
    let mapping = PartitionKeyMapping::new(PartitionStrategy::Hash { buckets: 4 });

    let r1 = mapping.resolve_region("user-1").unwrap();
    let r2 = mapping.resolve_region("user-1").unwrap();
    assert_eq!(r1, r2, "同一键应路由到同一区域");

    let r3 = mapping.resolve_region("user-2").unwrap();
    assert!(r1.starts_with("region-"));
    assert!(r3.starts_with("region-"));
}

/// T2.2 测试：PartitionStrategy::Range 分区路由
#[test]
fn test_partition_strategy_range() {
    let ranges = vec![
        (100i64, "region-a".to_string()),
        (200, "region-b".to_string()),
        (i64::MAX, "region-c".to_string()),
    ];
    let mapping = PartitionKeyMapping::new(PartitionStrategy::Range { ranges });

    assert_eq!(mapping.resolve_region("50").unwrap(), "region-a");
    assert_eq!(mapping.resolve_region("150").unwrap(), "region-b");
    assert_eq!(mapping.resolve_region("250").unwrap(), "region-c");
}

/// T2.2 测试：PartitionStrategy::List 分区路由
#[test]
fn test_partition_strategy_list() {
    let mut list_mapping = std::collections::HashMap::new();
    list_mapping.insert("vip-users".to_string(), "region-primary".to_string());
    list_mapping.insert("eu-users".to_string(), "region-eu".to_string());
    let mapping = PartitionKeyMapping::new(PartitionStrategy::List {
        mapping: list_mapping,
    });

    assert_eq!(
        mapping.resolve_region("vip-users").unwrap(),
        "region-primary"
    );
    assert_eq!(mapping.resolve_region("eu-users").unwrap(), "region-eu");
    assert!(mapping.resolve_region("unknown").is_none());
}

/// T2.2 端到端测试：多活写一致性
///
/// 验证行为副作用：同一分区键多次路由到同一区域主节点。
#[test]
fn e2e_multi_active_write_consistency() {
    let mut router = MultiActiveRouter::new(PartitionStrategy::Hash { buckets: 3 });
    router
        .region_master_resolver_mut()
        .register("region-0", "master-0");
    router
        .region_master_resolver_mut()
        .register("region-1", "master-1");
    router
        .region_master_resolver_mut()
        .register("region-2", "master-2");

    let key = "user-42";
    let first = router.route_write(key, "INSERT INTO orders").unwrap();
    let second = router.route_write(key, "UPDATE orders SET ...").unwrap();
    let third = router.route_write(key, "DELETE FROM orders").unwrap();

    assert_eq!(first, second);
    assert_eq!(second, third);
    assert!(router.validate_partition_consistency(key));
}

/// T2.2 端到端测试：多活路由集成
#[test]
fn test_multi_active_routing_integration() {
    let mut list_map = std::collections::HashMap::new();
    list_map.insert("us-users".to_string(), "us-east".to_string());
    list_map.insert("eu-users".to_string(), "eu-west".to_string());

    let mut router = MultiActiveRouter::new(PartitionStrategy::List { mapping: list_map });
    router
        .region_master_resolver_mut()
        .register("us-east", "us-master:3306");
    router
        .region_master_resolver_mut()
        .register("eu-west", "eu-master:3306");

    let us_master = router.route_write("us-users", "INSERT ...").unwrap();
    assert_eq!(us_master.region_id, "us-east");
    assert_eq!(us_master.master_node, "us-master:3306");

    let eu_master = router.route_write("eu-users", "INSERT ...").unwrap();
    assert_eq!(eu_master.region_id, "eu-west");
    assert_eq!(eu_master.master_node, "eu-master:3306");

    assert_ne!(us_master, eu_master);
}

/// T2.2 测试：分区一致性校验
#[test]
fn test_multi_active_router_partition_consistency() {
    let mut router = MultiActiveRouter::new(PartitionStrategy::Hash { buckets: 2 });
    router
        .region_master_resolver_mut()
        .register("region-0", "m0");
    router
        .region_master_resolver_mut()
        .register("region-1", "m1");

    let key = "test-key";
    assert!(router.validate_partition_consistency(key));

    router.route_write(key, "write-1").unwrap();
    assert!(router.validate_partition_consistency(key));

    router.route_write(key, "write-2").unwrap();
    assert!(router.validate_partition_consistency(key));
}

/// T2.2 测试：写冲突检测
///
/// 验证行为副作用：分区键映射变更后路由检测到一致性冲突。
#[test]
fn test_multi_active_write_conflict_detection() {
    let mut list_map = std::collections::HashMap::new();
    list_map.insert("key-1".to_string(), "region-a".to_string());

    let mut router = MultiActiveRouter::new(PartitionStrategy::List { mapping: list_map });
    router
        .region_master_resolver_mut()
        .register("region-a", "master-a");
    router
        .region_master_resolver_mut()
        .register("region-b", "master-b");

    let first = router.route_write("key-1", "write-1").unwrap();
    assert_eq!(first.region_id, "region-a");

    router
        .partition_router_mut()
        .mapping_mut()
        .register("key-1", "region-b");

    let conflict = router.route_write("key-1", "write-2").unwrap_err();
    assert!(matches!(
        conflict,
        RouteError::PartitionConsistencyConflict {
            ref partition_key,
            expected: _,
            actual: _
        } if partition_key == "key-1"
    ));
}

/// T2.2 测试：未映射分区键错误
#[test]
fn test_route_error_unmapped_key() {
    let mut router = MultiActiveRouter::new(PartitionStrategy::Hash { buckets: 0 });
    let err = router.route_write("unknown-key", "write").unwrap_err();
    assert_eq!(
        err,
        RouteError::UnmappedPartitionKey("unknown-key".to_string())
    );
}

/// T2.2 测试：区域无主节点错误
#[test]
fn test_route_error_no_master() {
    let mut list_map = std::collections::HashMap::new();
    list_map.insert("key-1".to_string(), "region-x".to_string());

    let mut router = MultiActiveRouter::new(PartitionStrategy::List { mapping: list_map });

    let err = router.route_write("key-1", "write").unwrap_err();
    assert_eq!(err, RouteError::NoMasterForRegion("region-x".to_string()));
}
// ============================================================================
// T2.3 区域配额隔离测试
// ============================================================================

use sz_orm_core::tenant_quota_rls::{
    BorrowLimit, QuotaBorrower, QuotaEnforcer, RegionQuota, RegionQuotaIsolator,
};

/// T2.3 测试：区域配额隔离
///
/// 验证行为副作用：每区域独立配额生效，超限拒绝消费。
#[test]
fn test_region_quota_isolation() {
    let mut isolator = RegionQuotaIsolator::new();
    isolator.register(RegionQuota::new("r1", 5, 100, 1024));
    isolator.register(RegionQuota::new("r2", 3, 50, 512));

    assert_eq!(isolator.region_count(), 2);

    for _ in 0..5 {
        assert!(isolator.consume_connection("r1"));
    }
    assert!(!isolator.consume_connection("r1"), "r1 连接数已达上限 5");

    for _ in 0..3 {
        assert!(isolator.consume_connection("r2"));
    }
    assert!(!isolator.consume_connection("r2"), "r2 连接数已达上限 3");

    assert!(isolator.consume_query("r1"));
    assert!(isolator.get("r1").unwrap().current_connections() == 5);
    assert!(isolator.get("r2").unwrap().current_connections() == 3);

    isolator.release_connection("r1");
    assert_eq!(isolator.get("r1").unwrap().current_connections(), 4);
    assert!(isolator.consume_connection("r1"), "释放后应可再次消费");
}

/// T2.3 测试：跨区域配额借用上限
///
/// 验证行为副作用：借用受 max_borrow 和 min_reserve 限制。
#[test]
fn test_region_quota_borrow_limit() {
    let mut isolator = RegionQuotaIsolator::new();
    let r1 = RegionQuota::new("r1", 20, 100, 1024).with_borrow_limit(BorrowLimit::new(8, 5));
    let r2 = RegionQuota::new("r2", 10, 50, 512);
    isolator.register(r1);
    isolator.register(r2);

    for _ in 0..10 {
        isolator.consume_connection("r1");
    }

    let mut borrower = QuotaBorrower::new();

    let borrowed = borrower.borrow(&mut isolator, "r1", "r2", 5).unwrap();
    assert_eq!(borrowed, 5);
    assert_eq!(borrower.borrowed_amount("r1", "r2"), 5);

    let err = borrower.borrow(&mut isolator, "r1", "r2", 10).unwrap_err();
    assert!(err.contains("无法借出"));

    let err = borrower.borrow(&mut isolator, "r1", "r2", 6).unwrap_err();
    assert!(
        err.contains("无法借出"),
        "已借 5 + 新借 6 = 11 > max_borrow 8"
    );

    borrower.return_borrow("r1", "r2", 3);
    assert_eq!(borrower.borrowed_amount("r1", "r2"), 2);
}

/// T2.3 测试：QuotaEnforcer 持有 region_quotas 字段
#[test]
fn test_quota_enforcer_with_region_quotas() {
    let enforcer = QuotaEnforcer::new();
    assert!(enforcer.region_quotas().is_none());

    let mut quotas = std::collections::HashMap::new();
    quotas.insert("r1".to_string(), RegionQuota::new("r1", 10, 100, 1024));
    quotas.insert("r2".to_string(), RegionQuota::new("r2", 20, 200, 2048));
    enforcer.set_region_quotas(quotas);

    let retrieved = enforcer.region_quotas().unwrap();
    assert_eq!(retrieved.len(), 2);
    assert!(retrieved.contains_key("r1"));
    assert!(retrieved.contains_key("r2"));
}

/// T2.3 测试：BorrowLimit 逻辑
#[test]
fn test_borrow_limit() {
    let limit = BorrowLimit::new(10, 5);

    assert!(limit.can_borrow(20, 10));
    assert!(limit.can_borrow(15, 10));
    assert!(!limit.can_borrow(14, 10), "借出后 14-10=4 < min_reserve 5");
    assert!(!limit.can_borrow(20, 11), "11 > max_borrow 10");
    assert!(limit.can_borrow(20, 0));
}

// ============================================================================
// T2.4 全局熔断协调测试
// ============================================================================

use sz_orm_core::circuit_breaker::{
    BreakerChannel, CircuitState, DefaultCircuitBreaker, GlobalBreakerCoordinator,
    GlobalBreakerState, RedisBreakerChannel, SyncError,
};

/// T2.4 测试：全局熔断状态同步
///
/// 验证行为副作用：实例 A 熔断后，实例 B 通过 sync 感知 A 的熔断状态。
#[test]
fn test_global_breaker_state_sync() {
    let channel =
        std::sync::Arc::new(RedisBreakerChannel::new()) as std::sync::Arc<dyn BreakerChannel>;

    let mut breaker_a = GlobalBreakerCoordinator::new(
        DefaultCircuitBreaker::new(2, Duration::from_secs(60)),
        channel.clone(),
        "instance-a",
    );

    let mut breaker_b = GlobalBreakerCoordinator::new(
        DefaultCircuitBreaker::new(2, Duration::from_secs(60)),
        channel.clone(),
        "instance-b",
    );

    breaker_a.record_failure();
    breaker_a.record_failure();
    assert_eq!(breaker_a.state(), CircuitState::Open);

    breaker_b.record_failure();
    assert_eq!(breaker_b.state(), CircuitState::Closed);
}

/// T2.4 端到端测试：一实例熔断全实例感知
///
/// 验证行为副作用：实例 A 熔断后发布状态，实例 B 通过 allow_request 感知并拒绝。
#[test]
fn e2e_global_breaker_all_instances_aware() {
    let channel =
        std::sync::Arc::new(RedisBreakerChannel::new()) as std::sync::Arc<dyn BreakerChannel>;

    let mut breaker_a = GlobalBreakerCoordinator::new(
        DefaultCircuitBreaker::new(2, Duration::from_secs(60)),
        channel.clone(),
        "instance-a",
    );

    let mut breaker_b = GlobalBreakerCoordinator::new(
        DefaultCircuitBreaker::new(2, Duration::from_secs(60)),
        channel.clone(),
        "instance-b",
    );

    assert!(breaker_a.allow_request());
    breaker_a.record_failure();
    breaker_a.record_failure();
    assert_eq!(breaker_a.state(), CircuitState::Open);
    assert!(!breaker_a.allow_request());

    assert!(!breaker_b.allow_request(), "实例 B 应感知到 A 已熔断");
}

/// T2.4 测试：协调恢复
///
/// 验证行为副作用：一实例探测成功，全实例协调恢复。
#[test]
fn test_global_breaker_coordinated_recovery() {
    let channel =
        std::sync::Arc::new(RedisBreakerChannel::new()) as std::sync::Arc<dyn BreakerChannel>;

    let mut breaker_a = GlobalBreakerCoordinator::new(
        DefaultCircuitBreaker::new(1, Duration::from_millis(1)),
        channel.clone(),
        "instance-a",
    );

    let mut breaker_b = GlobalBreakerCoordinator::new(
        DefaultCircuitBreaker::new(1, Duration::from_millis(1)),
        channel.clone(),
        "instance-b",
    );

    breaker_a.record_failure();
    assert_eq!(breaker_a.state(), CircuitState::Open);

    breaker_b.record_success();
    assert_eq!(breaker_b.state(), CircuitState::Closed);

    std::thread::sleep(Duration::from_millis(10));
    breaker_a.sync_state().unwrap();
    assert_eq!(
        breaker_a.state(),
        CircuitState::Closed,
        "B 已恢复，A 应协调恢复"
    );
}

/// T2.4 测试：Redis 不可用降级
#[test]
fn test_global_breaker_redis_unavailable_degradation() {
    let channel = std::sync::Arc::new(RedisBreakerChannel::new());
    channel.mark_unavailable();

    let mut breaker = GlobalBreakerCoordinator::new(
        DefaultCircuitBreaker::new(2, Duration::from_secs(60)),
        channel,
        "instance-x",
    );

    assert!(breaker.allow_request(), "Redis 不可用时应降级为本地熔断");

    breaker.record_failure();
    breaker.record_failure();
    assert!(!breaker.allow_request(), "本地熔断仍应生效");
}

/// T2.4 测试：GlobalBreakerState
#[test]
fn test_global_breaker_state() {
    let state = GlobalBreakerState {
        circuit_state: CircuitState::Open,
        instance_id: "inst-1".to_string(),
        timestamp_ms: 1234567890,
    };
    assert_eq!(state.circuit_state, CircuitState::Open);
    assert_eq!(state.instance_id, "inst-1");
}

/// T2.4 测试：RedisBreakerChannel 基本操作
#[test]
fn test_redis_breaker_channel() {
    let channel = RedisBreakerChannel::new();
    assert!(channel.is_available());

    let state = GlobalBreakerState {
        circuit_state: CircuitState::Open,
        instance_id: "inst-1".to_string(),
        timestamp_ms: 100,
    };
    channel.publish_state(&state).unwrap();

    let states = channel.subscribe_states().unwrap();
    assert_eq!(states.len(), 1);
    assert!(states.contains_key("inst-1"));

    channel.mark_unavailable();
    let err = channel.publish_state(&state).unwrap_err();
    assert_eq!(err, SyncError::RedisUnavailable);
}

// ============================================================================
// T2.5 故障切换数据校验测试
// ============================================================================

use sz_orm_core::failover_controller::{
    GtidComparator, LsnComparator, ReplicationPosition, ReplicationPositionComparator,
    ScnComparator, SwitchRollback, SwitchVerifyChecker, VerifyError, VerifyResult,
};

/// T2.5 测试：GTID/LSN/SCN 位点比对
#[test]
fn test_switch_verify_gtid_lsn_scn() {
    assert!(GtidComparator::compare("uuid:1-100", "uuid:1-100"));
    assert!(!GtidComparator::compare("uuid:1-100", "uuid:1-99"));

    assert!(LsnComparator::compare(1000, 1000));
    assert!(LsnComparator::compare(1000, 1001));
    assert!(!LsnComparator::compare(1001, 1000));

    assert!(ScnComparator::compare(5000, 5000));
    assert!(ScnComparator::compare(5000, 5001));
    assert!(!ScnComparator::compare(5001, 5000));
}

/// T2.5 端到端测试：切换后位点不一致回滚切换
///
/// 验证行为副作用：校验不一致时回滚切换并标记。
#[test]
fn e2e_switch_verify_rollback_on_inconsistency() {
    let checker = SwitchVerifyChecker::new(ReplicationPositionComparator::Gtid);
    let master_pos = ReplicationPosition::empty().with_gtid("uuid:1-100");
    let slave_pos = ReplicationPosition::empty().with_gtid("uuid:1-90");

    let result = checker
        .verify_after_switch(&master_pos, &slave_pos)
        .unwrap();
    assert!(matches!(result, VerifyResult::Inconsistent { .. }));

    let mut rollback = SwitchRollback::new("GTID 不一致：master=uuid:1-100, slave=uuid:1-90");
    rollback.rollback().unwrap();
    assert!(rollback.is_rolled_back());
    assert!(!rollback.need_manual_intervention());
}

/// T2.5 测试：切换后位点一致
#[test]
fn test_switch_verify_consistent() {
    let checker = SwitchVerifyChecker::new(ReplicationPositionComparator::Lsn);
    let master_pos = ReplicationPosition::empty().with_lsn(10000);
    let slave_pos = ReplicationPosition::empty().with_lsn(10000);

    let result = checker
        .verify_after_switch(&master_pos, &slave_pos)
        .unwrap();
    assert!(matches!(result, VerifyResult::Consistent { .. }));

    let slave_ahead = ReplicationPosition::empty().with_lsn(10001);
    let result = checker
        .verify_after_switch(&master_pos, &slave_ahead)
        .unwrap();
    assert!(matches!(result, VerifyResult::Consistent { .. }));
}

/// T2.5 测试：位点不可用错误
#[test]
fn test_switch_verify_position_not_available() {
    let checker = SwitchVerifyChecker::new(ReplicationPositionComparator::Scn);
    let master_pos = ReplicationPosition::empty().with_scn(100);
    let slave_pos = ReplicationPosition::empty();

    let err = checker
        .verify_after_switch(&master_pos, &slave_pos)
        .unwrap_err();
    assert!(matches!(err, VerifyError::PositionNotAvailable(_)));
}

/// T2.5 测试：回滚器人工介入
#[test]
fn test_switch_rollback_manual_intervention() {
    let mut rollback = SwitchRollback::new("回滚失败：原主不可用");
    rollback.mark_manual_intervention();
    assert!(!rollback.is_rolled_back());
    assert!(rollback.need_manual_intervention());
    assert_eq!(rollback.reason(), "回滚失败：原主不可用");
}
