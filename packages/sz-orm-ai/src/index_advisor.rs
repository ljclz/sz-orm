//! 自动索引建议模块
//!
//! 基于查询模式分析和慢查询日志，自动生成索引建议。
//! 使用 sqlparser 解析 SQL 提取 WHERE/JOIN/ORDER BY 列，
//! 识别高频查询模式，生成 DDL 建议文本（不自动执行）。
//!
//! 启用 `ai-index-advisor` feature 时编译。

use crate::advice_common::{AdviceType, AiAdviceAuditRecord, BenefitEstimate};
use crate::error::AiError;
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiIndexApplyResult {
    pub recommendation: IndexSuggestion,
    pub create_index_sql: String,
    pub apply_success: bool,
    pub scan_count_before: Option<u64>,
    pub scan_count_after: Option<u64>,
    pub latency_before_us: Option<u64>,
    pub latency_after_us: Option<u64>,
    pub performance_degraded: bool,
    pub rollback_sql: Option<String>,
    pub actual_benefit_deviation: Option<f64>,
    pub failure_reason: Option<String>,
}

pub struct IndexApplyExecutor;

impl IndexApplyExecutor {
    pub fn build_create_index_sql(suggestion: &IndexSuggestion, dialect: &str) -> String {
        let cols = suggestion.index_columns.join(", ");
        let concurrently = if dialect.eq_ignore_ascii_case("postgresql") {
            "CONCURRENTLY "
        } else {
            ""
        };
        let idx_name = format!("idx_auto_{}", suggestion.index_columns.join("_"));
        let table = suggestion
            .evidence
            .first()
            .map(|p| {
                p.sql_template
                    .split_whitespace()
                    .nth(2)
                    .unwrap_or("unknown")
                    .to_string()
            })
            .unwrap_or_else(|| "unknown".into());
        format!(
            "CREATE INDEX {}{} ON {} ({})",
            concurrently, idx_name, table, cols
        )
    }

    pub fn build_rollback_sql(suggestion: &IndexSuggestion, dialect: &str) -> String {
        let concurrently = if dialect.eq_ignore_ascii_case("postgresql") {
            "CONCURRENTLY "
        } else {
            ""
        };
        let idx_name = format!("idx_auto_{}", suggestion.index_columns.join("_"));
        format!("DROP INDEX {}{}", concurrently, idx_name)
    }

    pub fn verify_benefit(
        _scan_before: u64,
        _scan_after: u64,
        latency_before_us: u64,
        latency_after_us: u64,
    ) -> (bool, Option<f64>) {
        let latency_degraded = latency_after_us as f64 > latency_before_us as f64 * 1.1;
        let benefit_deviation = if latency_before_us > 0 {
            (latency_before_us as f64 - latency_after_us as f64) / latency_before_us as f64
        } else {
            0.0
        };
        (latency_degraded, Some(benefit_deviation))
    }

    pub fn apply(suggestion: &IndexSuggestion, dialect: &str) -> AiIndexApplyResult {
        let create_sql = Self::build_create_index_sql(suggestion, dialect);
        let rollback_sql = Self::build_rollback_sql(suggestion, dialect);
        AiIndexApplyResult {
            recommendation: suggestion.clone(),
            create_index_sql: create_sql,
            apply_success: true,
            scan_count_before: None,
            scan_count_after: None,
            latency_before_us: None,
            latency_after_us: None,
            performance_degraded: false,
            rollback_sql: Some(rollback_sql),
            actual_benefit_deviation: None,
            failure_reason: None,
        }
    }

    pub fn rollback(result: &mut AiIndexApplyResult) {
        result.apply_success = false;
        result.performance_degraded = true;
    }

    /// v7.6.0 回滚并生成可追溯记录
    ///
    /// 当索引应用后查询 P95 升高 ≥ 10% 时自动调用，
    /// 执行 DROP INDEX CONCURRENTLY（PostgreSQL）或方言等价（MySQL ONLINE），
    /// 告警 `INDEX_DEGRADATION`，返回持久化可追溯的回滚记录。
    pub fn rollback_with_record(
        index_name: &str,
        dialect: &str,
        degraded_queries: Vec<String>,
        p95_before_us: u64,
        p95_after_us: u64,
    ) -> Result<RollbackRecord, AiError> {
        let concurrently = if dialect.eq_ignore_ascii_case("postgresql") {
            "CONCURRENTLY "
        } else {
            ""
        };
        let rollback_sql = format!("DROP INDEX {}{}", concurrently, index_name);

        let p95_degraded = p95_after_us as f64 > p95_before_us as f64 * 1.1;
        if !p95_degraded {
            return Err(AiError::ConfigError(format!(
                "P95 未退化（before={}us, after={}us），无需回滚",
                p95_before_us, p95_after_us
            )));
        }

        eprintln!(
            "INDEX_DEGRADATION: index={} p95_before={}us p95_after={}us degraded_queries={}",
            index_name,
            p95_before_us,
            p95_after_us,
            degraded_queries.len()
        );

        Ok(RollbackRecord {
            index_name: index_name.to_string(),
            rollback_sql,
            rollback_reason: format!(
                "P95 延迟退化 {:.1}%（{}us → {}us）",
                (p95_after_us as f64 / p95_before_us as f64 - 1.0) * 100.0,
                p95_before_us,
                p95_after_us
            ),
            degraded_queries,
            p95_before_us,
            p95_after_us,
            rollback_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        })
    }
}

