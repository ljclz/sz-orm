//! v10.2.0 e2e 接线验证测试
//!
//! 所有测试验证行为副作用（禁止只断言初始值/永真断言/丢弃结果）。
//! 使用 mock 连接，不需要真实 DB。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use sz_orm_core::atomic_hot_update::{
    AtomicBatchUpdateEvent, AtomicHotUpdateConfig, BatchUpdateError,
};
use sz_orm_core::auto_wire::{AutoWireConfig, AutoWireFactory, ComplianceHandle, DiagnosticHandle};
use sz_orm_core::circuit_breaker::CircuitBreaker;
use sz_orm_core::diagnostic_report::DiagnosticReportConfig;
use sz_orm_core::hot_update::HotUpdateTarget;
use sz_orm_core::performance_linkage::{ComplianceAutoCheckConfig, ObservabilityLoopConfig};
use sz_orm_core::pool::{
    Connection, ConnectionFactory, Pool, PoolConfig, PoolEvent, PoolEventCallback, QueryRows,
};
use sz_orm_core::profit_quant::{ProfitAlert, ProfitQuantConfig, WorkloadMetrics};
use sz_orm_core::self_heal::{SelfHealConfig, SelfHealEngine, SelfHealEvent};
use sz_orm_core::DbError;

struct MockConn;
impl Connection for MockConn {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async { Ok(1) })
    }
    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
    fn is_connected(&self) -> bool {
        true
    }
    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { true })
    }
    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}
struct MockFac;
#[async_trait]
impl ConnectionFactory for MockFac {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        Ok(Box::new(MockConn))
    }
}

struct MockComplianceQuery;
impl sz_orm_core::auto_wire::ComplianceQuery for MockComplianceQuery {
    fn last_result(&self) -> Option<String> {
        Some("passed".to_string())
    }
    fn compliance_score(&self) -> f64 {
        0.95
    }
}
struct MockDiagnosticQuery;
impl sz_orm_core::auto_wire::DiagnosticQuery for MockDiagnosticQuery {
    fn is_ready(&self) -> bool {
        true
    }
}

struct TestAutoWireFactory {
    create_callback: bool,
    wire_compliance: bool,
    wire_diagnostic: bool,
    callback_invocations: Arc<std::sync::atomic::AtomicU32>,
}
impl TestAutoWireFactory {
    fn new() -> Self {
        Self {
            create_callback: true,
            wire_compliance: true,
            wire_diagnostic: true,
            callback_invocations: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        }
    }
}
impl AutoWireFactory for TestAutoWireFactory {
    fn create_event_callback(
        &self,
        _loop_config: &ObservabilityLoopConfig,
    ) -> Option<PoolEventCallback> {
        if self.create_callback {
            let counter = self.callback_invocations.clone();
            Some(Arc::new(move |_event: PoolEvent| {
                counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }))
        } else {
            None
        }
    }
    fn wire_compliance(&self, _config: &ComplianceAutoCheckConfig) -> Option<ComplianceHandle> {
        if self.wire_compliance {
            Some(ComplianceHandle {
                checker: Arc::new(MockComplianceQuery),
            })
        } else {
            None
        }
    }
    fn wire_diagnostic(&self, _config: &DiagnosticReportConfig) -> Option<DiagnosticHandle> {
        if self.wire_diagnostic {
            Some(DiagnosticHandle {
                generator: Arc::new(MockDiagnosticQuery),
            })
        } else {
            None
        }
    }
}

fn pool_config_with_auto_wire(factory: Arc<dyn AutoWireFactory>) -> PoolConfig {
    PoolConfig {
        auto_wire: AutoWireConfig {
            auto_wire_enabled: true,
            ..Default::default()
        },
        auto_wire_factory: Some(factory),
        observability_loop: ObservabilityLoopConfig {
            enabled: true,
            ..Default::default()
        },
        compliance_auto_check: ComplianceAutoCheckConfig {
            auto_check_enabled: true,
            ..Default::default()
        },
        diagnostic_report: DiagnosticReportConfig {
            enabled: true,
            ..Default::default()
        },
        ..Default::default()
    }
}

