//! 索引工作负载驱动 + 组合优化模块
//!
//! 基于工作负载（查询日志 + 频率 + 耗时）分析高频查询模式，
//! 生成覆盖 Top N 慢查询的复合索引建议，并优化索引组合。
//!
//! 启用 `ai-index-advisor` feature 时编译。

use crate::advice_common::BenefitEstimate;
use crate::index_advisor::{IndexAdvisor, IndexError, IndexSuggestion, QueryPattern, SlowQueryLog};
use serde::{Deserialize, Serialize};

// ==================== 工作负载摘要 ====================

/// 时间范围
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    /// 起始 Unix 时间戳（秒）
    pub start: i64,
    /// 结束 Unix 时间戳（秒）
    pub end: i64,
}

/// 工作负载摘要
///
/// 包含查询日志、时间范围和总查询数，用于驱动索引建议。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadSummary {
    /// 慢查询日志
    pub query_logs: Vec<SlowQueryLog>,
    /// 时间范围
    pub time_range: TimeRange,
    /// 总查询数（含非慢查询）
    pub total_queries: u64,
}

impl WorkloadSummary {
    /// 创建工作负载摘要
    pub fn new(query_logs: Vec<SlowQueryLog>, time_range: TimeRange, total_queries: u64) -> Self {
        Self {
            query_logs,
            time_range,
            total_queries,
        }
    }

    /// 样本量是否充足（>= 100）
    pub fn is_sufficient(&self) -> bool {
        self.query_logs.len() >= 100
    }

    /// 获取 Top N 慢查询（按执行时间降序）
    pub fn top_n_slow_queries(&self, n: usize) -> Vec<SlowQueryLog> {
        let mut logs = self.query_logs.clone();
        logs.sort_by_key(|a| std::cmp::Reverse(a.execution_time_ms));
        logs.into_iter().take(n).collect()
    }

    /// 从慢查询日志提取查询模式
    pub fn extract_patterns(&self) -> Vec<QueryPattern> {
        let mut pattern_map: std::collections::HashMap<String, (u64, Vec<String>)> =
            std::collections::HashMap::new();

        for log in &self.query_logs {
            let template = Self::sql_to_template(&log.sql);
            let entry = pattern_map
                .entry(template.clone())
                .or_insert((0, Vec::new()));
            entry.0 += 1;
            // 提取列名（简化：从 WHERE 子句提取）
            let cols = Self::extract_columns(&log.sql);
            for col in cols {
                if !entry.1.contains(&col) {
                    entry.1.push(col);
                }
            }
        }

        pattern_map
            .into_iter()
            .map(|(template, (freq, cols))| QueryPattern {
                sql_template: template,
                frequency: freq,
                columns_accessed: cols,
            })
            .collect()
    }

    /// 将 SQL 转为参数化模板（简化实现）
    fn sql_to_template(sql: &str) -> String {
        let lower = sql.to_lowercase();
        lower
            .replace("0x[0-9a-f]+", "?")
            .replace("'-?'", "?")
            .replace("'-?\\d+'", "?")
    }

    /// 从 SQL 提取列名（简化实现：从 WHERE/ORDER BY 子句提取）
    fn extract_columns(sql: &str) -> Vec<String> {
        let lower = sql.to_lowercase();
        let mut cols = Vec::new();

        if let Some(where_pos) = lower.find("where") {
            let after_where = &sql[where_pos + 5..];
            // 简化：按 AND/OR 分割，提取列名
            for cond in after_where.split([' ', ')']) {
                if cond.contains('=') || cond.contains('>') || cond.contains('<') {
                    if let Some(col) = cond.split(['=', '>', '<', '!']).next() {
                        let col = col.trim().trim_matches(|c: char| c.is_whitespace());
                        if !col.is_empty()
                            && !col.starts_with('\'')
                            && !col.chars().all(|c| c.is_ascii_digit())
                            && !cols.contains(&col.to_string())
                        {
                            cols.push(col.to_string());
                        }
                    }
                }
            }
        }

        cols
    }
}

// ==================== 工作负载驱动索引建议器 ====================

/// 工作负载驱动索引建议器
///
/// 从工作负载（查询日志 + 频率 + 耗时）分析高频查询模式，
/// 生成覆盖 Top N 慢查询的复合索引建议。
pub struct WorkloadDrivenIndexAdvisor {
    /// 内部索引建议器
    index_advisor: IndexAdvisor,
    /// 组合优化器
    combination_optimizer: IndexCombinationOptimizer,
    /// Top N 慢查询数量（默认 10）
    top_n: usize,
}