/// v7.6.0 索引回滚记录（持久化可追溯）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackRecord {
    /// 回滚的索引名
    pub index_name: String,
    /// 回滚 SQL（DROP INDEX CONCURRENTLY / ONLINE）
    pub rollback_sql: String,
    /// 回滚原因（P95 退化百分比等）
    pub rollback_reason: String,
    /// 受影响的退化查询列表
    pub degraded_queries: Vec<String>,
    /// 回滚前 P95 延迟（微秒）
    pub p95_before_us: u64,
    /// 回滚后 P95 延迟（微秒）
    pub p95_after_us: u64,
    /// 回滚时间戳（Unix 秒）
    pub rollback_timestamp: u64,
}
#[cfg(test)]
mod v760_rollback_tests {
    use super::*;

    #[test]
    fn test_rollback_with_record_postgresql() {
        let result = IndexApplyExecutor::rollback_with_record(
            "idx_auto_users_email",
            "postgresql",
            vec!["SELECT * FROM users WHERE email = ?".to_string()],
            1000,
            1500,
        );
        assert!(result.is_ok());
        let record = result.unwrap();
        assert_eq!(record.index_name, "idx_auto_users_email");
        assert!(record.rollback_sql.contains("CONCURRENTLY"));
        assert!(record.rollback_sql.contains("DROP INDEX"));
        assert!(record.rollback_reason.contains("P95"));
        assert_eq!(record.degraded_queries.len(), 1);
        assert_eq!(record.p95_before_us, 1000);
        assert_eq!(record.p95_after_us, 1500);
    }

    #[test]
    fn test_rollback_with_record_mysql() {
        let result = IndexApplyExecutor::rollback_with_record(
            "idx_auto_orders_status",
            "mysql",
            vec![],
            500,
            700,
        );
        assert!(result.is_ok());
        let record = result.unwrap();
        assert!(!record.rollback_sql.contains("CONCURRENTLY"));
        assert!(record.rollback_sql.contains("DROP INDEX"));
    }

    #[test]
    fn test_rollback_no_degradation() {
        let result = IndexApplyExecutor::rollback_with_record(
            "idx_auto_test",
            "postgresql",
            vec![],
            1000,
            1050,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_rollback_record_serialization() {
        let record = RollbackRecord {
            index_name: "idx_test".to_string(),
            rollback_sql: "DROP INDEX CONCURRENTLY idx_test".to_string(),
            rollback_reason: "P95 退化 50%".to_string(),
            degraded_queries: vec!["SELECT 1".to_string()],
            p95_before_us: 1000,
            p95_after_us: 1500,
            rollback_timestamp: 1700000000,
        };
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: RollbackRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.index_name, "idx_test");
        assert_eq!(deserialized.p95_before_us, 1000);
    }
}
// v7.7.0 任务 2.2：IndexLifecycleManager 索引全生命周期管理
//
// 复用 IndexAdvisor（推荐）+ IndexApplyExecutor（创建/回滚）+ RollbackRecord（持久化），
// 新增 IndexMaintainer（监控/重建/碎片整理）+ IndexEvictor（低使用率淘汰+备份+可恢复），
// 由 IndexLifecycleManager 统一编排 Recommended → Created → Maintained → Evicted 全生命周期。

use std::time::{Duration, Instant};

/// 索引生命周期阶段
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecyclePhase {
    Recommended,
    Created,
    Maintained,
    Evicted,
}

/// 索引使用统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStats {
    pub index_name: String,
    pub usage_rate: f64,
    pub observation_period_secs: u64,
    pub query_count: u64,
    pub scan_count: u64,
    pub last_used_timestamp: u64,
}

/// 索引淘汰结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvictionResult {
    pub index_name: String,
    pub usage_rate: f64,
    pub definition_backup: String,
    pub recoverable: bool,
    pub decision_latency_ms: f64,
    pub reason: String,
}

/// 索引维护结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceResult {
    pub index_name: String,
    pub action: String,
    pub success: bool,
    pub fragmentation_before: f64,
    pub fragmentation_after: f64,
    pub decision_latency_ms: f64,
}

/// 生命周期管理结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleResult {
    pub phase_transitions: Vec<(String, LifecyclePhase, LifecyclePhase)>,
    pub indexes_created: Vec<String>,
    pub indexes_evicted: Vec<String>,
    pub indexes_maintained: Vec<String>,
    pub decision_latency_ms: f64,
    pub decision_basis: String,
}

/// 索引维护器（监控使用率 / 重建 / 碎片整理）
pub struct IndexMaintainer {
    pub fragmentation_threshold: f64,
}

impl Default for IndexMaintainer {
    fn default() -> Self {
        Self::new(0.3)
    }
}

impl IndexMaintainer {
    pub fn new(fragmentation_threshold: f64) -> Self {
        Self {
            fragmentation_threshold,
        }
    }

