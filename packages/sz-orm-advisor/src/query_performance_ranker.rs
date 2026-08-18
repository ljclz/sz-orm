//! 查询性能排名器
//!
//! 提供 [`QueryPerformanceRanker`] 对查询按多维度排名，
//! 识别最需要优化的查询。

use std::collections::HashMap;
use std::fmt;

/// 查询性能指标
#[derive(Debug, Clone)]
pub struct QueryMetrics {
    /// 查询指纹（参数化后的模板）
    pub fingerprint: String,
    /// 调用次数
    pub call_count: u64,
    /// 总耗时（毫秒）
    pub total_time_ms: u64,
    /// 最大耗时（毫秒）
    pub max_time_ms: u64,
    /// 最小耗时（毫秒）
    pub min_time_ms: u64,
    /// 平均耗时（毫秒）
    pub avg_time_ms: f64,
    /// P95 耗时（毫秒）
    pub p95_time_ms: f64,
    /// P99 耗时（毫秒）
    pub p99_time_ms: f64,
    /// 总返回行数
    pub total_rows: u64,
    /// 平均返回行数
    pub avg_rows: f64,
    /// 临时表使用次数
    pub temp_table_count: u64,
    /// 文件排序次数
    pub filesort_count: u64,
    /// 全表扫描次数
    pub full_scan_count: u64,
    /// 索引使用次数
    pub index_used_count: u64,
    /// 最后执行时间戳
    pub last_executed: u64,
}

impl QueryMetrics {
    /// 创建新指标
    #[must_use]
    pub fn new(fingerprint: &str) -> Self {
        Self {
            fingerprint: fingerprint.to_string(),
            call_count: 0,
            total_time_ms: 0,
            max_time_ms: 0,
            min_time_ms: u64::MAX,
            avg_time_ms: 0.0,
            p95_time_ms: 0.0,
            p99_time_ms: 0.0,
            total_rows: 0,
            avg_rows: 0.0,
            temp_table_count: 0,
            filesort_count: 0,
            full_scan_count: 0,
            index_used_count: 0,
            last_executed: 0,
        }
    }

    /// 记录一次执行
    pub fn record_execution(&mut self, elapsed_ms: u64, rows: u64, timestamp: u64) {
        self.call_count += 1;
        self.total_time_ms += elapsed_ms;
        self.max_time_ms = self.max_time_ms.max(elapsed_ms);
        self.min_time_ms = self.min_time_ms.min(elapsed_ms);
        self.total_rows += rows;
        self.last_executed = self.last_executed.max(timestamp);
        self.avg_time_ms = self.total_time_ms as f64 / self.call_count as f64;
        self.avg_rows = self.total_rows as f64 / self.call_count as f64;
    }

    /// 设置百分位耗时
    pub fn set_percentiles(&mut self, p95: f64, p99: f64) {
        self.p95_time_ms = p95;
        self.p99_time_ms = p99;
    }

    /// 记算性能评分（0.0~100.0，越高越差）
    #[must_use]
    pub fn performance_score(&self) -> f64 {
        let time_score = self.avg_time_ms.min(1000.0) / 10.0;
        let scan_penalty = self.full_scan_count as f64 * 5.0;
        let temp_penalty = self.temp_table_count as f64 * 3.0;
        let sort_penalty = self.filesort_count as f64 * 2.0;
        let freq_factor = (self.call_count as f64).ln().max(1.0);
        (time_score + scan_penalty + temp_penalty + sort_penalty) * freq_factor
    }

    /// 索引命中率
    #[must_use]
    pub fn index_hit_rate(&self) -> f64 {
        let total = self.index_used_count + self.full_scan_count;
        if total == 0 {
            return 1.0;
        }
        self.index_used_count as f64 / total as f64
    }

    /// 是否有性能问题
    #[must_use]
    pub fn has_issues(&self) -> bool {
        self.full_scan_count > 0
            || self.temp_table_count > 0
            || self.filesort_count > 0
            || self.avg_time_ms > 100.0
    }

