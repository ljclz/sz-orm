//! 根因分析 + 阶段分解（`slow-query-diagnosis` feature）
//!
//! 复用既有 `Phase` 枚举 `packages/sz-orm-flamegraph/src/collector.rs:11`
//! （Build/Bind/PoolAcquire/SqlExecute/ResultMap），
//! 复用既有 `QueryPhaseTiming` `packages/sz-orm-flamegraph/src/collector.rs:39`
//! （phase + start_ms + duration_ms）。

use serde::{Deserialize, Serialize};
use sz_orm_flamegraph::{Phase, QueryPhaseTiming};

/// 诊断阶段（镜像 `Phase`，可序列化）
///
/// 与 `sz_orm_flamegraph::Phase` 一一对应，通过 `From<Phase>` 转换。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DiagnosisPhase {
    /// 查询构造（SQL 生成）
    Build,
    /// 参数绑定
    Bind,
    /// 连接池获取连接
    PoolAcquire,
    /// SQL 执行
    SqlExecute,
    /// 结果映射
    ResultMap,
}

impl From<Phase> for DiagnosisPhase {
    fn from(phase: Phase) -> Self {
        match phase {
            Phase::Build => DiagnosisPhase::Build,
            Phase::Bind => DiagnosisPhase::Bind,
            Phase::PoolAcquire => DiagnosisPhase::PoolAcquire,
            Phase::SqlExecute => DiagnosisPhase::SqlExecute,
            Phase::ResultMap => DiagnosisPhase::ResultMap,
        }
    }
}

impl DiagnosisPhase {
    /// 阶段名（与 `Phase::as_str` 一致）
    pub fn as_str(&self) -> &'static str {
        match self {
            DiagnosisPhase::Build => "query.build",
            DiagnosisPhase::Bind => "query.bind",
            DiagnosisPhase::PoolAcquire => "pool.acquire",
            DiagnosisPhase::SqlExecute => "db.execute",
            DiagnosisPhase::ResultMap => "result.map",
        }
    }
}

/// 慢查询根因类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RootCause {
    /// 连接池耗尽（PoolAcquire 占比过高）
    PoolExhaustion,
    /// SQL 低效（SqlExecute 占比过高）
    SqlInefficiency,
    /// 大结果集（ResultMap 占比过高）
    LargeResultSet,
    /// 构造开销（Build 占比过高）
    BuildOverhead,
    /// 多阶段瓶颈
    MixedCause,
    /// 未知（阶段耗时数据缺失）
    Unknown,
}

impl RootCause {
    /// 人类可读名称
    pub fn as_str(&self) -> &'static str {
        match self {
            RootCause::PoolExhaustion => "pool-exhaustion",
            RootCause::SqlInefficiency => "sql-inefficiency",
            RootCause::LargeResultSet => "large-result-set",
            RootCause::BuildOverhead => "build-overhead",
            RootCause::MixedCause => "mixed-cause",
            RootCause::Unknown => "unknown",
        }
    }
}

/// 严重度等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    /// 信息（略超阈值）
    Info,
    /// 警告（显著超阈值）
    Warning,
    /// 严重（远超阈值）
    Critical,
}

impl Severity {
    /// 人类可读名称
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Critical => "critical",
        }
    }
}

/// 阶段分解条目
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhaseBreakdown {
    /// 阶段（镜像 `Phase`，可序列化）
    pub phase: DiagnosisPhase,
    /// 阶段耗时（毫秒）
    pub elapsed_ms: u64,
    /// 占总耗时百分比
    pub percentage: f64,
    /// 是否异常（超过对应阈值）
    pub anomaly: bool,
}

/// 根因阈值配置
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosisConfig {
    /// PoolAcquire 占比阈值（默认 30%）
    pub pool_threshold_pct: f64,
    /// SqlExecute 占比阈值（默认 50%）
    pub sql_threshold_pct: f64,
    /// ResultMap 占比阈值（默认 30%）
    pub result_threshold_pct: f64,
    /// Build 占比阈值（默认 20%）
    pub build_threshold_pct: f64,
}

impl Default for DiagnosisConfig {
    fn default() -> Self {
        Self {
            pool_threshold_pct: 30.0,
            sql_threshold_pct: 50.0,
            result_threshold_pct: 30.0,
            build_threshold_pct: 20.0,
        }
    }
}

/// 分析根因（阶段耗时占比判定）
///
/// - PoolAcquire > pool_threshold → PoolExhaustion
/// - SqlExecute > sql_threshold → SqlInefficiency
/// - ResultMap > result_threshold → LargeResultSet
/// - Build > build_threshold → BuildOverhead
/// - 多阶段超阈值 → MixedCause
/// - 无数据 → Unknown
pub fn analyze_root_cause(timings: &[QueryPhaseTiming], config: &DiagnosisConfig) -> RootCause {
    let total_ms: u64 = timings.iter().map(|t| t.duration_ms).sum();
    if total_ms == 0 {
        return RootCause::Unknown;
    }

    let total = total_ms as f64;
    let mut flags = [false; 4];

    for t in timings {
        let pct = t.duration_ms as f64 / total * 100.0;
        match t.phase {
            Phase::PoolAcquire => flags[0] = pct > config.pool_threshold_pct,
            Phase::SqlExecute => flags[1] = pct > config.sql_threshold_pct,
            Phase::ResultMap => flags[2] = pct > config.result_threshold_pct,
            Phase::Build => flags[3] = pct > config.build_threshold_pct,
            Phase::Bind => {}
        }
    }

    let count = flags.iter().filter(|&&f| f).count();
    match count {
        0 => RootCause::Unknown,
        1 => {
            if flags[0] {
                RootCause::PoolExhaustion
            } else if flags[1] {
                RootCause::SqlInefficiency
            } else if flags[2] {
                RootCause::LargeResultSet
            } else {
                RootCause::BuildOverhead
            }
        }
        _ => RootCause::MixedCause,
    }
}

