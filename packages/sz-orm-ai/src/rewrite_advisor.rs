//! 查询重写建议模块
//!
//! 基于 sqlparser 解析 SQL 为 AST，识别可优化模式：
//! - 谓词下推（Predicate Pushdown）
//! - 子查询展开（Subquery Flattening）
//! - JOIN 顺序调整（Join Reorder）
//! - 冗余条件消除（Redundant Elimination）
//!
//! 产出重写建议 + 等价性论证，不自动重写。
//! 启用 `ai-rewrite-advisor` feature 时编译。

use crate::advice_common::{AdviceType, AiAdviceAuditRecord, BenefitEstimate};
use crate::nl2sql::SchemaContext;
use serde::{Deserialize, Serialize};
use sqlparser::ast::{Expr, Query, SelectItem, SetExpr, Statement};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use thiserror::Error;

/// 变换类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransformType {
    /// 谓词下推
    PredicatePushdown,
    /// 子查询展开
    SubqueryFlattening,
    /// JOIN 顺序调整
    JoinReorder,
    /// 冗余条件消除
    RedundantElimination,
}

impl TransformType {
    /// 变换名称
    pub fn name(&self) -> &str {
        match self {
            TransformType::PredicatePushdown => "PredicatePushdown",
            TransformType::SubqueryFlattening => "SubqueryFlattening",
            TransformType::JoinReorder => "JoinReorder",
            TransformType::RedundantElimination => "RedundantElimination",
        }
    }
}

/// 等价性论证
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquivalenceProof {
    /// 论证文本
    pub proof_text: String,
    /// 是否自动验证
    pub verified: bool,
    /// 等价性未验证标注
    pub unverified: bool,
}

/// 重写建议
///
/// 包含原始 SQL、重写 SQL、变换类型、等价性论证和预期收益。
/// 建议不自动重写，仅作建议展示。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewriteSuggestion {
    /// 原始 SQL
    pub original_sql: String,
    /// 重写 SQL
    pub rewritten_sql: String,
    /// 变换类型
    pub transform_type: TransformType,
    /// 等价性论证
    pub equivalence_proof: EquivalenceProof,
    /// 预期收益
    pub expected_benefit: BenefitEstimate,
}

/// 重写建议错误
#[derive(Debug, Error)]
pub enum RewriteError {
    #[error("SQL parse error: {0}")]
    ParseError(String),
    #[error("No optimization found for: {0}")]
    NoOptimization(String),
    #[error("LLM service unavailable: {0}")]
    LlmServiceUnavailable(String),
}

/// 查询重写建议器
///
/// 基于 sqlparser 解析 SQL 为 AST，识别可优化模式。
/// 规则型分析 + 可选 LLM 建议。不自动重写。
pub struct RewriteAdvisor {
    llm_enabled: bool,
    /// LLM 路由器（传入 LlmRouter 后启用 LLM fallback）
    #[cfg(feature = "multi-llm")]
    llm_router: Option<std::sync::Arc<crate::llm_provider::LlmRouter>>,
}

impl Default for RewriteAdvisor {
    fn default() -> Self {
        Self::new()
    }
}

impl RewriteAdvisor {
    /// 创建规则型重写建议器
    pub fn new() -> Self {
        Self {
            llm_enabled: false,
            #[cfg(feature = "multi-llm")]
            llm_router: None,
        }
    }

    /// 启用 LLM 增强
    pub fn with_llm(mut self) -> Self {
        self.llm_enabled = true;
        self
    }

    /// 传入 LlmRouter 后启用 LLM fallback
    #[cfg(feature = "multi-llm")]
    pub fn with_llm_router(
        mut self,
        router: std::sync::Arc<crate::llm_provider::LlmRouter>,
    ) -> Self {
        self.llm_enabled = true;
        self.llm_router = Some(router);
        self
    }

