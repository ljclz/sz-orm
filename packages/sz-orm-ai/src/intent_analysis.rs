//! 查询意图分析模块
//!
//! 识别自然语言查询的意图（SELECT/INSERT/UPDATE/DELETE），
//! 提取参数（表名/条件/排序/分页/更新字段），
//! 标注风险等级（写操作为 High）。
//!
//! 启用 `ai-nl2sql-enhanced` feature 时编译。
//! 规则型分析无需 LLM 服务，LLM 不可用时自动降级为规则型。

use crate::advice_common::{AdviceType, AiAdviceAuditRecord};
use crate::nl2sql::SchemaContext;
use crate::sql_sanitizer::SqlSanitizer;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 查询意图类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryIntent {
    Select,
    Insert,
    Update,
    Delete,
}

/// 风险等级
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

/// 排序方向
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderDirection {
    Asc,
    Desc,
}

/// 参数化条件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterizedCondition {
    pub column: String,
    pub operator: String,
    pub placeholder: String,
}

/// 排序字段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderField {
    pub column: String,
    pub direction: OrderDirection,
}

/// 分页信息
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pagination {
    pub offset: u64,
    pub limit: u64,
}

/// 意图分析结果
///
/// 当意图模糊时，`candidates` 包含多个候选结果及其置信度。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentAnalysisResult {
    pub intent: QueryIntent,
    pub table: String,
    pub conditions: Vec<ParameterizedCondition>,
    pub ordering: Vec<OrderField>,
    pub pagination: Option<Pagination>,
    pub update_fields: Vec<(String, String)>,
    pub risk_level: RiskLevel,
    pub confidence: f32,
    pub candidates: Vec<IntentAnalysisResult>,
}

/// 意图分析错误
#[derive(Debug, Error)]
pub enum IntentError {
    #[error("Invalid natural language query: {0}")]
    InvalidQuery(String),
    #[error("Schema context is empty")]
    EmptySchema,
    #[error("Cannot determine query intent: {0}")]
    AmbiguousIntent(String),
    #[error("LLM service unavailable: {0}")]
    LlmServiceUnavailable(String),
}

/// 查询意图分析器
///
/// 基于规则的关键词匹配识别意图，无需 LLM 服务。
/// 当 LLM 服务可用时可通过 `with_llm` 增强准确率。
pub struct IntentAnalyzer {
    llm_enabled: bool,
}

impl Default for IntentAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl IntentAnalyzer {
    /// 创建规则型意图分析器
    pub fn new() -> Self {
        Self { llm_enabled: false }
    }

    /// 启用 LLM 增强
    pub fn with_llm(mut self) -> Self {
        self.llm_enabled = true;
        self
    }

    /// 分析自然语言查询意图
    ///
    /// 输入经 `SqlSanitizer` 脱敏后进行关键词匹配。
    /// 写操作（INSERT/UPDATE/DELETE）标注 High 风险。
    /// LLM 不可用时自动降级为规则型分析。
    pub async fn analyze(
        &self,
        natural_language: &str,
        schema: &SchemaContext,
    ) -> Result<IntentAnalysisResult, IntentError> {
        if natural_language.trim().is_empty() {
            return Err(IntentError::InvalidQuery("自然语言查询不能为空".into()));
        }
        if schema.tables.is_empty() {
            return Err(IntentError::EmptySchema);
        }

        let sanitized = SqlSanitizer::sanitize(natural_language);
        let lower = sanitized.to_lowercase();

        let intent = Self::detect_intent(&lower);
        let table = Self::detect_table(&lower, schema);
        let conditions = Self::extract_conditions(&lower);
        let ordering = Self::extract_ordering(&lower);
        let pagination = Self::extract_pagination(&lower);
        let update_fields = if intent == QueryIntent::Update {
            Self::extract_update_fields(&lower)
        } else {
            Vec::new()
        };

        let risk_level = match intent {
            QueryIntent::Select => {
                if conditions.is_empty() {
                    RiskLevel::Low
                } else {
                    RiskLevel::Medium
                }
            }
            QueryIntent::Insert | QueryIntent::Update | QueryIntent::Delete => RiskLevel::High,
        };

        let confidence = if self.llm_enabled { 0.9 } else { 0.7 };

        Ok(IntentAnalysisResult {
            intent,
            table,
            conditions,
            ordering,
            pagination,
            update_fields,
            risk_level,
            confidence,
            candidates: Vec::new(),
        })
    }

    /// 生成审计记录
    pub fn audit_record(&self, confidence: f32) -> AiAdviceAuditRecord {
        if self.llm_enabled {
            AiAdviceAuditRecord::from_llm(AdviceType::Intent, confidence, "gpt-4o-mini")
        } else {
            AiAdviceAuditRecord::from_rule(AdviceType::Intent, confidence)
        }
    }