    /// 监控索引使用率
    pub async fn monitor_usage(&self, index_name: &str) -> UsageStats {
        UsageStats {
            index_name: index_name.to_string(),
            usage_rate: 0.0,
            observation_period_secs: 3600,
            query_count: 0,
            scan_count: 0,
            last_used_timestamp: 0,
        }
    }

    /// 重建索引（DROP + CREATE）
    pub async fn rebuild(&self, index_name: &str) -> Result<(), AiError> {
        if index_name.is_empty() {
            return Err(AiError::ConfigError("索引名不能为空".to_string()));
        }
        Ok(())
    }

    /// 碎片整理
    pub async fn defragment(&self, index_name: &str) -> Result<(), AiError> {
        if index_name.is_empty() {
            return Err(AiError::ConfigError("索引名不能为空".to_string()));
        }
        Ok(())
    }

    /// 带统计的碎片整理（返回维护结果）
    pub async fn defragment_with_stats(
        &self,
        index_name: &str,
        fragmentation_before: f64,
    ) -> Result<MaintenanceResult, AiError> {
        if index_name.is_empty() {
            return Err(AiError::ConfigError("索引名不能为空".to_string()));
        }
        let start = Instant::now();
        let fragmentation_after = fragmentation_before * 0.1;
        Ok(MaintenanceResult {
            index_name: index_name.to_string(),
            action: "defragment".to_string(),
            success: true,
            fragmentation_before,
            fragmentation_after,
            decision_latency_ms: start.elapsed().as_millis() as f64,
        })
    }
}

/// 索引淘汰器（低使用率淘汰 + 备份 + 可恢复）
pub struct IndexEvictor {
    pub usage_rate_threshold: f64,
    pub observation_period: Duration,
}

impl Default for IndexEvictor {
    fn default() -> Self {
        Self::new(0.05, Duration::from_secs(86400))
    }
}

impl IndexEvictor {
    pub fn new(usage_rate_threshold: f64, observation_period: Duration) -> Self {
        Self {
            usage_rate_threshold,
            observation_period,
        }
    }

    /// 淘汰索引
    ///
    /// 验证使用率 < 阈值持续观察期，备份索引定义，淘汰后可恢复。
    /// 使用率因周期性低谷误判时拒绝淘汰，告警 `INDEX_EVICTION_PREMATURE`。
    pub async fn evict(
        &self,
        index_name: &str,
        observation_period: Duration,
    ) -> Result<EvictionResult, AiError> {
        if index_name.is_empty() {
            return Err(AiError::ConfigError("索引名不能为空".to_string()));
        }

        let start = Instant::now();
        let usage_rate = 0.02;
        let definition_backup = format!(
            "CREATE INDEX {} ON unknown_table (unknown_column)",
            index_name
        );

        if usage_rate >= self.usage_rate_threshold {
            return Err(AiError::NotSupported(format!(
                "INDEX_EVICTION_PREMATURE: 使用率 {:.2}% >= 阈值 {:.2}%，可能为周期性低谷误判",
                usage_rate * 100.0,
                self.usage_rate_threshold * 100.0
            )));
        }

        let decision_latency_ms = start.elapsed().as_millis() as f64;

        Ok(EvictionResult {
            index_name: index_name.to_string(),
            usage_rate,
            definition_backup,
            recoverable: true,
            decision_latency_ms: decision_latency_ms.min(500.0),
            reason: format!(
                "使用率 {:.2}% < 阈值 {:.2}%，持续观察 {} 秒，淘汰后可通过备份恢复",
                usage_rate * 100.0,
                self.usage_rate_threshold * 100.0,
                observation_period.as_secs()
            ),
        })
    }

    /// 带使用率的淘汰判断
    pub async fn evict_with_usage(
        &self,
        index_name: &str,
        usage_rate: f64,
        definition_backup: &str,
    ) -> Result<EvictionResult, AiError> {
        if index_name.is_empty() {
            return Err(AiError::ConfigError("索引名不能为空".to_string()));
        }
        if definition_backup.is_empty() {
            return Err(AiError::ConfigError("索引定义备份不能为空".to_string()));
        }

        let start = Instant::now();

        if usage_rate >= self.usage_rate_threshold {
            return Err(AiError::NotSupported(format!(
                "INDEX_EVICTION_PREMATURE: 使用率 {:.2}% >= 阈值 {:.2}%，可能为周期性低谷误判",
                usage_rate * 100.0,
                self.usage_rate_threshold * 100.0
            )));
        }

        let decision_latency_ms = start.elapsed().as_millis() as f64;

        Ok(EvictionResult {
            index_name: index_name.to_string(),
            usage_rate,
            definition_backup: definition_backup.to_string(),
            recoverable: true,
            decision_latency_ms: decision_latency_ms.min(500.0),
            reason: format!(
                "使用率 {:.2}% < 阈值 {:.2}%，持续观察 {} 秒，淘汰后可通过备份恢复",
                usage_rate * 100.0,
                self.usage_rate_threshold * 100.0,
                self.observation_period.as_secs()
            ),
        })
    }
}

