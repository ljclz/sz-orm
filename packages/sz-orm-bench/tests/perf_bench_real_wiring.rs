//! 性能基准实测化端到端接线测试（v8.1.0 任务 2.8；v10.13.0 ROI-3 重构）
//!
//! 测试矩阵：
//! ① CiBenchIntegration 退化决策逻辑（受控数据注入，验证 Block/Pass/BlockUnstable 判定）
//! ② RealSqliteExecutor 真实 SQLite 执行（复用 real_db 真实查询负载，禁止硬编码延迟）
//! ③ PerfBudgetAlertEngine 接口 P99 超预算 ≤ 30s 告警附根因分析
//! ④ BenchRepeatabilityGuard 5 次基准偏差 ≤ 5% 稳定 + 偏差 > 5% 告警
//!
//! v10.13.0 ROI-3 整改说明：原 `RealSqliteExecutor` 返回硬编码延迟向量却标记
//! `is_real_db = true`（幻影接线测试）。现拆分为：
//! - `ControlledExecutor`：受控数据注入，仅验证 CiBenchIntegration 决策逻辑（诚实命名）；
//! - `RealSqliteExecutor`：真正连接 SQLite 临时文件、初始化数据集、执行
//!   [`sz_orm_bench::real_db::RealDbExecutor`] 真实查询负载，延迟来自真实执行。
//!
//! 真实执行链路测试不再断言与受控基线的固定退化比例（真实延迟不可控），
//! 而是断言产物真实性（is_real_db、非空延迟、正数统计量）。

#![cfg(feature = "perf-bench-real")]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use sz_orm_bench::real_db::{DatasetInitializer, RealDbExecutor};
use sz_orm_bench::{
    render_bench_prometheus_metrics, BenchConfig, BenchDbConfig, BenchError, BenchExecutor,
    BenchRepeatabilityGuard, BenchResult, CiBenchDecision, CiBenchIntegration, DbBackend,
    FrameworkType, WorkloadDef, WorkloadType,
};

// ── 受控执行器（决策逻辑测试专用，诚实命名） ──────────────────────────

/// 受控执行器：按调用次序轮换返回预置延迟向量（非真实 DB，仅用于验证决策逻辑）
struct ControlledExecutor {
    /// 每次调用依次返回的延迟向量（轮换）
    variants: Vec<Vec<u64>>,
    call_count: AtomicUsize,
    baseline: Option<Vec<BenchResult>>,
}

impl ControlledExecutor {
    fn new(variants: Vec<Vec<u64>>) -> Self {
        Self {
            variants,
            call_count: AtomicUsize::new(0),
            baseline: None,
        }
    }

    fn with_baseline(mut self, baseline: Vec<BenchResult>) -> Self {
        self.baseline = Some(baseline);
        self
    }
}

impl BenchExecutor for ControlledExecutor {
    fn execute(
        &self,
        _db_config: &BenchDbConfig,
        workload: &WorkloadDef,
    ) -> impl std::future::Future<Output = Result<BenchResult, BenchError>> + Send {
        let idx = self.call_count.fetch_add(1, Ordering::SeqCst) % self.variants.len();
        let latencies = self.variants[idx].clone();
        let framework = workload.framework;
        let workload_type = workload.workload;
        let dataset_size = workload.config.dataset_size;
        async move {
            let mem = sz_orm_bench::MemoryMetrics::capture();
            Ok(BenchResult::from_latencies_ext(
                framework,
                workload_type,
                latencies,
                mem.peak_rss_kb,
                mem.alloc_count,
                mem.alloc_bytes,
                false,
                DbBackend::Sqlite,
                None,
                dataset_size,
            ))
        }
    }

    fn load_baseline(&self, _baseline_ref: &str) -> Result<Option<Vec<BenchResult>>, BenchError> {
        Ok(self.baseline.clone())
    }
}

// ── 真实 SQLite 执行器（复用 real_db 真实查询负载） ───────────────────

