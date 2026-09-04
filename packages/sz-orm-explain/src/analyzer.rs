//! 查询计划深度分析：问题检测、索引建议、执行统计、成本估算、优化建议、基线比较。
//!
//! 所有类型在 `explain-analyzer` feature 下编译，复用
//! [`crate::ExplainPlan`] / [`crate::ScanType`] / [`crate::regression`] 既有类型。
//!
//! ## 主要类型
//!
//! - [`QueryPlanAnalyzer`] / [`PlanIssue`] — 计划问题检测
//! - [`IndexAdvisor`] / [`IndexAdvice`] / [`QueryPattern`] — 索引建议
//! - [`ExecutionStats`] / [`ExecutionStatsCollector`] — 执行统计
//! - [`CostEstimator`] / [`CostEstimate`] / [`CostFactors`] — 成本估算
//! - [`OptimizationReport`] / [`OptimizationSuggestion`] — 优化建议
//! - [`BaselineComparison`] / [`ComparisonResult`] — 基线比较

use crate::regression::{PlanBaseline, PlanRegression};
use crate::{ExplainPlan, ScanType};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

// =========================================================================
// QueryPlanAnalyzer — 计划问题检测
// =========================================================================

/// 问题严重度。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum IssueSeverity {
    /// 信息（无需处理）。
    Info,
    /// 警告（建议处理）。
    Warning,
    /// 严重（应处理）。
    Critical,
}

impl IssueSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }
}

/// 检测到的单个计划问题。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PlanIssue {
    /// 问题类别。
    pub kind: IssueKind,
    /// 严重度。
    pub severity: IssueSeverity,
    /// 人类可读描述。
    pub description: String,
}

/// 问题类别枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum IssueKind {
    /// 全表扫描。
    FullTableScan,
    /// 大结果集。
    LargeResult,
    /// 缺失索引。
    MissingIndex,
    /// 使用文件排序。
    Filesort,
    /// 使用临时表。
    TemporaryTable,
    /// 高成本估算。
    HighCost,
}

impl IssueKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FullTableScan => "full-table-scan",
            Self::LargeResult => "large-result",
            Self::MissingIndex => "missing-index",
            Self::Filesort => "filesort",
            Self::TemporaryTable => "temporary-table",
            Self::HighCost => "high-cost",
        }
    }
}

/// 查询计划分析器：检测 [`ExplainPlan`] 中的性能问题。
#[derive(Debug, Clone)]
pub struct QueryPlanAnalyzer {
    /// 大结果集行数阈值（默认 1000）。
    pub large_result_threshold: u64,
    /// 缺失索引行数阈值（默认 100）。
    pub missing_index_threshold: u64,
    /// 高成本阈值（默认 1000.0）。
    pub high_cost_threshold: f64,
}

impl Default for QueryPlanAnalyzer {
    fn default() -> Self {
        Self {
            large_result_threshold: 1000,
            missing_index_threshold: 100,
            high_cost_threshold: 1000.0,
        }
    }
}

impl QueryPlanAnalyzer {
    /// 创建默认分析器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置大结果集阈值。
    pub fn with_large_result_threshold(mut self, n: u64) -> Self {
        self.large_result_threshold = n;
        self
    }

    /// 设置缺失索引阈值。
    pub fn with_missing_index_threshold(mut self, n: u64) -> Self {
        self.missing_index_threshold = n;
        self
    }

    /// 设置高成本阈值。
    pub fn with_high_cost_threshold(mut self, c: f64) -> Self {
        self.high_cost_threshold = c;
        self
    }

    /// 分析单个执行计划，返回检测到的问题列表（按严重度降序）。
    pub fn analyze(&self, plan: &ExplainPlan) -> Vec<PlanIssue> {
        let mut issues = Vec::new();

        // 1. 全表扫描
        if plan.is_full_table_scan() {
            issues.push(PlanIssue {
                kind: IssueKind::FullTableScan,
                severity: IssueSeverity::Critical,
                description: format!(
                    "table '{}' uses full table scan (rows={})",
                    plan.table, plan.rows
                ),
            });
        }

        // 2. 大结果集
        if plan.is_large_result(self.large_result_threshold) {
            issues.push(PlanIssue {
                kind: IssueKind::LargeResult,
                severity: IssueSeverity::Warning,
                description: format!(
                    "table '{}' returns {} rows (threshold={})",
                    plan.table, plan.rows, self.large_result_threshold
                ),
            });
        }

        // 3. 缺失索引
        if plan.missing_index(self.missing_index_threshold) {
            issues.push(PlanIssue {
                kind: IssueKind::MissingIndex,
                severity: IssueSeverity::Critical,
                description: format!(
                    "table '{}' missing index (scan={:?}, rows={})",
                    plan.table, plan.scan_type, plan.rows
                ),
            });
        }

        // 4. Using filesort
        if plan.has_extra("filesort") || plan.has_extra("Using filesort") {
            issues.push(PlanIssue {
                kind: IssueKind::Filesort,
                severity: IssueSeverity::Warning,
                description: format!("table '{}' requires filesort", plan.table),
            });
        }

        // 5. Using temporary
        if plan.has_extra("temporary") || plan.has_extra("Using temporary") {
            issues.push(PlanIssue {
                kind: IssueKind::TemporaryTable,
                severity: IssueSeverity::Warning,
                description: format!("table '{}' uses temporary table", plan.table),
            });
        }

        // 6. 高成本（基于行数与扫描类型估算）
        let cost = estimate_cost(plan);
        if cost > self.high_cost_threshold {
            issues.push(PlanIssue {
                kind: IssueKind::HighCost,
                severity: IssueSeverity::Critical,
                description: format!(
                    "table '{}' estimated cost {:.2} exceeds threshold {:.2}",
                    plan.table, cost, self.high_cost_threshold
                ),
            });
        }

        // 按严重度降序排列
        issues.sort_by_key(|i| std::cmp::Reverse(i.severity));
        issues
    }

