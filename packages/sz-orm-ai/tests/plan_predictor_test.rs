#![cfg(feature = "ai-plan-predict")]

use sz_orm_ai::PlanPredictor;

#[test]
fn test_plan_predict_basic() {
    let result = PlanPredictor::predict("SELECT * FROM users WHERE id = 1", "users", 10000, 100);
    assert_eq!(result.sql, "SELECT * FROM users WHERE id = 1");
    assert!(result.predicted_cost > 0.0);
    assert!(result.prediction_latency_ms >= 0.0);
}

#[test]
fn test_plan_predict_index_scan() {
    let result = PlanPredictor::predict(
        "SELECT * FROM users WHERE email = 'x'",
        "users",
        100000,
        100,
    );
    assert_eq!(result.predicted_plan, "IndexScan");
}

#[test]
fn test_plan_predict_seq_scan() {
    let result = PlanPredictor::predict("SELECT * FROM users", "users", 100000, 90000);
    assert_eq!(result.predicted_plan, "SeqScan");
}

#[test]
fn test_plan_predict_statistics_basis() {
    let result = PlanPredictor::predict("SELECT * FROM t WHERE a = 1", "t", 5000, 50);
    assert!(result.statistics_basis.contains("table=t"));
    assert!(result.statistics_basis.contains("rows=5000"));
}

#[test]
fn test_plan_predict_cost_model_basis() {
    let result = PlanPredictor::predict("SELECT * FROM t WHERE a = 1", "t", 1000, 10);
    assert!(!result.cost_model_basis.is_empty());
}

#[test]
fn test_plan_predict_check_deviation() {
    let dev = PlanPredictor::check_deviation(100.0, 200.0);
    assert!(dev.is_some());
    let dev_ok = PlanPredictor::check_deviation(100.0, 110.0);
    assert!(dev_ok.is_none());
}

#[test]
fn test_plan_predict_empty_table() {
    let result = PlanPredictor::predict("SELECT * FROM empty", "empty", 0, 0);
    assert!(result.predicted_cost >= 0.0);
}