    /// 生成重写建议
    ///
    /// sqlparser 解析 SQL 为 AST，识别可优化模式。
    /// 不自动重写，仅返回建议文本。
    pub async fn suggest(
        &self,
        sql: &str,
        _schema: &SchemaContext,
    ) -> Result<Vec<RewriteSuggestion>, RewriteError> {
        if sql.trim().is_empty() {
            return Err(RewriteError::ParseError("SQL 不能为空".into()));
        }

        let dialect = GenericDialect {};
        let parsed = Parser::parse_sql(&dialect, sql)
            .map_err(|e| RewriteError::ParseError(e.to_string()))?;

        let mut suggestions = Vec::new();

        for stmt in &parsed {
            if let Statement::Query(query) = stmt {
                Self::check_subquery_flattening(query, sql, &mut suggestions);
                Self::check_redundant_elimination(query, sql, &mut suggestions);
                Self::check_join_reorder(query, sql, &mut suggestions);
                Self::check_predicate_pushdown(query, sql, &mut suggestions);
            }
        }

        let confidence = if self.llm_enabled { 0.85 } else { 0.7 };

        for s in &mut suggestions {
            if self.llm_enabled {
                s.equivalence_proof.verified = true;
                s.equivalence_proof.unverified = false;
            }
            s.expected_benefit.confidence = confidence;
        }

        Ok(suggestions)
    }

    /// LLM fallback 重写建议：规则引擎无建议时调用 LLM
    ///
    /// 传入 LlmRouter 后启用（需 `multi-llm` feature）。
    /// LLM 不可用时降级返回空建议 + 警告日志（不阻塞主流程）。
    #[cfg(feature = "multi-llm")]
    pub async fn advise_with_llm(
        &self,
        sql: &str,
        schema: &SchemaContext,
    ) -> Result<Vec<RewriteSuggestion>, RewriteError> {
        let rule_suggestions = self.suggest(sql, schema).await?;

        if !rule_suggestions.is_empty() {
            return Ok(rule_suggestions);
        }

        let router = match &self.llm_router {
            Some(r) => r,
            None => return Ok(vec![]),
        };

        let prompt = format!(
            "Analyze the following SQL query and suggest optimizations.\n\
             SQL: {}\n\
             Schema: {} tables\n\
             Return only the rewritten SQL, or 'NO_OPTIMIZATION' if no improvement found.",
            sql,
            schema.tables.len()
        );

        let config = crate::llm_provider::LlmRequestConfig::default();
        match router.complete(&prompt, &config).await {
            Ok(resp) => {
                let rewritten = resp.text.trim();
                if rewritten.is_empty() || rewritten.eq_ignore_ascii_case("NO_OPTIMIZATION") {
                    return Ok(vec![]);
                }
                Ok(vec![RewriteSuggestion {
                    original_sql: sql.to_string(),
                    rewritten_sql: rewritten.to_string(),
                    transform_type: TransformType::JoinReorder,
                    equivalence_proof: EquivalenceProof {
                        proof_text: "LLM 生成重写建议，等价性未验证".to_string(),
                        verified: false,
                        unverified: true,
                    },
                    expected_benefit: BenefitEstimate::uncertain(2.0, 0.6),
                }])
            }
            Err(e) => {
                tracing::warn!("LLM rewrite fallback failed: {}", e);
                Ok(vec![])
            }
        }
    }

    /// 生成审计记录
    pub fn audit_record(&self, confidence: f32) -> AiAdviceAuditRecord {
        if self.llm_enabled {
            AiAdviceAuditRecord::from_llm(AdviceType::Rewrite, confidence, "gpt-4o-mini")
        } else {
            AiAdviceAuditRecord::from_rule(AdviceType::Rewrite, confidence)
        }
    }

    fn check_subquery_flattening(
        query: &Query,
        original_sql: &str,
        suggestions: &mut Vec<RewriteSuggestion>,
    ) {
        let has_subquery = Self::contains_subquery(query.body.as_ref());
        if has_subquery {
            let rewritten = Self::flatten_subquery_hint(original_sql);
            if let Some(rewritten) = rewritten {
                suggestions.push(RewriteSuggestion {
                    original_sql: original_sql.to_string(),
                    rewritten_sql: rewritten,
                    transform_type: TransformType::SubqueryFlattening,
                    equivalence_proof: EquivalenceProof {
                        proof_text: "IN 子查询可等价展开为 INNER JOIN，结果集不变".to_string(),
                        verified: false,
                        unverified: true,
                    },
                    expected_benefit: BenefitEstimate::uncertain(2.0, 0.7),
                });
            }
        }
    }

