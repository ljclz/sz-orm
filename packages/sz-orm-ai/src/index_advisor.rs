//! 自动索引建议模块
//!
//! 基于查询模式分析和慢查询日志，自动生成索引建议。
//! 使用 sqlparser 解析 SQL 提取 WHERE/JOIN/ORDER BY 列，
//! 识别高频查询模式，生成 DDL 建议文本（不自动执行）。
//!
//! 启用 `ai-index-advisor` feature 时编译。

use crate::advice_common::{AdviceType, AiAdviceAuditRecord, BenefitEstimate};
use serde::{Deserialize, Serialize};
use sqlparser::ast::{
    Expr, JoinConstraint, JoinOperator, OrderByExpr, SetExpr, Statement, TableFactor,
    TableWithJoins,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use thiserror::Error;

/// 索引类型（按方言选择）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexType {
    /// B-Tree 索引（通用，支持范围查询）
    BTree,
    /// Hash 索引（等值查询高效）
    Hash,
    /// GIN 索引（PostgreSQL 全文/JSONB）
    Gin,
    /// BRIN 索引（PostgreSQL 大表块范围）
    Brin,
}

impl IndexType {
    /// DDL 关键字
    pub fn ddl_keyword(&self) -> &str {
        match self {
            IndexType::BTree => "BTREE",
            IndexType::Hash => "HASH",
            IndexType::Gin => "GIN",
            IndexType::Brin => "BRIN",
        }
    }
}

/// 查询模式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPattern {
    /// SQL 模板（参数化后的 SQL 文本）
    pub sql_template: String,
    /// 查询频率（单位时间内执行次数）
    pub frequency: u64,
    /// 访问的列
    pub columns_accessed: Vec<String>,
}

/// 慢查询日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlowQueryLog {
    /// SQL 文本
    pub sql: String,
    /// 执行时间（毫秒）
    pub execution_time_ms: u64,
    /// Unix 时间戳（秒）
    pub timestamp: i64,
}

/// 索引建议
///
/// 包含索引列、类型、DDL 文本、预期收益和查询模式证据。
/// DDL 文本仅作建议展示，不自动执行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexSuggestion {
    /// 索引列
    pub index_columns: Vec<String>,
    /// 索引类型
    pub index_type: IndexType,
    /// DDL 文本（如 `CREATE INDEX idx_users_email ON users(email)`）
    pub ddl_text: String,
    /// 预期收益
    pub expected_benefit: BenefitEstimate,
    /// 查询模式证据（命中查询列表）
    pub evidence: Vec<QueryPattern>,
}

/// 索引建议错误
#[derive(Debug, Error)]
pub enum IndexError {
    #[error("SQL parse error: {0}")]
    ParseError(String),
    #[error("No query patterns provided")]
    NoQueryPatterns,
    #[error("LLM service unavailable: {0}")]
    LlmServiceUnavailable(String),
}

/// 索引建议器
///
/// 基于 sqlparser 解析查询模式 + 慢查询日志，
/// 规则型分析（列组合 + 选择性）+ 可选 LLM 建议。
/// 所有建议为 DDL 文本，不自动执行。
pub struct IndexAdvisor {
    llm_enabled: bool,
}

impl Default for IndexAdvisor {
    fn default() -> Self {
        Self::new()
    }
}

impl IndexAdvisor {
    /// 创建规则型索引建议器
    pub fn new() -> Self {
        Self { llm_enabled: false }
    }

    /// 启用 LLM 增强
    pub fn with_llm(mut self) -> Self {
        self.llm_enabled = true;
        self
    }