impl Default for WorkloadDrivenIndexAdvisor {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkloadDrivenIndexAdvisor {
    /// 创建工作负载驱动索引建议器
    pub fn new() -> Self {
        Self {
            index_advisor: IndexAdvisor::new(),
            combination_optimizer: IndexCombinationOptimizer::new(),
            top_n: 10,
        }
    }

    /// 启用 LLM 增强
    pub fn with_llm(mut self) -> Self {
        self.index_advisor = self.index_advisor.with_llm();
        self
    }

    /// 设置 Top N
    pub fn with_top_n(mut self, n: usize) -> Self {
        self.top_n = n;
        self
    }

    /// 设置最大索引数
    pub fn with_max_indexes(mut self, max: usize) -> Self {
        self.combination_optimizer = self.combination_optimizer.with_max_indexes(max);
        self
    }

    /// 基于工作负载生成索引建议
    ///
    /// 分析工作负载中的高频查询模式，生成覆盖 Top N 慢查询的复合索引建议。
    /// 样本量 < 100 时返回建议但标注"样本量不足，建议置信度低"。
    pub async fn advise_for_workload(
        &self,
        workload: &WorkloadSummary,
    ) -> Result<WorkloadAdviceResult, IndexError> {
        let is_sufficient = workload.is_sufficient();

        // 提取查询模式
        let patterns = workload.extract_patterns();
        if patterns.is_empty() {
            return Err(IndexError::NoQueryPatterns);
        }

        // 获取 Top N 慢查询
        let top_slow = workload.top_n_slow_queries(self.top_n);

        // 生成候选索引建议
        let candidates = self.index_advisor.suggest(&patterns, &top_slow).await?;

        // 组合优化
        let optimized = self.combination_optimizer.optimize(candidates);

        // 样本量不足时降低置信度
        let suggestions = if is_sufficient {
            optimized
        } else {
            optimized
                .into_iter()
                .map(|mut s| {
                    s.expected_benefit.confidence *= 0.5;
                    s.expected_benefit.uncertain = true;
                    s
                })
                .collect()
        };

        Ok(WorkloadAdviceResult {
            suggestions,
            is_sufficient,
            sample_size: workload.query_logs.len(),
            top_n_analyzed: top_slow.len(),
        })
    }
}

/// 工作负载建议结果
#[derive(Debug, Clone)]
pub struct WorkloadAdviceResult {
    /// 索引建议列表
    pub suggestions: Vec<IndexSuggestion>,
    /// 样本量是否充足
    pub is_sufficient: bool,
    /// 样本量
    pub sample_size: usize,
    /// 分析的 Top N 慢查询数
    pub top_n_analyzed: usize,
}

// ==================== 索引组合优化器 ====================

/// 索引组合优化器
///
/// 分析索引间互斥/互补关系，输出最优索引组合。
/// 识别冗余索引（被覆盖的索引）并建议删除。
pub struct IndexCombinationOptimizer {
    /// 最大索引数（默认 5）
    max_indexes: usize,
}

impl Default for IndexCombinationOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

impl IndexCombinationOptimizer {
    /// 创建组合优化器
    pub fn new() -> Self {
        Self { max_indexes: 5 }
    }

    /// 设置最大索引数
    pub fn with_max_indexes(mut self, max: usize) -> Self {
        self.max_indexes = max;
        self
    }