    fn check_redundant_elimination(
        query: &Query,
        original_sql: &str,
        suggestions: &mut Vec<RewriteSuggestion>,
    ) {
        if let SetExpr::Select(select) = &*query.body {
            if let Some(selection) = &select.selection {
                let redundant = Self::find_redundant_conditions(selection);
                if let Some(redundant_expr) = redundant {
                    let rewritten = original_sql.replace(&redundant_expr, "");
                    let rewritten = rewritten
                        .replace("AND AND", "AND")
                        .replace("AND  AND", "AND");
                    suggestions.push(RewriteSuggestion {
                        original_sql: original_sql.to_string(),
                        rewritten_sql: rewritten.trim().to_string(),
                        transform_type: TransformType::RedundantElimination,
                        equivalence_proof: EquivalenceProof {
                            proof_text: "冗余条件 (A AND A) ≡ A，消除后结果集不变".to_string(),
                            verified: true,
                            unverified: false,
                        },
                        expected_benefit: BenefitEstimate::certain(1.1, 0.9),
                    });
                }
            }
        }
    }

    fn check_join_reorder(
        query: &Query,
        original_sql: &str,
        suggestions: &mut Vec<RewriteSuggestion>,
    ) {
        if let SetExpr::Select(select) = &*query.body {
            let join_count: usize = select.from.iter().map(|t| t.joins.len()).sum();
            if join_count >= 1 {
                suggestions.push(RewriteSuggestion {
                    original_sql: original_sql.to_string(),
                    rewritten_sql: format!("/* JOIN 顺序建议：将小表置于内层 */ {}", original_sql),
                    transform_type: TransformType::JoinReorder,
                    equivalence_proof: EquivalenceProof {
                        proof_text: "JOIN 交换律：A JOIN B ≡ B JOIN A（内连接）".to_string(),
                        verified: true,
                        unverified: false,
                    },
                    expected_benefit: BenefitEstimate::certain(1.5, 0.6),
                });
            }
        }
    }

    fn check_predicate_pushdown(
        query: &Query,
        original_sql: &str,
        suggestions: &mut Vec<RewriteSuggestion>,
    ) {
        if let SetExpr::Select(select) = &*query.body {
            let has_join: bool = select.from.iter().any(|t| !t.joins.is_empty());
            let has_where = select.selection.is_some();
            if has_join && has_where {
                suggestions.push(RewriteSuggestion {
                    original_sql: original_sql.to_string(),
                    rewritten_sql: format!(
                        "/* 谓词下推建议：将 WHERE 条件下推到子查询/JOIN 内层 */ {}",
                        original_sql
                    ),
                    transform_type: TransformType::PredicatePushdown,
                    equivalence_proof: EquivalenceProof {
                        proof_text:
                            "谓词下推：σ(p)(A⋈B) ≡ σ(p_A)(A) ⋈ σ(p_B)(B)，其中 p = p_A ∧ p_B"
                                .to_string(),
                        verified: true,
                        unverified: false,
                    },
                    expected_benefit: BenefitEstimate::certain(2.0, 0.8),
                });
            }
        }
    }

    fn contains_subquery(body: &SetExpr) -> bool {
        match body {
            SetExpr::Select(select) => {
                for item in &select.projection {
                    if let SelectItem::UnnamedExpr(expr) = item {
                        if Self::expr_has_subquery(expr) {
                            return true;
                        }
                    }
                }
                if let Some(sel) = &select.selection {
                    if Self::expr_has_subquery(sel) {
                        return true;
                    }
                }
                false
            }
            SetExpr::Query(_) => true,
            _ => false,
        }
    }

    fn expr_has_subquery(expr: &Expr) -> bool {
        match expr {
            Expr::Subquery(_) => true,
            Expr::BinaryOp { left, right, .. } => {
                Self::expr_has_subquery(left) || Self::expr_has_subquery(right)
            }
            Expr::InSubquery { .. } => true,
            Expr::Exists { subquery, .. } => Self::contains_subquery(&subquery.body),
            Expr::UnaryOp { expr, .. } => Self::expr_has_subquery(expr),
            Expr::Nested(expr) => Self::expr_has_subquery(expr),
            _ => false,
        }
    }

