//! 诊断辅助工具：指标统计、瓶颈排序、修复动作建议
//!
//! 本模块不依赖 `slow-query-diagnosis` feature，可独立使用。

use std::collections::HashMap;

/// 诊断指标统计：跟踪各严重度的诊断次数
#[derive(Debug, Clone, Default)]
pub struct DiagnosisMetrics {
    info_count: u64,
    warning_count: u64,
    critical_count: u64,
}

impl DiagnosisMetrics {
    /// 创建空指标
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一次诊断（severity: 0=info, 1=warning, ≥2=critical）
    pub fn record(&mut self, severity: u8) {
        match severity {
            0 => self.info_count += 1,
            1 => self.warning_count += 1,
            _ => self.critical_count += 1,
        }
    }

    /// 总诊断次数
    pub fn total(&self) -> u64 {
        self.info_count + self.warning_count + self.critical_count
    }

    /// Info 级别次数
    pub fn info_count(&self) -> u64 {
        self.info_count
    }

    /// Warning 级别次数
    pub fn warning_count(&self) -> u64 {
        self.warning_count
    }

    /// Critical 级别次数
    pub fn critical_count(&self) -> u64 {
        self.critical_count
    }

    /// Critical 占比（空返回 0.0）
    pub fn critical_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            self.critical_count as f64 / total as f64
        }
    }
}

/// 瓶颈排序器：按频率排序各阶段瓶颈
#[derive(Debug, Clone, Default)]
pub struct BottleneckRanker {
    counts: HashMap<String, u64>,
}

impl BottleneckRanker {
    /// 创建空排序器
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加/累加某阶段的瓶颈次数
    pub fn add(&mut self, phase: &str, count: u64) {
        *self.counts.entry(phase.to_string()).or_insert(0) += count;
    }

    /// 返回 Top-N 瓶颈阶段（按次数降序）
    pub fn top_n(&self, n: usize) -> Vec<(String, u64)> {
        let mut entries: Vec<(String, u64)> =
            self.counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
        entries.sort_by_key(|(_, v)| std::cmp::Reverse(*v));
        entries.truncate(n);
        entries
    }

    /// 总阶段数
    pub fn total_phases(&self) -> usize {
        self.counts.len()
    }
}

/// 诊断汇总：聚合多次诊断的基本统计
#[derive(Debug, Clone, Default)]
pub struct DiagnosisSummary {
    count: usize,
    total_elapsed_ms: u64,
    max_elapsed_ms: u64,
    root_causes: HashMap<String, usize>,
}

impl DiagnosisSummary {
    /// 创建空汇总
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加一条诊断记录
    pub fn add_report(&mut self, root_cause: &str, elapsed_ms: u64) {
        self.count += 1;
        self.total_elapsed_ms += elapsed_ms;
        if elapsed_ms > self.max_elapsed_ms {
            self.max_elapsed_ms = elapsed_ms;
        }
        *self.root_causes.entry(root_cause.to_string()).or_insert(0) += 1;
    }

    /// 诊断总数
    pub fn count(&self) -> usize {
        self.count
    }

    /// 平均耗时（毫秒，空返回 0.0）
    pub fn avg_elapsed_ms(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.total_elapsed_ms as f64 / self.count as f64
        }
    }

    /// 最大耗时（毫秒）
    pub fn max_elapsed_ms(&self) -> u64 {
        self.max_elapsed_ms
    }

    /// 根因分布（按出现次数降序）
    pub fn root_cause_distribution(&self) -> Vec<(String, usize)> {
        let mut entries: Vec<(String, usize)> = self
            .root_causes
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        entries.sort_by_key(|(_, v)| std::cmp::Reverse(*v));
        entries
    }
}

/// 修复动作建议
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixAction {
    /// 添加索引（指定索引名）
    AddIndex(String),
    /// 改写查询
    RewriteQuery,
    /// 调整连接池大小（建议值）
    AdjustPoolSize(u64),
    /// 使用分页（每页行数）
    UsePagination(u64),
    /// 预编译查询
    PrecompileQuery,
}

impl FixAction {
    /// 动作描述
    pub fn description(&self) -> String {
        match self {
            Self::AddIndex(name) => format!("add index: {name}"),
            Self::RewriteQuery => "rewrite query to eliminate filesort/temporary".to_string(),
            Self::AdjustPoolSize(size) => format!("adjust pool size to {size}"),
            Self::UsePagination(page_size) => format!("use pagination with page size {page_size}"),
            Self::PrecompileQuery => "precompile query to reduce build overhead".to_string(),
        }
    }

