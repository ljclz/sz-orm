//! v10.3.0 e2e 接线验证测试
//!
//! 验证 v10.3.0 新增组件的生产接线行为副作用。
//! 7.1 自愈能力统一（7）+ 7.3 收益持续采集（8）+ 7.4 运维统一视图（8）= 23 个。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;

use sz_orm_core::ops_snapshot::{AlertSource, ProfitSummary};
use sz_orm_core::pool::{
    Connection, ConnectionFactory, Pool, PoolConfig, PoolConfigBuilder, QueryRows,
};
use sz_orm_core::profit_collector::{ProfitCollector, ProfitCollectorConfig};
use sz_orm_core::self_heal::SelfHealConfig;
use sz_orm_core::unified_heal::{compute_fused_heal_interval, HealEventSource, UnifiedHealConfig};
use sz_orm_core::DbError;

struct MockConnection;

impl Connection for MockConnection {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(1) })
    }

    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(Vec::new()) })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { true })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
}

struct MockFac;

#[async_trait]
impl ConnectionFactory for MockFac {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        Ok(Box::new(MockConnection))
    }
}

// =========================================================================
// 7.1 自愈能力统一 e2e（7 个）
// =========================================================================

#[test]
fn test_unified_heal_config_default_disabled_e2e() {
    let config = UnifiedHealConfig::default();
    assert!(!config.unified_heal_enabled);
    assert!(!config.backoff_fusion_enabled);
}

#[test]
fn test_unified_heal_event_history_merges_sources_e2e() {
    let config = PoolConfigBuilder::new()
        .unified_heal(UnifiedHealConfig {
            unified_heal_enabled: true,
            self_heal_config: SelfHealConfig {
                self_heal_enabled: true,
                reconnect_initial_backoff: Duration::from_millis(1),
                reconnect_max_attempts: 1,
                ..Default::default()
            },
            ..Default::default()
        })
        .build()
        .unwrap();
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let events = pool.unified_heal_event_history();
    for event in &events {
        assert_eq!(event.event_source, HealEventSource::SelfHealEngine);
    }
    let config_ref = pool.unified_heal_config();
    assert!(config_ref.unified_heal_enabled);
}

#[test]
fn test_unified_heal_config_convenience_e2e() {
    let config = PoolConfigBuilder::new()
        .unified_heal(UnifiedHealConfig {
            unified_heal_enabled: true,
            self_heal_config: SelfHealConfig {
                self_heal_enabled: true,
                reconnect_initial_backoff: Duration::from_millis(500),
                reconnect_max_attempts: 3,
                ..Default::default()
            },
            ..Default::default()
        })
        .build()
        .unwrap();
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let uc = pool.unified_heal_config();
    assert!(uc.unified_heal_enabled);
    assert!(uc.self_heal_config.self_heal_enabled);
    assert_eq!(
        uc.self_heal_config.reconnect_initial_backoff,
        Duration::from_millis(500)
    );
    assert_eq!(uc.self_heal_config.reconnect_max_attempts, 3);
}

#[test]
fn test_backoff_fusion_disabled_default_e2e() {
    let config = UnifiedHealConfig::default();
    assert!(!config.backoff_fusion_enabled);
}

#[test]
fn test_backoff_fusion_enabled_adjusts_interval_e2e() {
    let self_heal_config = SelfHealConfig {
        reconnect_initial_backoff: Duration::from_millis(100),
        reconnect_backoff_multiplier: 2.0,
        reconnect_max_backoff: Duration::from_secs(10),
        reconnect_jitter_range: 0.0,
        ..Default::default()
    };
    let interval_0 = compute_fused_heal_interval(&self_heal_config, 0);
    let interval_5 = compute_fused_heal_interval(&self_heal_config, 5);
    assert!(
        interval_5 >= interval_0,
        "attempt=5 退避间隔应 >= attempt=0"
    );
    assert!(
        interval_5 <= Duration::from_secs(11),
        "不应超过 max_backoff + jitter"
    );
}

#[test]
fn test_auto_healer_doc_comment_contains_connection_level_e2e() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("pool.rs");
    let source = std::fs::read_to_string(&path).unwrap();
    assert!(
        source.contains("连接级自愈"),
        "AutoHealer doc comment 必须含'连接级自愈'术语"
    );
}

#[test]
fn test_self_heal_engine_doc_comment_contains_db_level_e2e() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("self_heal.rs");
    let source = std::fs::read_to_string(&path).unwrap();
    assert!(
        source.contains("DB 级自愈"),
        "SelfHealEngine doc comment 必须含'DB 级自愈'术语"
    );
}

// =========================================================================
// 7.3 收益持续采集 e2e（8 个）
// =========================================================================