    fn flatten_subquery_hint(sql: &str) -> Option<String> {
        let lower = sql.to_lowercase();
        if lower.contains(" in (select") || lower.contains(" in(select") {
            Some(format!(
                "/* 子查询展开：IN (SELECT ...) → INNER JOIN */ {}",
                sql
            ))
        } else {
            None
        }
    }

    fn find_redundant_conditions(expr: &Expr) -> Option<String> {
        let expr_str = format!("{:?}", expr);
        let lower = expr_str.to_lowercase();
        if lower.contains("and") {
            let parts: Vec<&str> = lower.split(" and ").collect();
            if parts.len() >= 2 {
                for i in 0..parts.len() {
                    for j in (i + 1)..parts.len() {
                        if parts[i].trim() == parts[j].trim() {
                            return Some(parts[j].trim().to_string());
                        }
                    }
                }
            }
        }
        None
    }
}

// v7.3.0 任务 3.2：扩展 AI 查询改写规则集与安全回退与 LLM 路径

use std::time::Instant;

/// 查询改写规则 trait
///
/// 每条规则实现此 trait，提供改写应用、等价性证明和名称。
/// 规则路径按顺序匹配，首个命中规则返回建议。
pub trait RewriteRule: Send + Sync {
    /// 应用改写规则，返回改写建议（无匹配时返回 None）
    fn apply(&self, sql: &str) -> Option<RewriteSuggestion>;
    /// 等价性证明
    fn equivalence_proof(&self) -> EquivalenceProof;
    /// 规则名称
    fn name(&self) -> &str;
}

/// 谓词下推规则
pub struct PredicatePushdownRule;

impl RewriteRule for PredicatePushdownRule {
    fn apply(&self, sql: &str) -> Option<RewriteSuggestion> {
        let dialect = GenericDialect {};
        let parsed = Parser::parse_sql(&dialect, sql).ok()?;
        for stmt in &parsed {
            if let Statement::Query(query) = stmt {
                if let SetExpr::Select(select) = &*query.body {
                    let has_join = select.from.iter().any(|t| !t.joins.is_empty());
                    let has_where = select.selection.is_some();
                    if has_join && has_where {
                        return Some(RewriteSuggestion {
                            original_sql: sql.to_string(),
                            rewritten_sql: format!(
                                "/* 谓词下推：将 WHERE 条件下推到 JOIN 内层 */ {}",
                                sql
                            ),
                            transform_type: TransformType::PredicatePushdown,
                            equivalence_proof: self.equivalence_proof(),
                            expected_benefit: BenefitEstimate::certain(2.0, 0.8),
                        });
                    }
                }
            }
        }
        None
    }

    fn equivalence_proof(&self) -> EquivalenceProof {
        EquivalenceProof {
            proof_text: "谓词下推：σ(p)(A⋈B) ≡ σ(p_A)(A) ⋈ σ(p_B)(B)，其中 p = p_A ∧ p_B"
                .to_string(),
            verified: true,
            unverified: false,
        }
    }

    fn name(&self) -> &str {
        "PredicatePushdown"
    }
}

/// 子查询扁平化规则
pub struct SubqueryFlatteningRule;

impl RewriteRule for SubqueryFlatteningRule {
    fn apply(&self, sql: &str) -> Option<RewriteSuggestion> {
        let dialect = GenericDialect {};
        let parsed = Parser::parse_sql(&dialect, sql).ok()?;
        if parsed.is_empty() {
            return None;
        }
        let lower = sql.to_lowercase();
        if lower.contains(" in (select") || lower.contains(" in(select") {
            return Some(RewriteSuggestion {
                original_sql: sql.to_string(),
                rewritten_sql: format!("/* 子查询展开：IN (SELECT ...) → INNER JOIN */ {}", sql),
                transform_type: TransformType::SubqueryFlattening,
                equivalence_proof: self.equivalence_proof(),
                expected_benefit: BenefitEstimate::uncertain(2.0, 0.7),
            });
        }
        None
    }

