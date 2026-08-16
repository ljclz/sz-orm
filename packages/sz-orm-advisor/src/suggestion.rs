//! 统一优化建议结构 + 六种建议类型（`query-advisor` feature）
//!
//! 提供 `OptimizationSuggestion` 到既有 AI 建议结构的转换：
//! - [`OptimizationSuggestion::to_index_suggestion`] → `sz_orm_ai::IndexSuggestion`
//! - [`OptimizationSuggestion::to_rewrite_suggestion`] → `sz_orm_ai::RewriteSuggestion`
//! - [`OptimizationSuggestion::to_tuning_suggestion`] → 本地 [`TuningSuggestion`]（与 `sz_orm_ai::TuningSuggestion` 字段兼容）

use serde::{Deserialize, Serialize};

use sz_orm_ai::{
    BenefitEstimate, EquivalenceProof, IndexSuggestion, IndexType, QueryPattern, RewriteSuggestion,
    TransformType,
};

/// 六种优化建议类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SuggestionType {
    /// 添加索引（全表扫描 + 大结果集）
    AddIndex,
    /// 删除冗余索引
    DropIndex,
    /// 改用游标分页（大结果集）
    UsePagination,
    /// 启用缓存（热点查询）
    EnableCache,
    /// 改写查询（SQL 结构优化）
    RewriteQuery,
    /// 调整连接池大小（池获取耗时高）
    AdjustPoolSize,
}

impl SuggestionType {
    /// 人类可读名称
    pub fn as_str(&self) -> &'static str {
        match self {
            SuggestionType::AddIndex => "add-index",
            SuggestionType::DropIndex => "drop-index",
            SuggestionType::UsePagination => "use-pagination",
            SuggestionType::EnableCache => "enable-cache",
            SuggestionType::RewriteQuery => "rewrite-query",
            SuggestionType::AdjustPoolSize => "adjust-pool-size",
        }
    }

    pub fn all() -> &'static [SuggestionType] {
        &[
            SuggestionType::AddIndex,
            SuggestionType::DropIndex,
            SuggestionType::UsePagination,
            SuggestionType::EnableCache,
            SuggestionType::RewriteQuery,
            SuggestionType::AdjustPoolSize,
        ]
    }

    pub fn is_ddl(&self) -> bool {
        matches!(self, SuggestionType::AddIndex | SuggestionType::DropIndex)
    }
}

/// 统一优化建议结构
///
/// 由 [`crate::advisor::OptimizationAdvisor::suggest`] 规则引擎生成，
/// 可转换为既有 AI 建议结构（`IndexSuggestion`/`RewriteSuggestion`/`TuningSuggestion`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizationSuggestion {
    /// 建议类型
    pub suggestion_type: SuggestionType,
    /// 目标查询（SQL 摘要或 query_key）
    pub target_query: String,
    /// 人类可读描述
    pub description: String,
    /// 可执行动作（DDL / SQL 改写 / 配置调整）
    pub action: String,
    /// 置信度（0.0~1.0，< 0.5 需人工确认）
    pub confidence: f64,
    /// 预估改善（如 "减少 90% 扫描行数"）
    pub estimated_improvement: Option<String>,
    /// 冲突标记（与高置信度建议冲突时标记 "conflict, skipped"）
    pub conflict_note: Option<String>,
}

impl OptimizationSuggestion {
    pub fn new(
        suggestion_type: SuggestionType,
        target_query: impl Into<String>,
        description: impl Into<String>,
        action: impl Into<String>,
        confidence: f64,
    ) -> Self {
        Self {
            suggestion_type,
            target_query: target_query.into(),
            description: description.into(),
            action: action.into(),
            confidence,
            estimated_improvement: None,
            conflict_note: None,
        }
    }

    pub fn is_high_confidence(&self) -> bool {
        self.confidence >= 0.8
    }

    pub fn has_improvement_estimate(&self) -> bool {
        self.estimated_improvement.is_some()
    }

    pub fn summary(&self) -> String {
        format!(
            "[{}] {} (confidence={:.2})",
            self.suggestion_type.as_str(),
            self.description,
            self.confidence
        )
    }

    /// 是否需要人工确认（置信度 < 0.5）
    pub fn needs_manual_confirmation(&self) -> bool {
        self.confidence < 0.5
    }

    /// 是否被冲突跳过
    pub fn is_skipped(&self) -> bool {
        self.conflict_note
            .as_ref()
            .is_some_and(|n| n.contains("skipped"))
    }