/// 构建阶段分解（各阶段耗时占比 + 异常标记）
pub fn build_phase_breakdown(
    timings: &[QueryPhaseTiming],
    config: &DiagnosisConfig,
) -> Vec<PhaseBreakdown> {
    let total_ms: u64 = timings.iter().map(|t| t.duration_ms).sum();
    if total_ms == 0 {
        return Vec::new();
    }

    let total = total_ms as f64;
    timings
        .iter()
        .map(|t| {
            let pct = t.duration_ms as f64 / total * 100.0;
            let anomaly = match t.phase {
                Phase::PoolAcquire => pct > config.pool_threshold_pct,
                Phase::SqlExecute => pct > config.sql_threshold_pct,
                Phase::ResultMap => pct > config.result_threshold_pct,
                Phase::Build => pct > config.build_threshold_pct,
                Phase::Bind => false,
            };
            PhaseBreakdown {
                phase: DiagnosisPhase::from(t.phase),
                elapsed_ms: t.duration_ms,
                percentage: pct,
                anomaly,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timing(phase: Phase, ms: u64) -> QueryPhaseTiming {
        QueryPhaseTiming {
            phase,
            start_ms: 0,
            duration_ms: ms,
        }
    }

    #[test]
    fn pool_exhaustion_detected() {
        let timings = vec![
            timing(Phase::PoolAcquire, 60),
            timing(Phase::SqlExecute, 30),
            timing(Phase::ResultMap, 10),
        ];
        let config = DiagnosisConfig::default();
        assert_eq!(
            analyze_root_cause(&timings, &config),
            RootCause::PoolExhaustion
        );
    }

    #[test]
    fn sql_inefficiency_detected() {
        let timings = vec![
            timing(Phase::SqlExecute, 80),
            timing(Phase::ResultMap, 15),
            timing(Phase::PoolAcquire, 5),
        ];
        let config = DiagnosisConfig::default();
        assert_eq!(
            analyze_root_cause(&timings, &config),
            RootCause::SqlInefficiency
        );
    }

    #[test]
    fn large_result_set_detected() {
        let timings = vec![
            timing(Phase::ResultMap, 40),
            timing(Phase::SqlExecute, 35),
            timing(Phase::PoolAcquire, 25),
        ];
        let config = DiagnosisConfig::default();
        assert_eq!(
            analyze_root_cause(&timings, &config),
            RootCause::LargeResultSet
        );
    }

    #[test]
    fn build_overhead_detected() {
        let timings = vec![
            timing(Phase::Build, 25),
            timing(Phase::SqlExecute, 50),
            timing(Phase::ResultMap, 25),
        ];
        let config = DiagnosisConfig::default();
        assert_eq!(
            analyze_root_cause(&timings, &config),
            RootCause::BuildOverhead
        );
    }

    #[test]
    fn mixed_cause_detected() {
        let timings = vec![
            timing(Phase::PoolAcquire, 35),
            timing(Phase::SqlExecute, 55),
            timing(Phase::ResultMap, 10),
        ];
        let config = DiagnosisConfig::default();
        assert_eq!(analyze_root_cause(&timings, &config), RootCause::MixedCause);
    }

    #[test]
    fn empty_timings_returns_unknown() {
        let timings: Vec<QueryPhaseTiming> = vec![];
        let config = DiagnosisConfig::default();
        assert_eq!(analyze_root_cause(&timings, &config), RootCause::Unknown);
    }

    #[test]
    fn zero_duration_returns_unknown() {
        let timings = vec![timing(Phase::SqlExecute, 0)];
        let config = DiagnosisConfig::default();
        assert_eq!(analyze_root_cause(&timings, &config), RootCause::Unknown);
    }

    #[test]
    fn phase_breakdown_marks_anomalies() {
        let timings = vec![
            timing(Phase::PoolAcquire, 60),
            timing(Phase::SqlExecute, 30),
            timing(Phase::ResultMap, 10),
        ];
        let config = DiagnosisConfig::default();
        let breakdown = build_phase_breakdown(&timings, &config);
        assert_eq!(breakdown.len(), 3);
        assert!(breakdown[0].anomaly);
        assert!(!breakdown[1].anomaly);
        assert!(!breakdown[2].anomaly);
    }

    #[test]
    fn phase_breakdown_empty_for_no_timings() {
        let timings: Vec<QueryPhaseTiming> = vec![];
        let config = DiagnosisConfig::default();
        assert!(build_phase_breakdown(&timings, &config).is_empty());
    }

    #[test]
    fn phase_breakdown_percentages_sum_to_100() {
        let timings = vec![
            timing(Phase::Build, 10),
            timing(Phase::SqlExecute, 50),
            timing(Phase::ResultMap, 40),
        ];
        let config = DiagnosisConfig::default();
        let breakdown = build_phase_breakdown(&timings, &config);
        let sum: f64 = breakdown.iter().map(|b| b.percentage).sum();
        assert!((sum - 100.0).abs() < 0.01);
    }
}