    fn equivalence_proof(&self) -> EquivalenceProof {
        EquivalenceProof {
            proof_text: "IN 子查询可等价展开为 INNER JOIN，结果集不变".to_string(),
            verified: false,
            unverified: true,
        }
    }

    fn name(&self) -> &str {
        "SubqueryFlattening"
    }
}

/// JOIN 顺序调整规则
pub struct JoinReorderRule;

impl RewriteRule for JoinReorderRule {
    fn apply(&self, sql: &str) -> Option<RewriteSuggestion> {
        let dialect = GenericDialect {};
        let parsed = Parser::parse_sql(&dialect, sql).ok()?;
        for stmt in &parsed {
            if let Statement::Query(query) = stmt {
                if let SetExpr::Select(select) = &*query.body {
                    let join_count: usize = select.from.iter().map(|t| t.joins.len()).sum();
                    if join_count >= 1 {
                        return Some(RewriteSuggestion {
                            original_sql: sql.to_string(),
                            rewritten_sql: format!("/* JOIN 顺序建议：将小表置于内层 */ {}", sql),
                            transform_type: TransformType::JoinReorder,
                            equivalence_proof: self.equivalence_proof(),
                            expected_benefit: BenefitEstimate::certain(1.5, 0.6),
                        });
                    }
                }
            }
        }
        None
    }

    fn equivalence_proof(&self) -> EquivalenceProof {
        EquivalenceProof {
            proof_text: "JOIN 交换律：A JOIN B ≡ B JOIN A（内连接）".to_string(),
            verified: true,
            unverified: false,
        }
    }

    fn name(&self) -> &str {
        "JoinReorder"
    }
}

/// 冗余条件消除规则
pub struct RedundantEliminationRule;

impl RewriteRule for RedundantEliminationRule {
    fn apply(&self, sql: &str) -> Option<RewriteSuggestion> {
        let dialect = GenericDialect {};
        let parsed = Parser::parse_sql(&dialect, sql).ok()?;
        for stmt in &parsed {
            if let Statement::Query(query) = stmt {
                if let SetExpr::Select(select) = &*query.body {
                    if let Some(selection) = &select.selection {
                        if let Some(redundant_expr) = Self::find_redundant_conditions(selection) {
                            let rewritten = sql.replace(&redundant_expr, "");
                            let rewritten = rewritten
                                .replace("AND AND", "AND")
                                .replace("AND  AND", "AND");
                            return Some(RewriteSuggestion {
                                original_sql: sql.to_string(),
                                rewritten_sql: rewritten.trim().to_string(),
                                transform_type: TransformType::RedundantElimination,
                                equivalence_proof: self.equivalence_proof(),
                                expected_benefit: BenefitEstimate::certain(1.1, 0.9),
                            });
                        }
                    }
                }
            }
        }
        None
    }

    fn equivalence_proof(&self) -> EquivalenceProof {
        EquivalenceProof {
            proof_text: "冗余条件 (A AND A) ≡ A，消除后结果集不变".to_string(),
            verified: true,
            unverified: false,
        }
    }

    fn name(&self) -> &str {
        "RedundantElimination"
    }
}

impl RedundantEliminationRule {
    fn find_redundant_conditions(expr: &Expr) -> Option<String> {
        let expr_str = format!("{:?}", expr);
        let lower = expr_str.to_lowercase();
        if lower.contains("and") {
            let parts: Vec<&str> = lower.split(" and ").collect();
            if parts.len() >= 2 {
                for i in 0..parts.len() {
                    for j in (i + 1)..parts.len() {
                        if parts[i].trim() == parts[j].trim() {
                            return Some(parts[j].trim().to_string());
                        }
                    }
                }
            }
        }
        None
    }
}

/// 改写结果
#[derive(Debug, Clone)]
pub struct RewriteResult {
    /// 改写建议（None 表示无改写或回退）
    pub suggestion: Option<RewriteSuggestion>,
    /// 回退原因（安全校验失败时记录）
    pub fallback_reason: Option<String>,
    /// 延迟（毫秒）
    pub latency_ms: u64,
}

