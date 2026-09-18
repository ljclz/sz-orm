//! v7.4.0 任务 4.1：RewriteRule/IndexAdvice/Nl2sqlResult 边界测试

use sz_orm_ai::{
    ColumnPruningRule, ConstantFoldingRule, LimitPushdownRule, Nl2sqlResult, RewriteEngine,
    RewriteRule, SqlQuery, TransformType, EquivalenceVerificationResult,
};
use sz_orm_ai::nl2sql::IntentAnalysis;

fn make_nl2sql_result(sql: &str, confidence: f32) -> Nl2sqlResult {
    Nl2sqlResult {
        sql: SqlQuery {
            sql: sql.to_string(),
            explanation: String::new(),
            confidence,
            dialect: None,
            cache_hit: false,
        },
        intent: IntentAnalysis {
            intent: "select".to_string(),
            entities: Vec::new(),
            confidence,
        },
        latency_ms: 10,
        injection_filtered: false,
    }
}

#[test]
fn test_rewrite_engine_empty_sql() {
    let engine = RewriteEngine::new();
    let result = engine.rewrite("");
    assert!(result.suggestion.is_none());
}

#[test]
fn test_rewrite_engine_whitespace_only() {
    let engine = RewriteEngine::new();
    let result = engine.rewrite("   ");
    assert!(result.suggestion.is_none());
}

#[test]
fn test_constant_folding_no_match() {
    let rule = ConstantFoldingRule;
    assert!(rule.apply("SELECT 1").is_none());
    assert!(rule.apply("SELECT * FROM t WHERE x = 1").is_none());
}

#[test]
fn test_column_pruning_no_where() {
    let rule = ColumnPruningRule;
    assert!(rule.apply("SELECT * FROM users").is_none());
}

#[test]
fn test_limit_pushdown_no_subquery() {
    let rule = LimitPushdownRule;
    assert!(rule.apply("SELECT * FROM users LIMIT 10").is_none());
}

#[test]
fn test_nl2sql_result_low_confidence() {
    let result = make_nl2sql_result("SELECT 1", 0.1);
    assert!(result.sql.confidence < 0.5);
}

#[test]
fn test_nl2sql_result_high_confidence() {
    let result = make_nl2sql_result("SELECT 1", 0.95);
    assert!(result.sql.confidence > 0.8);
}

#[test]
fn test_nl2sql_result_injection_filter_safe() {
    let mut result = make_nl2sql_result("SELECT id FROM users WHERE id = $1", 1.0);
    result.enforce_injection_filter();
    assert!(!result.injection_filtered);
}

#[test]
fn test_nl2sql_result_injection_filter_unsafe() {
    let mut result = make_nl2sql_result("SELECT id FROM users; DROP TABLE users--", 1.0);
    result.enforce_injection_filter();
    assert!(result.injection_filtered);
}

#[test]
fn test_equivalence_result_equivalent_zero_rows() {
    let r = EquivalenceVerificationResult::equivalent(0);
    assert!(r.is_equivalent);
    assert_eq!(r.original_row_count, 0);
}

#[test]
fn test_equivalence_result_not_equivalent_with_diff() {
    let r = EquivalenceVerificationResult::not_equivalent(3, 5, "行数不匹配".to_string());
    assert!(!r.is_equivalent);
    assert!(r.diff_details.as_ref().unwrap().contains("行数不匹配"));
}

#[test]
fn test_transform_type_all_variants_name() {
    assert_eq!(TransformType::PredicatePushdown.name(), "PredicatePushdown");
    assert_eq!(TransformType::SubqueryFlattening.name(), "SubqueryFlattening");
    assert_eq!(TransformType::JoinReorder.name(), "JoinReorder");
    assert_eq!(TransformType::RedundantElimination.name(), "RedundantElimination");
    assert_eq!(TransformType::LimitPushdown.name(), "LimitPushdown");
    assert_eq!(TransformType::ConstantFolding.name(), "ConstantFolding");
    assert_eq!(TransformType::ColumnPruning.name(), "ColumnPruning");
}