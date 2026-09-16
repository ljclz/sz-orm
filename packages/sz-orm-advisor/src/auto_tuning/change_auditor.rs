//! 变更审计（`query-auto-tuning` feature）
//!
//! 记录每次调优变更的完整上下文：建议内容、执行前后的计划摘要、
//! 性能指标、执行原因、时间戳，供事后追溯与合规审计。

use std::collections::HashMap;
use std::time::Instant;

use crate::suggestion::{OptimizationSuggestion, SuggestionType};

/// 变更类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    /// 应用建议
    Apply,
    /// 回滚建议
    Rollback,
}

/// 变更前后的计划摘要
#[derive(Debug, Clone)]
pub struct PlanSnapshot {
    /// 扫描类型描述
    pub scan_type: String,
    /// 表名
    pub table: String,
    /// 预估行数
    pub estimated_rows: u64,
}

/// 变更审计记录
#[derive(Debug, Clone)]
pub struct TuningChangeRecord {
    /// 建议内容
    pub suggestion: OptimizationSuggestion,
    /// 变更类型
    pub kind: ChangeKind,
    /// 变更前计划
    pub before_plan: Option<PlanSnapshot>,
    /// 变更后计划
    pub after_plan: Option<PlanSnapshot>,
    /// 变更前耗时（ms）
    pub before_ms: f64,
    /// 变更后耗时（ms）
    pub after_ms: f64,
    /// 变更原因
    pub reason: String,
    /// 时间戳
    pub timestamp: Instant,
}

/// 变更审计摘要
#[derive(Debug, Clone)]
pub struct AuditSummary {
    /// 总变更数
    pub total_changes: usize,
    /// 应用数
    pub apply_count: usize,
    /// 回滚数
    pub rollback_count: usize,
    /// 按建议类型统计
    pub by_type: HashMap<SuggestionType, usize>,
}

/// 变更审计器
#[derive(Debug)]
pub struct TuningChangeAuditor {
    records: HashMap<String, Vec<TuningChangeRecord>>,
}

impl TuningChangeAuditor {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }

    /// 记录建议应用
    #[allow(clippy::too_many_arguments)]
    pub fn record_apply(
        &mut self,
        query_key: &str,
        suggestion: OptimizationSuggestion,
        before_plan: Option<PlanSnapshot>,
        after_plan: Option<PlanSnapshot>,
        before_ms: f64,
        after_ms: f64,
        reason: impl Into<String>,
    ) {
        let record = TuningChangeRecord {
            suggestion,
            kind: ChangeKind::Apply,
            before_plan,
            after_plan,
            before_ms,
            after_ms,
            reason: reason.into(),
            timestamp: Instant::now(),
        };
        self.records
            .entry(query_key.to_string())
            .or_default()
            .push(record);
    }

    /// 记录建议回滚
    pub fn record_rollback(
        &mut self,
        query_key: &str,
        suggestion: OptimizationSuggestion,
        reason: impl Into<String>,
    ) {
        let record = TuningChangeRecord {
            suggestion,
            kind: ChangeKind::Rollback,
            before_plan: None,
            after_plan: None,
            before_ms: 0.0,
            after_ms: 0.0,
            reason: reason.into(),
            timestamp: Instant::now(),
        };
        self.records
            .entry(query_key.to_string())
            .or_default()
            .push(record);
    }

    /// 获取查询的变更历史
    pub fn history(&self, query_key: &str) -> &[TuningChangeRecord] {
        self.records
            .get(query_key)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// 生成审计摘要
    pub fn summary(&self) -> AuditSummary {
        let mut apply_count = 0;
        let mut rollback_count = 0;
        let mut by_type = HashMap::new();
        for records in self.records.values() {
            for r in records {
                match r.kind {
                    ChangeKind::Apply => apply_count += 1,
                    ChangeKind::Rollback => rollback_count += 1,
                }
                *by_type.entry(r.suggestion.suggestion_type).or_default() += 1;
            }
        }
        AuditSummary {
            total_changes: apply_count + rollback_count,
            apply_count,
            rollback_count,
            by_type,
        }
    }

    /// 所有记录数
    pub fn len(&self) -> usize {
        self.records.values().map(|v| v.len()).sum()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

impl Default for TuningChangeAuditor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suggestion() -> OptimizationSuggestion {
        OptimizationSuggestion::new(
            SuggestionType::AddIndex,
            "q1",
            "full scan",
            "CREATE INDEX idx ON t(a)",
            0.9,
        )
    }

    #[test]
    fn record_apply_and_history() {
        let mut auditor = TuningChangeAuditor::new();
        auditor.record_apply(
            "q1",
            suggestion(),
            Some(PlanSnapshot {
                scan_type: "FullTable".into(),
                table: "users".into(),
                estimated_rows: 10000,
            }),
            Some(PlanSnapshot {
                scan_type: "IndexRange".into(),
                table: "users".into(),
                estimated_rows: 100,
            }),
            200.0,
            10.0,
            "high confidence auto-apply",
        );
        let history = auditor.history("q1");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].kind, ChangeKind::Apply);
        assert!(history[0].before_ms > history[0].after_ms);
    }

    #[test]
    fn record_rollback() {
        let mut auditor = TuningChangeAuditor::new();
        auditor.record_apply("q1", suggestion(), None, None, 200.0, 250.0, "apply");
        auditor.record_rollback("q1", suggestion(), "performance regression");
        let history = auditor.history("q1");
        assert_eq!(history.len(), 2);
        assert_eq!(history[1].kind, ChangeKind::Rollback);
    }

    #[test]
    fn summary_counts() {
        let mut auditor = TuningChangeAuditor::new();
        auditor.record_apply("q1", suggestion(), None, None, 100.0, 50.0, "ok");
        auditor.record_apply("q2", suggestion(), None, None, 200.0, 100.0, "ok");
        auditor.record_rollback("q1", suggestion(), "regression");
        let summary = auditor.summary();
        assert_eq!(summary.total_changes, 3);
        assert_eq!(summary.apply_count, 2);
        assert_eq!(summary.rollback_count, 1);
    }

    #[test]
    fn empty_history_for_unknown_query() {
        let auditor = TuningChangeAuditor::new();
        assert!(auditor.history("unknown").is_empty());
        assert!(auditor.is_empty());
    }
}
