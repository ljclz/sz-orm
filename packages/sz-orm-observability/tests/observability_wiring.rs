//! v8.1.0 组 7 可观测性增强端到端接线测试。
//!
//! 4 个 e2e 测试验证生产调用点可达：
//! 1. EndToEndTracing 全链路 span 串联跨服务/组件/DB 脱敏导出
//! 2. UnifiedMetricsCollector 六维指标统一采集 Prometheus 导出
//! 3. AlertRuleEngine 阈值/异常/组合告警 ≤ 10s 评估通知路由
//! 4. GrafanaDashboardExporter 六维仪表盘 JSON 可导入 Grafana

#![cfg(feature = "observability")]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sz_orm_observability::alert_rule_engine::{
    AlertRule, AlertRuleEngine, AlertRuleType, ComparisonOp, LogicOp, MetricsSnapshot,
};
use sz_orm_observability::alert_storm_suppressor::{
    AlertSeverity, AlertStormSuppressor, SuppressStrategy,
};
use sz_orm_observability::grafana_dashboard_exporter::GrafanaDashboardExporter;
use sz_orm_observability::self_health::{HealthStatus, ObsSubsystem, ObservabilitySelfHealth};
use sz_orm_observability::unified_collector::{
    InMemoryMetricsSource, MetricDimension, MetricSample, UnifiedMetricsCollector,
};
use sz_orm_observability::MetricsRegistry;

/// e2e 1：EndToEndTracing 全链路 span 串联跨服务/组件/DB 脱敏导出。
///
/// 生产调用点：`sz_orm_tracing::end_to_end::EndToEndTracing::export`
#[tokio::test]
async fn e2e_end_to_end_tracing_full_chain_desensitize_export() {
    use sz_orm_tracing::end_to_end::{EndToEndTracing, TraceBackend, TraceDesensitizer};
    use sz_orm_tracing::Span;

    // 构造跨服务/组件/DB 的全链路 span
    let tracer = EndToEndTracing::new(
        1.0, // 100% 采样
        TraceBackend::Tempo,
        TraceDesensitizer::default(),
        "order-service",
    )
    .expect("tracer init");

    // 根 span：HTTP 入口
    let mut root =
        Span::new("trace-001", "span-root", "HTTP POST /orders").with_service("order-service");
    root.tags.insert(
        "Authorization".to_string(),
        "Bearer secret-token".to_string(),
    );
    root.tags
        .insert("user_id".to_string(), "user-123".to_string());

    // 子 span 1：DB 查询
    let mut db_span = Span::new("trace-001", "span-db", "SELECT * FROM orders")
        .with_parent("span-root")
        .with_service("order-service");
    db_span
        .tags
        .insert("password".to_string(), "db-password-123".to_string());
    db_span.tags.insert(
        "sql".to_string(),
        "SELECT * FROM orders WHERE id = ?".to_string(),
    );

    // 子 span 2：调用下游服务
    let downstream = Span::new("trace-001", "span-downstream", "gRPC payment-service")
        .with_parent("span-root")
        .with_service("order-service");

    let spans = vec![root.clone(), db_span.clone(), downstream.clone()];

    // 校验链路完整性（parent_id 串联无断）
    tracer
        .validate_chain(&spans)
        .expect("chain should be intact");

    // 导出（含脱敏）
    tracer.export(&spans).await.expect("export should succeed");

    // 验证脱敏：敏感字段已脱敏为 ***
    let exported = tracer.get_spans();
    assert_eq!(exported.len(), 3);

    let exported_root = exported
        .iter()
        .find(|s| s.span_id() == "span-root")
        .expect("root span should exist");
    assert_eq!(
        exported_root.tags.get("Authorization").unwrap(),
        "***",
        "Authorization must be desensitized"
    );
    assert_eq!(
        exported_root.tags.get("user_id").unwrap(),
        "user-123",
        "non-sensitive tag must be preserved"
    );

    let exported_db = exported
        .iter()
        .find(|s| s.span_id() == "span-db")
        .expect("db span should exist");
    assert_eq!(
        exported_db.tags.get("password").unwrap(),
        "***",
        "password must be desensitized"
    );
    assert_eq!(
        exported_db.tags.get("sql").unwrap(),
        "SELECT * FROM orders WHERE id = ?",
        "sql tag must be preserved"
    );

    // 验证导出后端为 Tempo
    assert_eq!(tracer.export_backend(), &TraceBackend::Tempo);
}