    /// 分析多个计划，返回 (计划索引, 问题列表) 对。
    pub fn analyze_many(&self, plans: &[ExplainPlan]) -> Vec<(usize, Vec<PlanIssue>)> {
        plans
            .iter()
            .enumerate()
            .map(|(i, p)| (i, self.analyze(p)))
            .collect()
    }

    /// 是否存在严重问题。
    pub fn has_critical(&self, plan: &ExplainPlan) -> bool {
        self.analyze(plan)
            .iter()
            .any(|i| i.severity == IssueSeverity::Critical)
    }
}

/// 简单成本估算（内部复用）：全表扫描 = rows，索引扫描 = log2(rows+1)*10。
fn estimate_cost(plan: &ExplainPlan) -> f64 {
    let rows = plan.rows as f64;
    match plan.scan_type {
        ScanType::FullTable => rows,
        ScanType::IndexRange => rows * 0.5,
        ScanType::IndexLookup => (rows + 1.0).log2() * 10.0,
        ScanType::UniqueLookup => (rows + 1.0).log2() * 5.0,
        ScanType::Other => rows * 0.8,
    }
}

// =========================================================================
// IndexAdvisor — 索引建议
// =========================================================================

/// 查询模式描述：表名 + WHERE/ORDER BY/JOIN 列。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QueryPattern {
    /// 表名。
    pub table: String,
    /// WHERE 条件列（等值/范围）。
    pub where_columns: Vec<String>,
    /// ORDER BY 列。
    pub order_by_columns: Vec<String>,
    /// JOIN 列。
    pub join_columns: Vec<String>,
}

impl QueryPattern {
    /// 创建查询模式。
    pub fn new(table: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            where_columns: Vec::new(),
            order_by_columns: Vec::new(),
            join_columns: Vec::new(),
        }
    }

    /// 添加 WHERE 列。
    pub fn with_where(mut self, col: impl Into<String>) -> Self {
        self.where_columns.push(col.into());
        self
    }

    /// 添加 ORDER BY 列。
    pub fn with_order_by(mut self, col: impl Into<String>) -> Self {
        self.order_by_columns.push(col.into());
        self
    }

    /// 添加 JOIN 列。
    pub fn with_join(mut self, col: impl Into<String>) -> Self {
        self.join_columns.push(col.into());
        self
    }

    /// 是否有 WHERE 条件。
    pub fn has_where(&self) -> bool {
        !self.where_columns.is_empty()
    }
}

/// 单条索引建议。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IndexAdvice {
    /// 目标表。
    pub table: String,
    /// 建议的索引列（有序）。
    pub columns: Vec<String>,
    /// 建议理由。
    pub reason: String,
    /// 预估收益等级（1=低，2=中，3=高）。
    pub benefit_level: u8,
}

impl IndexAdvice {
    /// 生成建议的索引名（`idx_<col1>_<col2>`）。
    pub fn suggested_name(&self) -> String {
        format!("idx_{}", self.columns.join("_"))
    }
}

/// 索引建议器：基于 [`QueryPattern`] 生成 [`IndexAdvice`]。
#[derive(Debug, Clone, Default)]
pub struct IndexAdvisor {
    /// 已存在的索引（表名 → 索引列集合列表），用于避免重复建议。
    pub existing_indexes: HashMap<String, Vec<Vec<String>>>,
}

impl IndexAdvisor {
    /// 创建空的建议器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册已存在的索引。
    pub fn with_existing_index(mut self, table: &str, columns: Vec<String>) -> Self {
        self.existing_indexes
            .entry(table.to_string())
            .or_default()
            .push(columns);
        self
    }