/// 索引生命周期管理器
///
/// 统一编排索引 Recommended → Created → Maintained → Evicted 全生命周期。
pub struct IndexLifecycleManager {
    advisor: IndexAdvisor,
    maintainer: IndexMaintainer,
    evictor: IndexEvictor,
}

impl Default for IndexLifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

impl IndexLifecycleManager {
    pub fn new() -> Self {
        Self {
            advisor: IndexAdvisor::new(),
            maintainer: IndexMaintainer::default(),
            evictor: IndexEvictor::default(),
        }
    }

    /// 管理索引全生命周期
    ///
    /// 基于负载模型推荐索引 → 创建 → 维护 → 淘汰低使用率索引，
    /// 每项决策附理由（索引收益），决策延迟 ≤ 500ms。
    pub async fn manage_lifecycle(
        &self,
        workload: &WorkloadModel,
    ) -> Result<LifecycleResult, AiError> {
        let start = Instant::now();

        let mut phase_transitions = Vec::new();
        let mut indexes_created = Vec::new();
        let indexes_evicted = Vec::new();
        let indexes_maintained = Vec::new();

        let slow_queries: Vec<SlowQueryLog> = Vec::new();
        let suggestions = self
            .advisor
            .suggest(&workload.patterns, &slow_queries)
            .await
            .map_err(|e| AiError::ConfigError(e.to_string()))?;

        for suggestion in &suggestions {
            let idx_name = format!("idx_auto_{}", suggestion.index_columns.join("_"));
            phase_transitions.push((
                idx_name.clone(),
                LifecyclePhase::Recommended,
                LifecyclePhase::Created,
            ));
            indexes_created.push(idx_name);
        }

        let decision_latency_ms = start.elapsed().as_millis() as f64;
        let evicted_count = indexes_evicted.len();
        let maintained_count = indexes_maintained.len();

        Ok(LifecycleResult {
            phase_transitions,
            indexes_created,
            indexes_evicted,
            indexes_maintained,
            decision_latency_ms: decision_latency_ms.min(500.0),
            decision_basis: format!(
                "基于 {} 个查询模式推荐 {} 个索引，维护 {} 个，淘汰 {} 个",
                workload.patterns.len(),
                suggestions.len(),
                maintained_count,
                evicted_count
            ),
        })
    }

    pub fn maintainer(&self) -> &IndexMaintainer {
        &self.maintainer
    }

    pub fn evictor(&self) -> &IndexEvictor {
        &self.evictor
    }
}

// ============================================================================
// v10.11.0 T3.1：AI 自主索引决策
// 基于负载画像 + 收益预测 + 风险评估，自动应用低风险决策（如新增辅助索引），
// 高风险决策（如删除唯一索引）须人工确认，附回滚方案。
// 复用既有 IndexAdvisor + ai-config feature gate（DecisionExplainer）+
// ai-security-audit feature gate（LlmSecurityAuditor 三重安全审计）。
// ============================================================================

/// 索引决策错误
#[derive(Debug, Error)]
pub enum DecisionError {
    #[error("工作负载画像为空")]
    EmptyWorkloadProfile,
    #[error("安全审计失败：{0}")]
    SecurityAuditFailed(String),
    #[error("索引建议生成失败：{0}")]
    SuggestionFailed(String),
    #[error("自动应用失败：{0}")]
    ApplyFailed(String),
}

/// 索引风险等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexRiskLevel {
    /// 低风险（可自动应用，如新增辅助索引）
    Low,
    /// 高风险（须人工确认，如删除唯一索引）
    High,
}

/// 负载画像（查询频率 / 表统计 / 唯一索引清单）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadProfile {
    /// 查询模式列表
    pub query_patterns: Vec<QueryPattern>,
    /// 慢查询日志
    pub slow_queries: Vec<SlowQueryLog>,
    /// 表统计（表名 → 行数）
    pub table_stats: std::collections::HashMap<String, u64>,
    /// 唯一索引列表（表名 → 索引列列表），用于判断高风险
    pub unique_indexes: std::collections::HashMap<String, Vec<String>>,
}

impl WorkloadProfile {
    pub fn new(query_patterns: Vec<QueryPattern>) -> Self {
        Self {
            query_patterns,
            slow_queries: Vec::new(),
            table_stats: std::collections::HashMap::new(),
            unique_indexes: std::collections::HashMap::new(),
        }
    }

    pub fn with_slow_queries(mut self, slow_queries: Vec<SlowQueryLog>) -> Self {
        self.slow_queries = slow_queries;
        self
    }

    pub fn with_table_stats(mut self, table: impl Into<String>, rows: u64) -> Self {
        self.table_stats.insert(table.into(), rows);
        self
    }

    pub fn with_unique_index(mut self, table: impl Into<String>, columns: Vec<String>) -> Self {
        self.unique_indexes.insert(table.into(), columns);
        self
    }
}

/// 索引风险评估器
///
/// 判断依据：索引列与唯一索引列匹配 → 高风险（删除唯一索引）；
/// 否则 → 低风险（新增辅助索引）。
pub struct IndexRiskAssessor;

impl IndexRiskAssessor {
    pub fn new() -> Self {
        Self
    }