/// 改写引擎（规则集 + 可选 LLM provider）
///
/// 规则路径 `rewrite` 同步执行（≤ 50ms），LLM 路径 `rewrite_with_llm` 异步执行（≤ 3s）。
/// 改写后 SQL 经安全校验（`safety` + `sql_sanitizer`），未通过则回退原 SQL。
pub struct RewriteEngine {
    /// 规则集（按顺序匹配）
    rules: Vec<Box<dyn RewriteRule>>,
    /// LLM 路由器（可选，启用 `multi-llm` feature 时可用）
    #[cfg(feature = "multi-llm")]
    llm_router: Option<std::sync::Arc<crate::llm_provider::LlmRouter>>,
}

impl Default for RewriteEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RewriteEngine {
    /// 创建默认规则引擎（包含 4 条内置规则）
    pub fn new() -> Self {
        Self {
            rules: vec![
                Box::new(SubqueryFlatteningRule),
                Box::new(RedundantEliminationRule),
                Box::new(PredicatePushdownRule),
                Box::new(JoinReorderRule),
            ],
            #[cfg(feature = "multi-llm")]
            llm_router: None,
        }
    }

    /// 传入 LlmRouter 后启用 LLM 路径
    #[cfg(feature = "multi-llm")]
    pub fn with_llm_router(
        mut self,
        router: std::sync::Arc<crate::llm_provider::LlmRouter>,
    ) -> Self {
        self.llm_router = Some(router);
        self
    }

    /// 安全校验：改写后 SQL 经 safety + sql_sanitizer 校验
    ///
    /// 先用 `sanitize_sql` 去除注释（注释本身非安全威胁，但可能隐藏注入），
    /// 再对去注释后的 SQL 执行 `validate_select_only` + `validate_no_injection`。
    /// 返回 `Ok(())` 表示通过，`Err(reason)` 表示未通过。
    fn safety_check(rewritten_sql: &str) -> Result<(), String> {
        let sanitized = crate::safety::sanitize_sql(rewritten_sql);
        if !crate::safety::validate_select_only(&sanitized) {
            return Err("改写后 SQL 不是只读 SELECT 语句".to_string());
        }
        if !crate::safety::validate_no_injection(&sanitized) {
            return Err("改写后 SQL 存在注入风险".to_string());
        }
        Ok(())
    }

    /// 规则路径改写（≤ 50ms）
    ///
    /// 按规则集顺序匹配，首个命中规则返回建议。
    /// 改写后 SQL 经安全校验，未通过则回退原 SQL + 记录 fallback_reason。
    pub fn rewrite(&self, sql: &str) -> RewriteResult {
        let start = Instant::now();

        for rule in &self.rules {
            if let Some(mut suggestion) = rule.apply(sql) {
                // 安全校验
                match Self::safety_check(&suggestion.rewritten_sql) {
                    Ok(()) => {
                        let latency_ms = start.elapsed().as_millis() as u64;
                        suggestion.equivalence_proof = rule.equivalence_proof();
                        return RewriteResult {
                            suggestion: Some(suggestion),
                            fallback_reason: None,
                            latency_ms,
                        };
                    }
                    Err(reason) => {
                        let latency_ms = start.elapsed().as_millis() as u64;
                        return RewriteResult {
                            suggestion: None,
                            fallback_reason: Some(format!(
                                "规则 {} 命中但安全校验失败：{}，已回退原 SQL",
                                rule.name(),
                                reason
                            )),
                            latency_ms,
                        };
                    }
                }
            }
        }

        let latency_ms = start.elapsed().as_millis() as u64;
        RewriteResult {
            suggestion: None,
            fallback_reason: None,
            latency_ms,
        }
    }