/// 真实 SQLite 执行器：连接 SQLite 临时文件 → 初始化数据集 → 执行真实查询负载。
/// 延迟来自真实 DB 执行（v10.13.0 ROI-3：禁止硬编码延迟冒充 is_real_db）。
struct RealSqliteExecutor;

static REAL_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

fn real_sqlite_connection() -> (String, std::path::PathBuf) {
    let id = REAL_DB_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "sz_orm_bench_wiring_{}_{}.sqlite",
        std::process::id(),
        id
    ));
    let path_str = path.to_string_lossy().replace('\\', "/");
    (format!("sqlite://{path_str}?mode=rwc"), path)
}

fn cleanup_sqlite(path: &std::path::PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
}

impl BenchExecutor for RealSqliteExecutor {
    fn execute(
        &self,
        _db_config: &BenchDbConfig,
        workload: &WorkloadDef,
    ) -> impl std::future::Future<Output = Result<BenchResult, BenchError>> + Send {
        let workload = workload.clone();
        async move {
            let (conn, db_path) = real_sqlite_connection();
            // 真实数据集初始化（建表 + 插入 workload.config.dataset_size 行）
            DatasetInitializer::init(DbBackend::Sqlite, &conn, workload.config.dataset_size)
                .await?;
            // 真实查询负载：real_db::RealDbExecutor 含预热 + measure_rounds 轮真实 SQL 执行
            let mut bench_config = workload.config.clone();
            bench_config.db_connection = conn.clone();
            let executor = RealDbExecutor::new(bench_config);
            let latencies = executor.run(workload.framework, workload.workload).await?;
            cleanup_sqlite(&db_path);

            let mem = sz_orm_bench::MemoryMetrics::capture();
            Ok(BenchResult::from_latencies_ext(
                workload.framework,
                workload.workload,
                latencies,
                mem.peak_rss_kb,
                mem.alloc_count,
                mem.alloc_bytes,
                true, // is_real_db = true（真实执行，非模拟）
                DbBackend::Sqlite,
                None,
                workload.config.dataset_size,
            ))
        }
    }

    fn load_baseline(&self, _baseline_ref: &str) -> Result<Option<Vec<BenchResult>>, BenchError> {
        Ok(None)
    }
}

// ── 测试辅助 ─────────────────────────────────────────────────────────