    /// 为单个查询模式生成索引建议。
    pub fn advise(&self, pattern: &QueryPattern) -> Option<IndexAdvice> {
        // 无 WHERE 且无 JOIN 时无需索引
        if pattern.where_columns.is_empty() && pattern.join_columns.is_empty() {
            return None;
        }

        // 构造建议列：WHERE 等值列 + ORDER BY 列（去重保序）
        let mut columns = Vec::new();
        for col in &pattern.where_columns {
            if !columns.contains(col) {
                columns.push(col.clone());
            }
        }
        for col in &pattern.order_by_columns {
            if !columns.contains(col) {
                columns.push(col.clone());
            }
        }
        for col in &pattern.join_columns {
            if !columns.contains(col) {
                columns.push(col.clone());
            }
        }

        if columns.is_empty() {
            return None;
        }

        // 检查是否已存在覆盖该列前缀的索引
        if self.has_covering_index(&pattern.table, &columns) {
            return None;
        }

        // 收益等级：JOIN 列 +1，WHERE 多列 +1，ORDER BY +1
        let mut benefit = 1u8;
        if !pattern.join_columns.is_empty() {
            benefit += 1;
        }
        if pattern.where_columns.len() > 1 {
            benefit += 1;
        }
        if !pattern.order_by_columns.is_empty() {
            benefit += 1;
        }
        let benefit = benefit.min(3);

        let reason = if !pattern.join_columns.is_empty() {
            format!("join column(s) on '{}' benefit from index", pattern.table)
        } else if pattern.where_columns.len() > 1 {
            format!(
                "composite index on '{}' accelerates multi-column filter",
                pattern.table
            )
        } else {
            format!(
                "single-column index on '{}' accelerates lookup",
                pattern.table
            )
        };

        Some(IndexAdvice {
            table: pattern.table.clone(),
            columns,
            reason,
            benefit_level: benefit,
        })
    }

    /// 为多个查询模式生成建议（去重：同表同列只保留首条）。
    pub fn advise_many(&self, patterns: &[QueryPattern]) -> Vec<IndexAdvice> {
        let mut seen: Vec<(String, Vec<String>)> = Vec::new();
        let mut advices = Vec::new();
        for p in patterns {
            if let Some(advice) = self.advise(p) {
                let key = (advice.table.clone(), advice.columns.clone());
                if !seen.contains(&key) {
                    seen.push(key);
                    advices.push(advice);
                }
            }
        }
        advices
    }

    /// 检查表是否已有覆盖指定列前缀的索引。
    fn has_covering_index(&self, table: &str, columns: &[String]) -> bool {
        let existing = match self.existing_indexes.get(table) {
            Some(v) => v,
            None => return false,
        };
        for idx_cols in existing {
            // 已有索引的前缀覆盖建议列
            if idx_cols.len() >= columns.len() {
                let prefix = &idx_cols[..columns.len()];
                if prefix == columns {
                    return true;
                }
            }
        }
        false
    }
}

// =========================================================================
// ExecutionStats — 执行统计
// =========================================================================

/// 单个查询的执行统计快照。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExecutionStats {
    /// 查询标识。
    pub query_key: String,
    /// 总执行次数。
    pub total_executions: u64,
    /// 成功次数。
    pub success_count: u64,
    /// 失败次数。
    pub failure_count: u64,
    /// 累计耗时（毫秒）。
    pub total_elapsed_ms: u64,
    /// 最小耗时（毫秒）。
    pub min_elapsed_ms: u64,
    /// 最大耗时（毫秒）。
    pub max_elapsed_ms: u64,
}

impl ExecutionStats {
    /// 创建空统计。
    pub fn new(query_key: impl Into<String>) -> Self {
        Self {
            query_key: query_key.into(),
            total_executions: 0,
            success_count: 0,
            failure_count: 0,
            total_elapsed_ms: 0,
            min_elapsed_ms: 0,
            max_elapsed_ms: 0,
        }
    }

    /// 平均耗时（毫秒）。
    pub fn avg_elapsed_ms(&self) -> f64 {
        if self.total_executions == 0 {
            0.0
        } else {
            self.total_elapsed_ms as f64 / self.total_executions as f64
        }
    }

    /// 错误率（0.0-1.0）。
    pub fn error_rate(&self) -> f64 {
        if self.total_executions == 0 {
            0.0
        } else {
            self.failure_count as f64 / self.total_executions as f64
        }
    }

    /// 记录一次执行。
    pub fn record(&mut self, elapsed: Duration, success: bool) {
        let ms = elapsed.as_millis() as u64;
        self.total_executions += 1;
        if success {
            self.success_count += 1;
        } else {
            self.failure_count += 1;
        }
        self.total_elapsed_ms = self.total_elapsed_ms.saturating_add(ms);
        if self.min_elapsed_ms == 0 || ms < self.min_elapsed_ms {
            self.min_elapsed_ms = ms;
        }
        if ms > self.max_elapsed_ms {
            self.max_elapsed_ms = ms;
        }
    }
}

/// 执行统计采集器（线程安全）。
pub struct ExecutionStatsCollector {
    stats: Mutex<HashMap<String, ExecutionStats>>,
    global_total: AtomicU64,
    global_failures: AtomicU64,
}

impl Default for ExecutionStatsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionStatsCollector {
    /// 创建空采集器。
    pub fn new() -> Self {
        Self {
            stats: Mutex::new(HashMap::new()),
            global_total: AtomicU64::new(0),
            global_failures: AtomicU64::new(0),
        }
    }

    /// 记录一次执行。
    pub fn record(&self, query_key: &str, elapsed: Duration, success: bool) {
        self.global_total.fetch_add(1, Ordering::Relaxed);
        if !success {
            self.global_failures.fetch_add(1, Ordering::Relaxed);
        }
        if let Ok(mut stats) = self.stats.lock() {
            let entry = stats
                .entry(query_key.to_string())
                .or_insert_with(|| ExecutionStats::new(query_key));
            entry.record(elapsed, success);
        }
    }

