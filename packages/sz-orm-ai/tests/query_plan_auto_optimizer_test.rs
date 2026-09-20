#![cfg(feature = "plan-auto-optimize")]

use sz_orm_ai::query_plan_optimizer::*;

#[tokio::test]
async fn e2e_auto_optimize_seq_scan() {
    let optimizer = QueryPlanAutoOptimizer::new();
    let result = optimizer
        .auto_optimize("SELECT * FROM big_table WHERE age > 25")
        .await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert!(r.p95_improvement >= 10.0);
    assert!(r.decision_latency_ms <= 100.0);
    assert!(!r.optimization_actions.is_empty());
}

#[tokio::test]
async fn e2e_auto_optimize_join() {
    let optimizer = QueryPlanAutoOptimizer::new();
    let result = optimizer
        .auto_optimize("SELECT * FROM a JOIN b ON a.id = b.id")
        .await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert!(r.p95_improvement >= 10.0);
    assert!(r.decision_latency_ms <= 100.0);
}

#[tokio::test]
async fn e2e_auto_optimize_index_lookup() {
    let optimizer = QueryPlanAutoOptimizer::new();
    let result = optimizer
        .auto_optimize("SELECT * FROM users WHERE id = 42")
        .await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert!(r.p95_improvement >= 10.0);
    assert!(r.decision_latency_ms <= 100.0);
}

#[tokio::test]
async fn e2e_auto_optimize_decision_explainable() {
    let optimizer = QueryPlanAutoOptimizer::new();
    let result = optimizer
        .auto_optimize("SELECT * FROM t WHERE x = 1")
        .await
        .unwrap();
    assert!(!result.optimization_basis.is_empty());
    assert!(!result.optimization_actions.is_empty());
}