fn make_workload_def() -> WorkloadDef {
    let mut config = BenchConfig::new();
    // 缩小数据集/轮数使真实执行测试保持秒级（默认 10000 行 × 10 轮过重）
    config.dataset_size = 200;
    config.warmup_rounds = 1;
    config.measure_rounds = 5;
    config.pool_size = 5;
    WorkloadDef::new(FrameworkType::SzOrm, WorkloadType::SingleRowQuery, config)
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

// ── ① CiBenchIntegration 决策逻辑（受控数据注入） ────────────────────

#[tokio::test]
async fn test_ci_bench_regression_block_full_chain() {
    // 基线：P95=200μs, QPS=10000
    let baseline = vec![make_result_with_p95(200.0, 10000.0)];
    // 当前：每次执行返回相同延迟向量（5 次重复偏差 0%，可重复性通过），
    // P95 ≈ 477μs，相对基线退化 ≈ 138% ≥ 10%
    let executor =
        ControlledExecutor::new(vec![vec![150, 180, 200, 220, 250, 300, 350, 400, 450, 500]])
            .with_baseline(baseline);
    let ci = CiBenchIntegration::with_defaults(executor, "main");

    let report = ci
        .run_ci_bench(&make_test_db_config(), &make_workload_def())
        .await
        .unwrap();

    // 验证全链路：生产 DB 守卫放行 → 执行 → 可重复性校验 → 基线对比 → 退化阻断
    assert_eq!(
        report.decision,
        CiBenchDecision::Block,
        "退化 ≥ 10% 应阻断合入"
    );
    assert_eq!(report.alert_code, "BENCH_REGRESSION");
    assert!(report.degradation_pct.unwrap() >= 10.0);
    assert!(report.current_results.len() == 5);
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

#[tokio::test]
async fn test_ci_bench_pass_when_degradation_below_threshold() {
    let baseline = vec![make_result_with_p95(200.0, 10000.0)];
    // 当前延迟向量 P95 = 195μs < 基线 200μs（无退化），且 5 次重复一致
    let executor = ControlledExecutor::new(vec![vec![100, 150, 200]]).with_baseline(baseline);
    let ci = CiBenchIntegration::with_defaults(executor, "main");
    let report = ci
        .run_ci_bench(&make_test_db_config(), &make_workload_def())
        .await
        .unwrap();
    assert_eq!(report.decision, CiBenchDecision::Pass);
    assert!(report.alert_code.is_empty());
}

#[tokio::test]
async fn test_ci_bench_block_when_baseline_missing_fail_closed() {
    // v10.13.0 ROI-2：基线缺失从"放行"改为阻断（fail-closed）
    let executor = ControlledExecutor::new(vec![vec![100, 150, 200]]);
    let ci = CiBenchIntegration::with_defaults(executor, "nonexistent-ref");
    let report = ci
        .run_ci_bench(&make_test_db_config(), &make_workload_def())
        .await
        .unwrap();
    assert_eq!(report.decision, CiBenchDecision::BlockError);
    assert_eq!(report.alert_code, "BENCH_BASELINE_MISSING");
}

#[tokio::test]
async fn test_ci_bench_block_unstable_when_deviation_exceeds_5pct() {
    let baseline = vec![make_result_with_p95(200.0, 10000.0)];
    // 5 次重复轮换两组差异悬殊的延迟 → QPS 偏差 (max-min)/max > 5% → BlockUnstable
    // （可重复性校验先于基线对比执行）
    let executor = ControlledExecutor::new(vec![vec![100, 150, 200], vec![1000, 1500, 2000]])
        .with_baseline(baseline);
    let ci = CiBenchIntegration::with_defaults(executor, "main");
    let report = ci
        .run_ci_bench(&make_test_db_config(), &make_workload_def())
        .await
        .unwrap();
    assert_eq!(report.decision, CiBenchDecision::BlockUnstable);
    assert_eq!(report.alert_code, "BENCH_UNSTABLE");
}

// ── ② 真实 SQLite 执行链路（v10.13.0 ROI-3 新增） ────────────────────

#[tokio::test]
async fn test_real_sqlite_executor_produces_real_measurements() {
    let executor = RealSqliteExecutor;
    let result = executor
        .execute(&make_test_db_config(), &make_workload_def())
        .await
        .expect("真实 SQLite 执行应成功");

    // 产物真实性断言（替代原硬编码延迟下的固定退化比例断言）
    assert!(result.is_real_db, "真实执行器产物必须标记 is_real_db");
    assert!(
        !result.raw_latencies.is_empty(),
        "真实执行必须有非空延迟样本"
    );
    assert!(
        result.raw_latencies.iter().all(|&l| l > 0),
        "真实执行延迟样本必须为正数"
    );
    assert!(result.p50_us > 0.0 && result.p95_us >= result.p50_us);
    assert!(result.throughput_ops > 0.0);
    assert_eq!(result.db_backend, DbBackend::Sqlite);
    assert_eq!(result.dataset_size, 200);
}

#[tokio::test]
async fn test_real_sqlite_executor_repeats_stably() {
    // 重复 3 次真实执行，延迟样本数稳定（真实负载可重复执行，非一次性 mock）
    let executor = RealSqliteExecutor;
    for _ in 0..3 {
        let result = executor
            .execute(&make_test_db_config(), &make_workload_def())
            .await
            .expect("重复真实执行应成功");
        assert!(result.is_real_db);
        assert!(!result.raw_latencies.is_empty());
    }
}

// ── ③ PerfBudgetAlertEngine 接口 P99 超预算 ≤ 30s 告警附根因分析 ──

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

// ── ④ BenchRepeatabilityGuard 5 次基准偏差 ≤ 5% 稳定 + 偏差 > 5% 告警 ──

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