    /// 获取某查询的统计快照。
    pub fn stats(&self, query_key: &str) -> Option<ExecutionStats> {
        self.stats.lock().ok()?.get(query_key).cloned()
    }

    /// 获取全部统计快照。
    pub fn all_stats(&self) -> Vec<ExecutionStats> {
        match self.stats.lock() {
            Ok(stats) => stats.values().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 全局总执行次数。
    pub fn global_total(&self) -> u64 {
        self.global_total.load(Ordering::Relaxed)
    }

    /// 全局失败次数。
    pub fn global_failures(&self) -> u64 {
        self.global_failures.load(Ordering::Relaxed)
    }

    /// 全局错误率。
    pub fn global_error_rate(&self) -> f64 {
        let total = self.global_total();
        if total == 0 {
            0.0
        } else {
            self.global_failures() as f64 / total as f64
        }
    }

    /// 列出平均耗时超过阈值的查询。
    pub fn slow_queries(&self, threshold_ms: f64) -> Vec<String> {
        match self.stats.lock() {
            Ok(stats) => stats
                .values()
                .filter(|s| s.avg_elapsed_ms() > threshold_ms)
                .map(|s| s.query_key.clone())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 重置全部统计。
    pub fn reset(&self) {
        if let Ok(mut stats) = self.stats.lock() {
            stats.clear();
        }
        self.global_total.store(0, Ordering::Relaxed);
        self.global_failures.store(0, Ordering::Relaxed);
    }
}

// =========================================================================
// CostEstimator — 成本估算
// =========================================================================

/// 成本因子分解。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CostFactors {
    /// IO 成本（页读取）。
    pub io_cost: f64,
    /// CPU 成本（行处理）。
    pub cpu_cost: f64,
    /// 行数成本（结果传输）。
    pub rows_cost: f64,
}

impl CostFactors {
    /// 总成本。
    pub fn total(&self) -> f64 {
        self.io_cost + self.cpu_cost + self.rows_cost
    }
}

/// 成本估算结果。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CostEstimate {
    /// 目标表。
    pub table: String,
    /// 成本因子。
    pub factors: CostFactors,
    /// 总成本。
    pub total_cost: f64,
}

/// 成本估算器：基于 [`ExplainPlan`] 估算查询成本。
#[derive(Debug, Clone)]
pub struct CostEstimator {
    /// 每页 IO 成本系数（默认 1.0）。
    pub io_cost_per_page: f64,
    /// 每行 CPU 成本系数（默认 0.1）。
    pub cpu_cost_per_row: f64,
    /// 每行结果成本系数（默认 0.05）。
    pub rows_cost_per_row: f64,
    /// 每页行数（默认 100）。
    pub rows_per_page: u64,
}

impl Default for CostEstimator {
    fn default() -> Self {
        Self {
            io_cost_per_page: 1.0,
            cpu_cost_per_row: 0.1,
            rows_cost_per_row: 0.05,
            rows_per_page: 100,
        }
    }
}

impl CostEstimator {
    /// 创建默认估算器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 估算单个计划的成本。
    pub fn estimate(&self, plan: &ExplainPlan) -> CostEstimate {
        let rows = plan.rows as f64;
        let pages = if self.rows_per_page > 0 {
            plan.rows.div_ceil(self.rows_per_page)
        } else {
            plan.rows
        } as f64;

        let (io_cost, cpu_cost) = match plan.scan_type {
            ScanType::FullTable => {
                // 全表扫描：读所有页 + 处理所有行
                (pages * self.io_cost_per_page, rows * self.cpu_cost_per_row)
            }
            ScanType::IndexRange => {
                // 索引范围扫描：读部分页
                (
                    pages * 0.3 * self.io_cost_per_page,
                    rows * 0.5 * self.cpu_cost_per_row,
                )
            }
            ScanType::IndexLookup => {
                // 索引点查：少量页
                (
                    3.0 * self.io_cost_per_page,
                    rows * 0.2 * self.cpu_cost_per_row,
                )
            }
            ScanType::UniqueLookup => {
                // 唯一索引点查：1 页
                (1.0 * self.io_cost_per_page, 1.0 * self.cpu_cost_per_row)
            }
            ScanType::Other => (
                pages * 0.5 * self.io_cost_per_page,
                rows * 0.5 * self.cpu_cost_per_row,
            ),
        };

        let rows_cost = rows * self.rows_cost_per_row;
        let factors = CostFactors {
            io_cost,
            cpu_cost,
            rows_cost,
        };
        CostEstimate {
            table: plan.table.clone(),
            total_cost: factors.total(),
            factors,
        }
    }

    /// 估算多个计划的总成本。
    pub fn estimate_total(&self, plans: &[ExplainPlan]) -> f64 {
        plans.iter().map(|p| self.estimate(p).total_cost).sum()
    }

    /// 比较两个计划的成本差异（正数表示 current 更贵）。
    pub fn cost_delta(&self, baseline: &ExplainPlan, current: &ExplainPlan) -> f64 {
        self.estimate(current).total_cost - self.estimate(baseline).total_cost
    }
}

// =========================================================================
// OptimizationSuggestion — 优化建议
// =========================================================================

/// 优化建议类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SuggestionKind {
    /// 添加索引。
    AddIndex,
    /// 重写查询。
    RewriteQuery,
    /// 分区表。
    PartitionTable,
    /// 调整连接顺序。
    AdjustJoinOrder,
    /// 限制返回行数。
    LimitRows,
    /// 使用覆盖索引。
    UseCoveringIndex,
}

impl SuggestionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AddIndex => "add-index",
            Self::RewriteQuery => "rewrite-query",
            Self::PartitionTable => "partition-table",
            Self::AdjustJoinOrder => "adjust-join-order",
            Self::LimitRows => "limit-rows",
            Self::UseCoveringIndex => "use-covering-index",
        }
    }
}