/// e2e 2：UnifiedMetricsCollector 六维指标统一采集 Prometheus 导出。
///
/// 生产调用点：`sz_orm_observability::unified_collector::UnifiedMetricsCollector::collect` + `export_prometheus`
#[tokio::test]
async fn e2e_unified_metrics_collector_six_dimensions_prometheus() {
    let registry = Arc::new(MetricsRegistry::new());
    let mut collector = UnifiedMetricsCollector::new(registry.clone());

    // 注册自定义指标到 Prometheus 注册表
    collector.register_custom_counter("sz_orm_query_total", "Total query count");
    collector.register_custom_gauge("sz_orm_pool_active_connections", "Active pool connections");

    // 构造六维指标采集源
    let mut source = InMemoryMetricsSource::new();
    let now = chrono::Utc::now().timestamp_millis();
    let six_dim_metrics = vec![
        ("sz_orm_query_qps", MetricDimension::Performance, 1500.0),
        ("sz_orm_ai_autonomous_decisions", MetricDimension::Ai, 42.0),
        (
            "sz_orm_raft_leader_elections",
            MetricDimension::Distributed,
            3.0,
        ),
        (
            "sz_orm_compliance_violations",
            MetricDimension::Security,
            0.0,
        ),
        ("sz_orm_plugin_downloads", MetricDimension::Ecosystem, 128.0),
        (
            "sz_orm_trace_spans_total",
            MetricDimension::Observability,
            9999.0,
        ),
    ];
    for (name, dim, value) in &six_dim_metrics {
        source.add_sample(MetricSample {
            name: name.to_string(),
            dimension: *dim,
            value: *value,
            timestamp_ms: now,
            labels: HashMap::new(),
        });
    }
    collector.add_source(Box::new(source));

    // 采集六维指标
    let batch = collector
        .collect()
        .await
        .expect("collect should succeed with all six dimensions");
    assert_eq!(
        batch.samples.len(),
        6,
        "should collect all six dimension metrics"
    );

    // 验证六维全覆盖
    let collected_dims = collector.collected_dimensions();
    assert_eq!(collected_dims.len(), 6, "all six dimensions collected");

    // 导出 Prometheus 格式
    let prom_output = collector.export_prometheus();
    assert!(
        prom_output.contains("sz_orm_query_total"),
        "Prometheus output should contain custom counter"
    );
    assert!(
        prom_output.contains("sz_orm_pool_active_connections"),
        "Prometheus output should contain custom gauge"
    );

    // 验证采集开销 ≤ 1% CPU（快速完成）
    let start = Instant::now();
    let _ = collector.collect().await;
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 50,
        "collect overhead should be < 50ms, got {elapsed:?}"
    );
}