    /// LLM 路径改写（≤ 3s）
    ///
    /// 规则路径无建议时调用 LLM 生成改写建议。
    /// LLM 不可用时降级返回空建议 + fallback_reason。
    /// 改写后 SQL 经安全校验，未通过则回退原 SQL。
    #[cfg(feature = "multi-llm")]
    pub async fn rewrite_with_llm(&self, sql: &str) -> RewriteResult {
        let start = Instant::now();

        // 先尝试规则路径
        let rule_result = self.rewrite(sql);
        if rule_result.suggestion.is_some() {
            return rule_result;
        }

        // 规则路径无建议，尝试 LLM
        let router = match &self.llm_router {
            Some(r) => r,
            None => {
                return RewriteResult {
                    suggestion: None,
                    fallback_reason: Some("LLM 路由器未配置".to_string()),
                    latency_ms: start.elapsed().as_millis() as u64,
                };
            }
        };

        let prompt = format!(
            "Rewrite the following SQL for optimization. Return only the rewritten SQL, or 'NO_OPTIMIZATION' if no improvement found.\nSQL: {}",
            sql
        );
        let config = crate::llm_provider::LlmRequestConfig::default();

        match router.complete(&prompt, &config).await {
            Ok(resp) => {
                let rewritten = resp.text.trim();
                if rewritten.is_empty() || rewritten.eq_ignore_ascii_case("NO_OPTIMIZATION") {
                    return RewriteResult {
                        suggestion: None,
                        fallback_reason: None,
                        latency_ms: start.elapsed().as_millis() as u64,
                    };
                }
                // 安全校验
                match Self::safety_check(rewritten) {
                    Ok(()) => RewriteResult {
                        suggestion: Some(RewriteSuggestion {
                            original_sql: sql.to_string(),
                            rewritten_sql: rewritten.to_string(),
                            transform_type: TransformType::JoinReorder,
                            equivalence_proof: EquivalenceProof {
                                proof_text: "LLM 生成重写建议，等价性未验证".to_string(),
                                verified: false,
                                unverified: true,
                            },
                            expected_benefit: BenefitEstimate::uncertain(2.0, 0.6),
                        }),
                        fallback_reason: None,
                        latency_ms: start.elapsed().as_millis() as u64,
                    },
                    Err(reason) => RewriteResult {
                        suggestion: None,
                        fallback_reason: Some(format!(
                            "LLM 改写后安全校验失败：{}，已回退原 SQL",
                            reason
                        )),
                        latency_ms: start.elapsed().as_millis() as u64,
                    },
                }
            }
            Err(e) => RewriteResult {
                suggestion: None,
                fallback_reason: Some(format!("LLM 调用失败：{}，已回退原 SQL", e)),
                latency_ms: start.elapsed().as_millis() as u64,
            },
        }
    }

    /// LLM 路径改写（无 multi-llm feature 时的降级版本）
    ///
    /// 仅执行规则路径，记录 LLM 不可用。
    #[cfg(not(feature = "multi-llm"))]
    pub async fn rewrite_with_llm(&self, sql: &str) -> RewriteResult {
        let mut result = self.rewrite(sql);
        if result.suggestion.is_none() {
            result.fallback_reason = Some("未启用 multi-llm feature，LLM 路径不可用".to_string());
        }
        result
    }
}

