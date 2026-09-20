//! v7.6.0 组2.7：HistoryBasedPredictor 端到端测试
//!
//! 验证基于历史数据的计划预测：
//! - 延迟 ≤ 200ms
//! - 偏差标注
//! - 历史数据不足错误

use sz_orm_ai::{HistoryBasedPredictor, QueryHistoryCollector, QueryHistoryEntry};

#[test]
fn test_e2e_history_predictor_latency() {
    let history: Vec<QueryHistoryEntry> = (0..10)
        .map(|i| QueryHistoryEntry {
            sql_fingerprint: "SELECT * FROM users WHERE id = ?".to_string(),
            executed_plan: "IndexScan".to_string(),
            actual_cost: 10.0 + i as f64 * 0.3,
            execution_time: 1.0 + i as f64 * 0.05,
            row_count: 100,
        })
        .collect();

    let result =
        HistoryBasedPredictor::predict_from_history("SELECT * FROM users WHERE id = 42", &history);
    assert!(result.is_ok());
    let pred = result.unwrap();
    assert!(pred.prediction_latency_ms <= 200.0, "预测延迟应 ≤ 200ms");
    assert!(pred.history_data_basis.is_some());
}

#[test]
fn test_e2e_history_predictor_insufficient_data() {
    let history = vec![QueryHistoryEntry {
        sql_fingerprint: "SELECT * FROM users WHERE id = ?".to_string(),
        executed_plan: "IndexScan".to_string(),
        actual_cost: 10.0,
        execution_time: 1.0,
        row_count: 100,
    }];

    let result =
        HistoryBasedPredictor::predict_from_history("SELECT * FROM users WHERE id = 42", &history);
    assert!(result.is_err());
}

#[test]
fn test_e2e_history_collector_from_logs() {
    let records = vec![
        (
            "SELECT * FROM t WHERE id = ?".to_string(),
            "IndexScan".to_string(),
            5.0,
            0.5,
            10,
        ),
        (
            "SELECT * FROM t".to_string(),
            "SeqScan".to_string(),
            50.0,
            5.0,
            1000,
        ),
    ];
    let entries = QueryHistoryCollector::from_logs(&records);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].sql_fingerprint, "SELECT * FROM t WHERE id = ?");
}

#[test]
fn test_e2e_history_predictor_matched_fingerprint() {
    let history: Vec<QueryHistoryEntry> = (0..5)
        .map(|i| QueryHistoryEntry {
            sql_fingerprint: "SELECT * FROM orders WHERE status = ?".to_string(),
            executed_plan: "IndexScan".to_string(),
            actual_cost: 8.0 + i as f64 * 0.2,
            execution_time: 0.8,
            row_count: 50,
        })
        .collect();

    let result = HistoryBasedPredictor::predict_from_history(
        "SELECT * FROM orders WHERE status = 1",
        &history,
    );
    assert!(result.is_ok());
    let pred = result.unwrap();
    assert!(pred.predicted_plan.contains("IndexScan"));
    assert!(pred.predicted_cost >= 8.0);
}