/// e2e 3：AlertRuleEngine 阈值/异常/组合告警 ≤ 10s 评估通知路由。
///
/// 生产调用点：`sz_orm_observability::alert_rule_engine::AlertRuleEngine::evaluate`
#[tokio::test]
async fn e2e_alert_rule_engine_threshold_anomaly_combined_routing() {
    let mut engine = AlertRuleEngine::new(AlertStormSuppressor::new(SuppressStrategy::None));

    // 规则 1：阈值告警 - CPU > 80% → 路由到 pagerduty
    engine.add_rule(
        AlertRule::threshold(
            "high_cpu",
            "cpu_usage",
            ComparisonOp::Gt,
            80.0,
            AlertSeverity::Warning,
        )
        .with_route("pagerduty"),
    );

    // 规则 2：异常告警 - 延迟 3σ 偏离 → 路由到 slack
    engine.add_rule(
        AlertRule::anomaly(
            "latency_anomaly",
            "latency_ms",
            Duration::from_secs(60),
            3.0, // 3σ
            AlertSeverity::Critical,
        )
        .with_route("slack"),
    );

    // 规则 3：组合告警 - CPU > 90% AND MEM > 90% → 路由到 pagerduty-critical
    engine.add_rule(
        AlertRule::combined(
            "resource_exhaustion",
            vec![
                AlertRuleType::Threshold {
                    metric: "cpu_usage".to_string(),
                    op: ComparisonOp::Gt,
                    threshold: 90.0,
                },
                AlertRuleType::Threshold {
                    metric: "mem_usage".to_string(),
                    op: ComparisonOp::Gt,
                    threshold: 90.0,
                },
            ],
            LogicOp::And,
            AlertSeverity::Critical,
        )
        .with_route("pagerduty-critical"),
    );

    // 构造指标快照：CPU 95%、MEM 92%、延迟异常
    let mut snapshot = MetricsSnapshot::new();
    snapshot.set("cpu_usage", 95.0);
    snapshot.set("mem_usage", 92.0);
    snapshot.set("latency_ms", 800.0); // 异常高延迟
    snapshot.set_history(
        "latency_ms",
        vec![100.0, 105.0, 95.0, 100.0, 110.0, 90.0, 100.0, 102.0],
    );

    // 评估（≤ 10s）
    let start = Instant::now();
    let alerts = engine
        .evaluate(&snapshot)
        .await
        .expect("evaluate should succeed");
    let elapsed = start.elapsed();

    // 应触发 3 个告警：high_cpu + latency_anomaly + resource_exhaustion
    assert_eq!(
        alerts.len(),
        3,
        "should trigger 3 alerts (threshold + anomaly + combined)"
    );
    assert!(
        elapsed.as_millis() < 100,
        "eval should complete < 100ms, got {elapsed:?}"
    );

    // 验证通知路由
    let routes: Vec<&str> = alerts.iter().map(|a| a.route.as_str()).collect();
    assert!(
        routes.contains(&"pagerduty"),
        "high_cpu should route to pagerduty"
    );
    assert!(
        routes.contains(&"slack"),
        "latency_anomaly should route to slack"
    );
    assert!(
        routes.contains(&"pagerduty-critical"),
        "resource_exhaustion should route to pagerduty-critical"
    );

    // 验证告警级别
    let critical_alerts: Vec<_> = alerts
        .iter()
        .filter(|a| a.severity == AlertSeverity::Critical)
        .collect();
    assert!(
        critical_alerts.len() >= 2,
        "should have at least 2 critical alerts"
    );

    // 验证准确率 ≥ 99%（无漏报）
    let accuracy = engine.accuracy();
    assert!(
        accuracy >= 0.99,
        "accuracy should be >= 99%, got {accuracy}"
    );
}