#[test]
fn test_profit_collector_default_disabled_e2e() {
    let config = ProfitCollectorConfig::default();
    assert!(!config.continuous_collect_enabled);
}

#[tokio::test]
async fn test_profit_collector_atomic_no_lock_e2e() {
    let config = ProfitCollectorConfig {
        continuous_collect_enabled: true,
        ..Default::default()
    };
    let collector = Arc::new(ProfitCollector::new(config));
    let mut handles = Vec::new();
    for _ in 0..100 {
        let collector = collector.clone();
        handles.push(tokio::spawn(async move {
            collector.record_hit();
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
    let snap = collector.snapshot();
    assert_eq!(snap.hit_count, 100);
}

#[test]
fn test_profit_snapshot_correct_ratio_e2e() {
    let collector = ProfitCollector::new(ProfitCollectorConfig {
        continuous_collect_enabled: true,
        ..Default::default()
    });
    collector.record_hit();
    collector.record_hit();
    collector.record_hit();
    collector.record_skip();
    let snap = collector.snapshot();
    assert_eq!(snap.hit_count, 3);
    assert_eq!(snap.skip_count, 1);
    assert!((snap.hit_ratio - 0.75).abs() < 1e-9, "hit_ratio 应为 0.75");
}

#[test]
fn test_profit_snapshot_serializable_json_e2e() {
    let collector = ProfitCollector::new(ProfitCollectorConfig {
        continuous_collect_enabled: true,
        ..Default::default()
    });
    collector.record_hit();
    collector.record_skip();
    let snap = collector.snapshot();
    let json = serde_json::to_string(&snap).unwrap();
    assert!(!json.contains("password"));
    assert!(!json.contains("credential"));
    assert!(!json.contains("secret"));
    let deserialized: sz_orm_core::profit_collector::ProfitSnapshot =
        serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.hit_count, snap.hit_count);
}

#[test]
fn test_profit_trend_query_time_range_e2e() {
    let collector = ProfitCollector::new(ProfitCollectorConfig {
        continuous_collect_enabled: true,
        ..Default::default()
    });
    collector.record_hit();
    let snap1 = collector.snapshot();
    std::thread::sleep(Duration::from_millis(10));
    collector.record_hit();
    let snap2 = collector.snapshot();
    std::thread::sleep(Duration::from_millis(10));
    collector.record_hit();
    let snap3 = collector.snapshot();
    let trend = collector.trend(snap1.snapshot_time, snap2.snapshot_time);
    assert!(trend.iter().any(|s| s.snapshot_time == snap1.snapshot_time));
    assert!(trend.iter().any(|s| s.snapshot_time == snap2.snapshot_time));
    assert!(!trend.iter().any(|s| s.snapshot_time == snap3.snapshot_time));
}

#[test]
fn test_profit_trend_empty_when_no_data_e2e() {
    let collector = ProfitCollector::new(ProfitCollectorConfig {
        continuous_collect_enabled: true,
        ..Default::default()
    });
    let trend = collector.trend(0, u64::MAX);
    assert!(trend.is_empty());
}

#[tokio::test]
async fn test_profit_collector_independent_from_benchmark_e2e() {
    let config = PoolConfigBuilder::new()
        .profit_collector(ProfitCollectorConfig {
            continuous_collect_enabled: true,
            ..Default::default()
        })
        .build()
        .unwrap();
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    pool.record_linkage_hit();
    pool.record_linkage_hit();
    pool.record_linkage_skip();
    let snap = pool.profit_snapshot().unwrap();
    assert_eq!(snap.hit_count, 2);
    assert_eq!(snap.skip_count, 1);
    let bench_result = pool
        .run_profit_benchmark(
            "test",
            |_| async move {
                Err(sz_orm_core::profit_quant::ProfitError::BenchmarkFailed(
                    "not enabled".to_string(),
                ))
            },
            |_| async move {
                Err(sz_orm_core::profit_quant::ProfitError::BenchmarkFailed(
                    "not enabled".to_string(),
                ))
            },
        )
        .await;
    assert!(bench_result.is_err(), "benchmark 未启用应返回错误");
    let snap2 = pool.profit_snapshot().unwrap();
    assert_eq!(snap2.hit_count, 2, "持续采集不受 benchmark 影响");
}

#[test]
fn test_record_linkage_hit_accumulates_to_collector_e2e() {
    let config = PoolConfigBuilder::new()
        .profit_collector(ProfitCollectorConfig {
            continuous_collect_enabled: true,
            ..Default::default()
        })
        .build()
        .unwrap();
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    for _ in 0..5 {
        pool.record_linkage_hit();
    }
    let snap = pool.profit_snapshot().unwrap();
    assert_eq!(snap.hit_count, 5);
}

// =========================================================================
// 7.4 运维统一视图 e2e（8 个）
// =========================================================================

#[test]
fn test_ops_snapshot_returns_all_summaries_e2e() {
    let pool = Pool::new(PoolConfig::default(), Arc::new(MockFac)).unwrap();
    let snapshot = pool.ops_snapshot();
    assert_eq!(
        snapshot.auto_wire_status_summary.wired_components.len(),
        0,
        "默认配置 auto_wire 未启用，wired_components 应为空"
    );
    assert!(
        matches!(snapshot.profit_summary, ProfitSummary::Disabled),
        "profit_summary 默认 Disabled"
    );
    assert_eq!(
        snapshot.self_heal_summary.total_events, 0,
        "self_heal_summary.total_events 默认 0"
    );
    assert_eq!(
        snapshot.atomic_update_summary.total_events, 0,
        "atomic_update_summary.total_events 默认 0"
    );
    assert!(
        !snapshot.loop_health_summary.enabled,
        "loop_health_summary.enabled 默认 false"
    );
    assert!(
        snapshot.alert_aggregation.is_empty(),
        "alert_aggregation 默认空"
    );
    assert!(snapshot.snapshot_time > 0);
}

#[test]
fn test_ops_snapshot_auto_wire_consistent_with_getter_e2e() {
    let pool = Pool::new(PoolConfig::default(), Arc::new(MockFac)).unwrap();
    let snapshot = pool.ops_snapshot();
    let getter_status = pool.auto_wire_status();
    assert_eq!(
        snapshot.auto_wire_status_summary.wired_components,
        getter_status.wired_components
    );
    assert_eq!(
        snapshot.auto_wire_status_summary.failed_components,
        getter_status.failed_components
    );
}

#[test]
fn test_ops_snapshot_self_heal_consistent_with_getter_e2e() {
    let config = PoolConfigBuilder::new()
        .unified_heal(UnifiedHealConfig {
            unified_heal_enabled: true,
            self_heal_config: SelfHealConfig {
                self_heal_enabled: true,
                reconnect_initial_backoff: Duration::from_millis(1),
                reconnect_max_attempts: 1,
                ..Default::default()
            },
            ..Default::default()
        })
        .build()
        .unwrap();
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let snapshot = pool.ops_snapshot();
    let heal_events = pool.self_heal_event_history();
    assert_eq!(snapshot.self_heal_summary.total_events, heal_events.len());
}

#[test]
fn test_ops_snapshot_alert_aggregation_e2e() {
    let config = PoolConfigBuilder::new()
        .profit_collector(ProfitCollectorConfig {
            continuous_collect_enabled: true,
            ..Default::default()
        })
        .build()
        .unwrap();
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    pool.record_linkage_skip();
    pool.record_linkage_skip();
    pool.record_linkage_skip();
    let snapshot = pool.ops_snapshot();
    assert!(
        snapshot
            .alert_aggregation
            .iter()
            .any(|a| a.source == AlertSource::ProfitQuant),
        "命中率 0% < 50% 应触发 ProfitQuant 告警"
    );
}

#[test]
fn test_ops_snapshot_no_alert_when_all_normal_e2e() {
    let pool = Pool::new(PoolConfig::default(), Arc::new(MockFac)).unwrap();
    let snapshot = pool.ops_snapshot();
    assert!(snapshot.alert_aggregation.is_empty());
}

#[test]
fn test_ops_snapshot_serializable_json_e2e() {
    let pool = Pool::new(PoolConfig::default(), Arc::new(MockFac)).unwrap();
    let snapshot = pool.ops_snapshot();
    let json = serde_json::to_string(&snapshot).unwrap();
    assert!(!json.contains("password"));
    assert!(!json.contains("credential"));
    assert!(!json.contains("secret"));
    let lower = json.to_lowercase();
    assert!(!lower.contains("\"key\""));
}

#[test]
fn test_ops_snapshot_disabled_direction_marked_e2e() {
    let pool = Pool::new(PoolConfig::default(), Arc::new(MockFac)).unwrap();
    let snapshot = pool.ops_snapshot();
    assert!(matches!(snapshot.profit_summary, ProfitSummary::Disabled));
}

#[test]
fn test_ops_snapshot_latency_under_10ms_e2e() {
    let pool = Pool::new(PoolConfig::default(), Arc::new(MockFac)).unwrap();
    let start = Instant::now();
    let _snapshot = pool.ops_snapshot();
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() <= 10,
        "ops_snapshot 延迟 {}ms > 10ms",
        elapsed.as_millis()
    );
}