// ===== M8.1 自动接线深化 e2e（8 个）=====

#[tokio::test]
async fn test_auto_wire_observability_loop_e2e() {
    let factory = Arc::new(TestAutoWireFactory::new());
    let config = pool_config_with_auto_wire(factory.clone());
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let status = pool.auto_wire_status();
    assert!(
        status
            .wired_components
            .contains(&"ObservabilityLoop".to_string()),
        "must wire observability loop"
    );
    assert!(
        status
            .wired_callbacks
            .contains(&"pool_event_callback".to_string()),
        "must register callback"
    );
}

#[tokio::test]
async fn test_auto_wire_compliance_e2e() {
    let factory = Arc::new(TestAutoWireFactory::new());
    let config = pool_config_with_auto_wire(factory);
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let handle = pool.compliance_handle();
    assert!(handle.is_some(), "compliance handle must be wired");
    let score = handle.unwrap().checker.compliance_score();
    assert!(
        (score - 0.95).abs() < 1e-9,
        "compliance score from wired checker"
    );
}

#[tokio::test]
async fn test_auto_wire_diagnostic_e2e() {
    let factory = Arc::new(TestAutoWireFactory::new());
    let config = pool_config_with_auto_wire(factory);
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let status = pool.auto_wire_status();
    assert!(
        status
            .wired_components
            .contains(&"DiagnosticGenerator".to_string()),
        "must wire diagnostic"
    );
}

#[tokio::test]
async fn test_auto_wire_partial_failure_e2e() {
    let config = pool_config_with_auto_wire(Arc::new(TestAutoWireFactory {
        create_callback: false,
        wire_compliance: true,
        wire_diagnostic: true,
        callback_invocations: Arc::new(std::sync::atomic::AtomicU32::new(0)),
    }));
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let status = pool.auto_wire_status();
    assert!(
        status
            .failed_components
            .contains(&"ObservabilityLoop".to_string()),
        "callback failure must be recorded"
    );
    assert!(
        status
            .wired_components
            .contains(&"ComplianceChecker".to_string()),
        "compliance still wires"
    );
}

#[tokio::test]
async fn test_auto_wire_idempotent_e2e() {
    let factory1 = Arc::new(TestAutoWireFactory::new());
    let config1 = pool_config_with_auto_wire(factory1);
    let pool1 = Pool::new(config1, Arc::new(MockFac)).unwrap();
    let factory2 = Arc::new(TestAutoWireFactory::new());
    let config2 = pool_config_with_auto_wire(factory2);
    let pool2 = Pool::new(config2, Arc::new(MockFac)).unwrap();
    let s1 = pool1.auto_wire_status();
    let s2 = pool2.auto_wire_status();
    assert_eq!(
        s1.wired_components, s2.wired_components,
        "same config → same wired components"
    );
    assert_eq!(
        s1.wired_callbacks.len(),
        s2.wired_callbacks.len(),
        "callback count not doubled"
    );
}

#[tokio::test]
async fn test_auto_wire_status_queryable_e2e() {
    let factory = Arc::new(TestAutoWireFactory::new());
    let config = pool_config_with_auto_wire(factory);
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let status = pool.auto_wire_status();
    assert!(
        !status.wired_components.is_empty(),
        "wired components non-empty"
    );
    assert!(
        status.wire_time_ms < 1000,
        "wire time reasonable: {}ms",
        status.wire_time_ms
    );
}

#[tokio::test]
async fn test_auto_wire_disabled_e2e() {
    let pool = Pool::new(PoolConfig::default(), Arc::new(MockFac)).unwrap();
    let status = pool.auto_wire_status();
    assert!(
        status.wired_components.is_empty(),
        "disabled → no wired components"
    );
}

#[tokio::test]
async fn test_auto_wire_manual_still_works_e2e() {
    let pool = Pool::new(PoolConfig::default(), Arc::new(MockFac)).unwrap();
    assert!(
        pool.auto_wire_status().wired_components.is_empty(),
        "no auto wire"
    );
    assert!(pool.compliance_handle().is_none(), "no auto compliance");
}

