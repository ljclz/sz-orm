//! TASK-028 集成测试：NL2SQL 评估框架端到端验证

use sz_orm_model_ops::evaluator::{
    EvalFailure, EvalResult, EvalSample, FailureType, Nl2SqlEvaluator,
};

fn make_test_samples() -> Vec<EvalSample> {
    vec![
        EvalSample {
            nl_query: "查询所有用户".to_string(),
            expected_sql: "SELECT * FROM users".to_string(),
            expected_results: serde_json::json!({"sql": "SELECT * FROM users", "rows": []}),
        },
        EvalSample {
            nl_query: "统计订单数".to_string(),
            expected_sql: "SELECT COUNT(*) FROM orders".to_string(),
            expected_results: serde_json::json!({"sql": "SELECT COUNT(*) FROM orders", "rows": []}),
        },
        EvalSample {
            nl_query: "查询活跃用户".to_string(),
            expected_sql: "SELECT * FROM users WHERE status = 'active'".to_string(),
            expected_results: serde_json::json!({"sql": "SELECT * FROM users WHERE status = 'active'", "rows": []}),
        },
    ]
}

#[test]
fn test_evaluate_all_correct() {
    let evaluator = Nl2SqlEvaluator::new();
    let samples = make_test_samples();
    let result = evaluator
        .evaluate(&samples, |nl| {
            if nl.contains("所有用户") {
                Ok("SELECT * FROM users".to_string())
            } else if nl.contains("订单数") {
                Ok("SELECT COUNT(*) FROM orders".to_string())
            } else {
                Ok("SELECT * FROM users WHERE status = 'active'".to_string())
            }
        })
        .unwrap();

    assert_eq!(result.total_samples, 3);
    assert_eq!(result.exact_match_accuracy, 1.0);
    assert_eq!(result.execution_accuracy, 1.0);
    assert!(result.failures.is_empty());
}

#[test]
fn test_evaluate_with_sql_mismatch() {
    let evaluator = Nl2SqlEvaluator::new();
    let samples = make_test_samples();
    let result = evaluator
        .evaluate(&samples, |nl| {
            if nl.contains("所有用户") {
                Ok("SELECT * FROM users".to_string())
            } else if nl.contains("订单数") {
                Ok("SELECT count(*) from orders".to_string())
            } else {
                Ok("SELECT * FROM users WHERE status = 'active'".to_string())
            }
        })
        .unwrap();

    assert_eq!(result.exact_match_accuracy, 1.0, "归一化后应匹配");
}

#[test]
fn test_evaluate_with_generation_error() {
    let evaluator = Nl2SqlEvaluator::new();
    let samples = make_test_samples();
    let result = evaluator
        .evaluate(&samples, |_| Err("模型不可用".to_string()))
        .unwrap();

    assert_eq!(result.execution_accuracy, 0.0);
    assert_eq!(result.failures.len(), 3);
    assert!(result
        .failures
        .iter()
        .all(|f| f.failure_type == FailureType::GenerationFailed));
}

#[test]
fn test_evaluate_partial_success() {
    let evaluator = Nl2SqlEvaluator::new();
    let samples = make_test_samples();
    let result = evaluator
        .evaluate(&samples, |nl| {
            if nl.contains("所有用户") {
                Ok("SELECT * FROM users".to_string())
            } else {
                Err("无法生成".to_string())
            }
        })
        .unwrap();

    assert_eq!(result.execution_accuracy, 1.0 / 3.0);
    assert_eq!(result.failures.len(), 2);
}

#[test]
fn test_generate_report_content() {
    let evaluator = Nl2SqlEvaluator::new();
    let result = EvalResult {
        total_samples: 10,
        execution_accuracy: 0.8,
        exact_match_accuracy: 0.6,
        failures: vec![EvalFailure {
            sample_index: 3,
            nl_query: "测试".to_string(),
            expected_sql: "SELECT 1".to_string(),
            generated_sql: "SELECT 2".to_string(),
            failure_type: FailureType::SqlMismatch,
        }],
    };
    let report = evaluator.generate_report(&result);
    assert!(report.contains("NL2SQL 评估报告"));
    assert!(report.contains("执行准确率"));
    assert!(report.contains("精确匹配率"));
    assert!(report.contains("失败详情"));
    assert!(report.contains("样本 #3"));
}

#[test]
fn test_empty_samples() {
    let evaluator = Nl2SqlEvaluator::new();
    let result = evaluator
        .evaluate(&[], |_| Ok("SELECT 1".to_string()))
        .unwrap();
    assert_eq!(result.total_samples, 0);
    assert_eq!(result.execution_accuracy, 0.0);
    assert_eq!(result.exact_match_accuracy, 0.0);
}