/// 单条优化建议。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OptimizationSuggestion {
    /// 建议类别。
    pub kind: SuggestionKind,
    /// 目标表（可为空）。
    pub table: String,
    /// 建议描述。
    pub description: String,
    /// 预估收益（0.0-1.0，1.0 表示完全消除成本）。
    pub estimated_improvement: f64,
}

/// 优化建议报告。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OptimizationReport {
    /// 目标查询标识。
    pub query_key: String,
    /// 建议列表（按预估收益降序）。
    pub suggestions: Vec<OptimizationSuggestion>,
    /// 整体评分（0-100，越高越好）。
    pub overall_score: u8,
}

impl OptimizationReport {
    /// 是否有建议。
    pub fn has_suggestions(&self) -> bool {
        !self.suggestions.is_empty()
    }

    /// 建议数量。
    pub fn len(&self) -> usize {
        self.suggestions.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.suggestions.is_empty()
    }

    /// 最高预估收益。
    pub fn max_improvement(&self) -> f64 {
        self.suggestions
            .iter()
            .map(|s| s.estimated_improvement)
            .fold(0.0_f64, f64::max)
    }
}

/// 优化建议生成器：结合 [`QueryPlanAnalyzer`] 与 [`CostEstimator`] 生成建议。
#[derive(Debug, Clone)]
pub struct OptimizationAdvisor {
    analyzer: QueryPlanAnalyzer,
    cost_estimator: CostEstimator,
}

impl Default for OptimizationAdvisor {
    fn default() -> Self {
        Self::new()
    }
}

impl OptimizationAdvisor {
    /// 创建默认建议器。
    pub fn new() -> Self {
        Self {
            analyzer: QueryPlanAnalyzer::new(),
            cost_estimator: CostEstimator::new(),
        }
    }

    /// 为单个计划生成优化报告。
    pub fn advise(&self, query_key: &str, plan: &ExplainPlan) -> OptimizationReport {
        let issues = self.analyzer.analyze(plan);
        let cost = self.cost_estimator.estimate(plan);
        let mut suggestions = Vec::new();

        for issue in &issues {
            let (kind, improvement) = match issue.kind {
                IssueKind::FullTableScan => {
                    // 全表扫描 → 建议添加索引
                    let imp = if cost.total_cost > 0.0 {
                        1.0 - (plan.rows as f64 * 0.1 / cost.total_cost).min(1.0)
                    } else {
                        0.5
                    };
                    (SuggestionKind::AddIndex, imp)
                }
                IssueKind::MissingIndex => (SuggestionKind::AddIndex, 0.7),
                IssueKind::LargeResult => (SuggestionKind::LimitRows, 0.4),
                IssueKind::Filesort => (SuggestionKind::UseCoveringIndex, 0.5),
                IssueKind::TemporaryTable => (SuggestionKind::RewriteQuery, 0.3),
                IssueKind::HighCost => (SuggestionKind::PartitionTable, 0.6),
            };
            suggestions.push(OptimizationSuggestion {
                kind,
                table: plan.table.clone(),
                description: issue.description.clone(),
                estimated_improvement: improvement,
            });
        }

        // 按预估收益降序
        suggestions.sort_by(|a, b| {
            b.estimated_improvement
                .partial_cmp(&a.estimated_improvement)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 整体评分：100 - 严重问题数*20 - 警告数*5（下限 0）
        let critical_count = issues
            .iter()
            .filter(|i| i.severity == IssueSeverity::Critical)
            .count() as u8;
        let warning_count = issues
            .iter()
            .filter(|i| i.severity == IssueSeverity::Warning)
            .count() as u8;
        let score = 100u8
            .saturating_sub(critical_count.saturating_mul(20))
            .saturating_sub(warning_count.saturating_mul(5));

        OptimizationReport {
            query_key: query_key.to_string(),
            suggestions,
            overall_score: score,
        }
    }
}

// =========================================================================
// BaselineComparison — 基线比较
// =========================================================================

/// 基线比较结果。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ComparisonResult {
    /// 退化的查询数。
    pub regressions: Vec<PlanRegression>,
    /// 改善的查询数（扫描类型降级或行数减少）。
    pub improvements: Vec<String>,
    /// 未变化的查询数。
    pub unchanged: Vec<String>,
}

impl ComparisonResult {
    /// 是否有退化。
    pub fn has_regressions(&self) -> bool {
        !self.regressions.is_empty()
    }

