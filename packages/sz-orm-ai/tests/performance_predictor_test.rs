//! TASK-015: PerformancePredictor 单元测试
//!
//! 验证基于统计信息的查询性能预测：重写前 SQL A vs 重写后 SQL B，
//! 输出预测耗时 + 加速比。

use sz_orm_ai::query_plan_optimizer::{
    PerformancePredictor, QueryCharacteristics, TableStatistics,
};

fn make_stats() -> Vec<TableStatistics> {
    vec![
        TableStatistics::new("users", 1_000_000)
            .with_column_cardinality("id", 1_000_000)
            .with_column_cardinality("email", 1_000_000)
            .with_column_cardinality("status", 5)
            .with_index_selectivity("id", 1.0)
            .with_index_selectivity("email", 1.0)
            .with_avg_row_size(128),
        TableStatistics::new("orders", 10_000_000)
            .with_column_cardinality("user_id", 1_000_000)
            .with_column_cardinality("status", 10)
            .with_index_selectivity("user_id", 0.001)
            .with_avg_row_size(64),
    ]
}

#[test]
fn test_predict_full_table_scan() {
    let predictor = PerformancePredictor::new();
    let stats = make_stats();

    let prediction = predictor.predict("SELECT * FROM users", &stats);

    assert!(prediction.estimated_ms > 0.0);
    assert_eq!(prediction.estimated_rows_scanned, 1_000_000);
    assert!(!prediction.uses_index);
    assert!(prediction.cost_score > 0.0);
    assert!(prediction.rationale.contains("全表扫描"));
}

#[test]
fn test_predict_index_scan() {
    let predictor = PerformancePredictor::new();
    let stats = make_stats();

    let prediction = predictor.predict(
        "SELECT id, email FROM users WHERE email = 'test@example.com'",
        &stats,
    );

    assert!(prediction.uses_index);
    assert!(prediction.estimated_rows_scanned < 1_000_000);
    assert!(prediction.rationale.contains("索引扫描"));
}

#[test]
fn test_predict_with_limit() {
    let predictor = PerformancePredictor::new();
    let stats = make_stats();

    let no_limit = predictor.predict("SELECT * FROM orders", &stats);
    let with_limit = predictor.predict("SELECT * FROM orders LIMIT 100", &stats);

    assert!(with_limit.estimated_ms < no_limit.estimated_ms);
    assert!(with_limit.rationale.contains("LIMIT"));
}

#[test]
fn test_predict_join_cost() {
    let predictor = PerformancePredictor::new();
    let stats = make_stats();

    let no_join = predictor.predict("SELECT * FROM users", &stats);
    let with_join = predictor.predict(
        "SELECT * FROM users JOIN orders ON users.id = orders.user_id",
        &stats,
    );

    assert!(with_join.estimated_ms > no_join.estimated_ms);
    assert!(with_join.rationale.contains("JOIN"));
}

#[test]
fn test_predict_select_star_overhead() {
    let predictor = PerformancePredictor::new();
    let stats = make_stats();

    let select_star = predictor.predict("SELECT * FROM users WHERE id = 1", &stats);
    let select_cols = predictor.predict("SELECT id, email FROM users WHERE id = 1", &stats);

    assert!(select_star.estimated_ms > select_cols.estimated_ms);
}

#[test]
fn test_compare_speedup() {
    let predictor = PerformancePredictor::new();
    let stats = make_stats();

    let original = "SELECT * FROM users WHERE status = 'active'";
    let optimized = "SELECT id, email FROM users WHERE email = 'test@example.com'";

    let (orig_pred, opt_pred, speedup) = predictor.compare(original, optimized, &stats);

    assert!(orig_pred.estimated_ms > 0.0);
    assert!(opt_pred.estimated_ms > 0.0);
    assert!(speedup > 0.0);
    assert!(!orig_pred.uses_index);
    assert!(opt_pred.uses_index);
}

#[test]
fn test_predict_no_stats_fallback() {
    let predictor = PerformancePredictor::new();
    let stats: Vec<TableStatistics> = vec![];

    let prediction = predictor.predict("SELECT * FROM unknown_table", &stats);

    assert!(prediction.estimated_ms > 0.0);
    assert!(prediction.rationale.contains("无表统计信息"));
}

#[test]
fn test_query_characteristics_from_sql() {
    let chars = QueryCharacteristics::from_sql(
        "SELECT id, name FROM users JOIN orders ON users.id = orders.user_id WHERE users.status = 'active' ORDER BY users.id LIMIT 10",
    );

    assert!(chars.tables.contains(&"users".to_string()));
    assert!(chars.tables.contains(&"orders".to_string()));
    assert_eq!(chars.join_count, 1);
    assert_eq!(chars.limit, Some(10));
    assert!(!chars.order_by_columns.is_empty());
    assert!(!chars.uses_select_star);
}

#[test]
fn test_query_characteristics_select_star() {
    let chars = QueryCharacteristics::from_sql("SELECT * FROM users");
    assert!(chars.uses_select_star);
    assert!(chars.tables.contains(&"users".to_string()));
}

#[test]
fn test_query_characteristics_subquery() {
    let chars = QueryCharacteristics::from_sql(
        "SELECT * FROM users WHERE id IN (SELECT user_id FROM orders)",
    );
    assert!(chars.subquery_count >= 1);
}

#[test]
fn test_table_statistics_builder() {
    let stat = TableStatistics::new("products", 50000)
        .with_column_cardinality("category_id", 100)
        .with_index_selectivity("category_id", 0.02)
        .with_avg_row_size(256);

    assert_eq!(stat.table_name, "products");
    assert_eq!(stat.row_count, 50000);
    assert_eq!(stat.avg_row_size_bytes, 256);
    assert_eq!(stat.column_cardinality.get("category_id"), Some(&100));
    assert_eq!(stat.index_selectivity.get("category_id"), Some(&0.02));
}

#[test]
fn test_predict_high_selectivity_index() {
    let predictor = PerformancePredictor::new();
    let stats = vec![TableStatistics::new("users", 1_000_000)
        .with_index_selectivity("id", 1.0)
        .with_avg_row_size(64)];

    let prediction = predictor.predict("SELECT id FROM users WHERE id = 42", &stats);

    assert!(prediction.uses_index);
    assert!(prediction.estimated_rows_scanned < 100);
    assert!(prediction.estimated_ms < 10.0);
}

#[test]
fn test_predict_low_selectivity_index() {
    let predictor = PerformancePredictor::new();
    let stats = vec![TableStatistics::new("users", 1_000_000)
        .with_index_selectivity("status", 0.01)
        .with_avg_row_size(64)];

    let prediction = predictor.predict("SELECT id FROM users WHERE status = 'active'", &stats);

    assert!(prediction.uses_index);
    assert!(prediction.estimated_rows_scanned > 500_000);
}

#[test]
fn test_compare_optimized_faster() {
    let predictor = PerformancePredictor::new();
    let stats = make_stats();

    let original = "SELECT * FROM orders";
    let optimized = "SELECT id FROM orders WHERE user_id = 42 LIMIT 10";

    let (orig, opt, speedup) = predictor.compare(original, optimized, &stats);

    assert!(speedup > 1.0, "优化版本应更快，实际加速比: {}", speedup);
    assert!(orig.estimated_ms > opt.estimated_ms);
}
