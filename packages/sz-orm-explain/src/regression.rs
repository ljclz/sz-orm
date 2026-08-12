//! 执行计划回归检测（`explain-analyzer` feature）
//!
//! 提供基线快照（[`PlanSnapshot`]）与当前执行计划的对比（[`compare`]），
//! 用于 CI 中检测查询性能退化：索引丢失、扫描类型升级（IndexRange → FullTable）、
//! 行数显著增长。
//!
//! 典型 CI 流程：
//!
//! ```rust,ignore
//! // 1. 生成基线
//! let baseline = PlanSnapshot::new("find_user_by_id", plan.clone());
//! baseline.save("plans/baseline.json")?;
//!
//! // 2. 对比当前计划
//! let regressions = sz_orm_explain::regression::check_regressions("plans/baseline.json", current_json, 2)?;
//! ```

use crate::ExplainPlan;
use std::collections::HashMap;

/// 单条查询的执行计划基线快照
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PlanSnapshot {
    /// 查询唯一标识（如 `find_user_by_id` 或 SQL 摘要）
    pub query_key: String,
    /// 基线执行计划
    pub plan: ExplainPlan,
    /// 采集时间（ISO 8601）
    pub captured_at: String,
}

/// 基线集合（JSON 文件格式）
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PlanBaseline {
    /// query_key → 基线快照
    pub snapshots: HashMap<String, PlanSnapshot>,
}

impl PlanSnapshot {
    /// 创建基线快照
    pub fn new(query_key: impl Into<String>, plan: ExplainPlan) -> Self {
        Self {
            query_key: query_key.into(),
            plan,
            captured_at: now_iso8601(),
        }
    }
}

impl PlanBaseline {
    /// 添加/覆盖一条基线
    pub fn upsert(&mut self, snapshot: PlanSnapshot) {
        self.snapshots.insert(snapshot.query_key.clone(), snapshot);
    }