    /// 是否有改善。
    pub fn has_improvements(&self) -> bool {
        !self.improvements.is_empty()
    }

    /// 退化数量。
    pub fn regression_count(&self) -> usize {
        self.regressions.len()
    }
}

/// 基线比较器：比较当前 [`PlanBaseline`] 与基线。
#[derive(Debug, Clone)]
pub struct BaselineComparison {
    /// 行数增长阈值倍数。
    pub rows_growth_factor: u64,
}

impl Default for BaselineComparison {
    fn default() -> Self {
        Self {
            rows_growth_factor: 2,
        }
    }
}

impl BaselineComparison {
    /// 创建默认比较器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置行数增长阈值倍数。
    pub fn with_rows_growth_factor(mut self, factor: u64) -> Self {
        self.rows_growth_factor = factor;
        self
    }

    /// 比较两个基线集合。
    pub fn compare(&self, baseline: &PlanBaseline, current: &PlanBaseline) -> ComparisonResult {
        let mut regressions = Vec::new();
        let mut improvements = Vec::new();
        let mut unchanged = Vec::new();

        for (key, base_snap) in &baseline.snapshots {
            if let Some(cur_snap) = current.snapshots.get(key) {
                let regs = crate::regression::compare(
                    &base_snap.plan,
                    &cur_snap.plan,
                    key,
                    self.rows_growth_factor,
                );
                if regs.is_empty() {
                    // 检查是否改善：扫描类型严重度降低或行数减少
                    if is_improved(&base_snap.plan, &cur_snap.plan) {
                        improvements.push(key.clone());
                    } else {
                        unchanged.push(key.clone());
                    }
                } else {
                    regressions.extend(regs);
                }
            }
        }

        ComparisonResult {
            regressions,
            improvements,
            unchanged,
        }
    }

    /// 从 JSON 比较两个基线集合。
    pub fn compare_json(
        &self,
        baseline_json: &str,
        current_json: &str,
    ) -> Result<ComparisonResult, serde_json::Error> {
        let baseline: PlanBaseline = serde_json::from_str(baseline_json)?;
        let current: PlanBaseline = serde_json::from_str(current_json)?;
        Ok(self.compare(&baseline, &current))
    }
}

/// 判断当前计划是否相对基线改善（扫描类型严重度降低或行数减少）。
fn is_improved(baseline: &ExplainPlan, current: &ExplainPlan) -> bool {
    let base_severity = scan_severity(baseline.scan_type);
    let cur_severity = scan_severity(current.scan_type);
    cur_severity < base_severity || (cur_severity == base_severity && current.rows < baseline.rows)
}

/// 扫描类型严重度（数值越大越严重）。
fn scan_severity(scan: ScanType) -> u8 {
    match scan {
        ScanType::FullTable => 5,
        ScanType::IndexRange => 4,
        ScanType::IndexLookup => 3,
        ScanType::UniqueLookup => 2,
        ScanType::Other => 1,
    }
}

// =========================================================================
// 测试
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(
        scan: ScanType,
        table: &str,
        index: Option<&str>,
        rows: u64,
        extra: Vec<String>,
    ) -> ExplainPlan {
        ExplainPlan {
            scan_type: scan,
            table: table.to_string(),
            index: index.map(|s| s.to_string()),
            rows,
            extra,
        }
    }

    // ---- QueryPlanAnalyzer (5 tests) ----

    #[test]
    fn analyzer_detects_full_table_scan() {
        let analyzer = QueryPlanAnalyzer::new();
        let p = plan(ScanType::FullTable, "users", None, 50000, vec![]);
        let issues = analyzer.analyze(&p);
        assert!(issues.iter().any(|i| i.kind == IssueKind::FullTableScan));
        assert!(analyzer.has_critical(&p));
    }

    #[test]
    fn analyzer_detects_large_result() {
        let analyzer = QueryPlanAnalyzer::new().with_large_result_threshold(100);
        let p = plan(ScanType::IndexLookup, "t", Some("idx"), 500, vec![]);
        let issues = analyzer.analyze(&p);
        assert!(issues.iter().any(|i| i.kind == IssueKind::LargeResult));
    }

    #[test]
    fn analyzer_detects_filesort_and_temporary() {
        let analyzer = QueryPlanAnalyzer::new();
        let p = plan(
            ScanType::IndexRange,
            "t",
            Some("idx"),
            10,
            vec!["Using filesort".into(), "Using temporary".into()],
        );
        let issues = analyzer.analyze(&p);
        assert!(issues.iter().any(|i| i.kind == IssueKind::Filesort));
        assert!(issues.iter().any(|i| i.kind == IssueKind::TemporaryTable));
    }

    #[test]
    fn analyzer_clean_plan_has_no_issues() {
        let analyzer = QueryPlanAnalyzer::new();
        let p = plan(ScanType::UniqueLookup, "t", Some("pk"), 1, vec![]);
        let issues = analyzer.analyze(&p);
        assert!(
            issues.is_empty(),
            "clean plan should have no issues, got {:?}",
            issues
        );
    }

