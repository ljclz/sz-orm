//! CI 基准集成（v8.1.0 任务 2.4，feature gate: `perf-regression-ci`）
//!
//! 流程：PR 触发 → BenchProdDbGuard 校验非生产 DB → 连接真实 DB（不可用报错 `BENCH_DB_UNAVAILABLE`，
//! 禁止回退模拟延迟 ADR-003）→ 执行基准 workload → BenchRepeatabilityGuard 校验偏差 →
//! RegressionBaseline 对比基线 → 退化 ≥ 10% 阻断合入 → 产出 CiBenchReport。

use serde::{Deserialize, Serialize};

use crate::prod_db_guard::BenchProdDbGuard;
use crate::regression_baseline::BaselineComparison;
use crate::repeatability_guard::{BenchRepeatabilityGuard, RepeatabilityReport};
use crate::{BenchDbConfig, BenchError, BenchResult, RegressionBaseline, WorkloadDef};

/// CI 基准决策
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CiBenchDecision {
    /// 放行（退化 < 阈值）
    Pass,
    /// 阻断合入（退化 ≥ 阈值）
    Block,
    /// 阻断合入（基准不稳定）
    BlockUnstable,
    /// 阻断合入（生产 DB / DB 不可用 / 基线缺失等）
    BlockError,
}

/// CI 基准报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiBenchReport {
    /// 当前基准结果（多次执行）
    pub current_results: Vec<BenchResult>,
    /// 基线结果
    pub baseline_results: Option<Vec<BenchResult>>,
    /// 退化百分比（正值=退化，负值=改善），None 表示无法对比
    pub degradation_pct: Option<f64>,
    /// 决策
    pub decision: CiBenchDecision,
    /// 可重复性报告
    pub repeatability: Option<RepeatabilityReport>,
    /// 基线对比报告
    pub baseline_comparison: Option<BaselineComparison>,
    /// 复现步骤
    pub repro_steps: String,
    /// 告警码（空=无告警）
    pub alert_code: String,
}

/// 基准执行器 trait（async fn in trait，Rust 1.81 原生支持）
///
/// 实现方负责连接真实 DB 并执行基准 workload。
/// ADR-003：禁止回退模拟延迟，DB 不可用直接返回 `BenchError::DbUnavailable`。
pub trait BenchExecutor: Send + Sync {
    /// 执行单次基准 workload，返回真实 DB 结果
    fn execute(
        &self,
        db_config: &BenchDbConfig,
        workload: &WorkloadDef,
    ) -> impl std::future::Future<Output = Result<BenchResult, BenchError>> + Send;

    /// 加载基线结果（通过 git ref 或文件路径），None 表示基线不存在
    fn load_baseline(&self, baseline_ref: &str) -> Result<Option<Vec<BenchResult>>, BenchError>;
}

/// CI 基准集成
pub struct CiBenchIntegration<E: BenchExecutor> {
    executor: E,
    baseline_ref: String,
    degradation_threshold: f64,
    repeat_times: usize,
    prod_guard: BenchProdDbGuard,
    repeatability_guard: BenchRepeatabilityGuard,
}

impl<E: BenchExecutor> CiBenchIntegration<E> {
    /// 创建 CI 基准集成：指定执行器、基线 ref、退化阈值（默认 0.10）、重复次数（默认 5）
    pub fn new(
        executor: E,
        baseline_ref: impl Into<String>,
        degradation_threshold: f64,
        repeat_times: usize,
    ) -> Self {
        Self {
            executor,
            baseline_ref: baseline_ref.into(),
            degradation_threshold,
            repeat_times,
            prod_guard: BenchProdDbGuard::new(),
            repeatability_guard: BenchRepeatabilityGuard::with_defaults(),
        }
    }

    /// 使用默认配置（10% 退化阈值，5 次重复）
    pub fn with_defaults(executor: E, baseline_ref: impl Into<String>) -> Self {
        Self::new(executor, baseline_ref, 0.10, 5)
    }