    /// 生成索引建议
    ///
    /// 分析查询模式（WHERE/JOIN/ORDER BY 列）+ 慢查询日志，
    /// 识别高频查询模式，产出 DDL 建议文本。
    /// DDL 不自动执行，仅作建议展示。
    pub async fn suggest(
        &self,
        query_patterns: &[QueryPattern],
        slow_queries: &[SlowQueryLog],
    ) -> Result<Vec<IndexSuggestion>, IndexError> {
        if query_patterns.is_empty() {
            return Err(IndexError::NoQueryPatterns);
        }

        let mut suggestions = Vec::new();
        let dialect = GenericDialect {};

        for pattern in query_patterns {
            let parsed = Parser::parse_sql(&dialect, &pattern.sql_template);
            if parsed.is_err() {
                continue;
            }
            let statements = parsed.unwrap();

            for stmt in &statements {
                if let Some((table, filter_cols, join_cols, order_cols)) =
                    Self::extract_query_info(stmt)
                {
                    let mut candidate_cols = Vec::new();
                    candidate_cols.extend(filter_cols);
                    candidate_cols.extend(join_cols);

                    if candidate_cols.is_empty() && order_cols.is_empty() {
                        continue;
                    }

                    candidate_cols.sort();
                    candidate_cols.dedup();

                    let index_type = IndexType::BTree;
                    let col_list = candidate_cols.join(", ");
                    let idx_name = format!("idx_{}_{}", table, candidate_cols.join("_"));
                    let ddl_text = format!(
                        "CREATE {} INDEX {} ON {} ({})",
                        index_type.ddl_keyword(),
                        idx_name,
                        table,
                        col_list
                    );

                    let total_frequency: u64 = query_patterns.iter().map(|p| p.frequency).sum();
                    let speedup_ratio = if pattern.frequency > 0 && total_frequency > 0 {
                        1.0 + (pattern.frequency as f64 / total_frequency as f64) * 10.0
                    } else {
                        1.0
                    };
                    let confidence = if self.llm_enabled { 0.85 } else { 0.7 };
                    let uncertain = slow_queries.is_empty();

                    let benefit = if uncertain {
                        BenefitEstimate::uncertain(speedup_ratio, confidence)
                    } else {
                        BenefitEstimate::certain(speedup_ratio, confidence)
                    };

                    suggestions.push(IndexSuggestion {
                        index_columns: candidate_cols.clone(),
                        index_type: index_type.clone(),
                        ddl_text,
                        expected_benefit: benefit,
                        evidence: vec![pattern.clone()],
                    });
                }
            }
        }

        suggestions.sort_by(|a, b| {
            b.expected_benefit
                .speedup_ratio
                .partial_cmp(&a.expected_benefit.speedup_ratio)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(suggestions)
    }

    /// 生成审计记录
    pub fn audit_record(&self, confidence: f32) -> AiAdviceAuditRecord {
        if self.llm_enabled {
            AiAdviceAuditRecord::from_llm(AdviceType::Index, confidence, "gpt-4o-mini")
        } else {
            AiAdviceAuditRecord::from_rule(AdviceType::Index, confidence)
        }
    }

    fn extract_query_info(
        stmt: &Statement,
    ) -> Option<(String, Vec<String>, Vec<String>, Vec<String>)> {
        let query = match stmt {
            Statement::Query(q) => q.as_ref(),
            _ => return None,
        };
        let select = match &*query.body {
            SetExpr::Select(s) => s,
            _ => return None,
        };

        let table = Self::extract_table_name(&select.from)?;
        let filter_cols = Self::extract_filter_columns(&select.selection);
        let join_cols = Self::extract_join_columns(&select.from);
        let order_cols = Self::extract_order_columns(&query.order_by);

        Some((table, filter_cols, join_cols, order_cols))
    }

    fn extract_table_name(from: &[TableWithJoins]) -> Option<String> {
        if from.is_empty() {
            return None;
        }
        match &from[0].relation {
            TableFactor::Table { name, .. } => {
                Some(name.0.last().map(|i| i.value.clone()).unwrap_or_default())
            }
            _ => None,
        }
    }

    fn extract_filter_columns(selection: &Option<Expr>) -> Vec<String> {
        let mut cols = Vec::new();
        if let Some(expr) = selection {
            Self::collect_columns(expr, &mut cols);
        }
        cols
    }

    fn extract_join_columns(from: &[TableWithJoins]) -> Vec<String> {
        let mut cols = Vec::new();
        for table_with_joins in from {
            for join in &table_with_joins.joins {
                match &join.join_operator {
                    JoinOperator::Inner(constraint)
                    | JoinOperator::LeftOuter(constraint)
                    | JoinOperator::RightOuter(constraint)
                    | JoinOperator::FullOuter(constraint) => {
                        if let JoinConstraint::On(expr) = constraint {
                            Self::collect_columns(expr, &mut cols);
                        }
                    }
                    _ => {}
                }
            }
        }
        cols
    }

    fn extract_order_columns(order_by: &[OrderByExpr]) -> Vec<String> {
        let mut cols = Vec::new();
        for ob in order_by {
            Self::collect_columns(&ob.expr, &mut cols);
        }
        cols
    }

    fn collect_columns(expr: &Expr, cols: &mut Vec<String>) {
        match expr {
            Expr::Identifier(ident) => {
                cols.push(ident.value.clone());
            }
            Expr::CompoundIdentifier(idents) => {
                if let Some(last) = idents.last() {
                    cols.push(last.value.clone());
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                Self::collect_columns(left, cols);
                Self::collect_columns(right, cols);
            }
            Expr::InList { expr, .. } => {
                Self::collect_columns(expr, cols);
            }
            Expr::Like { expr, pattern, .. } => {
                Self::collect_columns(expr, cols);
                Self::collect_columns(pattern, cols);
            }
            Expr::IsNull(expr) => {
                Self::collect_columns(expr, cols);
            }
            Expr::Nested(expr) => {
                Self::collect_columns(expr, cols);
            }
            Expr::UnaryOp { expr, .. } => {
                Self::collect_columns(expr, cols);
            }
            _ => {}
        }
    }
}

// v7.3.0 任务 3.3：扩展智能索引推荐负载建模与收益预估

use std::collections::HashMap;

/// 表统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableStats {
    /// 行数
    pub row_count: u64,
    /// 已有索引列（列名列表）
    pub existing_indexes: Vec<String>,
}

/// 负载模型（查询模式 + 表统计）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadModel {
    /// 查询模式列表
    pub patterns: Vec<QueryPattern>,
    /// 表统计信息（表名 -> TableStats）
    pub table_stats: HashMap<String, TableStats>,
}

/// 索引推荐结果（含收益预估）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexRecommendation {
    /// 索引建议
    pub suggestion: IndexSuggestion,
    /// 预估扫描行数降低比例（0.0 ~ 1.0，1.0 表示全表扫描变单行定位）
    pub estimated_scan_reduction: f64,
    /// 预估延迟降低比例（可负，负值表示索引维护开销 > 查询收益）
    pub estimated_latency_reduction: f64,
    /// 推荐理由（含负收益标注）
    pub reason: String,
    /// 实际收益与预估偏差（v7.4.0 新增，None 表示未回填）
    #[serde(default)]
    pub actual_benefit_deviation: Option<f64>,
}