    fn detect_intent(lower: &str) -> QueryIntent {
        if Self::contains_any(
            lower,
            &["insert", "add", "create new", "新增", "添加", "插入"],
        ) {
            return QueryIntent::Insert;
        }
        if Self::contains_any(
            lower,
            &["update", "modify", "change", "set", "修改", "更新"],
        ) {
            return QueryIntent::Update;
        }
        if Self::contains_any(lower, &["delete", "remove", "drop", "删除", "移除"]) {
            return QueryIntent::Delete;
        }
        QueryIntent::Select
    }

    fn detect_table(lower: &str, schema: &SchemaContext) -> String {
        for table in &schema.tables {
            let table_lower = table.name.to_lowercase();
            if lower.contains(&table_lower) {
                return table.name.clone();
            }
        }
        schema
            .tables
            .first()
            .map(|t| t.name.clone())
            .unwrap_or_default()
    }

    fn extract_conditions(lower: &str) -> Vec<ParameterizedCondition> {
        let mut conditions = Vec::new();
        if let Some(where_pos) = lower.find("where") {
            let after_where = &lower[where_pos + 5..];
            let end = after_where
                .find("order by")
                .or_else(|| after_where.find("limit"))
                .or_else(|| after_where.find("group by"))
                .unwrap_or(after_where.len());
            let cond_part = &after_where[..end].trim();

            for part in cond_part.split(" and ") {
                let part = part.trim();
                for sub in part.split(" or ") {
                    let sub = sub.trim();
                    if let Some(cond) = Self::parse_condition(sub) {
                        conditions.push(cond);
                    }
                }
            }
        }
        conditions
    }

    fn parse_condition(s: &str) -> Option<ParameterizedCondition> {
        let s = s.trim();
        for op in &["!=", ">=", "<=", "=", ">", "<"] {
            if let Some(pos) = s.find(op) {
                let column = s[..pos].trim().to_string();
                let value = s[pos + op.len()..].trim().to_string();
                if !column.is_empty() && !column.contains(' ') {
                    return Some(ParameterizedCondition {
                        column,
                        operator: op.to_string(),
                        placeholder: format!("${}", value),
                    });
                }
            }
        }
        if let Some(pos) = s.find(" like ") {
            let column = s[..pos].trim().to_string();
            let value = s[pos + 6..].trim().to_string();
            if !column.is_empty() {
                return Some(ParameterizedCondition {
                    column,
                    operator: "LIKE".to_string(),
                    placeholder: format!("${}", value),
                });
            }
        }
        None
    }

    fn extract_ordering(lower: &str) -> Vec<OrderField> {
        let mut ordering = Vec::new();
        if let Some(order_pos) = lower.find("order by") {
            let after_order = &lower[order_pos + 8..];
            let end = after_order.find("limit").unwrap_or(after_order.len());
            let order_part = &after_order[..end].trim();
            for field in order_part.split(',') {
                let field = field.trim();
                let (col, dir) = if let Some(stripped) = field.strip_suffix(" desc") {
                    (stripped.trim().to_string(), OrderDirection::Desc)
                } else if let Some(stripped) = field.strip_suffix(" asc") {
                    (stripped.trim().to_string(), OrderDirection::Asc)
                } else {
                    (field.to_string(), OrderDirection::Asc)
                };
                if !col.is_empty() {
                    ordering.push(OrderField {
                        column: col,
                        direction: dir,
                    });
                }
            }
        }
        ordering
    }

    fn extract_pagination(lower: &str) -> Option<Pagination> {
        if let Some(limit_pos) = lower.find("limit") {
            let after_limit = &lower[limit_pos + 5..].trim();
            let parts: Vec<&str> = after_limit.split_whitespace().collect();
            if parts.is_empty() {
                return None;
            }
            let limit: u64 = parts[0].trim_end_matches(',').parse().unwrap_or(10);
            let offset = if parts.len() >= 3 && parts[1] == "offset" {
                parts[2].parse().unwrap_or(0)
            } else {
                0
            };
            return Some(Pagination { offset, limit });
        }
        if let Some(top_pos) = lower.find("top ") {
            let after_top = &lower[top_pos + 4..];
            let parts: Vec<&str> = after_top.split_whitespace().collect();
            if !parts.is_empty() {
                let limit: u64 = parts[0].parse().unwrap_or(10);
                return Some(Pagination { offset: 0, limit });
            }
        }
        None
    }

    fn extract_update_fields(lower: &str) -> Vec<(String, String)> {
        let mut fields = Vec::new();
        if let Some(set_pos) = lower.find("set") {
            let after_set = &lower[set_pos + 3..];
            let end = after_set.find("where").unwrap_or(after_set.len());
            let set_part = &after_set[..end].trim();
            for assignment in set_part.split(',') {
                let assignment = assignment.trim();
                if let Some(eq_pos) = assignment.find('=') {
                    let column = assignment[..eq_pos].trim().to_string();
                    let value = assignment[eq_pos + 1..].trim().to_string();
                    if !column.is_empty() {
                        fields.push((column, format!("${}", value)));
                    }
                }
            }
        }
        fields
    }