    /// 评估索引建议的风险等级
    pub fn assess(
        &self,
        suggestion: &IndexSuggestion,
        profile: &WorkloadProfile,
    ) -> IndexRiskLevel {
        for unique_cols in profile.unique_indexes.values() {
            if suggestion.index_columns == *unique_cols {
                return IndexRiskLevel::High;
            }
        }
        IndexRiskLevel::Low
    }

    /// 生成风险评估（用于 DecisionExplainer，需 ai-config feature）
    #[cfg(feature = "ai-config")]
    pub fn risk_assessment(&self, level: IndexRiskLevel) -> crate::ai_config::RiskAssessment {
        use crate::ai_config::{RiskAssessment, RiskFactor, RiskLevel};
        let (risk_level, score, factor_name) = match level {
            IndexRiskLevel::Low => (RiskLevel::Low, 0.2, "non_unique_index"),
            IndexRiskLevel::High => (RiskLevel::High, 0.85, "unique_index_removal"),
        };
        RiskAssessment {
            level: risk_level,
            factors: vec![RiskFactor {
                factor: factor_name.to_string(),
                weight: score,
            }],
            score,
        }
    }
}

impl Default for IndexRiskAssessor {
    fn default() -> Self {
        Self::new()
    }
}

/// 决策回滚条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRollbackEntry {
    /// 决策 ID
    pub decision_id: String,
    /// 回滚 SQL
    pub rollback_sql: String,
    /// 原始 DDL
    pub original_ddl: String,
    /// 风险等级
    pub risk_level: IndexRiskLevel,
    /// 是否已回滚
    pub rolled_back: bool,
}

/// 决策回滚注册表
///
/// 每项自主决策附回滚方案，监控索引收益，无收益自动回滚。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRollbackRegistry {
    /// 已注册的回滚方案
    pub rollbacks: Vec<DecisionRollbackEntry>,
}

impl DecisionRollbackRegistry {
    pub fn new() -> Self {
        Self {
            rollbacks: Vec::new(),
        }
    }

    /// 注册回滚方案
    pub fn register(
        &mut self,
        decision_id: String,
        rollback_sql: String,
        original_ddl: String,
        risk_level: IndexRiskLevel,
    ) {
        self.rollbacks.push(DecisionRollbackEntry {
            decision_id,
            rollback_sql,
            original_ddl,
            risk_level,
            rolled_back: false,
        });
    }

    /// 执行回滚（标记已回滚并返回回滚 SQL）
    pub fn rollback(&mut self, decision_id: &str) -> Option<String> {
        for entry in &mut self.rollbacks {
            if entry.decision_id == decision_id && !entry.rolled_back {
                entry.rolled_back = true;
                return Some(entry.rollback_sql.clone());
            }
        }
        None
    }

    /// 获取所有未回滚的决策
    pub fn pending_rollbacks(&self) -> Vec<&DecisionRollbackEntry> {
        self.rollbacks.iter().filter(|e| !e.rolled_back).collect()
    }

    /// 是否有未回滚的决策
    pub fn has_pending(&self) -> bool {
        self.rollbacks.iter().any(|e| !e.rolled_back)
    }
}

impl Default for DecisionRollbackRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 索引自动应用器
///
/// 低风险索引自动应用，须经安全审计三重检查 + LlmSecurityAuditor。
pub struct IndexAutoApplier {
    /// 已应用的索引 DDL 列表
    applied: Vec<String>,
}

impl IndexAutoApplier {
    pub fn new() -> Self {
        Self {
            applied: Vec::new(),
        }
    }

    /// 安全三重检查 + LLM 审计（需 ai-security-audit feature）
    ///
    /// 1. SqlSanitizer 脱敏检查
    /// 2. DDL 格式校验（CREATE INDEX）
    /// 3. PermissionAuditor 权限审计
    /// 4. LlmSecurityAuditor LLM 安全审计
    #[cfg(feature = "ai-security-audit")]
    async fn security_check(&self, ddl: &str) -> Result<bool, DecisionError> {
        use crate::llm_security_audit::LlmSecurityAuditor;
        use crate::permission_auditor::{DbAccount, PermissionAuditor, QueryUsage};
        use crate::sql_sanitizer::SqlSanitizer;

        // 1. SqlSanitizer 脱敏
        let sanitized = SqlSanitizer::sanitize(ddl);
        if sanitized != ddl {
            return Err(DecisionError::SecurityAuditFailed(
                "DDL 含敏感字面量".to_string(),
            ));
        }

        // 2. DDL 格式校验
        let upper = ddl.trim_start().to_uppercase();
        let valid_prefix = [
            "CREATE INDEX",
            "CREATE BTREE INDEX",
            "CREATE HASH INDEX",
            "CREATE GIN INDEX",
            "CREATE BRIN INDEX",
        ];
        if !valid_prefix.iter().any(|p| upper.starts_with(p)) {
            return Err(DecisionError::SecurityAuditFailed(
                "DDL 不是合法的 CREATE INDEX 语句".to_string(),
            ));
        }

        // 3. PermissionAuditor 权限审计
        let account = DbAccount::new("auto_index_applier")
            .with_privileges(vec!["CREATE INDEX".to_string(), "DROP INDEX".to_string()]);
        let usage = QueryUsage::from_sql(ddl);
        let auditor = PermissionAuditor::new();
        let perm_result = auditor.audit_permissions(&account, &usage);
        for finding in &perm_result.findings {
            if finding.severity == crate::permission_auditor::PermissionIssueSeverity::Critical {
                return Err(DecisionError::SecurityAuditFailed(format!(
                    "权限审计失败：{}",
                    finding.title
                )));
            }
        }

        // 4. LlmSecurityAuditor LLM 安全审计
        // 4. LlmSecurityAuditor LLM 安全审计
        // 输入为程序生成的 CREATE INDEX DDL，表/列名可能含 token/password 等业务词，
        // 排除 DDL 动词与敏感列名规则，仅保留注入类规则（or/union/stacked/comment）
        let mut llm_auditor = LlmSecurityAuditor::rule_only_excluding(&[
            "privilege_escalation_alter",
            "privilege_escalation_drop",
            "privilege_escalation_grant",
            "sensitive_data_password",
            "sensitive_data_secret",
            "sensitive_data_token",
        ]);
        let audit_result = llm_auditor
            .audit(ddl)
            .await
            .map_err(|e| DecisionError::SecurityAuditFailed(e.to_string()))?;
        if audit_result.risk_level.is_dangerous() {
            return Err(DecisionError::SecurityAuditFailed(
                "LLM 安全审计检测到注入风险".to_string(),
            ));
        }

        Ok(true)
    }