    /// 转换为既有 AI 索引建议结构（`IndexSuggestion`）
    ///
    /// 仅当建议类型为 `AddIndex` 时返回 `Some`，其他类型返回 `None`。
    /// `action` 字段应包含 DDL 文本（如 `CREATE INDEX idx ON tbl(col)`），
    /// 从中解析索引列；若解析失败则使用空向量。
    pub fn to_index_suggestion(&self) -> Option<IndexSuggestion> {
        if self.suggestion_type != SuggestionType::AddIndex {
            return None;
        }
        let index_columns = parse_index_columns(&self.action);
        Some(IndexSuggestion {
            index_columns,
            index_type: IndexType::BTree,
            ddl_text: self.action.clone(),
            expected_benefit: BenefitEstimate {
                speedup_ratio: 1.0 + self.confidence * 10.0,
                confidence: self.confidence as f32,
                uncertain: self.confidence < 0.5,
            },
            evidence: vec![QueryPattern {
                sql_template: self.target_query.clone(),
                frequency: 1,
                columns_accessed: parse_index_columns(&self.action),
            }],
        })
    }

    /// 转换为既有 AI 重写建议结构（`RewriteSuggestion`）
    ///
    /// 仅当建议类型为 `RewriteQuery` 时返回 `Some`，其他类型返回 `None`。
    /// `target_query` 为原始 SQL，`action` 为重写后 SQL。
    pub fn to_rewrite_suggestion(&self) -> Option<RewriteSuggestion> {
        if self.suggestion_type != SuggestionType::RewriteQuery {
            return None;
        }
        Some(RewriteSuggestion {
            original_sql: self.target_query.clone(),
            rewritten_sql: self.action.clone(),
            transform_type: guess_transform_type(&self.description),
            equivalence_proof: EquivalenceProof {
                proof_text: "Rule-based rewrite; equivalence not formally verified.".into(),
                verified: false,
                unverified: true,
            },
            expected_benefit: BenefitEstimate {
                speedup_ratio: 1.0 + self.confidence * 5.0,
                confidence: self.confidence as f32,
                uncertain: self.confidence < 0.5,
            },
        })
    }

    /// 转换为调优建议结构（`TuningSuggestion`）
    ///
    /// 所有建议类型均可转换：`AddIndex`/`DropIndex` → `Index`，
    /// `RewriteQuery` → `Rewrite`，其余 → `Schema`。
    /// 风险等级由置信度决定：≥ 0.8 → Low，≥ 0.5 → Medium，< 0.5 → High。
    pub fn to_tuning_suggestion(&self) -> Option<TuningSuggestion> {
        let ai_type = match self.suggestion_type {
            SuggestionType::AddIndex | SuggestionType::DropIndex => TuningSuggestionType::Index,
            SuggestionType::RewriteQuery => TuningSuggestionType::Rewrite,
            SuggestionType::UsePagination
            | SuggestionType::EnableCache
            | SuggestionType::AdjustPoolSize => TuningSuggestionType::Schema,
        };
        let risk = if self.confidence >= 0.8 {
            RiskLevel::Low
        } else if self.confidence >= 0.5 {
            RiskLevel::Medium
        } else {
            RiskLevel::High
        };
        Some(TuningSuggestion {
            suggestion_type: ai_type,
            sql_before: self.target_query.clone(),
            sql_after: self.action.clone(),
            expected_gain: Some((self.confidence * 100.0) as f32),
            risk,
            reason: self.description.clone(),
        })
    }
}

/// 从 DDL 文本中解析索引列名
///
/// 支持 `CREATE INDEX idx ON table(col1, col2)` 格式，
/// 提取括号内逗号分隔的列名列表。解析失败返回空向量。
fn parse_index_columns(ddl: &str) -> Vec<String> {
    if let Some(start) = ddl.rfind('(') {
        if let Some(end) = ddl.find(')').filter(|&e| e > start) {
            return ddl[start + 1..end]
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    Vec::new()
}

/// 从描述文本推测变换类型
fn guess_transform_type(description: &str) -> TransformType {
    let lower = description.to_lowercase();
    if lower.contains("pushdown") {
        TransformType::PredicatePushdown
    } else if lower.contains("subquery") || lower.contains("flatten") {
        TransformType::SubqueryFlattening
    } else if lower.contains("join") || lower.contains("reorder") {
        TransformType::JoinReorder
    } else {
        TransformType::RedundantElimination
    }
}

/// 调优建议类型（与 `sz_orm_ai::auto_tuning::SuggestionType` 字段兼容）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TuningSuggestionType {
    /// 索引建议
    Index,
    /// SQL 重写建议
    Rewrite,
    /// Schema 变更建议
    Schema,
}

impl TuningSuggestionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TuningSuggestionType::Index => "index",
            TuningSuggestionType::Rewrite => "rewrite",
            TuningSuggestionType::Schema => "schema",
        }
    }
}