    /// 运行 CI 基准对比
    pub async fn run_ci_bench(
        &self,
        db_config: &BenchDbConfig,
        workload: &WorkloadDef,
    ) -> Result<CiBenchReport, BenchError> {
        // 1. 生产 DB 守卫校验
        if let Err(e) = self.prod_guard.validate(db_config) {
            return Ok(CiBenchReport {
                current_results: vec![],
                baseline_results: None,
                degradation_pct: None,
                decision: CiBenchDecision::BlockError,
                repeatability: None,
                baseline_comparison: None,
                repro_steps: format!("生产 DB 守卫拒绝：{e}"),
                alert_code: "BENCH_PROD_DB_FORBIDDEN".to_string(),
            });
        }

        // 2. 执行基准 workload（重复 repeat_times 次，连接真实 DB，不回退模拟延迟）
        let mut current_results = Vec::with_capacity(self.repeat_times);
        for i in 0..self.repeat_times {
            match self.executor.execute(db_config, workload).await {
                Ok(result) => current_results.push(result),
                Err(e) => {
                    return Ok(CiBenchReport {
                        current_results,
                        baseline_results: None,
                        degradation_pct: None,
                        decision: CiBenchDecision::BlockError,
                        repeatability: None,
                        baseline_comparison: None,
                        repro_steps: format!(
                            "第 {i} 次基准执行失败（DB 不可用）：{e}。\
                             ADR-003 禁止回退模拟延迟，请检查 DB 连接"
                        ),
                        alert_code: "BENCH_DB_UNAVAILABLE".to_string(),
                    });
                }
            }
        }

        // 3. 可重复性校验
        let repeatability = match self.repeatability_guard.check_deviation(&current_results) {
            Ok(report) => report,
            Err(e) => {
                return Ok(CiBenchReport {
                    current_results,
                    baseline_results: None,
                    degradation_pct: None,
                    decision: CiBenchDecision::BlockError,
                    repeatability: None,
                    baseline_comparison: None,
                    repro_steps: format!("可重复性校验失败：{e}"),
                    alert_code: "BENCH_INSUFFICIENT_SAMPLES".to_string(),
                });
            }
        };
        if !repeatability.stable {
            let deviation_pct = repeatability.deviation_pct;
            let suggestion = repeatability.suggestion.clone();
            return Ok(CiBenchReport {
                current_results,
                baseline_results: None,
                degradation_pct: None,
                decision: CiBenchDecision::BlockUnstable,
                repeatability: Some(repeatability),
                baseline_comparison: None,
                repro_steps: format!(
                    "基准不稳定：QPS 偏差 {deviation_pct:.2}% 超过 5% 阈值。建议：{suggestion}"
                ),
                alert_code: "BENCH_UNSTABLE".to_string(),
            });
        }

        // 4. 加载基线并对比
        let baseline_results = self.executor.load_baseline(&self.baseline_ref)?;
        let (baseline_comparison, degradation_pct, decision, repro_steps, alert_code) =
            match &baseline_results {
                None => (
                    None,
                    None,
                    CiBenchDecision::Pass,
                    format!(
                        "基线 `{}` 缺失，跳过对比，标记 BENCH_BASELINE_MISSING 放行",
                        self.baseline_ref
                    ),
                    "BENCH_BASELINE_MISSING".to_string(),
                ),
                Some(baseline) => {
                    let baseline_obj = RegressionBaseline::new(
                        &self.baseline_ref,
                        workload.config.clone(),
                        baseline.clone(),
                        db_config.db_version.as_deref().unwrap_or("unknown"),
                    );
                    let comparison = baseline_obj.compare(&current_results);
                    let degradation = comparison
                        .workload_comparisons
                        .iter()
                        .map(|wc| wc.degradation_pct)
                        .fold(0.0f64, f64::max);
                    let threshold_pct = self.degradation_threshold * 100.0;
                    if comparison.has_regression && degradation >= threshold_pct {
                        (
                            Some(comparison),
                            Some(degradation),
                            CiBenchDecision::Block,
                            format!(
                                "退化 {degradation:.2}% ≥ 阈值 {threshold_pct:.2}%，阻断合入。\
                                 复现：cargo test -p sz-orm-bench --features perf-bench-real \
                                 --test perf_bench_real_wiring"
                            ),
                            "BENCH_REGRESSION".to_string(),
                        )
                    } else {
                        (
                            Some(comparison),
                            Some(degradation),
                            CiBenchDecision::Pass,
                            format!("退化 {degradation:.2}% < 阈值 {threshold_pct:.2}%，放行合入"),
                            String::new(),
                        )
                    }
                }
            };

        Ok(CiBenchReport {
            current_results,
            baseline_results,
            degradation_pct,
            decision,
            repeatability: Some(repeatability),
            baseline_comparison,
            repro_steps,
            alert_code,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BenchConfig, FrameworkType, WorkloadType};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 测试用执行器：返回预置结果（非模拟延迟，测试注入）
    struct MockExecutor {
        results: Vec<BenchResult>,
        baseline: Option<Vec<BenchResult>>,
        fail_count: AtomicUsize,
        call_count: AtomicUsize,
    }

    impl MockExecutor {
        fn new(results: Vec<BenchResult>) -> Self {
            Self {
                results,
                baseline: None,
                fail_count: AtomicUsize::new(0),
                call_count: AtomicUsize::new(0),
            }
        }

        fn with_baseline(mut self, baseline: Vec<BenchResult>) -> Self {
            self.baseline = Some(baseline);
            self
        }

        fn with_fail_count(mut self, count: usize) -> Self {
            self.fail_count = AtomicUsize::new(count);
            self
        }
    }

    impl BenchExecutor for MockExecutor {
        async fn execute(
            &self,
            _db_config: &BenchDbConfig,
            _workload: &WorkloadDef,
        ) -> Result<BenchResult, BenchError> {
            if self.fail_count.load(Ordering::Relaxed) > 0 {
                self.fail_count.fetch_sub(1, Ordering::Relaxed);
                return Err(BenchError::DbUnavailable("测试模拟 DB 不可用".into()));
            }
            if self.results.is_empty() {
                return Err(BenchError::DbUnavailable("无预置结果".into()));
            }
            // 轮流返回不同结果以模拟多次基准执行
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(self.results[idx % self.results.len()].clone())
        }

        fn load_baseline(
            &self,
            _baseline_ref: &str,
        ) -> Result<Option<Vec<BenchResult>>, BenchError> {
            Ok(self.baseline.clone())
        }
    }

    fn make_result(p95: f64, qps: f64) -> BenchResult {
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

    fn make_workload_def() -> WorkloadDef {
        WorkloadDef::new(
            FrameworkType::SzOrm,
            WorkloadType::SingleRowQuery,
            BenchConfig::new(),
        )
    }

    fn make_test_db_config() -> BenchDbConfig {
        BenchDbConfig::new("mysql://root:test123@127.0.0.1:3306/sz_orm_test")
            .with_db_version("mysql 9.6")
    }

    fn make_prod_db_config() -> BenchDbConfig {
        BenchDbConfig::new("mysql://root:pass@10.0.0.1:3306/prod_db")
    }

    #[tokio::test]
    async fn test_block_on_production_db() {
        let executor = MockExecutor::new(vec![make_result(200.0, 10000.0); 5]);
        let ci = CiBenchIntegration::with_defaults(executor, "main");
        let report = ci
            .run_ci_bench(&make_prod_db_config(), &make_workload_def())
            .await
            .unwrap();
        assert_eq!(report.decision, CiBenchDecision::BlockError);
        assert_eq!(report.alert_code, "BENCH_PROD_DB_FORBIDDEN");
    }

    #[tokio::test]
    async fn test_block_on_db_unavailable() {
        let executor = MockExecutor::new(vec![]).with_fail_count(3);
        let ci = CiBenchIntegration::with_defaults(executor, "main");
        let report = ci
            .run_ci_bench(&make_test_db_config(), &make_workload_def())
            .await
            .unwrap();
        assert_eq!(report.decision, CiBenchDecision::BlockError);
        assert_eq!(report.alert_code, "BENCH_DB_UNAVAILABLE");
    }

    #[tokio::test]
    async fn test_pass_when_no_regression_and_baseline_missing() {
        let executor = MockExecutor::new(vec![make_result(200.0, 10000.0); 5]);
        let ci = CiBenchIntegration::with_defaults(executor, "nonexistent-ref");
        let report = ci
            .run_ci_bench(&make_test_db_config(), &make_workload_def())
            .await
            .unwrap();
        assert_eq!(report.decision, CiBenchDecision::Pass);
        assert_eq!(report.alert_code, "BENCH_BASELINE_MISSING");
        assert!(report.repeatability.unwrap().stable);
    }

    #[tokio::test]
    async fn test_block_when_degradation_exceeds_threshold() {
        let baseline = vec![make_result(200.0, 10000.0)];
        let current = vec![make_result(250.0, 8000.0); 5];
        let executor = MockExecutor::new(current).with_baseline(baseline);
        let ci = CiBenchIntegration::with_defaults(executor, "main");
        let report = ci
            .run_ci_bench(&make_test_db_config(), &make_workload_def())
            .await
            .unwrap();
        assert_eq!(report.decision, CiBenchDecision::Block);
        assert_eq!(report.alert_code, "BENCH_REGRESSION");
        assert!(report.degradation_pct.unwrap() >= 10.0);
    }

    #[tokio::test]
    async fn test_pass_when_no_degradation() {
        let baseline = vec![make_result(200.0, 10000.0)];
        let current = vec![make_result(205.0, 10000.0); 5];
        let executor = MockExecutor::new(current).with_baseline(baseline);
        let ci = CiBenchIntegration::with_defaults(executor, "main");
        let report = ci
            .run_ci_bench(&make_test_db_config(), &make_workload_def())
            .await
            .unwrap();
        assert_eq!(report.decision, CiBenchDecision::Pass);
        assert!(report.alert_code.is_empty());
    }

    #[tokio::test]
    async fn test_block_unstable_when_deviation_exceeds_5pct() {
        let results = vec![
            make_result(200.0, 10000.0),
            make_result(200.0, 8000.0),
            make_result(200.0, 9500.0),
            make_result(200.0, 11000.0),
            make_result(200.0, 9000.0),
        ];
        let executor = MockExecutor::new(results);
        let ci = CiBenchIntegration::with_defaults(executor, "main");
        let report = ci
            .run_ci_bench(&make_test_db_config(), &make_workload_def())
            .await
            .unwrap();
        assert_eq!(report.decision, CiBenchDecision::BlockUnstable);
        assert_eq!(report.alert_code, "BENCH_UNSTABLE");
    }

    #[tokio::test]
    async fn test_custom_degradation_threshold() {
        let baseline = vec![make_result(200.0, 10000.0)];
        let current = vec![make_result(225.0, 9000.0); 5]; // 12.5% 退化
        let executor = MockExecutor::new(current).with_baseline(baseline);
        let ci = CiBenchIntegration::new(executor, "main", 0.15, 5); // 15% 阈值
        let report = ci
            .run_ci_bench(&make_test_db_config(), &make_workload_def())
            .await
            .unwrap();
        assert_eq!(report.decision, CiBenchDecision::Pass);
    }

    #[tokio::test]
    async fn test_repro_steps_populated() {
        let executor = MockExecutor::new(vec![make_result(200.0, 10000.0); 5]);
        let ci = CiBenchIntegration::with_defaults(executor, "main");
        let report = ci
            .run_ci_bench(&make_test_db_config(), &make_workload_def())
            .await
            .unwrap();
        assert!(!report.repro_steps.is_empty());
    }

    #[tokio::test]
    async fn test_current_results_captured() {
        let results = vec![make_result(200.0, 10000.0); 5];
        let executor = MockExecutor::new(results.clone());
        let ci = CiBenchIntegration::with_defaults(executor, "main");
        let report = ci
            .run_ci_bench(&make_test_db_config(), &make_workload_def())
            .await
            .unwrap();
        assert_eq!(report.current_results.len(), 5);
        assert!(report.current_results[0].is_real_db);
    }
}