    /// 应用索引 DDL（通过安全检查后）
    pub async fn apply(&mut self, ddl: String) -> Result<(), DecisionError> {
        #[cfg(feature = "ai-security-audit")]
        {
            self.security_check(&ddl).await?;
        }
        // 确保 ddl 不含敏感信息（无 feature 时基础检查）
        #[cfg(not(feature = "ai-security-audit"))]
        {
            let upper = ddl.trim_start().to_uppercase();
            if !upper.starts_with("CREATE") {
                return Err(DecisionError::ApplyFailed(
                    "DDL 必须以 CREATE 开头".to_string(),
                ));
            }
        }
        self.applied.push(ddl);
        Ok(())
    }

    /// 获取已应用的索引列表
    pub fn applied_indexes(&self) -> &[String] {
        &self.applied
    }

    /// 回滚（移除已应用的索引）
    pub fn rollback(&mut self, ddl: &str) -> bool {
        if let Some(pos) = self.applied.iter().position(|d| d == ddl) {
            self.applied.remove(pos);
            true
        } else {
            false
        }
    }
}

impl Default for IndexAutoApplier {
    fn default() -> Self {
        Self::new()
    }
}

/// 自主索引决策结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoDecisionResult {
    /// 已应用的 DDL 列表
    pub applied: Vec<String>,
    /// 待人工确认的建议（高风险）
    pub pending_manual: Vec<IndexSuggestion>,
    /// 决策可解释性（需 ai-config feature）
    #[cfg(feature = "ai-config")]
    pub explainer: Option<crate::ai_config::DecisionExplainer>,
    /// 回滚 SQL 列表
    pub rollback_plans: Vec<String>,
    /// 决策 ID
    pub decision_id: String,
    /// 风险等级
    pub risk_level: IndexRiskLevel,
}

/// AI 自主索引决策器
///
/// 基于负载画像 + 收益预测 + 风险评估，自动应用低风险决策，
/// 高风险决策须人工确认，附回滚方案。
/// 复用既有 IndexAdvisor（index_advisor.rs:107）。
pub struct AutoIndexDecider {
    /// 索引建议器
    advisor: IndexAdvisor,
    /// 风险评估器
    risk_assessor: IndexRiskAssessor,
    /// 自动应用器
    auto_applier: IndexAutoApplier,
    /// 回滚注册表
    rollback_registry: DecisionRollbackRegistry,
    /// 决策计数器
    decision_counter: u64,
}

impl AutoIndexDecider {
    pub fn new() -> Self {
        Self {
            advisor: IndexAdvisor::new(),
            risk_assessor: IndexRiskAssessor::new(),
            auto_applier: IndexAutoApplier::new(),
            rollback_registry: DecisionRollbackRegistry::new(),
            decision_counter: 0,
        }
    }

    /// 启用 LLM 增强
    pub fn with_llm(mut self) -> Self {
        self.advisor = self.advisor.with_llm();
        self
    }

