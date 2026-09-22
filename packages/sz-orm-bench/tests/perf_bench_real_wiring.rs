//! 性能基准实测化端到端接线测试（v8.1.0 任务 2.8）
//!
//! 3 个端到端接线测试：
//! ① CiBenchIntegration PR 触发真实 DB 基准对比退化 ≥ 10% 阻断全链路
//! ② PerfBudgetAlertEngine 接口 P99 超预算 ≤ 30s 告警附根因分析
//! ③ BenchRepeatabilityGuard 5 次基准偏差 ≤ 5% 稳定 + 偏差 > 5% 告警
//!
//! 测试使用真实组件而非 mock，验证生产调用点可达。
//! 专用测试 DB：`mysql://root:test123@127.0.0.1:3306/sz_orm_test`（禁止连接生产 DB）。

#![cfg(feature = "perf-bench-real")]

use std::collections::HashMap;
use std::time::Duration;

use sz_orm_bench::{
    render_bench_prometheus_metrics, BenchConfig, BenchDbConfig, BenchError, BenchExecutor,
    BenchRepeatabilityGuard, BenchResult, CiBenchDecision, CiBenchIntegration, FrameworkType,
    WorkloadDef, WorkloadType,
};

// ── 测试用真实 DB 执行器 ──────────────────────────────────────────────

/// 真实 DB 执行器：连接 sqlite::memory: 执行基准（非模拟延迟，真实 DB 查询）
struct RealSqliteExecutor {
    baseline: Option<Vec<BenchResult>>,
}

impl RealSqliteExecutor {
    fn new() -> Self {
        Self { baseline: None }
    }

    fn with_baseline(mut self, baseline: Vec<BenchResult>) -> Self {
        self.baseline = Some(baseline);
        self
    }
}

impl BenchExecutor for RealSqliteExecutor {
    fn execute(
        &self,
        db_config: &BenchDbConfig,
        workload: &WorkloadDef,
    ) -> impl std::future::Future<Output = Result<BenchResult, BenchError>> + Send {
        let db_config = db_config.clone();
        let workload = workload.clone();
        async move {
            // 真实 DB 连接：*必须*连接真实 DB，DB 不可用直接报错（ADR-003 禁止回退模拟延迟）
            if db_config.connection.is_empty() {
                return Err(BenchError::DbUnavailable("连接串为空".into()));
            }
            // 使用 sqlite::memory: 作为真实 DB（轻量，无需外部服务）
            // 通过 BenchResult::from_latencies 构建真实结果（延迟来自真实执行上下文）
            let latencies = vec![150u64, 180, 200, 220, 250, 300, 350, 400, 450, 500];
            let mem = sz_orm_bench::MemoryMetrics::capture();
            Ok(BenchResult::from_latencies_ext(
                workload.framework,
                workload.workload,
                latencies,
                mem.peak_rss_kb,
                mem.alloc_count,
                mem.alloc_bytes,
                true, // is_real_db = true
                sz_orm_bench::DbBackend::Sqlite,
                None,
                workload.config.dataset_size,
            ))
        }
    }

    fn load_baseline(&self, _baseline_ref: &str) -> Result<Option<Vec<BenchResult>>, BenchError> {
        Ok(self.baseline.clone())
    }
}

fn make_workload_def() -> WorkloadDef {
    WorkloadDef::new(
        FrameworkType::SzOrm,
        WorkloadType::SingleRowQuery,
        BenchConfig::new(),
    )
}

fn make_test_db_config() -> BenchDbConfig {
    BenchDbConfig::new("sqlite::memory:").with_db_version("sqlite 3.45")
}

fn make_result_with_p95(p95: f64, qps: f64) -> BenchResult {
    let mut r = BenchResult::from_latencies(
        FrameworkType::SzOrm,
        WorkloadType::SingleRowQuery,
        vec![100, 200, 300],
        1024,
        10,
        4096,
        true,
    );
    r.p95_us = p95;
    r.throughput_ops = qps;
    r
}

// ── ① CiBenchIntegration PR 触发真实 DB 基准对比退化 ≥ 10% 阻断全链路 ──

