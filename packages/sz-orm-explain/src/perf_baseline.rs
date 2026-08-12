//! 性能回归基准线 + CI 自动比对（`perf-baseline` feature）
//!
//! 复用既有 `PlanSnapshot` `packages/sz-orm-explain/src/regression.rs:23`，
//! 复用既有 `PlanRegression` `packages/sz-orm-explain/src/regression.rs:69`，
//! 复用既有 `check_regressions` `packages/sz-orm-explain/src/regression.rs:161`，
//! 复用既有 `QueryPhaseTiming` `packages/sz-orm-flamegraph/src/collector.rs:39`。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sz_orm_flamegraph::QueryPhaseTiming;

use crate::regression::{compare, PlanRegression};
use crate::ExplainPlan;

/// 性能基线（各阶段耗时基线 + 执行计划基线）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfBaseline {
    /// 查询标识
    pub query_key: String,
    /// 各阶段耗时基线（阶段名 → 毫秒）
    pub phase_baselines: HashMap<String, u64>,
    /// 总耗时基线（毫秒）
    pub total_elapsed_ms: u64,
    /// 执行计划基线
    pub plan: ExplainPlan,
    /// 采集时间（ISO 8601）
    pub captured_at: String,
}

impl PerfBaseline {
    /// 从 `QueryPhaseTiming` 采集各阶段耗时基线
    pub fn new(
        query_key: impl Into<String>,
        plan: ExplainPlan,
        timings: &[QueryPhaseTiming],
    ) -> Self {
        let mut phase_baselines = HashMap::new();
        let mut total = 0u64;
        for t in timings {
            phase_baselines.insert(t.phase.as_str().to_string(), t.duration_ms);
            total += t.duration_ms;
        }
        Self {
            query_key: query_key.into(),
            phase_baselines,
            total_elapsed_ms: total,
            plan,
            captured_at: now_iso8601(),
        }
    }

    /// JSON 序列化
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// JSON 反序列化
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// 性能基线集合（JSON 文件格式）
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PerfBaselineSet {
    /// query_key → 基线
    pub baselines: HashMap<String, PerfBaseline>,
}

impl PerfBaselineSet {
    /// 添加/覆盖一条基线
    pub fn upsert(&mut self, baseline: PerfBaseline) {
        self.baselines.insert(baseline.query_key.clone(), baseline);
    }

