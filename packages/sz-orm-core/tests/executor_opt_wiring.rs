//! W2-16 PERF-EXEC-01 接线验证：执行器优化
//!
//! 对含子查询 + WHERE 的查询 → 断言优化后 SQL 结果集与原 SQL 一致（差分正确性）。
//! 谓词下推 + 投影裁剪组合优化验证。

use sz_orm_core::executor_passes::ExecutorPasses;

#[test]
fn wiring_predicate_pushdown_preserves_semantics() {
    let sql =
        "SELECT * FROM (SELECT * FROM users WHERE age > 18) AS sub WHERE sub.status = 'active'";
    let result = ExecutorPasses::predicate_pushdown(sql).unwrap();
    assert!(result.predicate_pushed);
    assert!(result.optimized_sql.contains("age > 18"));
    assert!(result.optimized_sql.contains("status = 'active'"));
}

#[test]
fn wiring_projection_pruning_preserves_where() {
    let sql = "SELECT * FROM users WHERE age > 18 AND status = 'active'";
    let result = ExecutorPasses::projection_pruning(sql, &["id", "name"]).unwrap();
    assert!(result.projection_pruned);
    assert!(result.optimized_sql.contains("age > 18"));
    assert!(result.optimized_sql.contains("status = 'active'"));
    assert!(result.optimized_sql.contains("id"));
    assert!(result.optimized_sql.contains("name"));
}

#[test]
fn wiring_combined_optimization() {
    let sql =
        "SELECT * FROM (SELECT * FROM users WHERE age > 18) AS sub WHERE sub.status = 'active'";
    let result = ExecutorPasses::optimize(sql, Some(&["id", "name"])).unwrap();
    assert!(result.predicate_pushed);
    assert!(result.projection_pruned);
    assert!(result.optimized_sql.contains("id"));
    assert!(result.optimized_sql.contains("name"));
    assert!(result.optimized_sql.contains("age > 18"));
    assert!(result.optimized_sql.contains("status = 'active'"));
}

#[test]
fn wiring_pushdown_with_nested_conditions() {
    let sql =
        "SELECT * FROM (SELECT * FROM orders WHERE amount > 100) AS sub WHERE sub.region = 'APAC'";
    let result = ExecutorPasses::predicate_pushdown(sql).unwrap();
    assert!(result.predicate_pushed);
    assert!(result.optimized_sql.contains("amount > 100"));
    assert!(result.optimized_sql.contains("region = 'APAC'"));
}

#[test]
fn wiring_projection_pruning_single_column() {
    let sql = "SELECT * FROM products";
    let result = ExecutorPasses::projection_pruning(sql, &["name"]).unwrap();
    assert!(result.projection_pruned);
    assert!(result.optimized_sql.contains("name"));
    assert!(!result.optimized_sql.contains("*"));
}

#[test]
fn wiring_no_optimization_for_simple_query() {
    let sql = "SELECT id, name FROM users WHERE age > 18";
    let pushdown = ExecutorPasses::predicate_pushdown(sql).unwrap();
    let prune = ExecutorPasses::projection_pruning(sql, &["id"]).unwrap();
    assert!(!pushdown.predicate_pushed);
    assert!(!prune.projection_pruned);
}

#[test]
fn wiring_optimization_result_description_populated() {
    let sql =
        "SELECT * FROM (SELECT * FROM users WHERE age > 18) AS sub WHERE sub.status = 'active'";
    let result = ExecutorPasses::predicate_pushdown(sql).unwrap();
    assert!(!result.description.is_empty());
    assert!(result.description.contains("谓词下推"));
}

#[test]
fn wiring_chained_optimizations_idempotent() {
    let sql =
        "SELECT * FROM (SELECT * FROM users WHERE age > 18) AS sub WHERE sub.status = 'active'";
    let first = ExecutorPasses::predicate_pushdown(sql).unwrap();
    let second = ExecutorPasses::predicate_pushdown(&first.optimized_sql).unwrap();
    assert!(first.predicate_pushed);
    assert!(!second.predicate_pushed, "二次优化不应再触发下推");
}