    /// 严重程度
    #[must_use]
    pub fn severity(&self) -> QuerySeverity {
        let score = self.performance_score();
        if score > 80.0 {
            QuerySeverity::Critical
        } else if score > 50.0 {
            QuerySeverity::High
        } else if score > 20.0 {
            QuerySeverity::Medium
        } else {
            QuerySeverity::Low
        }
    }
}

impl fmt::Display for QueryMetrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "QueryMetrics({}, calls={}, avg={:.1}ms, score={:.1})",
            self.fingerprint,
            self.call_count,
            self.avg_time_ms,
            self.performance_score()
        )
    }
}

/// 查询严重程度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum QuerySeverity {
    /// 低
    Low,
    /// 中
    Medium,
    /// 高
    High,
    /// 严重
    Critical,
}

impl QuerySeverity {
    /// 返回描述
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            QuerySeverity::Low => "low",
            QuerySeverity::Medium => "medium",
            QuerySeverity::High => "high",
            QuerySeverity::Critical => "critical",
        }
    }

    /// 返回数值权重
    #[must_use]
    pub fn weight(&self) -> u32 {
        match self {
            QuerySeverity::Low => 1,
            QuerySeverity::Medium => 2,
            QuerySeverity::High => 3,
            QuerySeverity::Critical => 4,
        }
    }
}

/// 排名维度
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankBy {
    /// 按总耗时
    TotalTime,
    /// 按平均耗时
    AvgTime,
    /// 按调用次数
    CallCount,
    /// 按性能评分
    PerformanceScore,
    /// 按 P95 耗时
    P95Time,
    /// 按 P99 耗时
    P99Time,
}

/// 排名条目
#[derive(Debug, Clone)]
pub struct RankEntry {
    /// 排名（从 1 开始）
    pub rank: usize,
    /// 查询指纹
    pub fingerprint: String,
    /// 评分值
    pub score: f64,
    /// 严重程度
    pub severity: QuerySeverity,
    /// 调用次数
    pub call_count: u64,
    /// 平均耗时
    pub avg_time_ms: f64,
}

/// 查询性能排名器
#[derive(Debug, Default)]
pub struct QueryPerformanceRanker {
    /// 查询指标集合
    metrics: HashMap<String, QueryMetrics>,
}

impl QueryPerformanceRanker {
    /// 创建新排名器
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加或更新查询指标
    pub fn upsert(&mut self, metrics: QueryMetrics) {
        self.metrics.insert(metrics.fingerprint.clone(), metrics);
    }

    /// 记录查询执行
    pub fn record(&mut self, fingerprint: &str, elapsed_ms: u64, rows: u64, timestamp: u64) {
        let entry = self
            .metrics
            .entry(fingerprint.to_string())
            .or_insert_with(|| QueryMetrics::new(fingerprint));
        entry.record_execution(elapsed_ms, rows, timestamp);
    }

    /// 标记全表扫描
    pub fn mark_full_scan(&mut self, fingerprint: &str) {
        if let Some(m) = self.metrics.get_mut(fingerprint) {
            m.full_scan_count += 1;
        }
    }

    /// 标记临时表
    pub fn mark_temp_table(&mut self, fingerprint: &str) {
        if let Some(m) = self.metrics.get_mut(fingerprint) {
            m.temp_table_count += 1;
        }
    }

    /// 标记文件排序
    pub fn mark_filesort(&mut self, fingerprint: &str) {
        if let Some(m) = self.metrics.get_mut(fingerprint) {
            m.filesort_count += 1;
        }
    }

    /// 标记索引使用
    pub fn mark_index_used(&mut self, fingerprint: &str) {
        if let Some(m) = self.metrics.get_mut(fingerprint) {
            m.index_used_count += 1;
        }
    }

    /// 获取指标
    #[must_use]
    pub fn get(&self, fingerprint: &str) -> Option<&QueryMetrics> {
        self.metrics.get(fingerprint)
    }