    /// 优先级（0=最高，越大越低）
    pub fn priority(&self) -> u8 {
        match self {
            Self::AddIndex(_) => 0,
            Self::RewriteQuery => 1,
            Self::AdjustPoolSize(_) => 2,
            Self::UsePagination(_) => 3,
            Self::PrecompileQuery => 4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- DiagnosisMetrics tests ---

    #[test]
    fn metrics_new_empty() {
        let m = DiagnosisMetrics::new();
        assert_eq!(m.total(), 0);
        assert_eq!(m.info_count(), 0);
        assert_eq!(m.warning_count(), 0);
        assert_eq!(m.critical_count(), 0);
    }

    #[test]
    fn metrics_record_info() {
        let mut m = DiagnosisMetrics::new();
        m.record(0);
        m.record(0);
        assert_eq!(m.info_count(), 2);
        assert_eq!(m.total(), 2);
    }

    #[test]
    fn metrics_record_warning() {
        let mut m = DiagnosisMetrics::new();
        m.record(1);
        assert_eq!(m.warning_count(), 1);
    }

    #[test]
    fn metrics_record_critical() {
        let mut m = DiagnosisMetrics::new();
        m.record(2);
        m.record(3);
        assert_eq!(m.critical_count(), 2);
    }

    #[test]
    fn metrics_total_counts() {
        let mut m = DiagnosisMetrics::new();
        m.record(0);
        m.record(1);
        m.record(2);
        m.record(2);
        assert_eq!(m.total(), 4);
    }

    #[test]
    fn metrics_critical_rate() {
        let mut m = DiagnosisMetrics::new();
        m.record(0);
        m.record(2);
        m.record(2);
        assert!((m.critical_rate() - (2.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn metrics_critical_rate_empty() {
        let m = DiagnosisMetrics::new();
        assert_eq!(m.critical_rate(), 0.0);
    }

    // --- BottleneckRanker tests ---

    #[test]
    fn ranker_new_empty() {
        let r = BottleneckRanker::new();
        assert_eq!(r.total_phases(), 0);
        assert!(r.top_n(3).is_empty());
    }

    #[test]
    fn ranker_add_and_top() {
        let mut r = BottleneckRanker::new();
        r.add("pool.acquire", 10);
        r.add("db.execute", 5);
        let top = r.top_n(1);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].0, "pool.acquire");
        assert_eq!(top[0].1, 10);
    }

    #[test]
    fn ranker_top_n_sorted_desc() {
        let mut r = BottleneckRanker::new();
        r.add("a", 3);
        r.add("b", 10);
        r.add("c", 7);
        let top = r.top_n(3);
        assert_eq!(top[0].1, 10);
        assert_eq!(top[1].1, 7);
        assert_eq!(top[2].1, 3);
    }

    #[test]
    fn ranker_total_phases() {
        let mut r = BottleneckRanker::new();
        r.add("a", 1);
        r.add("b", 2);
        r.add("a", 3);
        assert_eq!(r.total_phases(), 2);
    }

    #[test]
    fn ranker_top_n_more_than_available() {
        let mut r = BottleneckRanker::new();
        r.add("a", 1);
        let top = r.top_n(10);
        assert_eq!(top.len(), 1);
    }

    // --- DiagnosisSummary tests ---

    #[test]
    fn summary_new_empty() {
        let s = DiagnosisSummary::new();
        assert_eq!(s.count(), 0);
        assert_eq!(s.avg_elapsed_ms(), 0.0);
        assert_eq!(s.max_elapsed_ms(), 0);
    }

    #[test]
    fn summary_add_report() {
        let mut s = DiagnosisSummary::new();
        s.add_report("pool-exhaustion", 100);
        s.add_report("sql-inefficiency", 200);
        assert_eq!(s.count(), 2);
    }

    #[test]
    fn summary_avg_elapsed() {
        let mut s = DiagnosisSummary::new();
        s.add_report("a", 100);
        s.add_report("b", 300);
        assert!((s.avg_elapsed_ms() - 200.0).abs() < 1e-9);
    }

    #[test]
    fn summary_max_elapsed() {
        let mut s = DiagnosisSummary::new();
        s.add_report("a", 100);
        s.add_report("b", 500);
        s.add_report("c", 200);
        assert_eq!(s.max_elapsed_ms(), 500);
    }

    #[test]
    fn summary_root_cause_distribution() {
        let mut s = DiagnosisSummary::new();
        s.add_report("a", 10);
        s.add_report("b", 20);
        s.add_report("a", 30);
        let dist = s.root_cause_distribution();
        assert_eq!(dist.len(), 2);
        assert_eq!(dist[0].0, "a");
        assert_eq!(dist[0].1, 2);
        assert_eq!(dist[1].0, "b");
        assert_eq!(dist[1].1, 1);
    }

    // --- FixAction tests ---

    #[test]
    fn fix_action_description_nonempty() {
        assert!(!FixAction::RewriteQuery.description().is_empty());
        assert!(!FixAction::PrecompileQuery.description().is_empty());
        assert!(!FixAction::AddIndex("idx".to_string())
            .description()
            .is_empty());
        assert!(!FixAction::AdjustPoolSize(10).description().is_empty());
        assert!(!FixAction::UsePagination(100).description().is_empty());
    }

    #[test]
    fn fix_action_priority_ordering() {
        assert!(
            FixAction::AddIndex("x".to_string()).priority() < FixAction::RewriteQuery.priority()
        );
        assert!(FixAction::RewriteQuery.priority() < FixAction::AdjustPoolSize(10).priority());
        assert!(
            FixAction::AdjustPoolSize(10).priority() < FixAction::UsePagination(100).priority()
        );
        assert!(FixAction::UsePagination(100).priority() < FixAction::PrecompileQuery.priority());
    }

    #[test]
    fn fix_action_all_variants_distinct() {
        let actions = [
            FixAction::AddIndex("a".to_string()),
            FixAction::RewriteQuery,
            FixAction::AdjustPoolSize(10),
            FixAction::UsePagination(100),
            FixAction::PrecompileQuery,
        ];
        for i in 0..actions.len() {
            for j in (i + 1)..actions.len() {
                assert_ne!(actions[i], actions[j]);
            }
        }
    }

    #[test]
    fn fix_action_add_index_description_contains_name() {
        let a = FixAction::AddIndex("idx_users_email".to_string());
        assert!(a.description().contains("idx_users_email"));
    }
}