    #[test]
    fn analyzer_issues_sorted_by_severity_desc() {
        let analyzer = QueryPlanAnalyzer::new();
        let p = plan(
            ScanType::FullTable,
            "t",
            None,
            50000,
            vec!["Using filesort".into()],
        );
        let issues = analyzer.analyze(&p);
        for w in issues.windows(2) {
            assert!(
                w[0].severity >= w[1].severity,
                "issues should be sorted by severity desc"
            );
        }
    }

    // ---- IndexAdvisor (5 tests) ----

    #[test]
    fn index_advisor_suggests_for_where_column() {
        let advisor = IndexAdvisor::new();
        let pattern = QueryPattern::new("users").with_where("email");
        let advice = advisor.advise(&pattern).expect("should advise");
        assert_eq!(advice.table, "users");
        assert_eq!(advice.columns, vec!["email".to_string()]);
        assert!(advice.benefit_level >= 1);
    }

    #[test]
    fn index_advisor_suggests_composite_for_multi_columns() {
        let advisor = IndexAdvisor::new();
        let pattern = QueryPattern::new("orders")
            .with_where("user_id")
            .with_where("status")
            .with_order_by("created_at");
        let advice = advisor.advise(&pattern).expect("should advise");
        assert_eq!(
            advice.columns,
            vec![
                "user_id".to_string(),
                "status".to_string(),
                "created_at".to_string()
            ]
        );
        assert!(advice.benefit_level >= 2);
    }

    #[test]
    fn index_advisor_skips_when_no_where_or_join() {
        let advisor = IndexAdvisor::new();
        let pattern = QueryPattern::new("t");
        assert!(advisor.advise(&pattern).is_none());
    }

    #[test]
    fn index_advisor_skips_when_index_exists() {
        let advisor = IndexAdvisor::new().with_existing_index("users", vec!["email".into()]);
        let pattern = QueryPattern::new("users").with_where("email");
        assert!(
            advisor.advise(&pattern).is_none(),
            "should not suggest existing index"
        );
    }

    #[test]
    fn index_advisor_advise_many_dedupes() {
        let advisor = IndexAdvisor::new();
        let patterns = vec![
            QueryPattern::new("t").with_where("a"),
            QueryPattern::new("t").with_where("a"),
        ];
        let advices = advisor.advise_many(&patterns);
        assert_eq!(advices.len(), 1, "duplicate patterns should be deduped");
    }

    // ---- ExecutionStats (4 tests) ----

    #[test]
    fn execution_stats_record_and_query() {
        let mut stats = ExecutionStats::new("q1");
        stats.record(Duration::from_millis(10), true);
        stats.record(Duration::from_millis(20), true);
        stats.record(Duration::from_millis(5), false);
        assert_eq!(stats.total_executions, 3);
        assert_eq!(stats.success_count, 2);
        assert_eq!(stats.failure_count, 1);
        assert_eq!(stats.min_elapsed_ms, 5);
        assert_eq!(stats.max_elapsed_ms, 20);
        let avg = stats.avg_elapsed_ms();
        assert!((11.0..=12.0).contains(&avg));
    }

    #[test]
    fn execution_stats_error_rate() {
        let mut stats = ExecutionStats::new("q1");
        for _ in 0..7 {
            stats.record(Duration::from_millis(1), true);
        }
        for _ in 0..3 {
            stats.record(Duration::from_millis(1), false);
        }
        let rate = stats.error_rate();
        assert!((0.29..=0.31).contains(&rate));
    }

    #[test]
    fn execution_stats_collector_thread_safe() {
        let collector = ExecutionStatsCollector::new();
        collector.record("q1", Duration::from_millis(10), true);
        collector.record("q1", Duration::from_millis(20), false);
        collector.record("q2", Duration::from_millis(5), true);
        assert_eq!(collector.global_total(), 3);
        assert_eq!(collector.global_failures(), 1);
        let q1 = collector.stats("q1").expect("q1 should exist");
        assert_eq!(q1.total_executions, 2);
    }

    #[test]
    fn execution_stats_collector_slow_queries() {
        let collector = ExecutionStatsCollector::new();
        for _ in 0..10 {
            collector.record("fast", Duration::from_millis(1), true);
        }
        for _ in 0..5 {
            collector.record("slow", Duration::from_millis(100), true);
        }
        let slow = collector.slow_queries(50.0);
        assert_eq!(slow, vec!["slow".to_string()]);
    }

    // ---- CostEstimator (4 tests) ----

    #[test]
    fn cost_estimator_full_table_scan_expensive() {
        let estimator = CostEstimator::new();
        let full = plan(ScanType::FullTable, "t", None, 10000, vec![]);
        let lookup = plan(ScanType::UniqueLookup, "t", Some("pk"), 1, vec![]);
        let full_cost = estimator.estimate(&full).total_cost;
        let lookup_cost = estimator.estimate(&lookup).total_cost;
        assert!(
            full_cost > lookup_cost,
            "full scan should be more expensive"
        );
    }