    /// 获取可变指标
    pub fn get_mut(&mut self, fingerprint: &str) -> Option<&mut QueryMetrics> {
        self.metrics.get_mut(fingerprint)
    }

    /// 所有指标
    #[must_use]
    pub fn all(&self) -> Vec<&QueryMetrics> {
        self.metrics.values().collect()
    }

    /// 按维度排名
    #[must_use]
    pub fn rank(&self, by: RankBy, limit: usize) -> Vec<RankEntry> {
        let mut entries: Vec<RankEntry> = self
            .metrics
            .values()
            .map(|m| RankEntry {
                rank: 0,
                fingerprint: m.fingerprint.clone(),
                score: self.score_by(m, by),
                severity: m.severity(),
                call_count: m.call_count,
                avg_time_ms: m.avg_time_ms,
            })
            .collect();
        entries.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (i, entry) in entries.iter_mut().enumerate() {
            entry.rank = i + 1;
        }
        entries.into_iter().take(limit).collect()
    }

    /// 按维度获取评分
    fn score_by(&self, m: &QueryMetrics, by: RankBy) -> f64 {
        match by {
            RankBy::TotalTime => m.total_time_ms as f64,
            RankBy::AvgTime => m.avg_time_ms,
            RankBy::CallCount => m.call_count as f64,
            RankBy::PerformanceScore => m.performance_score(),
            RankBy::P95Time => m.p95_time_ms,
            RankBy::P99Time => m.p99_time_ms,
        }
    }

    /// 获取有性能问题的查询
    #[must_use]
    pub fn problematic_queries(&self) -> Vec<&QueryMetrics> {
        self.metrics.values().filter(|m| m.has_issues()).collect()
    }

    /// 按严重程度过滤
    #[must_use]
    pub fn by_severity(&self, min_severity: QuerySeverity) -> Vec<&QueryMetrics> {
        self.metrics
            .values()
            .filter(|m| m.severity() >= min_severity)
            .collect()
    }

    /// 获取严重查询
    #[must_use]
    pub fn critical_queries(&self) -> Vec<&QueryMetrics> {
        self.by_severity(QuerySeverity::Critical)
    }

    /// 查询总数
    #[must_use]
    pub fn query_count(&self) -> usize {
        self.metrics.len()
    }

    /// 总调用次数
    #[must_use]
    pub fn total_calls(&self) -> u64 {
        self.metrics.values().map(|m| m.call_count).sum()
    }

    /// 总耗时
    #[must_use]
    pub fn total_time_ms(&self) -> u64 {
        self.metrics.values().map(|m| m.total_time_ms).sum()
    }

    /// 平均索引命中率
    #[must_use]
    pub fn avg_index_hit_rate(&self) -> f64 {
        if self.metrics.is_empty() {
            return 1.0;
        }
        let total: f64 = self.metrics.values().map(|m| m.index_hit_rate()).sum();
        total / self.metrics.len() as f64
    }

    /// 生成排名报告
    #[must_use]
    pub fn report(&self, top_n: usize) -> PerformanceReport {
        let top = self.rank(RankBy::PerformanceScore, top_n);
        let problematic = self.problematic_queries();
        let critical = self.critical_queries();
        PerformanceReport {
            total_queries: self.query_count(),
            total_calls: self.total_calls(),
            total_time_ms: self.total_time_ms(),
            avg_index_hit_rate: self.avg_index_hit_rate(),
            problematic_count: problematic.len(),
            critical_count: critical.len(),
            top_queries: top,
        }
    }
}

impl fmt::Display for QueryPerformanceRanker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "QueryPerformanceRanker(queries={}, calls={})",
            self.query_count(),
            self.total_calls()
        )
    }
}