/// 风险等级（与 `sz_orm_ai::auto_tuning::RiskLevel` 字段兼容）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    /// 低风险（可自动执行）
    Low,
    /// 中风险（需确认）
    Medium,
    /// 高风险（不自动执行）
    High,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::Low => "low",
            RiskLevel::Medium => "medium",
            RiskLevel::High => "high",
        }
    }
}

/// 调优建议（与 `sz_orm_ai::auto_tuning::TuningSuggestion` 字段兼容）
///
/// 由 [`OptimizationSuggestion::to_tuning_suggestion`] 生成，
/// 供 AI 自动调优闭环（`AutoTuningPipeline`）消费。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TuningSuggestion {
    /// 建议类型
    pub suggestion_type: TuningSuggestionType,
    /// 调优前 SQL
    pub sql_before: String,
    /// 调优后 SQL（或 DDL，如 CREATE INDEX）
    pub sql_after: String,
    /// 预期收益（百分比）
    pub expected_gain: Option<f32>,
    /// 风险等级
    pub risk: RiskLevel,
    /// 建议原因
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestion_type_serialize_roundtrip() {
        let types = [
            SuggestionType::AddIndex,
            SuggestionType::DropIndex,
            SuggestionType::UsePagination,
            SuggestionType::EnableCache,
            SuggestionType::RewriteQuery,
            SuggestionType::AdjustPoolSize,
        ];
        for t in &types {
            let json = serde_json::to_string(t).unwrap();
            let back: SuggestionType = serde_json::from_str(&json).unwrap();
            assert_eq!(*t, back);
        }
    }

    #[test]
    fn low_confidence_needs_confirmation() {
        let s = OptimizationSuggestion {
            suggestion_type: SuggestionType::AddIndex,
            target_query: "q1".into(),
            description: "test".into(),
            action: "CREATE INDEX".into(),
            confidence: 0.3,
            estimated_improvement: None,
            conflict_note: None,
        };
        assert!(s.needs_manual_confirmation());
    }

    #[test]
    fn high_confidence_no_confirmation() {
        let s = OptimizationSuggestion {
            suggestion_type: SuggestionType::AddIndex,
            target_query: "q1".into(),
            description: "test".into(),
            action: "CREATE INDEX".into(),
            confidence: 0.9,
            estimated_improvement: None,
            conflict_note: None,
        };
        assert!(!s.needs_manual_confirmation());
    }

    #[test]
    fn boundary_confidence_zero_and_one() {
        let zero = OptimizationSuggestion {
            suggestion_type: SuggestionType::AddIndex,
            target_query: "q".into(),
            description: "d".into(),
            action: "a".into(),
            confidence: 0.0,
            estimated_improvement: None,
            conflict_note: None,
        };
        assert!(zero.needs_manual_confirmation());

        let one = OptimizationSuggestion {
            suggestion_type: SuggestionType::AddIndex,
            target_query: "q".into(),
            description: "d".into(),
            action: "a".into(),
            confidence: 1.0,
            estimated_improvement: None,
            conflict_note: None,
        };
        assert!(!one.needs_manual_confirmation());
    }

    #[test]
    fn skipped_detection() {
        let s = OptimizationSuggestion {
            suggestion_type: SuggestionType::DropIndex,
            target_query: "q".into(),
            description: "d".into(),
            action: "a".into(),
            confidence: 0.3,
            estimated_improvement: None,
            conflict_note: Some("conflict, skipped".into()),
        };
        assert!(s.is_skipped());
    }

    #[test]
    fn to_index_suggestion_for_add_index() {
        let s = OptimizationSuggestion {
            suggestion_type: SuggestionType::AddIndex,
            target_query: "SELECT * FROM users WHERE email = ?".into(),
            description: "full table scan on users".into(),
            action: "CREATE INDEX idx_users_email ON users(email)".into(),
            confidence: 0.9,
            estimated_improvement: None,
            conflict_note: None,
        };
        let idx = s.to_index_suggestion().expect("AddIndex should convert");
        assert_eq!(idx.index_columns, vec!["email"]);
        assert_eq!(idx.index_type, IndexType::BTree);
        assert!(idx.ddl_text.contains("CREATE INDEX"));
        assert!(idx.expected_benefit.speedup_ratio > 1.0);
        assert!(!idx.expected_benefit.uncertain);
        assert_eq!(idx.evidence.len(), 1);
    }

    #[test]
    fn to_index_suggestion_returns_none_for_non_add_index() {
        let s = OptimizationSuggestion {
            suggestion_type: SuggestionType::UsePagination,
            target_query: "q".into(),
            description: "d".into(),
            action: "a".into(),
            confidence: 0.8,
            estimated_improvement: None,
            conflict_note: None,
        };
        assert!(s.to_index_suggestion().is_none());
    }

    #[test]
    fn to_index_suggestion_multi_column() {
        let s = OptimizationSuggestion {
            suggestion_type: SuggestionType::AddIndex,
            target_query: "q".into(),
            description: "d".into(),
            action: "CREATE INDEX idx ON orders(user_id, created_at)".into(),
            confidence: 0.85,
            estimated_improvement: None,
            conflict_note: None,
        };
        let idx = s.to_index_suggestion().unwrap();
        assert_eq!(idx.index_columns, vec!["user_id", "created_at"]);
    }

    #[test]
    fn to_rewrite_suggestion_for_rewrite_query() {
        let s = OptimizationSuggestion {
            suggestion_type: SuggestionType::RewriteQuery,
            target_query: "SELECT * FROM (SELECT * FROM t WHERE x=1) sub".into(),
            description: "pushdown predicate to inner query".into(),
            action: "SELECT * FROM t WHERE x=1".into(),
            confidence: 0.7,
            estimated_improvement: None,
            conflict_note: None,
        };
        let rw = s
            .to_rewrite_suggestion()
            .expect("RewriteQuery should convert");
        assert_eq!(
            rw.original_sql,
            "SELECT * FROM (SELECT * FROM t WHERE x=1) sub"
        );
        assert_eq!(rw.rewritten_sql, "SELECT * FROM t WHERE x=1");
        assert_eq!(rw.transform_type, TransformType::PredicatePushdown);
        assert!(rw.equivalence_proof.unverified);
        assert!(!rw.equivalence_proof.verified);
    }

    #[test]
    fn to_rewrite_suggestion_returns_none_for_non_rewrite() {
        let s = OptimizationSuggestion {
            suggestion_type: SuggestionType::AddIndex,
            target_query: "q".into(),
            description: "d".into(),
            action: "a".into(),
            confidence: 0.9,
            estimated_improvement: None,
            conflict_note: None,
        };
        assert!(s.to_rewrite_suggestion().is_none());
    }

    #[test]
    fn to_tuning_suggestion_for_all_types() {
        let add_index = OptimizationSuggestion {
            suggestion_type: SuggestionType::AddIndex,
            target_query: "q1".into(),
            description: "add index".into(),
            action: "CREATE INDEX idx ON t(c)".into(),
            confidence: 0.9,
            estimated_improvement: None,
            conflict_note: None,
        };
        let tuning = add_index.to_tuning_suggestion().unwrap();
        assert_eq!(tuning.suggestion_type, TuningSuggestionType::Index);
        assert_eq!(tuning.risk, RiskLevel::Low);
        assert!(tuning.expected_gain.is_some());

        let rewrite = OptimizationSuggestion {
            suggestion_type: SuggestionType::RewriteQuery,
            target_query: "q2".into(),
            description: "rewrite".into(),
            action: "rewritten".into(),
            confidence: 0.6,
            estimated_improvement: None,
            conflict_note: None,
        };
        let tuning = rewrite.to_tuning_suggestion().unwrap();
        assert_eq!(tuning.suggestion_type, TuningSuggestionType::Rewrite);
        assert_eq!(tuning.risk, RiskLevel::Medium);

        let cache = OptimizationSuggestion {
            suggestion_type: SuggestionType::EnableCache,
            target_query: "q3".into(),
            description: "cache".into(),
            action: "enable cache".into(),
            confidence: 0.3,
            estimated_improvement: None,
            conflict_note: None,
        };
        let tuning = cache.to_tuning_suggestion().unwrap();
        assert_eq!(tuning.suggestion_type, TuningSuggestionType::Schema);
        assert_eq!(tuning.risk, RiskLevel::High);
    }

    #[test]
    fn parse_index_columns_extracts_correctly() {
        assert_eq!(
            parse_index_columns("CREATE INDEX idx ON tbl(a, b, c)"),
            vec!["a", "b", "c"]
        );
        assert_eq!(
            parse_index_columns("CREATE INDEX idx ON tbl(email)"),
            vec!["email"]
        );
        assert!(parse_index_columns("no parentheses here").is_empty());
    }
}