    #[test]
    fn cost_estimator_factors_decomposition() {
        let estimator = CostEstimator::new();
        let p = plan(ScanType::IndexRange, "t", Some("idx"), 500, vec![]);
        let estimate = estimator.estimate(&p);
        assert!(estimate.factors.io_cost > 0.0);
        assert!(estimate.factors.cpu_cost > 0.0);
        assert!(estimate.factors.rows_cost > 0.0);
        let sum = estimate.factors.io_cost + estimate.factors.cpu_cost + estimate.factors.rows_cost;
        assert!((sum - estimate.total_cost).abs() < 0.001);
    }

    #[test]
    fn cost_estimator_total_for_multiple_plans() {
        let estimator = CostEstimator::new();
        let plans = vec![
            plan(ScanType::FullTable, "a", None, 1000, vec![]),
            plan(ScanType::UniqueLookup, "b", Some("pk"), 1, vec![]),
        ];
        let total = estimator.estimate_total(&plans);
        assert!(total > 0.0);
    }

    #[test]
    fn cost_estimator_delta() {
        let estimator = CostEstimator::new();
        let base = plan(ScanType::FullTable, "t", None, 10000, vec![]);
        let cur = plan(ScanType::UniqueLookup, "t", Some("pk"), 1, vec![]);
        let delta = estimator.cost_delta(&base, &cur);
        assert!(delta < 0.0, "current should be cheaper than baseline");
    }

    // ---- OptimizationAdvisor (4 tests) ----

    #[test]
    fn optimization_advisor_generates_suggestions_for_full_scan() {
        let advisor = OptimizationAdvisor::new();
        let p = plan(ScanType::FullTable, "t", None, 50000, vec![]);
        let report = advisor.advise("q1", &p);
        assert!(report.has_suggestions());
        assert!(report
            .suggestions
            .iter()
            .any(|s| s.kind == SuggestionKind::AddIndex));
        assert!(report.overall_score < 100);
    }

    #[test]
    fn optimization_advisor_clean_plan_high_score() {
        let advisor = OptimizationAdvisor::new();
        let p = plan(ScanType::UniqueLookup, "t", Some("pk"), 1, vec![]);
        let report = advisor.advise("q1", &p);
        assert!(!report.has_suggestions());
        assert_eq!(report.overall_score, 100);
    }

    #[test]
    fn optimization_advisor_suggestions_sorted_by_improvement() {
        let advisor = OptimizationAdvisor::new();
        let p = plan(
            ScanType::FullTable,
            "t",
            None,
            50000,
            vec!["Using filesort".into()],
        );
        let report = advisor.advise("q1", &p);
        for w in report.suggestions.windows(2) {
            assert!(
                w[0].estimated_improvement >= w[1].estimated_improvement,
                "suggestions should be sorted by improvement desc"
            );
        }
    }

    #[test]
    fn optimization_report_max_improvement() {
        let advisor = OptimizationAdvisor::new();
        let p = plan(ScanType::FullTable, "t", None, 50000, vec![]);
        let report = advisor.advise("q1", &p);
        let max = report.max_improvement();
        assert!(max > 0.0);
    }

    // ---- BaselineComparison (4 tests) ----

    use crate::regression::PlanSnapshot;

    fn make_baseline(query_key: &str, p: ExplainPlan) -> PlanBaseline {
        let mut baseline = PlanBaseline::default();
        baseline.upsert(PlanSnapshot::new(query_key, p));
        baseline
    }

    #[test]
    fn baseline_comparison_detects_regression() {
        let cmp = BaselineComparison::new();
        let base = make_baseline(
            "q1",
            plan(ScanType::IndexRange, "t", Some("idx"), 100, vec![]),
        );
        let cur = make_baseline("q1", plan(ScanType::FullTable, "t", None, 50000, vec![]));
        let result = cmp.compare(&base, &cur);
        assert!(result.has_regressions());
        assert!(result.regression_count() > 0);
    }

    #[test]
    fn baseline_comparison_detects_improvement() {
        let cmp = BaselineComparison::new();
        let base = make_baseline("q1", plan(ScanType::FullTable, "t", None, 1000, vec![]));
        let cur = make_baseline(
            "q1",
            plan(ScanType::UniqueLookup, "t", Some("pk"), 1, vec![]),
        );
        let result = cmp.compare(&base, &cur);
        assert!(result.has_improvements());
        assert!(result.improvements.contains(&"q1".to_string()));
    }

    #[test]
    fn baseline_comparison_unchanged() {
        let cmp = BaselineComparison::new();
        let p = plan(ScanType::IndexRange, "t", Some("idx"), 100, vec![]);
        let base = make_baseline("q1", p.clone());
        let cur = make_baseline("q1", p);
        let result = cmp.compare(&base, &cur);
        assert!(!result.has_regressions());
        assert!(!result.has_improvements());
        assert!(result.unchanged.contains(&"q1".to_string()));
    }

    #[test]
    fn baseline_comparison_json_roundtrip() {
        let cmp = BaselineComparison::new().with_rows_growth_factor(3);
        let base = make_baseline(
            "q1",
            plan(ScanType::IndexRange, "t", Some("idx"), 100, vec![]),
        );
        let cur = make_baseline("q1", plan(ScanType::FullTable, "t", None, 1000, vec![]));
        let result = cmp
            .compare_json(&base.to_json().unwrap(), &cur.to_json().unwrap())
            .unwrap();
        assert!(result.has_regressions());
    }
}