/// e2e 4：GrafanaDashboardExporter 六维仪表盘 JSON 可导入 Grafana。
///
/// 生产调用点：`sz_orm_observability::grafana_dashboard_exporter::GrafanaDashboardExporter::export`
#[tokio::test]
async fn e2e_grafana_dashboard_exporter_six_dimensions_importable() {
    let exporter = GrafanaDashboardExporter::new();

    // 导出仪表盘 JSON
    let dashboard = exporter
        .export()
        .await
        .expect("dashboard export should succeed");

    // 验证基本结构
    assert_eq!(dashboard.title, "SZ-ORM Observability Dashboard");
    assert_eq!(dashboard.uid, "sz-orm-obs");
    assert_eq!(dashboard.schema_version, 38);

    // 验证六维面板覆盖
    let panels = dashboard
        .json
        .get("panels")
        .expect("panels should exist")
        .as_array()
        .expect("panels should be array");
    assert_eq!(panels.len(), 6, "should have 6 dimension panels");

    let panel_titles: Vec<String> = panels
        .iter()
        .map(|p| {
            p.get("title")
                .expect("title should exist")
                .as_str()
                .expect("title should be string")
                .to_string()
        })
        .collect();

    // 六维全覆盖
    assert!(panel_titles.contains(&"Performance".to_string()));
    assert!(panel_titles.contains(&"AI Autonomous".to_string()));
    assert!(panel_titles.contains(&"Distributed Consensus".to_string()));
    assert!(panel_titles.contains(&"Security & Compliance".to_string()));
    assert!(panel_titles.contains(&"Ecosystem".to_string()));
    assert!(panel_titles.contains(&"Observability Self".to_string()));

    // 验证可导入 Grafana（必需字段存在）
    assert!(
        GrafanaDashboardExporter::validate_importable(&dashboard),
        "dashboard should be importable to Grafana"
    );

    // 验证每个面板有数据源和查询
    for panel in panels {
        assert!(
            panel.get("datasource").is_some(),
            "panel should have datasource"
        );
        let targets = panel
            .get("targets")
            .expect("panel should have targets")
            .as_array()
            .expect("targets should be array");
        assert!(!targets.is_empty(), "panel should have at least one query");
    }

    // 验证缓存了成功版本
    assert!(
        exporter.last_success().is_some(),
        "should cache last success version"
    );

    // 模拟生成失败后降级使用上次成功版本
    let _err = exporter.simulate_failure();
    assert_eq!(exporter.failed_count(), 1);
    let fallback = exporter
        .last_success()
        .expect("should have fallback version");
    assert_eq!(fallback.uid, "sz-orm-obs");
}

/// e2e 附加：ObservabilitySelfHealth 自身健康 + 采集丢失告警 + 脱敏兼容。
///
/// 生产调用点：`sz_orm_observability::self_health::ObservabilitySelfHealth::health`
#[tokio::test]
async fn e2e_observability_self_health_desensitization_compat() {
    let health = ObservabilitySelfHealth::new();

    // 初始状态：所有子系统未采集 → Unhealthy
    let initial_report = health.health();
    assert_eq!(initial_report.overall, HealthStatus::Unhealthy);

    // 模拟各子系统采集成功
    for sub in ObsSubsystem::all() {
        health.record_collection_success(sub);
    }
    let healthy_report = health.health();
    assert_eq!(healthy_report.overall, HealthStatus::Healthy);
    assert!(
        healthy_report.collection_lost_alerts.is_empty(),
        "no collection lost alerts when healthy"
    );

    // 模拟指标采集丢失
    health.record_collection_lost(ObsSubsystem::Metrics);
    health.record_collection_lost(ObsSubsystem::Metrics);
    let degraded_report = health.health();
    assert_eq!(degraded_report.overall, HealthStatus::Degraded);
    assert!(
        !degraded_report.collection_lost_alerts.is_empty(),
        "should have collection lost alerts"
    );
    assert!(degraded_report.collection_lost_alerts[0].contains("METRICS_COLLECTION_LOST"));

    // 验证脱敏兼容：敏感数据已脱敏
    let mut desensitized_data = HashMap::new();
    desensitized_data.insert("password".to_string(), "***".to_string());
    desensitized_data.insert("user".to_string(), "alice".to_string());
    assert!(
        ObservabilitySelfHealth::verify_desensitization(&desensitized_data),
        "desensitized data should pass verification"
    );

    // 验证未脱敏数据被拒绝
    let mut raw_sensitive_data = HashMap::new();
    raw_sensitive_data.insert("password".to_string(), "secret123".to_string());
    assert!(
        !ObservabilitySelfHealth::verify_desensitization(&raw_sensitive_data),
        "raw sensitive data should fail verification"
    );

    // 验证兼容 v8.0.0 既有局部可观测性（局部模式仍可用）
    // self_health 模块在 observability feature 下启用，不破坏既有局部可观测性
    assert!(
        degraded_report.desensitization_verified,
        "desensitization should be verified by default"
    );
}