    /// 序列化为 JSON 字符串
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// 从 JSON 字符串解析
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// 检测到的计划回归
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PlanRegression {
    /// 扫描类型升级（退化），如 IndexRange → FullTable
    ScanTypeUpgrade {
        query_key: String,
        from: String,
        to: String,
    },
    /// 索引丢失（基线有索引，当前无）
    IndexLost { query_key: String, index: String },
    /// 预估行数显著增长（超过阈值倍数）
    RowsGrowth {
        query_key: String,
        before: u64,
        after: u64,
    },
}

impl PlanRegression {
    /// 人类可读描述
    pub fn describe(&self) -> String {
        match self {
            PlanRegression::ScanTypeUpgrade {
                query_key,
                from,
                to,
            } => {
                format!("query '{query_key}': scan type degraded from {from} to {to}")
            }
            PlanRegression::IndexLost { query_key, index } => {
                format!("query '{query_key}': index '{index}' no longer used")
            }
            PlanRegression::RowsGrowth {
                query_key,
                before,
                after,
            } => {
                format!("query '{query_key}': estimated rows grew from {before} to {after}")
            }
        }
    }
}

/// 对比基线计划与当前计划，返回全部回归（空 = 无回归）
pub fn compare(
    baseline: &ExplainPlan,
    current: &ExplainPlan,
    query_key: &str,
    rows_growth_factor: u64,
) -> Vec<PlanRegression> {
    let mut regressions = Vec::new();

    // 1. 扫描类型退化：按严重度排序（FullTable > IndexRange > IndexLookup > UniqueLookup > Other）
    if severity(current.scan_type) > severity(baseline.scan_type) {
        regressions.push(PlanRegression::ScanTypeUpgrade {
            query_key: query_key.to_string(),
            from: format!("{:?}", baseline.scan_type),
            to: format!("{:?}", current.scan_type),
        });
    }
    // 2. 索引丢失
    if baseline.index.is_some() && current.index.is_none() {
        regressions.push(PlanRegression::IndexLost {
            query_key: query_key.to_string(),
            index: baseline.index.clone().unwrap_or_default(),
        });
    }
    // 3. 行数显著增长（仅当基线行数 > 0 时比较）
    if baseline.rows > 0 && current.rows > baseline.rows.saturating_mul(rows_growth_factor) {
        regressions.push(PlanRegression::RowsGrowth {
            query_key: query_key.to_string(),
            before: baseline.rows,
            after: current.rows,
        });
    }
    regressions
}

/// 扫描类型严重度（数值越大越严重，用于退化判断）
fn severity(scan: crate::ScanType) -> u8 {
    match scan {
        crate::ScanType::FullTable => 5,
        crate::ScanType::IndexRange => 4,
        crate::ScanType::IndexLookup => 3,
        crate::ScanType::UniqueLookup => 2,
        crate::ScanType::Other => 1,
    }
}

/// CI 入口：读取基线 JSON，对比当前计划 JSON，返回回归列表
///
/// `rows_growth_factor` 为行数增长阈值倍数（如 2 表示当前行数超过基线 2 倍时报告 `RowsGrowth`）。
/// 建议值：2（保守）、3（常规）、5（宽松）。
pub fn check_regressions(
    baseline_json: &str,
    current_json: &str,
    rows_growth_factor: u64,
) -> Result<Vec<PlanRegression>, serde_json::Error> {
    let baseline: PlanBaseline = serde_json::from_str(baseline_json)?;
    let current: PlanBaseline = serde_json::from_str(current_json)?;
    let mut regressions = Vec::new();
    for (key, base) in &baseline.snapshots {
        if let Some(cur) = current.snapshots.get(key) {
            regressions.extend(compare(&base.plan, &cur.plan, key, rows_growth_factor));
        }
    }
    Ok(regressions)
}

/// 当前 UTC 时间的 ISO 8601 字符串（无外部依赖，Hinnant civil-from-days 算法）
fn now_iso8601() -> String {
    let Ok(d) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return "1970-01-01T00:00:00Z".to_string();
    };
    let secs = d.as_secs();
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    // civil-from-days：天数 → (年, 月, 日)
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if month <= 2 { y + 1 } else { y };

    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExplainPlan, ScanType};

    fn plan(scan: ScanType, index: Option<&str>, rows: u64) -> ExplainPlan {
        ExplainPlan {
            scan_type: scan,
            table: "users".into(),
            index: index.map(|s| s.to_string()),
            rows,
            extra: Vec::new(),
        }
    }

    #[test]
    fn detects_scan_type_upgrade() {
        let base = plan(ScanType::IndexRange, Some("idx"), 10);
        let cur = plan(ScanType::FullTable, None, 1150);
        let regs = compare(&base, &cur, "q1", 2);
        assert!(regs
            .iter()
            .any(|r| matches!(r, PlanRegression::ScanTypeUpgrade { .. })));
        assert!(regs
            .iter()
            .any(|r| matches!(r, PlanRegression::IndexLost { .. })));
    }

    #[test]
    fn detects_index_lost_only() {
        let base = plan(ScanType::IndexRange, Some("idx"), 10);
        let cur = plan(ScanType::IndexRange, None, 12);
        let regs = compare(&base, &cur, "q2", 2);
        assert_eq!(
            regs,
            vec![PlanRegression::IndexLost {
                query_key: "q2".into(),
                index: "idx".into()
            }]
        );
    }

    #[test]
    fn detects_rows_growth() {
        let base = plan(ScanType::IndexRange, Some("idx"), 100);
        let cur = plan(ScanType::IndexRange, Some("idx"), 500);
        let regs = compare(&base, &cur, "q3", 2);
        assert!(regs.iter().any(|r| matches!(
            r,
            PlanRegression::RowsGrowth {
                before: 100,
                after: 500,
                ..
            }
        )));
    }

    #[test]
    fn no_regression_when_improved() {
        let base = plan(ScanType::FullTable, None, 1150);
        let cur = plan(ScanType::UniqueLookup, Some("pk"), 1);
        let regs = compare(&base, &cur, "q4", 2);
        assert!(regs.is_empty());
    }

    #[test]
    fn snapshot_roundtrip() {
        let mut baseline = PlanBaseline::default();
        baseline.upsert(PlanSnapshot::new(
            "q1",
            plan(ScanType::FullTable, None, 1150),
        ));
        let json = baseline.to_json().expect("serialize");
        let parsed = PlanBaseline::from_json(&json).expect("deserialize");
        assert_eq!(parsed.snapshots.len(), 1);
        assert_eq!(parsed.snapshots["q1"].plan.rows, 1150);
    }

    #[test]
    fn check_regressions_integration() {
        let mut base = PlanBaseline::default();
        base.upsert(PlanSnapshot::new(
            "q1",
            plan(ScanType::IndexRange, Some("idx"), 10),
        ));
        let mut cur = PlanBaseline::default();
        cur.upsert(PlanSnapshot::new(
            "q1",
            plan(ScanType::FullTable, None, 1150),
        ));
        let regs = check_regressions(&base.to_json().unwrap(), &cur.to_json().unwrap(), 2).unwrap();
        // ScanTypeUpgrade + IndexLost + RowsGrowth（1150 > 10*2）共 3 项
        assert_eq!(regs.len(), 3);
        assert!(regs
            .iter()
            .any(|r| matches!(r, PlanRegression::ScanTypeUpgrade { .. })));
        assert!(regs
            .iter()
            .any(|r| matches!(r, PlanRegression::IndexLost { .. })));
        assert!(regs
            .iter()
            .any(|r| matches!(r, PlanRegression::RowsGrowth { .. })));
    }

    #[test]
    fn check_regressions_configurable_threshold() {
        let mut base = PlanBaseline::default();
        base.upsert(PlanSnapshot::new(
            "q1",
            plan(ScanType::IndexRange, Some("idx"), 100),
        ));
        let mut cur = PlanBaseline::default();
        cur.upsert(PlanSnapshot::new(
            "q1",
            plan(ScanType::IndexRange, Some("idx"), 300),
        ));
        // factor=2: 300 > 100*2 → RowsGrowth
        let regs_strict =
            check_regressions(&base.to_json().unwrap(), &cur.to_json().unwrap(), 2).unwrap();
        assert!(regs_strict
            .iter()
            .any(|r| matches!(r, PlanRegression::RowsGrowth { .. })));
        // factor=5: 300 <= 100*5 → 无 RowsGrowth
        let regs_loose =
            check_regressions(&base.to_json().unwrap(), &cur.to_json().unwrap(), 5).unwrap();
        assert!(!regs_loose
            .iter()
            .any(|r| matches!(r, PlanRegression::RowsGrowth { .. })));
    }
}