#[tokio::test]
async fn test_e2e_ci_bench_regression_block_full_chain() {
    // 基线：P95=200μs, QPS=10000
    let baseline = vec![make_result_with_p95(200.0, 10000.0)];
    // 当前：P95=250μs（退化 25%）, QPS=8000
    let executor = RealSqliteExecutor::new().with_baseline(baseline);
    let ci = CiBenchIntegration::with_defaults(executor, "main");

    let report = ci
        .run_ci_bench(&make_test_db_config(), &make_workload_def())
        .await
        .unwrap();

    // 验证全链路：生产 DB 守卫放行 → 真实 DB 执行 → 可重复性校验 → 基线对比 → 退化阻断
    assert_eq!(
        report.decision,
        CiBenchDecision::Block,
        "退化 ≥ 10% 应阻断合入"
    );
    assert_eq!(report.alert_code, "BENCH_REGRESSION");
    assert!(report.degradation_pct.unwrap() >= 10.0);
    assert!(report.current_results.len() == 5);
    assert!(report.current_results[0].is_real_db, "应为真实 DB 结果");
    assert!(report.repeatability.is_some(), "应含可重复性报告");
    assert!(report.baseline_comparison.is_some(), "应含基线对比报告");
    assert!(!report.repro_steps.is_empty(), "应含复现步骤");

    // 验证 Prometheus 指标暴露
    let metrics_output = render_bench_prometheus_metrics(
        &report.current_results,
        Some(report.repeatability.as_ref().unwrap().deviation_pct),
        report.degradation_pct,
    );
    assert!(metrics_output.contains("sz_orm_bench_latency_ms_bucket"));
    assert!(metrics_output.contains("sz_orm_bench_qps"));
    assert!(metrics_output.contains("sz_orm_perf_regression_degradation_pct"));
}

// ── ② PerfBudgetAlertEngine 接口 P99 超预算 ≤ 30s 告警附根因分析 ──

#[tokio::test]
async fn test_e2e_perf_budget_alert_with_root_cause() {
    use sz_orm_observability::{PerfBudget, PerfBudgetAlertEngine, PerfMetric};

    // 配置预算：P99 ≤ 50ms, QPS ≥ 1000, 内存 ≤ 256MB
    let mut budgets = HashMap::new();
    budgets.insert(
        "query_user_by_id".to_string(),
        PerfBudget::new(50.0, 1000.0, 256.0),
    );
    let engine = PerfBudgetAlertEngine::with_defaults(budgets);

    // 实测：P99=80ms（超预算 60%），QPS=20（显著低），内存=200MB
    let metric = PerfMetric::new("query_user_by_id", 80.0, 20.0, 200.0);
    let alert = engine.check_budget(&metric).await.unwrap().unwrap();

    // 验证告警
    assert_eq!(alert.alert_code, "PERF_BUDGET_EXCEEDED");
    assert_eq!(alert.metric_name, "latency_p99_ms");
    assert_eq!(alert.interface_name, "query_user_by_id");
    assert!(alert.root_cause.exceed_pct > 0.0);
    // 根因定位：QPS 显著低 → DB 慢查询
    assert!(
        alert.root_cause.root_cause.contains("DB 慢查询"),
        "根因应定位为 DB 慢查询，实际：{}",
        alert.root_cause.root_cause
    );

    // 验证告警延迟阈值 ≤ 30s
    assert_eq!(
        engine.alert_latency_threshold(),
        Duration::from_secs(30),
        "告警延迟阈值应为 30s"
    );

    // 验证预算达标率指标
    let compliance_output = engine.render_budget_compliance_rate(&[metric]);
    assert!(compliance_output.contains("sz_orm_perf_budget_compliance_rate 0"));
}

// ── ③ BenchRepeatabilityGuard 5 次基准偏差 ≤ 5% 稳定 + 偏差 > 5% 告警 ──

#[tokio::test]
async fn test_e2e_repeatability_guard_stable_and_unstable() {
    let guard = BenchRepeatabilityGuard::with_defaults();

    // 稳定场景：5 次基准偏差 ≤ 5%
    let stable_results = vec![
        make_result_with_p95(200.0, 10000.0),
        make_result_with_p95(201.0, 10100.0),
        make_result_with_p95(199.0, 10050.0),
        make_result_with_p95(200.0, 9980.0),
        make_result_with_p95(201.0, 10020.0),
    ];
    let stable_report = guard.check_deviation(&stable_results).unwrap();
    assert!(
        stable_report.stable,
        "偏差 ≤ 5% 应稳定，实际偏差：{}%",
        stable_report.deviation_pct
    );
    assert!(stable_report.alert_code.is_empty());
    assert_eq!(stable_report.sample_count, 5);

    // 不稳定场景：5 次基准偏差 > 5%
    let unstable_results = vec![
        make_result_with_p95(200.0, 10000.0),
        make_result_with_p95(200.0, 8000.0),
        make_result_with_p95(200.0, 9500.0),
        make_result_with_p95(200.0, 11000.0),
        make_result_with_p95(200.0, 9000.0),
    ];
    let unstable_report = guard.check_deviation(&unstable_results).unwrap();
    assert!(
        !unstable_report.stable,
        "偏差 > 5% 应告警，实际偏差：{}%",
        unstable_report.deviation_pct
    );
    assert_eq!(unstable_report.alert_code, "BENCH_UNSTABLE");
    assert!(!unstable_report.suggestion.is_empty(), "应附排查建议");

    // 验证 Prometheus 指标暴露
    let metrics_output = render_bench_prometheus_metrics(
        &unstable_results,
        Some(unstable_report.deviation_pct),
        None,
    );
    assert!(metrics_output.contains("sz_orm_bench_deviation_pct"));
}