/// 性能报告
#[derive(Debug, Clone)]
pub struct PerformanceReport {
    /// 查询总数
    pub total_queries: usize,
    /// 总调用次数
    pub total_calls: u64,
    /// 总耗时
    pub total_time_ms: u64,
    /// 平均索引命中率
    pub avg_index_hit_rate: f64,
    /// 有问题的查询数
    pub problematic_count: usize,
    /// 严重查询数
    pub critical_count: usize,
    /// Top N 查询
    pub top_queries: Vec<RankEntry>,
}

impl fmt::Display for PerformanceReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PerformanceReport(queries={}, calls={}, problematic={}, critical={})",
            self.total_queries, self.total_calls, self.problematic_count, self.critical_count
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_metrics_new() {
        let m = QueryMetrics::new("SELECT * FROM t");
        assert_eq!(m.fingerprint, "SELECT * FROM t");
        assert_eq!(m.call_count, 0);
    }

    #[test]
    fn test_record_execution() {
        let mut m = QueryMetrics::new("test");
        m.record_execution(10, 5, 1000);
        m.record_execution(20, 10, 2000);
        assert_eq!(m.call_count, 2);
        assert_eq!(m.total_time_ms, 30);
        assert_eq!(m.max_time_ms, 20);
        assert_eq!(m.min_time_ms, 10);
        assert!((m.avg_time_ms - 15.0).abs() < 1e-10);
        assert!((m.avg_rows - 7.5).abs() < 1e-10);
        assert_eq!(m.last_executed, 2000);
    }

    #[test]
    fn test_set_percentiles() {
        let mut m = QueryMetrics::new("test");
        m.set_percentiles(50.0, 80.0);
        assert!((m.p95_time_ms - 50.0).abs() < 1e-10);
        assert!((m.p99_time_ms - 80.0).abs() < 1e-10);
    }

    #[test]
    fn test_performance_score() {
        let mut m = QueryMetrics::new("test");
        m.record_execution(100, 1, 1000);
        let score = m.performance_score();
        assert!(score > 0.0);
    }

    #[test]
    fn test_performance_score_with_full_scan() {
        let mut m = QueryMetrics::new("test");
        m.record_execution(10, 1, 1000);
        m.full_scan_count = 5;
        let score = m.performance_score();
        assert!(score > 20.0);
    }

    #[test]
    fn test_index_hit_rate_no_data() {
        let m = QueryMetrics::new("test");
        assert!((m.index_hit_rate() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_index_hit_rate_with_data() {
        let mut m = QueryMetrics::new("test");
        m.index_used_count = 8;
        m.full_scan_count = 2;
        assert!((m.index_hit_rate() - 0.8).abs() < 1e-10);
    }

    #[test]
    fn test_has_issues_no_issues() {
        let mut m = QueryMetrics::new("test");
        m.record_execution(10, 1, 1000);
        assert!(!m.has_issues());
    }

    #[test]
    fn test_has_issues_full_scan() {
        let mut m = QueryMetrics::new("test");
        m.full_scan_count = 1;
        assert!(m.has_issues());
    }

    #[test]
    fn test_has_issues_slow() {
        let mut m = QueryMetrics::new("test");
        m.record_execution(200, 1, 1000);
        assert!(m.has_issues());
    }

    #[test]
    fn test_severity_low() {
        let mut m = QueryMetrics::new("test");
        m.record_execution(5, 1, 1000);
        assert_eq!(m.severity(), QuerySeverity::Low);
    }

    #[test]
    fn test_severity_with_full_scan() {
        let mut m = QueryMetrics::new("test");
        m.record_execution(10, 1, 1000);
        m.full_scan_count = 10;
        assert!(m.severity() >= QuerySeverity::High);
    }

    #[test]
    fn test_query_severity_description() {
        assert_eq!(QuerySeverity::Low.description(), "low");
        assert_eq!(QuerySeverity::Critical.description(), "critical");
    }

    #[test]
    fn test_query_severity_weight() {
        assert_eq!(QuerySeverity::Low.weight(), 1);
        assert_eq!(QuerySeverity::Critical.weight(), 4);
    }

    #[test]
    fn test_query_severity_ordering() {
        assert!(QuerySeverity::Low < QuerySeverity::Medium);
        assert!(QuerySeverity::High < QuerySeverity::Critical);
    }

    #[test]
    fn test_query_metrics_display() {
        let m = QueryMetrics::new("test");
        let s = format!("{}", m);
        assert!(s.contains("QueryMetrics"));
    }

    #[test]
    fn test_ranker_new() {
        let r = QueryPerformanceRanker::new();
        assert_eq!(r.query_count(), 0);
    }

    #[test]
    fn test_ranker_upsert() {
        let mut r = QueryPerformanceRanker::new();
        r.upsert(QueryMetrics::new("q1"));
        assert_eq!(r.query_count(), 1);
    }

    #[test]
    fn test_ranker_record() {
        let mut r = QueryPerformanceRanker::new();
        r.record("q1", 10, 5, 1000);
        r.record("q1", 20, 10, 2000);
        let m = r.get("q1").unwrap();
        assert_eq!(m.call_count, 2);
    }

    #[test]
    fn test_ranker_mark_full_scan() {
        let mut r = QueryPerformanceRanker::new();
        r.record("q1", 10, 5, 1000);
        r.mark_full_scan("q1");
        let m = r.get("q1").unwrap();
        assert_eq!(m.full_scan_count, 1);
    }

    #[test]
    fn test_ranker_mark_temp_table() {
        let mut r = QueryPerformanceRanker::new();
        r.record("q1", 10, 5, 1000);
        r.mark_temp_table("q1");
        let m = r.get("q1").unwrap();
        assert_eq!(m.temp_table_count, 1);
    }

    #[test]
    fn test_ranker_mark_filesort() {
        let mut r = QueryPerformanceRanker::new();
        r.record("q1", 10, 5, 1000);
        r.mark_filesort("q1");
        let m = r.get("q1").unwrap();
        assert_eq!(m.filesort_count, 1);
    }

    #[test]
    fn test_ranker_mark_index_used() {
        let mut r = QueryPerformanceRanker::new();
        r.record("q1", 10, 5, 1000);
        r.mark_index_used("q1");
        let m = r.get("q1").unwrap();
        assert_eq!(m.index_used_count, 1);
    }

    #[test]
    fn test_ranker_get_nonexistent() {
        let r = QueryPerformanceRanker::new();
        assert!(r.get("q1").is_none());
    }

    #[test]
    fn test_ranker_all() {
        let mut r = QueryPerformanceRanker::new();
        r.record("q1", 10, 5, 1000);
        r.record("q2", 20, 5, 1000);
        assert_eq!(r.all().len(), 2);
    }

    #[test]
    fn test_ranker_rank_by_total_time() {
        let mut r = QueryPerformanceRanker::new();
        r.record("q1", 100, 5, 1000);
        r.record("q2", 50, 5, 1000);
        let ranked = r.rank(RankBy::TotalTime, 10);
        assert_eq!(ranked[0].fingerprint, "q1");
        assert_eq!(ranked[0].rank, 1);
    }

    #[test]
    fn test_ranker_rank_by_call_count() {
        let mut r = QueryPerformanceRanker::new();
        r.record("q1", 10, 5, 1000);
        r.record("q2", 10, 5, 1000);
        r.record("q2", 10, 5, 2000);
        let ranked = r.rank(RankBy::CallCount, 10);
        assert_eq!(ranked[0].fingerprint, "q2");
    }

    #[test]
    fn test_ranker_rank_limit() {
        let mut r = QueryPerformanceRanker::new();
        r.record("q1", 10, 5, 1000);
        r.record("q2", 20, 5, 1000);
        r.record("q3", 30, 5, 1000);
        let ranked = r.rank(RankBy::TotalTime, 2);
        assert_eq!(ranked.len(), 2);
    }

    #[test]
    fn test_problematic_queries() {
        let mut r = QueryPerformanceRanker::new();
        r.record("q1", 10, 5, 1000);
        r.record("q2", 10, 5, 1000);
        r.mark_full_scan("q2");
        let problematic = r.problematic_queries();
        assert_eq!(problematic.len(), 1);
    }

    #[test]
    fn test_by_severity() {
        let mut r = QueryPerformanceRanker::new();
        r.record("q1", 10, 5, 1000);
        r.record("q2", 10, 5, 1000);
        for _ in 0..10 {
            r.mark_full_scan("q2");
        }
        let high = r.by_severity(QuerySeverity::High);
        assert!(!high.is_empty());
    }

    #[test]
    fn test_critical_queries() {
        let mut r = QueryPerformanceRanker::new();
        r.record("q1", 10, 5, 1000);
        let critical = r.critical_queries();
        assert!(critical.is_empty());
    }

    #[test]
    fn test_total_calls() {
        let mut r = QueryPerformanceRanker::new();
        r.record("q1", 10, 5, 1000);
        r.record("q1", 10, 5, 1000);
        r.record("q2", 10, 5, 1000);
        assert_eq!(r.total_calls(), 3);
    }

    #[test]
    fn test_total_time_ms() {
        let mut r = QueryPerformanceRanker::new();
        r.record("q1", 100, 5, 1000);
        r.record("q2", 200, 5, 1000);
        assert_eq!(r.total_time_ms(), 300);
    }

    #[test]
    fn test_avg_index_hit_rate_empty() {
        let r = QueryPerformanceRanker::new();
        assert!((r.avg_index_hit_rate() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_avg_index_hit_rate_with_data() {
        let mut r = QueryPerformanceRanker::new();
        r.record("q1", 10, 5, 1000);
        r.mark_index_used("q1");
        r.mark_index_used("q1");
        r.mark_full_scan("q1");
        let rate = r.avg_index_hit_rate();
        assert!(rate > 0.0 && rate < 1.0);
    }

    #[test]
    fn test_report() {
        let mut r = QueryPerformanceRanker::new();
        r.record("q1", 10, 5, 1000);
        r.record("q2", 200, 5, 1000);
        let report = r.report(10);
        assert_eq!(report.total_queries, 2);
        assert_eq!(report.total_calls, 2);
    }

    #[test]
    fn test_report_display() {
        let r = QueryPerformanceRanker::new();
        let report = r.report(10);
        let s = format!("{}", report);
        assert!(s.contains("PerformanceReport"));
    }

    #[test]
    fn test_ranker_display() {
        let r = QueryPerformanceRanker::new();
        let s = format!("{}", r);
        assert!(s.contains("QueryPerformanceRanker"));
    }

    #[test]
    fn test_rank_by_avg_time() {
        let mut r = QueryPerformanceRanker::new();
        r.record("q1", 100, 5, 1000);
        r.record("q2", 50, 5, 1000);
        let ranked = r.rank(RankBy::AvgTime, 10);
        assert_eq!(ranked[0].fingerprint, "q1");
    }

    #[test]
    fn test_rank_by_performance_score() {
        let mut r = QueryPerformanceRanker::new();
        r.record("q1", 10, 5, 1000);
        r.record("q2", 10, 5, 1000);
        r.mark_full_scan("q2");
        let ranked = r.rank(RankBy::PerformanceScore, 10);
        assert_eq!(ranked[0].fingerprint, "q2");
    }

    #[test]
    fn test_rank_by_p95() {
        let mut r = QueryPerformanceRanker::new();
        r.upsert(QueryMetrics::new("q1"));
        let m = r.get_mut("q1").unwrap();
        m.set_percentiles(100.0, 150.0);
        r.upsert(QueryMetrics::new("q2"));
        let m = r.get_mut("q2").unwrap();
        m.set_percentiles(50.0, 80.0);
        let ranked = r.rank(RankBy::P95Time, 10);
        assert_eq!(ranked[0].fingerprint, "q1");
    }
}