    /// JSON 序列化
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// JSON 反序列化
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// 性能回归类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerfRegression {
    /// 单阶段耗时退化
    PhaseSlowdown {
        query_key: String,
        phase: String,
        before: u64,
        after: u64,
    },
    /// 总耗时退化
    TotalSlowdown {
        query_key: String,
        before: u64,
        after: u64,
    },
    /// 执行计划回归（复用既有 `PlanRegression`）
    PlanRegression(PlanRegression),
}

impl PerfRegression {
    /// 人类可读描述
    pub fn describe(&self) -> String {
        match self {
            PerfRegression::PhaseSlowdown {
                query_key,
                phase,
                before,
                after,
            } => {
                format!("query '{query_key}': phase '{phase}' slowed from {before}ms to {after}ms")
            }
            PerfRegression::TotalSlowdown {
                query_key,
                before,
                after,
            } => {
                format!("query '{query_key}': total elapsed slowed from {before}ms to {after}ms")
            }
            PerfRegression::PlanRegression(r) => r.describe(),
        }
    }
}

/// CI 入口：比对基线与当前性能，返回全部回归
///
/// `threshold_factor` 为耗时退化阈值倍数（如 5 表示当前耗时超过基线 5 倍时报告）。
/// 建议值：2（保守）、3（常规）、5（宽松）。
pub fn check_perf_regressions(
    baseline_json: &str,
    current_json: &str,
    threshold_factor: u64,
) -> Result<Vec<PerfRegression>, serde_json::Error> {
    let baseline: PerfBaselineSet = serde_json::from_str(baseline_json)?;
    let current: PerfBaselineSet = serde_json::from_str(current_json)?;
    let mut regressions = Vec::new();

    for (key, base) in &baseline.baselines {
        if let Some(cur) = current.baselines.get(key) {
            // 各阶段耗时比对
            for (phase, &base_ms) in &base.phase_baselines {
                if let Some(&cur_ms) = cur.phase_baselines.get(phase) {
                    if base_ms > 0 && cur_ms > base_ms.saturating_mul(threshold_factor) {
                        regressions.push(PerfRegression::PhaseSlowdown {
                            query_key: key.clone(),
                            phase: phase.clone(),
                            before: base_ms,
                            after: cur_ms,
                        });
                    }
                }
            }
            // 总耗时比对
            if base.total_elapsed_ms > 0
                && cur.total_elapsed_ms > base.total_elapsed_ms.saturating_mul(threshold_factor)
            {
                regressions.push(PerfRegression::TotalSlowdown {
                    query_key: key.clone(),
                    before: base.total_elapsed_ms,
                    after: cur.total_elapsed_ms,
                });
            }
            // 执行计划比对（复用既有 compare）
            let plan_regressions = compare(&base.plan, &cur.plan, key, threshold_factor);
            for r in plan_regressions {
                regressions.push(PerfRegression::PlanRegression(r));
            }
        }
    }

    Ok(regressions)
}

/// 当前 UTC 时间的 ISO 8601 字符串
fn now_iso8601() -> String {
    let Ok(d) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return "1970-01-01T00:00:00Z".into();
    };
    let secs = d.as_secs();
    let days = secs / 86400;
    let time = secs % 86400;
    let hours = time / 3600;
    let minutes = (time % 3600) / 60;
    let seconds = time % 60;
    let (y, m, dd) = civil_from_days(days as i64);
    format!("{y:04}-{m:02}-{dd:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExplainPlan, ScanType};
    use sz_orm_flamegraph::Phase;

    fn timing(phase: Phase, ms: u64) -> QueryPhaseTiming {
        QueryPhaseTiming {
            phase,
            start_ms: 0,
            duration_ms: ms,
        }
    }

    fn plan(scan: ScanType, rows: u64) -> ExplainPlan {
        ExplainPlan {
            scan_type: scan,
            table: "users".into(),
            index: None,
            rows,
            extra: vec![],
        }
    }

    #[test]
    fn perf_baseline_collects_phase_timings() {
        let timings = vec![
            timing(Phase::Build, 5),
            timing(Phase::SqlExecute, 50),
            timing(Phase::ResultMap, 10),
        ];
        let bl = PerfBaseline::new("q1", plan(ScanType::IndexRange, 100), &timings);
        assert_eq!(bl.phase_baselines.get("query.build"), Some(&5));
        assert_eq!(bl.phase_baselines.get("db.execute"), Some(&50));
        assert_eq!(bl.phase_baselines.get("result.map"), Some(&10));
        assert_eq!(bl.total_elapsed_ms, 65);
    }

    #[test]
    fn perf_baseline_json_roundtrip() {
        let timings = vec![timing(Phase::SqlExecute, 50)];
        let bl = PerfBaseline::new("q1", plan(ScanType::IndexRange, 100), &timings);
        let json = bl.to_json().unwrap();
        let back = PerfBaseline::from_json(&json).unwrap();
        assert_eq!(bl, back);
    }

    #[test]
    fn empty_timings_still_creates_baseline() {
        let bl = PerfBaseline::new("q1", plan(ScanType::IndexRange, 100), &[]);
        assert!(bl.phase_baselines.is_empty());
        assert_eq!(bl.total_elapsed_ms, 0);
    }

    #[test]
    fn phase_slowdown_detected() {
        let base_timings = vec![timing(Phase::Build, 5)];
        let cur_timings = vec![timing(Phase::Build, 50)];
        let base = PerfBaseline::new("q1", plan(ScanType::IndexRange, 100), &base_timings);
        let cur = PerfBaseline::new("q1", plan(ScanType::IndexRange, 100), &cur_timings);

        let mut base_set = PerfBaselineSet::default();
        base_set.upsert(base);
        let mut cur_set = PerfBaselineSet::default();
        cur_set.upsert(cur);

        let regressions =
            check_perf_regressions(&base_set.to_json().unwrap(), &cur_set.to_json().unwrap(), 5)
                .unwrap();
        assert!(regressions.iter().any(
            |r| matches!(r, PerfRegression::PhaseSlowdown { phase, .. } if phase == "query.build")
        ));
    }

    #[test]
    fn total_slowdown_detected() {
        let base_timings = vec![timing(Phase::SqlExecute, 10)];
        let cur_timings = vec![timing(Phase::SqlExecute, 100)];
        let base = PerfBaseline::new("q1", plan(ScanType::IndexRange, 100), &base_timings);
        let cur = PerfBaseline::new("q1", plan(ScanType::IndexRange, 100), &cur_timings);

        let mut base_set = PerfBaselineSet::default();
        base_set.upsert(base);
        let mut cur_set = PerfBaselineSet::default();
        cur_set.upsert(cur);

        let regressions =
            check_perf_regressions(&base_set.to_json().unwrap(), &cur_set.to_json().unwrap(), 5)
                .unwrap();
        assert!(regressions
            .iter()
            .any(|r| matches!(r, PerfRegression::TotalSlowdown { .. })));
    }

    #[test]
    fn plan_regression_detected() {
        let base = PerfBaseline::new("q1", plan(ScanType::IndexRange, 100), &[]);
        let cur = PerfBaseline::new("q1", plan(ScanType::FullTable, 100), &[]);

        let mut base_set = PerfBaselineSet::default();
        base_set.upsert(base);
        let mut cur_set = PerfBaselineSet::default();
        cur_set.upsert(cur);

        let regressions =
            check_perf_regressions(&base_set.to_json().unwrap(), &cur_set.to_json().unwrap(), 5)
                .unwrap();
        assert!(regressions.iter().any(|r| matches!(
            r,
            PerfRegression::PlanRegression(PlanRegression::ScanTypeUpgrade { .. })
        )));
    }

    #[test]
    fn both_timing_and_plan_regression_detected() {
        let base_timings = vec![timing(Phase::Build, 5)];
        let cur_timings = vec![timing(Phase::Build, 50)];
        let base = PerfBaseline::new("q1", plan(ScanType::IndexRange, 100), &base_timings);
        let cur = PerfBaseline::new("q1", plan(ScanType::FullTable, 100), &cur_timings);

        let mut base_set = PerfBaselineSet::default();
        base_set.upsert(base);
        let mut cur_set = PerfBaselineSet::default();
        cur_set.upsert(cur);

        let regressions =
            check_perf_regressions(&base_set.to_json().unwrap(), &cur_set.to_json().unwrap(), 5)
                .unwrap();
        let has_phase = regressions
            .iter()
            .any(|r| matches!(r, PerfRegression::PhaseSlowdown { .. }));
        let has_plan = regressions.iter().any(|r| {
            matches!(
                r,
                PerfRegression::PlanRegression(PlanRegression::ScanTypeUpgrade { .. })
            )
        });
        assert!(has_phase && has_plan);
    }

    #[test]
    fn no_regression_when_within_threshold() {
        let base_timings = vec![timing(Phase::Build, 10)];
        let cur_timings = vec![timing(Phase::Build, 20)];
        let base = PerfBaseline::new("q1", plan(ScanType::IndexRange, 100), &base_timings);
        let cur = PerfBaseline::new("q1", plan(ScanType::IndexRange, 100), &cur_timings);

        let mut base_set = PerfBaselineSet::default();
        base_set.upsert(base);
        let mut cur_set = PerfBaselineSet::default();
        cur_set.upsert(cur);

        let regressions =
            check_perf_regressions(&base_set.to_json().unwrap(), &cur_set.to_json().unwrap(), 5)
                .unwrap();
        assert!(regressions.is_empty());
    }
}