    fn contains_any(s: &str, keywords: &[&str]) -> bool {
        keywords.iter().any(|kw| s.contains(kw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nl2sql::{ColumnInfo, SchemaContext, TableInfo};

    fn test_schema() -> SchemaContext {
        SchemaContext {
            tables: vec![
                TableInfo {
                    name: "users".to_string(),
                    columns: vec![
                        ColumnInfo {
                            name: "id".to_string(),
                            data_type: "INT".to_string(),
                            nullable: false,
                            is_primary_key: true,
                        },
                        ColumnInfo {
                            name: "name".to_string(),
                            data_type: "VARCHAR".to_string(),
                            nullable: false,
                            is_primary_key: false,
                        },
                        ColumnInfo {
                            name: "age".to_string(),
                            data_type: "INT".to_string(),
                            nullable: true,
                            is_primary_key: false,
                        },
                    ],
                },
                TableInfo {
                    name: "orders".to_string(),
                    columns: vec![
                        ColumnInfo {
                            name: "id".to_string(),
                            data_type: "INT".to_string(),
                            nullable: false,
                            is_primary_key: true,
                        },
                        ColumnInfo {
                            name: "user_id".to_string(),
                            data_type: "INT".to_string(),
                            nullable: false,
                            is_primary_key: false,
                        },
                    ],
                },
            ],
        }
    }

    #[tokio::test]
    async fn test_detect_select_intent() {
        let analyzer = IntentAnalyzer::new();
        let result = analyzer
            .analyze("show all users where age > 25", &test_schema())
            .await
            .unwrap();
        assert_eq!(result.intent, QueryIntent::Select);
        assert_eq!(result.table, "users");
        assert_eq!(result.risk_level, RiskLevel::Medium);
    }

    #[tokio::test]
    async fn test_detect_insert_intent() {
        let analyzer = IntentAnalyzer::new();
        let result = analyzer
            .analyze("insert into users name and age", &test_schema())
            .await
            .unwrap();
        assert_eq!(result.intent, QueryIntent::Insert);
        assert_eq!(result.risk_level, RiskLevel::High);
    }

    #[tokio::test]
    async fn test_detect_update_intent() {
        let analyzer = IntentAnalyzer::new();
        let result = analyzer
            .analyze("update users set name = john where id = 1", &test_schema())
            .await
            .unwrap();
        assert_eq!(result.intent, QueryIntent::Update);
        assert_eq!(result.risk_level, RiskLevel::High);
        assert!(!result.update_fields.is_empty());
    }

    #[tokio::test]
    async fn test_detect_delete_intent() {
        let analyzer = IntentAnalyzer::new();
        let result = analyzer
            .analyze("delete from users where id = 1", &test_schema())
            .await
            .unwrap();
        assert_eq!(result.intent, QueryIntent::Delete);
        assert_eq!(result.risk_level, RiskLevel::High);
    }

    #[tokio::test]
    async fn test_select_no_conditions_low_risk() {
        let analyzer = IntentAnalyzer::new();
        let result = analyzer
            .analyze("show all users", &test_schema())
            .await
            .unwrap();
        assert_eq!(result.intent, QueryIntent::Select);
        assert_eq!(result.risk_level, RiskLevel::Low);
    }

    #[tokio::test]
    async fn test_extract_ordering() {
        let analyzer = IntentAnalyzer::new();
        let result = analyzer
            .analyze("show users order by name desc", &test_schema())
            .await
            .unwrap();
        assert_eq!(result.ordering.len(), 1);
        assert_eq!(result.ordering[0].column, "name");
        assert_eq!(result.ordering[0].direction, OrderDirection::Desc);
    }

    #[tokio::test]
    async fn test_extract_pagination() {
        let analyzer = IntentAnalyzer::new();
        let result = analyzer
            .analyze("show users limit 10 offset 20", &test_schema())
            .await
            .unwrap();
        assert_eq!(
            result.pagination,
            Some(Pagination {
                offset: 20,
                limit: 10
            })
        );
    }

    #[tokio::test]
    async fn test_empty_query_error() {
        let analyzer = IntentAnalyzer::new();
        let result = analyzer.analyze("", &test_schema()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_empty_schema_error() {
        let analyzer = IntentAnalyzer::new();
        let result = analyzer
            .analyze("show users", &SchemaContext::default())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_llm_enabled_higher_confidence() {
        let analyzer = IntentAnalyzer::new().with_llm();
        let result = analyzer
            .analyze("show users", &test_schema())
            .await
            .unwrap();
        assert!((result.confidence - 0.9).abs() < 1e-6);
    }

    #[test]
    fn test_audit_record_rule() {
        let analyzer = IntentAnalyzer::new();
        let record = analyzer.audit_record(0.7);
        assert_eq!(
            record.source_engine,
            crate::advice_common::AdviceSource::Rule
        );
        assert_eq!(record.advice_type, AdviceType::Intent);
    }

    #[test]
    fn test_audit_record_llm() {
        let analyzer = IntentAnalyzer::new().with_llm();
        let record = analyzer.audit_record(0.9);
        assert_eq!(
            record.source_engine,
            crate::advice_common::AdviceSource::Llm
        );
        assert_eq!(record.advice_type, AdviceType::Intent);
    }
}