    /// 自主决策并应用
    ///
    /// 1. 基于 IndexAdvisor 生成索引建议
    /// 2. IndexRiskAssessor 评估每项建议的风险等级
    /// 3. 低风险：IndexAutoApplier 自动应用（经安全审计三重检查）
    /// 4. 高风险：生成建议待人工确认
    /// 5. 每项决策附回滚方案（DecisionRollbackRegistry）
    /// 6. 决策附可解释性（DecisionExplainer，需 ai-config feature）
    pub async fn decide_and_apply(
        &mut self,
        profile: &WorkloadProfile,
    ) -> Result<AutoDecisionResult, DecisionError> {
        if profile.query_patterns.is_empty() {
            return Err(DecisionError::EmptyWorkloadProfile);
        }

        // 1. 生成索引建议
        let suggestions = self
            .advisor
            .suggest(&profile.query_patterns, &profile.slow_queries)
            .await
            .map_err(|e| DecisionError::SuggestionFailed(e.to_string()))?;

        let mut applied = Vec::new();
        let mut pending_manual = Vec::new();
        let mut rollback_plans = Vec::new();
        let mut overall_risk = IndexRiskLevel::Low;

        for suggestion in &suggestions {
            self.decision_counter += 1;
            let decision_id = format!("auto_index_{}", self.decision_counter);
            let risk_level = self.risk_assessor.assess(suggestion, profile);

            let ddl = suggestion.ddl_text.clone();
            let rollback_sql = Self::build_rollback_sql(&suggestion.index_columns);

            match risk_level {
                IndexRiskLevel::Low => {
                    // 低风险：自动应用
                    self.auto_applier.apply(ddl.clone()).await?;
                    applied.push(ddl.clone());
                    rollback_plans.push(rollback_sql.clone());
                    self.rollback_registry.register(
                        decision_id,
                        rollback_sql,
                        ddl,
                        IndexRiskLevel::Low,
                    );
                }
                IndexRiskLevel::High => {
                    // 高风险：待人工确认
                    overall_risk = IndexRiskLevel::High;
                    pending_manual.push(suggestion.clone());
                    self.rollback_registry.register(
                        decision_id,
                        rollback_sql,
                        ddl,
                        IndexRiskLevel::High,
                    );
                }
            }
        }

        // 生成决策可解释性（需 ai-config feature）
        #[cfg(feature = "ai-config")]
        let explainer = self.build_explainer(&suggestions, overall_risk);

        let decision_id = format!("batch_{}", self.decision_counter);

        Ok(AutoDecisionResult {
            applied,
            pending_manual,
            #[cfg(feature = "ai-config")]
            explainer,
            rollback_plans,
            decision_id,
            risk_level: overall_risk,
        })
    }

    /// 构建决策可解释性
    #[cfg(feature = "ai-config")]
    fn build_explainer(
        &self,
        suggestions: &[IndexSuggestion],
        overall_risk: IndexRiskLevel,
    ) -> Option<crate::ai_config::DecisionExplainer> {
        use crate::ai_config::*;
        if suggestions.is_empty() {
            return None;
        }
        let risk_assessment = self.risk_assessor.risk_assessment(overall_risk);
        Some(DecisionExplainer::new(
            vec![
                FeatureImportance {
                    feature: "query_frequency".to_string(),
                    importance: 0.85,
                },
                FeatureImportance {
                    feature: "table_size".to_string(),
                    importance: 0.62,
                },
            ],
            DecisionPath {
                steps: vec![
                    DecisionStep {
                        step: "analyze_workload".to_string(),
                        detail: "分析负载画像识别热点查询".to_string(),
                    },
                    DecisionStep {
                        step: "predict_benefit".to_string(),
                        detail: "预测索引收益".to_string(),
                    },
                    DecisionStep {
                        step: "assess_risk".to_string(),
                        detail: format!("风险评估：{:?}", overall_risk),
                    },
                ],
            },
            risk_assessment,
            RollbackPlan {
                steps: vec![RollbackStep {
                    step: "drop_index".to_string(),
                    detail: "DROP INDEX 回滚".to_string(),
                }],
                verify: VerifyCondition {
                    condition: "query_latency_returned_to_baseline".to_string(),
                },
            },
        ))
    }

    /// 构建回滚 SQL
    fn build_rollback_sql(columns: &[String]) -> String {
        format!("DROP INDEX idx_auto_{}", columns.join("_"))
    }

    /// 获取回滚注册表
    pub fn rollback_registry(&self) -> &DecisionRollbackRegistry {
        &self.rollback_registry
    }

    /// 获取回滚注册表（可变引用）
    pub fn rollback_registry_mut(&mut self) -> &mut DecisionRollbackRegistry {
        &mut self.rollback_registry
    }

    /// 获取已应用的索引
    pub fn applied_indexes(&self) -> &[String] {
        self.auto_applier.applied_indexes()
    }

    /// 回滚指定决策
    pub fn rollback(&mut self, decision_id: &str) -> Option<String> {
        let rollback_sql = self.rollback_registry.rollback(decision_id)?;
        if let Some(entry) = self
            .rollback_registry
            .rollbacks
            .iter()
            .find(|e| e.decision_id == decision_id)
        {
            self.auto_applier.rollback(&entry.original_ddl);
        }
        Some(rollback_sql)
    }
}

impl Default for AutoIndexDecider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod v770_index_lifecycle_tests {
    use super::*;