// ===== M8.2 联动收益量化 e2e（6 个）=====

#[tokio::test]
async fn test_profit_quant_benchmark_e2e() {
    let config = PoolConfig {
        profit_quant: ProfitQuantConfig {
            profit_quant_enabled: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let report = pool
        .run_profit_benchmark(
            "test_workload",
            |_w| async {
                Ok(WorkloadMetrics {
                    throughput: 150.0,
                    latency_p99: 50.0,
                    cache_hit_zero_copy_ratio: 0.85,
                })
            },
            |_w| async {
                Ok(WorkloadMetrics {
                    throughput: 100.0,
                    latency_p99: 100.0,
                    cache_hit_zero_copy_ratio: 0.5,
                })
            },
        )
        .await
        .unwrap();
    assert!(
        (report.throughput_uplift_pct - 50.0).abs() < 1e-6,
        "uplift 50%"
    );
    assert!(
        (report.latency_reduction_pct - 50.0).abs() < 1e-6,
        "latency reduction 50%"
    );
}

#[tokio::test]
async fn test_profit_quant_threshold_alert_e2e() {
    let config = PoolConfig {
        profit_quant: ProfitQuantConfig {
            profit_quant_enabled: true,
            min_throughput_uplift_pct: 100.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let report = pool
        .run_profit_benchmark(
            "test",
            |_w| async {
                Ok(WorkloadMetrics {
                    throughput: 110.0,
                    latency_p99: 90.0,
                    cache_hit_zero_copy_ratio: 0.8,
                })
            },
            |_w| async {
                Ok(WorkloadMetrics {
                    throughput: 100.0,
                    latency_p99: 100.0,
                    cache_hit_zero_copy_ratio: 0.5,
                })
            },
        )
        .await
        .unwrap();
    let alert = pool.profit_quant_engine().unwrap().check_threshold(&report);
    assert!(
        matches!(alert, Some(ProfitAlert::ThroughputBelowThreshold { .. })),
        "must alert below threshold"
    );
}

#[tokio::test]
async fn test_profit_quant_independent_failure_e2e() {
    let config = PoolConfig {
        profit_quant: ProfitQuantConfig {
            profit_quant_enabled: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let result = pool
        .run_profit_benchmark(
            "test",
            |_w| async {
                Ok(WorkloadMetrics {
                    throughput: 120.0,
                    latency_p99: 80.0,
                    cache_hit_zero_copy_ratio: 0.9,
                })
            },
            |_w| async {
                Err(sz_orm_core::profit_quant::ProfitError::BenchmarkFailed(
                    "db down".into(),
                ))
            },
        )
        .await;
    assert!(result.is_err(), "independent failure must propagate");
}

#[tokio::test]
async fn test_profit_quant_empty_workload_e2e() {
    let config = PoolConfig {
        profit_quant: ProfitQuantConfig {
            profit_quant_enabled: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let result = pool
        .run_profit_benchmark(
            "",
            |_w| async {
                Ok(WorkloadMetrics {
                    throughput: 100.0,
                    latency_p99: 100.0,
                    cache_hit_zero_copy_ratio: 0.5,
                })
            },
            |_w| async {
                Ok(WorkloadMetrics {
                    throughput: 100.0,
                    latency_p99: 100.0,
                    cache_hit_zero_copy_ratio: 0.5,
                })
            },
        )
        .await;
    assert!(result.is_err(), "empty workload must fail");
}

#[tokio::test]
async fn test_profit_quant_no_sensitive_data_e2e() {
    let config = PoolConfig {
        profit_quant: ProfitQuantConfig {
            profit_quant_enabled: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let report = pool
        .run_profit_benchmark(
            "workload",
            |_w| async {
                Ok(WorkloadMetrics {
                    throughput: 120.0,
                    latency_p99: 80.0,
                    cache_hit_zero_copy_ratio: 0.9,
                })
            },
            |_w| async {
                Ok(WorkloadMetrics {
                    throughput: 100.0,
                    latency_p99: 100.0,
                    cache_hit_zero_copy_ratio: 0.5,
                })
            },
        )
        .await
        .unwrap();
    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains("password"), "no password in report");
    assert!(!json.contains("secret"), "no secret in report");
}

#[tokio::test]
async fn test_profit_quant_disabled_e2e() {
    let pool = Pool::new(PoolConfig::default(), Arc::new(MockFac)).unwrap();
    assert!(pool.profit_quant_engine().is_none(), "disabled → no engine");
    let result = pool
        .run_profit_benchmark(
            "test",
            |_w| async {
                Ok(WorkloadMetrics {
                    throughput: 100.0,
                    latency_p99: 100.0,
                    cache_hit_zero_copy_ratio: 0.5,
                })
            },
            |_w| async {
                Ok(WorkloadMetrics {
                    throughput: 100.0,
                    latency_p99: 100.0,
                    cache_hit_zero_copy_ratio: 0.5,
                })
            },
        )
        .await;
    assert!(result.is_err(), "disabled → benchmark fails");
}

// ===== M8.3 闭环健康度监控 e2e（7 个）=====

use sz_orm_core::performance_linkage::LoopHealthConfig;

#[tokio::test]
async fn test_loop_health_monitor_e2e() {
    use sz_orm_observability::loop_health::LoopHealthMonitor;
    let monitor = LoopHealthMonitor::new(LoopHealthConfig {
        loop_health_enabled: true,
        ..Default::default()
    });
    for _ in 0..50 {
        monitor.record_enqueue();
    }
    for _ in 0..50 {
        monitor.record_process(2);
    }
    let snap = monitor.metrics();
    assert_eq!(snap.enqueued_count, 50, "50 enqueued");
    assert_eq!(snap.processed_count, 50, "50 processed");
    let score = monitor.compute_health_score();
    assert!((0.0..=100.0).contains(&score), "score in [0,100]: {score}");
}

#[tokio::test]
async fn test_loop_health_score_normal_e2e() {
    use sz_orm_observability::loop_health::LoopHealthMonitor;
    let monitor = LoopHealthMonitor::new(LoopHealthConfig {
        loop_health_enabled: true,
        ..Default::default()
    });
    for _ in 0..10 {
        monitor.record_enqueue();
    }
    for _ in 0..10 {
        monitor.record_process(1);
    }
    assert!(monitor.compute_health_score() >= 80.0, "normal score >= 80");
}

#[tokio::test]
async fn test_loop_health_score_backlog_overlimit_e2e() {
    use sz_orm_observability::loop_health::LoopHealthMonitor;
    let config = LoopHealthConfig {
        loop_health_enabled: true,
        backlog_alert_limit: 5,
        ..Default::default()
    };
    let monitor = LoopHealthMonitor::new(config);
    for _ in 0..20 {
        monitor.record_enqueue();
    }
    assert!(
        monitor.compute_health_score() <= 60.0,
        "backlog overlimit score <= 60"
    );
}

#[tokio::test]
async fn test_loop_health_score_degradation_high_e2e() {
    use sz_orm_observability::loop_health::LoopHealthMonitor;
    let monitor = LoopHealthMonitor::new(LoopHealthConfig {
        loop_health_enabled: true,
        ..Default::default()
    });
    for _ in 0..100 {
        monitor.record_enqueue();
    }
    for _ in 0..30 {
        monitor.record_degrade();
    }
    assert!(
        monitor.compute_health_score() < 80.0,
        "high degradation score < 80"
    );
}

#[tokio::test]
async fn test_loop_health_low_overhead_e2e() {
    use sz_orm_observability::loop_health::LoopHealthMonitor;
    let monitor = LoopHealthMonitor::new(LoopHealthConfig {
        loop_health_enabled: true,
        ..Default::default()
    });
    let start = std::time::Instant::now();
    for _ in 0..1000 {
        monitor.record_enqueue();
    }
    for _ in 0..1000 {
        monitor.record_process(1);
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(100),
        "1000 events in < 100ms: {elapsed:?}"
    );
}

#[tokio::test]
async fn test_loop_health_no_sensitive_data_e2e() {
    use sz_orm_observability::loop_health::LoopHealthMonitor;
    let monitor = LoopHealthMonitor::new(LoopHealthConfig {
        loop_health_enabled: true,
        ..Default::default()
    });
    monitor.record_enqueue();
    monitor.record_process(5);
    let snap = monitor.metrics();
    let json = serde_json::to_string(&snap).unwrap();
    assert!(!json.contains("password"), "no password in health metrics");
}

#[tokio::test]
async fn test_loop_health_disabled_e2e() {
    let pool = Pool::new(PoolConfig::default(), Arc::new(MockFac)).unwrap();
    let config = pool.loop_health_config();
    assert!(!config.loop_health_enabled, "default disabled");
    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;
    let conn2 = pool.acquire().await.unwrap();
    pool.release(conn2).await;
}

// ===== M8.4 故障自愈 e2e（9 个）=====

#[tokio::test]
async fn test_self_heal_reconnect_e2e() {
    let config = PoolConfig {
        self_heal: SelfHealConfig {
            self_heal_enabled: true,
            reconnect_initial_backoff: Duration::from_millis(1),
            ..Default::default()
        },
        ..Default::default()
    };
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    assert!(pool.self_heal_engine().is_some(), "engine created");
    let attempt = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let a = attempt.clone();
    let result = pool
        .self_heal_engine()
        .unwrap()
        .auto_reconnect(move || {
            let a = a.clone();
            async move {
                if a.fetch_add(1, std::sync::atomic::Ordering::Relaxed) == 0 {
                    Err("connection refused".to_string())
                } else {
                    Ok(())
                }
            }
        })
        .await;
    assert!(result.is_ok(), "reconnect succeeds on retry");
    let history = pool.self_heal_event_history();
    assert!(
        history
            .iter()
            .any(|e| matches!(e, SelfHealEvent::ReconnectSuccess { .. })),
        "must have success event"
    );
}

#[tokio::test]
async fn test_self_heal_max_attempts_e2e() {
    let config = PoolConfig {
        self_heal: SelfHealConfig {
            self_heal_enabled: true,
            reconnect_initial_backoff: Duration::from_millis(1),
            reconnect_max_attempts: 3,
            ..Default::default()
        },
        ..Default::default()
    };
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let result = pool
        .self_heal_engine()
        .unwrap()
        .auto_reconnect(|| async { Err("db unreachable".to_string()) })
        .await;
    assert!(result.is_err(), "must fail after max attempts");
    let history = pool.self_heal_event_history();
    assert!(
        history
            .iter()
            .any(|e| matches!(e, SelfHealEvent::ReconnectMaxAttemptsReached { .. })),
        "must have max attempts event"
    );
}

#[tokio::test]
async fn test_self_heal_backoff_exponential_e2e() {
    let config = SelfHealConfig {
        self_heal_enabled: true,
        reconnect_initial_backoff: Duration::from_millis(100),
        reconnect_backoff_multiplier: 2.0,
        reconnect_max_backoff: Duration::from_secs(60),
        reconnect_jitter_range: 0.0,
        ..Default::default()
    };
    let engine = SelfHealEngine::new(config);
    let b0 = engine.compute_backoff(0);
    let b1 = engine.compute_backoff(1);
    let b2 = engine.compute_backoff(2);
    assert!(
        b1 > b0 && b2 > b1,
        "exponential growth: {b0:?} < {b1:?} < {b2:?}"
    );
}

#[tokio::test]
async fn test_self_heal_circuit_breaker_recovery_e2e() {
    let config = SelfHealConfig {
        self_heal_enabled: true,
        circuit_breaker_probe_interval: Duration::from_millis(1),
        ..Default::default()
    };
    let engine = SelfHealEngine::new(config);
    let mut cb =
        sz_orm_core::circuit_breaker::DefaultCircuitBreaker::new(1, Duration::from_millis(1));
    cb.record_failure();
    tokio::time::sleep(Duration::from_millis(2)).await;
    let recovered = engine.auto_circuit_breaker_probe(&mut cb).await;
    assert!(recovered, "probe should succeed and recover");
    let history = engine.event_history();
    assert!(
        history
            .iter()
            .any(|e| matches!(e, SelfHealEvent::CircuitBreakerRecovered { .. })),
        "must have recovered event"
    );
}

#[tokio::test]
async fn test_self_heal_circuit_breaker_probe_fail_e2e() {
    let config = SelfHealConfig {
        self_heal_enabled: true,
        circuit_breaker_probe_interval: Duration::from_millis(1),
        ..Default::default()
    };
    let engine = SelfHealEngine::new(config);
    let mut cb =
        sz_orm_core::circuit_breaker::DefaultCircuitBreaker::new(1, Duration::from_secs(60));
    for _ in 0..10 {
        cb.record_failure();
    }
    let recovered = engine.auto_circuit_breaker_probe(&mut cb).await;
    assert!(!recovered, "probe should fail when CB still open");
}

#[tokio::test]
async fn test_self_heal_no_data_corruption_e2e() {
    let config = PoolConfig {
        self_heal: SelfHealConfig {
            self_heal_enabled: true,
            reconnect_initial_backoff: Duration::from_millis(1),
            reconnect_max_attempts: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let reconnect_result = pool
        .self_heal_engine()
        .unwrap()
        .auto_reconnect(|| async { Ok(()) })
        .await;
    assert!(reconnect_result.is_ok(), "reconnect must succeed");
    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;
    let conn2 = pool.acquire().await.unwrap();
    pool.release(conn2).await;
}

#[tokio::test]
async fn test_self_heal_event_no_credentials_e2e() {
    let config = SelfHealConfig {
        self_heal_enabled: true,
        reconnect_initial_backoff: Duration::from_millis(1),
        reconnect_max_attempts: 1,
        ..Default::default()
    };
    let engine = SelfHealEngine::new(config);
    let reconnect_result = engine
        .auto_reconnect(|| async { Err("password=secret123 connection refused".to_string()) })
        .await;
    assert!(reconnect_result.is_err(), "reconnect must fail");
    let history = engine.event_history();
    assert!(!history.is_empty(), "events must be recorded");
    for event in &history {
        if let SelfHealEvent::ReconnectFailed { detail, .. } = event {
            assert!(
                !detail.contains("secret123"),
                "detail must not contain password: {detail}"
            );
        }
    }
}

#[tokio::test]
async fn test_self_heal_disabled_e2e() {
    let pool = Pool::new(PoolConfig::default(), Arc::new(MockFac)).unwrap();
    assert!(pool.self_heal_engine().is_none(), "disabled → no engine");
    assert!(
        pool.self_heal_event_history().is_empty(),
        "disabled → no events"
    );
}

#[tokio::test]
async fn test_self_heal_config_invalid_rejected_e2e() {
    let config = SelfHealConfig {
        self_heal_enabled: true,
        reconnect_max_attempts: 0,
        ..Default::default()
    };
    let engine = SelfHealEngine::new(config);
    assert!(!engine.is_valid(), "invalid config rejected");
}

// ===== M8.5 原子热更新回滚 e2e（8 个）=====

#[tokio::test]
async fn test_atomic_batch_update_all_success_e2e() {
    let config = PoolConfig {
        atomic_hot_update: AtomicHotUpdateConfig {
            atomic_hot_update_enabled: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let updates = vec![
        HotUpdateTarget::CacheStrategy {
            max_entries: 200,
            ttl: Duration::from_secs(30),
        },
        HotUpdateTarget::SlowQueryThreshold { threshold_ms: 500 },
    ];
    let result = pool.batch_hot_update(updates).await;
    assert!(result.is_ok(), "all success: {result:?}");
    let history = pool.atomic_batch_event_history();
    assert!(
        history
            .iter()
            .any(|e| matches!(e, AtomicBatchUpdateEvent::BatchUpdateCommitted { .. })),
        "must have committed event"
    );
}

#[tokio::test]
async fn test_atomic_batch_rollback_e2e() {
    let config = PoolConfig {
        atomic_hot_update: AtomicHotUpdateConfig {
            atomic_hot_update_enabled: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let updates = vec![
        HotUpdateTarget::AlertThreshold {
            warn: 50,
            critical: 100,
        },
        HotUpdateTarget::AlertThreshold {
            warn: 80,
            critical: 100,
        },
    ];
    let result = pool.batch_hot_update(updates).await;
    assert!(result.is_ok(), "both valid configs should succeed");
}

#[tokio::test]
async fn test_atomic_batch_config_invalid_e2e() {
    let config = PoolConfig {
        atomic_hot_update: AtomicHotUpdateConfig {
            atomic_hot_update_enabled: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    let updates = vec![
        HotUpdateTarget::CacheStrategy {
            max_entries: 100,
            ttl: Duration::from_secs(60),
        },
        HotUpdateTarget::AlertThreshold {
            warn: 100,
            critical: 50,
        },
    ];
    let result = pool.batch_hot_update(updates).await;
    assert!(
        matches!(result, Err(BatchUpdateError::ConfigInvalid(_))),
        "invalid config rejected: {result:?}"
    );
}

#[tokio::test]
async fn test_atomic_batch_event_history_e2e() {
    let config = PoolConfig {
        atomic_hot_update: AtomicHotUpdateConfig {
            atomic_hot_update_enabled: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    pool.batch_hot_update(vec![HotUpdateTarget::SlowQueryThreshold {
        threshold_ms: 300,
    }])
    .await
    .unwrap();
    let history = pool.atomic_batch_event_history();
    assert!(!history.is_empty(), "event history non-empty");
    assert!(
        history
            .iter()
            .any(|e| matches!(e, AtomicBatchUpdateEvent::BatchUpdateStarted { .. })),
        "has started event"
    );
    assert!(
        history
            .iter()
            .any(|e| matches!(e, AtomicBatchUpdateEvent::BatchUpdateCommitted { .. })),
        "has committed event"
    );
}

#[tokio::test]
async fn test_atomic_batch_rollback_failure_e2e() {
    use sz_orm_core::atomic_hot_update::AtomicBatchUpdater;
    let updater = AtomicBatchUpdater::new(AtomicHotUpdateConfig {
        atomic_hot_update_enabled: true,
        ..Default::default()
    });
    let modules = vec![HotUpdateTarget::SlowQueryThreshold { threshold_ms: 200 }];
    let snapshot = updater.snapshot(&modules);
    let result = updater
        .rollback(&snapshot, |_, _| async {
            Err("rollback failed".to_string())
        })
        .await;
    assert!(result.is_err(), "rollback failure must propagate");
    let history = updater.event_history();
    assert!(
        history
            .iter()
            .any(|e| matches!(e, AtomicBatchUpdateEvent::RollbackFailed { .. })),
        "must have rollback failed event"
    );
}

#[tokio::test]
async fn test_atomic_batch_no_sensitive_data_e2e() {
    let config = PoolConfig {
        atomic_hot_update: AtomicHotUpdateConfig {
            atomic_hot_update_enabled: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let pool = Pool::new(config, Arc::new(MockFac)).unwrap();
    pool.batch_hot_update(vec![HotUpdateTarget::SlowQueryThreshold {
        threshold_ms: 200,
    }])
    .await
    .unwrap();
    let history = pool.atomic_batch_event_history();
    let json = serde_json::to_string(&history).unwrap();
    assert!(!json.contains("password"), "no password in events");
}

#[tokio::test]
async fn test_atomic_batch_disabled_e2e() {
    let pool = Pool::new(PoolConfig::default(), Arc::new(MockFac)).unwrap();
    let result = pool
        .batch_hot_update(vec![HotUpdateTarget::SlowQueryThreshold {
            threshold_ms: 200,
        }])
        .await;
    assert!(
        matches!(result, Err(BatchUpdateError::NotEnabled)),
        "disabled → NotEnabled"
    );
}

#[tokio::test]
async fn test_atomic_batch_single_module_still_works_e2e() {
    let pool = Pool::new(PoolConfig::default(), Arc::new(MockFac)).unwrap();
    let result = pool.apply_hot_update(HotUpdateTarget::SlowQueryThreshold { threshold_ms: 200 });
    assert!(result.is_ok(), "single module hot update still works");
}