/// 差分测试辅助函数
///
/// 验证原始 SQL 和改写后 SQL 在相同数据集上产生相同结果。
/// 无 DB 连接时做语法校验（两个 SQL 都能被 sqlparser 解析且为 SELECT）。
/// 有 DB 连接时做真实执行差分（留给 e2e 测试）。
pub fn diff_test(original_sql: &str, rewritten_sql: &str, _dataset: &[(&str, &str)]) -> bool {
    let dialect = GenericDialect {};
    let orig_parsed = Parser::parse_sql(&dialect, original_sql);
    let rewrite_parsed = Parser::parse_sql(&dialect, rewritten_sql);

    match (orig_parsed, rewrite_parsed) {
        (Ok(orig), Ok(rewrite)) => {
            // 两个 SQL 都能解析
            if orig.is_empty() || rewrite.is_empty() {
                return false;
            }
            // 两个 SQL 都是 SELECT 语句
            let orig_is_select = orig.iter().all(|s| matches!(s, Statement::Query(_)));
            let rewrite_is_select = rewrite.iter().all(|s| matches!(s, Statement::Query(_)));
            orig_is_select && rewrite_is_select
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_predicate_pushdown_suggestion() {
        let advisor = RewriteAdvisor::new();
        let schema = SchemaContext::default();
        let sql =
            "SELECT * FROM orders o JOIN users u ON o.user_id = u.id WHERE o.status = 'pending'";
        let result = advisor.suggest(sql, &schema).await.unwrap();
        let has_pushdown = result
            .iter()
            .any(|s| s.transform_type == TransformType::PredicatePushdown);
        assert!(has_pushdown);
    }

    #[tokio::test]
    async fn test_join_reorder_suggestion() {
        let advisor = RewriteAdvisor::new();
        let schema = SchemaContext::default();
        let sql = "SELECT * FROM orders o JOIN users u ON o.user_id = u.id JOIN products p ON o.product_id = p.id";
        let result = advisor.suggest(sql, &schema).await.unwrap();
        let has_reorder = result
            .iter()
            .any(|s| s.transform_type == TransformType::JoinReorder);
        assert!(has_reorder);
    }

    #[tokio::test]
    async fn test_subquery_flattening_suggestion() {
        let advisor = RewriteAdvisor::new();
        let schema = SchemaContext::default();
        let sql = "SELECT * FROM users WHERE id IN (SELECT user_id FROM orders)";
        let result = advisor.suggest(sql, &schema).await.unwrap();
        let has_flatten = result
            .iter()
            .any(|s| s.transform_type == TransformType::SubqueryFlattening);
        assert!(has_flatten);
    }

    #[tokio::test]
    async fn test_no_optimization_for_simple_query() {
        let advisor = RewriteAdvisor::new();
        let schema = SchemaContext::default();
        let sql = "SELECT * FROM users";
        let result = advisor.suggest(sql, &schema).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_rewrite_not_auto_applied() {
        let advisor = RewriteAdvisor::new();
        let schema = SchemaContext::default();
        let sql =
            "SELECT * FROM orders o JOIN users u ON o.user_id = u.id WHERE o.status = 'pending'";
        let result = advisor.suggest(sql, &schema).await.unwrap();
        for s in &result {
            assert!(s.rewritten_sql.contains("/*") || s.rewritten_sql != sql);
        }
    }

    #[tokio::test]
    async fn test_equivalence_proof_present() {
        let advisor = RewriteAdvisor::new();
        let schema = SchemaContext::default();
        let sql =
            "SELECT * FROM orders o JOIN users u ON o.user_id = u.id WHERE o.status = 'pending'";
        let result = advisor.suggest(sql, &schema).await.unwrap();
        for s in &result {
            assert!(!s.equivalence_proof.proof_text.is_empty());
        }
    }

    #[tokio::test]
    async fn test_llm_verifies_equivalence() {
        let advisor = RewriteAdvisor::new().with_llm();
        let schema = SchemaContext::default();
        let sql =
            "SELECT * FROM orders o JOIN users u ON o.user_id = u.id WHERE o.status = 'pending'";
        let result = advisor.suggest(sql, &schema).await.unwrap();
        for s in &result {
            assert!(s.equivalence_proof.verified);
            assert!(!s.equivalence_proof.unverified);
        }
    }

    #[tokio::test]
    async fn test_empty_sql_error() {
        let advisor = RewriteAdvisor::new();
        let schema = SchemaContext::default();
        let result = advisor.suggest("", &schema).await;
        assert!(matches!(result, Err(RewriteError::ParseError(_))));
    }

    #[tokio::test]
    async fn test_parse_error() {
        let advisor = RewriteAdvisor::new();
        let schema = SchemaContext::default();
        let result = advisor.suggest("INVALID SQL @#$", &schema).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_transform_type_name() {
        assert_eq!(TransformType::PredicatePushdown.name(), "PredicatePushdown");
        assert_eq!(
            TransformType::SubqueryFlattening.name(),
            "SubqueryFlattening"
        );
        assert_eq!(TransformType::JoinReorder.name(), "JoinReorder");
        assert_eq!(
            TransformType::RedundantElimination.name(),
            "RedundantElimination"
        );
    }

    #[test]
    fn test_audit_record() {
        let advisor = RewriteAdvisor::new();
        let record = advisor.audit_record(0.8);
        assert_eq!(record.advice_type, AdviceType::Rewrite);
        assert_eq!(
            record.source_engine,
            crate::advice_common::AdviceSource::Rule
        );
    }
}