    /// 优化索引组合
    ///
    /// 分析索引间互斥/互补关系，输出最优索引组合。
    /// 10 候选 → max_indexes 最优，附理由。
    pub fn optimize(&self, candidates: Vec<IndexSuggestion>) -> Vec<IndexSuggestion> {
        // 1. 检测冗余索引（被覆盖的索引）
        let mut filtered = self.remove_redundant(candidates);

        if filtered.len() <= self.max_indexes {
            // 仍需按综合评分排序
            filtered.sort_by(|a, b| {
                b.expected_benefit
                    .composite_score()
                    .partial_cmp(&a.expected_benefit.composite_score())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            return filtered;
        }

        // 2. 按综合评分排序
        filtered.sort_by(|a, b| {
            b.expected_benefit
                .composite_score()
                .partial_cmp(&a.expected_benefit.composite_score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 3. 选取 Top N
        filtered.into_iter().take(self.max_indexes).collect()
    }

    /// 移除冗余索引
    ///
    /// 如果索引 A 的列是索引 B 的列的前缀，则 A 被 B 覆盖，可移除。
    /// 例如：idx(a) 被 idx(a, b) 覆盖。
    fn remove_redundant(&self, candidates: Vec<IndexSuggestion>) -> Vec<IndexSuggestion> {
        let mut result = Vec::new();

        for (i, candidate) in candidates.iter().enumerate() {
            let mut is_redundant = false;

            for (j, other) in candidates.iter().enumerate() {
                if i == j {
                    continue;
                }
                // 检查 candidate 的列是否是 other 的列的前缀
                if Self::is_prefix(&candidate.index_columns, &other.index_columns)
                    && candidate.index_columns.len() < other.index_columns.len()
                {
                    is_redundant = true;
                    break;
                }
            }

            if !is_redundant {
                result.push(candidate.clone());
            }
        }

        result
    }

    /// 检查 cols_a 是否是 cols_b 的前缀
    fn is_prefix(cols_a: &[String], cols_b: &[String]) -> bool {
        if cols_a.len() > cols_b.len() {
            return false;
        }
        cols_a
            .iter()
            .zip(cols_b.iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
    }

    /// 量化收益
    ///
    /// 量化查询加速比 / 写入开销 / 存储开销。
    pub fn quantify_benefit(
        &self,
        suggestion: &IndexSuggestion,
        table_row_count: u64,
        column_selectivity: f64,
    ) -> BenefitEstimate {
        // 加速比：基于列选择性估算
        // 选择性越低（越接近 0），索引收益越高
        let selectivity = column_selectivity.clamp(0.001, 1.0);
        let speedup = 1.0 + (1.0 / selectivity).ln().max(0.0);

        // 写入开销：索引列数越多，写入开销越大
        let write_overhead = suggestion.index_columns.len() as f64 * 0.1;

        // 存储开销：基于表行数 + 索引列数估算
        // 假设每行每列 8 字节，MB = rows * cols * 8 / 1024 / 1024
        let storage_cost_mb =
            (table_row_count as f64 * suggestion.index_columns.len() as f64 * 8.0)
                / (1024.0 * 1024.0);

        BenefitEstimate::certain(speedup, suggestion.expected_benefit.confidence)
            .with_write_overhead(write_overhead)
            .with_storage_cost(storage_cost_mb)
    }

    /// 检测冗余索引并生成删除建议
    ///
    /// 返回 (保留的索引, 建议删除的索引)
    pub fn detect_redundant(
        &self,
        existing_indexes: Vec<Vec<String>>,
    ) -> (Vec<Vec<String>>, Vec<RedundantIndexInfo>) {
        let mut keep = Vec::new();
        let mut redundant = Vec::new();

        for (i, idx) in existing_indexes.iter().enumerate() {
            let mut covered_by: Option<usize> = None;

            for (j, other) in existing_indexes.iter().enumerate() {
                if i == j {
                    continue;
                }
                if Self::is_prefix(idx, other) && idx.len() < other.len() {
                    covered_by = Some(j);
                    break;
                }
            }

            if let Some(by) = covered_by {
                redundant.push(RedundantIndexInfo {
                    redundant_columns: idx.clone(),
                    covered_by_columns: existing_indexes[by].clone(),
                    reason: format!(
                        "索引 ({}) 被复合索引 ({}) 覆盖，可删除以减少写入开销",
                        idx.join(", "),
                        existing_indexes[by].join(", ")
                    ),
                });
            } else {
                keep.push(idx.clone());
            }
        }

        (keep, redundant)
    }
}

/// 冗余索引信息
#[derive(Debug, Clone)]
pub struct RedundantIndexInfo {
    /// 冗余索引的列
    pub redundant_columns: Vec<String>,
    /// 覆盖它的索引的列
    pub covered_by_columns: Vec<String>,
    /// 冗余原因
    pub reason: String,
}

// ==================== IndexUsageStats ====================

/// 索引使用统计
///
/// 用于检测冗余索引和未使用索引。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexUsageStats {
    /// 索引名
    pub index_name: String,
    /// 索引列
    pub columns: Vec<String>,
    /// 使用次数
    pub usage_count: u64,
    /// 最后使用时间（Unix 时间戳）
    pub last_used: i64,
}

impl IndexUsageStats {
    /// 索引是否未使用（usage_count == 0）
    pub fn is_unused(&self) -> bool {
        self.usage_count == 0
    }

    /// 索引是否低使用率（usage_count < threshold）
    pub fn is_low_usage(&self, threshold: u64) -> bool {
        self.usage_count < threshold
    }
}

/// 检测未使用索引
pub fn detect_unused_indexes(stats: &[IndexUsageStats]) -> Vec<&IndexUsageStats> {
    stats.iter().filter(|s| s.is_unused()).collect()
}

/// 检测低使用率索引
pub fn detect_low_usage_indexes(
    stats: &[IndexUsageStats],
    threshold: u64,
) -> Vec<&IndexUsageStats> {
    stats.iter().filter(|s| s.is_low_usage(threshold)).collect()
}