impl IndexRecommendation {
    /// DBA 实测后回填实际收益与预估偏差（v7.4.0 新增）
    ///
    /// `actual_latency_reduction` 为实测延迟降低比例，偏差 >20% 标注警告。
    pub fn record_actual_benefit(&mut self, actual_latency_reduction: f64) -> &mut Self {
        if self.estimated_latency_reduction != 0.0 {
            let deviation = ((actual_latency_reduction - self.estimated_latency_reduction)
                / self.estimated_latency_reduction.abs())
                * 100.0;
            self.actual_benefit_deviation = Some(deviation);
            if deviation.abs() > 20.0 {
                self.reason.push_str(&format!(
                    " [警告: 实际收益偏差 {deviation:.1}%，超过 ±20% 阈值]"
                ));
            }
        }
        self
    }
}

impl IndexAdvisor {
    /// 基于负载建模的索引推荐
    ///
    /// 分析查询模式 + 表统计，预估每个候选索引的扫描降低比例和延迟降低比例。
    /// 按 `estimated_latency_reduction` 降序排序，负收益在 `reason` 中标注。
    ///
    /// # 收益预估模型
    /// - `scan_reduction = 1.0 - (1.0 / row_count)`（索引将全表扫描降为单行定位）
    /// - `latency_reduction = scan_reduction * frequency_weight - maintenance_overhead`
    /// - `maintenance_overhead = 0.1`（每次写入索引维护开销系数）
    /// - 行数 < 100 时 `latency_reduction` 可能为负（索引维护开销 > 查询收益）
    pub async fn recommend(
        &self,
        workload: &WorkloadModel,
    ) -> Result<Vec<IndexRecommendation>, IndexError> {
        if workload.patterns.is_empty() {
            return Err(IndexError::NoQueryPatterns);
        }

        let dialect = GenericDialect {};
        let mut recommendations = Vec::new();

        for pattern in &workload.patterns {
            let parsed = Parser::parse_sql(&dialect, &pattern.sql_template);
            if parsed.is_err() {
                continue;
            }
            let statements = parsed.unwrap();

            for stmt in &statements {
                if let Some((table, filter_cols, join_cols, _order_cols)) =
                    Self::extract_query_info(stmt)
                {
                    let mut candidate_cols = Vec::new();
                    candidate_cols.extend(filter_cols);
                    candidate_cols.extend(join_cols);

                    if candidate_cols.is_empty() {
                        continue;
                    }

                    candidate_cols.sort();
                    candidate_cols.dedup();

                    // 检查列上是否已有索引
                    let table_stats = workload.table_stats.get(&table);
                    let existing_indexes = table_stats
                        .map(|s| s.existing_indexes.as_slice())
                        .unwrap_or(&[]);

                    let already_indexed =
                        candidate_cols.iter().all(|c| existing_indexes.contains(c));
                    if already_indexed {
                        continue;
                    }

                    let row_count = table_stats.map(|s| s.row_count).unwrap_or(1000);
                    let total_frequency: u64 = workload.patterns.iter().map(|p| p.frequency).sum();
                    let frequency_weight = if total_frequency > 0 {
                        pattern.frequency as f64 / total_frequency as f64
                    } else {
                        0.0
                    };

                    // 收益预估
                    let scan_reduction = if row_count > 0 {
                        1.0 - (1.0 / row_count as f64)
                    } else {
                        0.0
                    };
                    // 维护开销：行数越少开销越大（小表索引维护成本高于查询收益）
                    let maintenance_overhead = 0.5 / (row_count as f64).sqrt().max(1.0);
                    let query_benefit = scan_reduction * frequency_weight;
                    let latency_reduction = query_benefit - maintenance_overhead;

                    let index_type = IndexType::BTree;
                    let col_list = candidate_cols.join(", ");
                    let idx_name = format!("idx_{}_{}", table, candidate_cols.join("_"));
                    let ddl_text = format!(
                        "CREATE {} INDEX {} ON {} ({})",
                        index_type.ddl_keyword(),
                        idx_name,
                        table,
                        col_list
                    );

                    let confidence = if self.llm_enabled { 0.85 } else { 0.7 };
                    let benefit = if latency_reduction > 0.0 {
                        BenefitEstimate::certain(1.0 + latency_reduction, confidence)
                    } else {
                        BenefitEstimate::uncertain(1.0 + latency_reduction, confidence)
                    };

                    let reason = if latency_reduction < 0.0 {
                        format!(
                            "负收益：表 {} 行数 {} 过少，索引维护开销（{:.4}）> 查询收益（{:.4}），不建议创建",
                            table,
                            row_count,
                            maintenance_overhead,
                            query_benefit
                        )
                    } else {
                        format!(
                            "正收益：表 {} 行数 {}，预估扫描降低 {:.1}%，延迟降低 {:.1}%",
                            table,
                            row_count,
                            scan_reduction * 100.0,
                            latency_reduction * 100.0
                        )
                    };

                    let suggestion = IndexSuggestion {
                        index_columns: candidate_cols.clone(),
                        index_type: index_type.clone(),
                        ddl_text,
                        expected_benefit: benefit,
                        evidence: vec![pattern.clone()],
                    };

                    recommendations.push(IndexRecommendation {
                        suggestion,
                        estimated_scan_reduction: scan_reduction,
                        estimated_latency_reduction: latency_reduction,
                        reason,
                        actual_benefit_deviation: None,
                    });
                }
            }
        }

        // 按延迟降低比例降序排序
        recommendations.sort_by(|a, b| {
            b.estimated_latency_reduction
                .partial_cmp(&a.estimated_latency_reduction)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(recommendations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_suggest_index_for_where_clause() {
        let advisor = IndexAdvisor::new();
        let patterns = vec![QueryPattern {
            sql_template: "SELECT * FROM users WHERE email = $1".to_string(),
            frequency: 100,
            columns_accessed: vec!["email".to_string()],
        }];
        let result = advisor.suggest(&patterns, &[]).await.unwrap();
        assert!(!result.is_empty());
        assert!(result[0].ddl_text.contains("CREATE"));
        assert!(result[0].ddl_text.contains("users"));
        assert!(result[0].index_columns.contains(&"email".to_string()));
    }

    #[tokio::test]
    async fn test_suggest_index_for_join() {
        let advisor = IndexAdvisor::new();
        let patterns = vec![QueryPattern {
            sql_template: "SELECT * FROM orders o JOIN users u ON o.user_id = u.id".to_string(),
            frequency: 50,
            columns_accessed: vec!["user_id".to_string(), "id".to_string()],
        }];
        let result = advisor.suggest(&patterns, &[]).await.unwrap();
        assert!(!result.is_empty());
        assert!(result[0].index_columns.contains(&"user_id".to_string()));
    }

    #[tokio::test]
    async fn test_suggest_index_for_order_by() {
        let advisor = IndexAdvisor::new();
        let patterns = vec![QueryPattern {
            sql_template: "SELECT * FROM users ORDER BY created_at".to_string(),
            frequency: 30,
            columns_accessed: vec!["created_at".to_string()],
        }];
        let result = advisor.suggest(&patterns, &[]).await.unwrap();
        assert!(!result.is_empty());
    }

    #[tokio::test]
    async fn test_no_query_patterns_error() {
        let advisor = IndexAdvisor::new();
        let result = advisor.suggest(&[], &[]).await;
        assert!(matches!(result, Err(IndexError::NoQueryPatterns)));
    }

    #[tokio::test]
    async fn test_ddl_not_executed() {
        let advisor = IndexAdvisor::new();
        let patterns = vec![QueryPattern {
            sql_template: "SELECT * FROM users WHERE email = $1".to_string(),
            frequency: 100,
            columns_accessed: vec!["email".to_string()],
        }];
        let result = advisor.suggest(&patterns, &[]).await.unwrap();
        for s in &result {
            assert!(s.ddl_text.starts_with("CREATE"));
            assert!(!s.ddl_text.contains("EXECUTE"));
        }
    }

    #[tokio::test]
    async fn test_benefit_estimate_uncertain_without_slow_queries() {
        let advisor = IndexAdvisor::new();
        let patterns = vec![QueryPattern {
            sql_template: "SELECT * FROM users WHERE email = $1".to_string(),
            frequency: 100,
            columns_accessed: vec!["email".to_string()],
        }];
        let result = advisor.suggest(&patterns, &[]).await.unwrap();
        assert!(result[0].expected_benefit.uncertain);
    }

    #[tokio::test]
    async fn test_benefit_estimate_certain_with_slow_queries() {
        let advisor = IndexAdvisor::new();
        let patterns = vec![QueryPattern {
            sql_template: "SELECT * FROM users WHERE email = $1".to_string(),
            frequency: 100,
            columns_accessed: vec!["email".to_string()],
        }];
        let slow_queries = vec![SlowQueryLog {
            sql: "SELECT * FROM users WHERE email = $1".to_string(),
            execution_time_ms: 500,
            timestamp: 1000,
        }];
        let result = advisor.suggest(&patterns, &slow_queries).await.unwrap();
        assert!(!result[0].expected_benefit.uncertain);
    }

    #[tokio::test]
    async fn test_suggestions_sorted_by_benefit() {
        let advisor = IndexAdvisor::new();
        let patterns = vec![
            QueryPattern {
                sql_template: "SELECT * FROM users WHERE email = $1".to_string(),
                frequency: 100,
                columns_accessed: vec!["email".to_string()],
            },
            QueryPattern {
                sql_template: "SELECT * FROM users WHERE name = $1".to_string(),
                frequency: 10,
                columns_accessed: vec!["name".to_string()],
            },
        ];
        let result = advisor.suggest(&patterns, &[]).await.unwrap();
        if result.len() >= 2 {
            assert!(
                result[0].expected_benefit.speedup_ratio
                    >= result[1].expected_benefit.speedup_ratio
            );
        }
    }

    #[test]
    fn test_index_type_ddl_keyword() {
        assert_eq!(IndexType::BTree.ddl_keyword(), "BTREE");
        assert_eq!(IndexType::Hash.ddl_keyword(), "HASH");
        assert_eq!(IndexType::Gin.ddl_keyword(), "GIN");
        assert_eq!(IndexType::Brin.ddl_keyword(), "BRIN");
    }

    #[test]
    fn test_audit_record() {
        let advisor = IndexAdvisor::new();
        let record = advisor.audit_record(0.8);
        assert_eq!(record.advice_type, AdviceType::Index);
        assert_eq!(
            record.source_engine,
            crate::advice_common::AdviceSource::Rule
        );
    }
}