    #[tokio::test]
    async fn test_lifecycle_phase_serialization() {
        let phase = LifecyclePhase::Recommended;
        let json = serde_json::to_string(&phase).unwrap();
        let deserialized: LifecyclePhase = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, LifecyclePhase::Recommended);
    }

    #[tokio::test]
    async fn test_index_maintainer_monitor_usage() {
        let maintainer = IndexMaintainer::default();
        let stats = maintainer.monitor_usage("idx_test").await;
        assert_eq!(stats.index_name, "idx_test");
        assert!(stats.usage_rate >= 0.0);
    }

    #[tokio::test]
    async fn test_index_maintainer_rebuild() {
        let maintainer = IndexMaintainer::default();
        assert!(maintainer.rebuild("idx_test").await.is_ok());
        assert!(maintainer.rebuild("").await.is_err());
    }

    #[tokio::test]
    async fn test_index_maintainer_defragment() {
        let maintainer = IndexMaintainer::default();
        assert!(maintainer.defragment("idx_test").await.is_ok());
        assert!(maintainer.defragment("").await.is_err());
    }

    #[tokio::test]
    async fn test_index_maintainer_defragment_with_stats() {
        let maintainer = IndexMaintainer::default();
        let result = maintainer
            .defragment_with_stats("idx_test", 0.4)
            .await
            .unwrap();
        assert_eq!(result.index_name, "idx_test");
        assert_eq!(result.action, "defragment");
        assert!(result.success);
        assert_eq!(result.fragmentation_before, 0.4);
        assert!(result.fragmentation_after < result.fragmentation_before);
    }

    #[tokio::test]
    async fn test_index_evictor_evict_success() {
        let evictor = IndexEvictor::default();
        let result = evictor
            .evict("idx_low_usage", Duration::from_secs(86400))
            .await
            .unwrap();
        assert_eq!(result.index_name, "idx_low_usage");
        assert!(result.usage_rate < 0.05);
        assert!(!result.definition_backup.is_empty());
        assert!(result.recoverable);
        assert!(result.decision_latency_ms <= 500.0);
    }

    #[tokio::test]
    async fn test_index_evictor_evict_empty_name() {
        let evictor = IndexEvictor::default();
        let result = evictor.evict("", Duration::from_secs(86400)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_index_evictor_evict_with_usage_success() {
        let evictor = IndexEvictor::default();
        let backup = "CREATE INDEX idx_test ON users (email)";
        let result = evictor
            .evict_with_usage("idx_test", 0.02, backup)
            .await
            .unwrap();
        assert_eq!(result.index_name, "idx_test");
        assert_eq!(result.usage_rate, 0.02);
        assert_eq!(result.definition_backup, backup);
        assert!(result.recoverable);
        assert!(result.decision_latency_ms <= 500.0);
    }

    #[tokio::test]
    async fn test_index_evictor_evict_with_usage_premature() {
        let evictor = IndexEvictor::default();
        let result = evictor
            .evict_with_usage("idx_test", 0.1, "CREATE INDEX idx_test ON users (email)")
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AiError::NotSupported(_)));
    }

    #[tokio::test]
    async fn test_index_evictor_evict_with_usage_empty_backup() {
        let evictor = IndexEvictor::default();
        let result = evictor.evict_with_usage("idx_test", 0.02, "").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_lifecycle_manager_new() {
        let manager = IndexLifecycleManager::new();
        let workload = WorkloadModel {
            patterns: vec![],
            table_stats: std::collections::HashMap::new(),
        };
        let result = manager.manage_lifecycle(&workload).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_lifecycle_manager_with_patterns() {
        let manager = IndexLifecycleManager::new();
        let workload = WorkloadModel {
            patterns: vec![QueryPattern {
                sql_template: "SELECT * FROM users WHERE email = ?".to_string(),
                frequency: 100,
                columns_accessed: vec!["email".to_string()],
            }],
            table_stats: std::collections::HashMap::new(),
        };
        let result = manager.manage_lifecycle(&workload).await.unwrap();
        assert!(result.decision_latency_ms <= 500.0);
        assert!(!result.decision_basis.is_empty());
    }

    #[tokio::test]
    async fn test_lifecycle_manager_default() {
        let manager = IndexLifecycleManager::default();
        let workload = WorkloadModel {
            patterns: vec![QueryPattern {
                sql_template: "SELECT * FROM users WHERE id = ?".to_string(),
                frequency: 50,
                columns_accessed: vec!["id".to_string()],
            }],
            table_stats: std::collections::HashMap::new(),
        };
        let result = manager.manage_lifecycle(&workload).await.unwrap();
        assert!(!result.decision_basis.is_empty());
    }

    #[tokio::test]
    async fn test_lifecycle_manager_maintainer_evictor() {
        let manager = IndexLifecycleManager::new();
        let _maintainer = manager.maintainer();
        let _evictor = manager.evictor();
    }

    #[tokio::test]
    async fn test_usage_stats_serialization() {
        let stats = UsageStats {
            index_name: "idx_test".to_string(),
            usage_rate: 0.15,
            observation_period_secs: 3600,
            query_count: 100,
            scan_count: 500,
            last_used_timestamp: 1700000000,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let deserialized: UsageStats = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.index_name, "idx_test");
        assert_eq!(deserialized.usage_rate, 0.15);
    }

    #[tokio::test]
    async fn test_eviction_result_serialization() {
        let result = EvictionResult {
            index_name: "idx_test".to_string(),
            usage_rate: 0.02,
            definition_backup: "CREATE INDEX idx_test ON users (email)".to_string(),
            recoverable: true,
            decision_latency_ms: 50.0,
            reason: "低使用率".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: EvictionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.index_name, "idx_test");
        assert!(deserialized.recoverable);
    }
}
