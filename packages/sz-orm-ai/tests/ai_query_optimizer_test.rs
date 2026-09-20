//! v7.7.0 任务 2.7：AiQueryOptimizer 端到端测试
//!
//! 验证 AI 驱动查询优化的完整流程：
//! - 自动改写（谓词下推/JOIN 重排序/子查询展开）
//! - 等价验证（is_equivalent + result_set_consistent）
//! - P95 改善 ≥ 10%
//! - 决策延迟 ≤ 200ms
//! - 可解释性（optimization_basis 非空）

#![cfg(feature = "ai-query-optimize")]

use sz_orm_ai::{AiError, AiQueryOptimizer, RewriteAction};

#[tokio::test]
async fn e2e_ai_query_optimizer_predicate_pushdown() {
    let optimizer = AiQueryOptimizer::new();
    let sql = "SELECT * FROM orders o JOIN users u ON o.user_id = u.id WHERE o.status = 'pending'";
    let result = optimizer.optimize(sql).await.unwrap();
    assert!(result
        .rewrite_actions
        .contains(&RewriteAction::PredicatePushdown));
    assert!(result.is_equivalent);
    assert!(result.result_set_consistent);
    assert!(result.p95_improvement >= 10.0);
    assert!(result.decision_latency_ms <= 200.0);
    assert!(!result.optimization_basis.is_empty());
}

#[tokio::test]
async fn e2e_ai_query_optimizer_join_reorder() {
    let optimizer = AiQueryOptimizer::new();
    let sql = "SELECT * FROM orders o JOIN users u ON o.user_id = u.id JOIN products p ON o.product_id = p.id";
    let result = optimizer.optimize(sql).await.unwrap();
    assert!(result.rewrite_actions.contains(&RewriteAction::JoinReorder));
    assert!(result.is_equivalent);
    assert!(result.p95_improvement >= 10.0);
}

#[tokio::test]
async fn e2e_ai_query_optimizer_subquery_expand() {
    let optimizer = AiQueryOptimizer::new();
    let sql = "SELECT * FROM users WHERE id IN (SELECT user_id FROM orders)";
    let result = optimizer.optimize(sql).await.unwrap();
    assert!(result
        .rewrite_actions
        .contains(&RewriteAction::SubqueryExpand));
    assert!(result.is_equivalent);
}

#[tokio::test]
async fn e2e_ai_query_optimizer_empty_sql_rejected() {
    let optimizer = AiQueryOptimizer::new();
    let result = optimizer.optimize("").await;
    assert!(result.is_err());
    assert!(matches!(result, Err(AiError::ConfigError(_))));
}

#[tokio::test]
async fn e2e_ai_query_optimizer_idempotent() {
    let optimizer = AiQueryOptimizer::new();
    let sql = "SELECT * FROM orders o JOIN users u ON o.user_id = u.id WHERE o.status = 'pending'";
    let r1 = optimizer.optimize(sql).await.unwrap();
    let r2 = optimizer.optimize(sql).await.unwrap();
    assert_eq!(r1.sql_after, r2.sql_after);
    assert_eq!(r1.p95_improvement, r2.p95_improvement);
}
