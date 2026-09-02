//! NL2SQL 评估框架（TASK-028）

use crate::types::ModelOpsError;
use serde::{Deserialize, Serialize};

/// 评估样本
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSample {
    pub nl_query: String,
    pub expected_sql: String,
    pub expected_results: serde_json::Value,
}

/// 评估结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    pub total_samples: usize,
    pub execution_accuracy: f64,
    pub exact_match_accuracy: f64,
    pub failures: Vec<EvalFailure>,
}

/// 精确匹配率
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalFailure {
    pub sample_index: usize,
    pub nl_query: String,
    pub expected_sql: String,
    pub generated_sql: String,
    pub failure_type: FailureType,
}

/// 失败类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FailureType {
    SqlMismatch,
    ResultMismatch,
    GenerationFailed,
}

/// NL2SQL 评估器
pub struct Nl2SqlEvaluator;

impl Nl2SqlEvaluator {
    pub fn new() -> Self {
        Self
    }

    /// 评估 NL2SQL 引擎的准确率
    pub fn evaluate<F>(
        &self,
        samples: &[EvalSample],
        generate_fn: F,
    ) -> Result<EvalResult, ModelOpsError>
    where
        F: Fn(&str) -> Result<String, String>,
    {
        let mut exact_matches = 0;
        let mut exec_matches = 0;
        let mut failures = Vec::new();

        for (idx, sample) in samples.iter().enumerate() {
            match generate_fn(&sample.nl_query) {
                Ok(generated_sql) => {
                    let normalized_expected = Self::normalize_sql(&sample.expected_sql);
                    let normalized_generated = Self::normalize_sql(&generated_sql);

                    if normalized_expected == normalized_generated {
                        exact_matches += 1;
                        exec_matches += 1;
                    } else {
                        let expected_results = &sample.expected_results;
                        let generated_results = Self::execute_stub(&generated_sql);

                        if generated_results == *expected_results {
                            exec_matches += 1;
                            failures.push(EvalFailure {
                                sample_index: idx,
                                nl_query: sample.nl_query.clone(),
                                expected_sql: sample.expected_sql.clone(),
                                generated_sql,
                                failure_type: FailureType::SqlMismatch,
                            });
                        } else {
                            failures.push(EvalFailure {
                                sample_index: idx,
                                nl_query: sample.nl_query.clone(),
                                expected_sql: sample.expected_sql.clone(),
                                generated_sql,
                                failure_type: FailureType::ResultMismatch,
                            });
                        }
                    }
                }
                Err(_) => {
                    failures.push(EvalFailure {
                        sample_index: idx,
                        nl_query: sample.nl_query.clone(),
                        expected_sql: sample.expected_sql.clone(),
                        generated_sql: String::new(),
                        failure_type: FailureType::GenerationFailed,
                    });
                }
            }
        }

        let total = samples.len();
        let execution_accuracy = if total > 0 {
            exec_matches as f64 / total as f64
        } else {
            0.0
        };
        let exact_match_accuracy = if total > 0 {
            exact_matches as f64 / total as f64
        } else {
            0.0
        };

        Ok(EvalResult {
            total_samples: total,
            execution_accuracy,
            exact_match_accuracy,
            failures,
        })
    }

    /// 归一化 SQL 用于比较（去空格、转小写）
    fn normalize_sql(sql: &str) -> String {
        sql.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }

    /// 模拟 SQL 执行（实际应连接数据库）
    fn execute_stub(sql: &str) -> serde_json::Value {
        serde_json::json!({"sql": sql, "rows": []})
    }

    /// 生成评估报告
    pub fn generate_report(&self, result: &EvalResult) -> String {
        let mut report = String::new();
        report.push_str("# NL2SQL 评估报告\n\n");
        report.push_str(&format!("- 总样本数: {}\n", result.total_samples));
        report.push_str(&format!(
            "- 执行准确率: {:.2}%\n",
            result.execution_accuracy * 100.0
        ));
        report.push_str(&format!(
            "- 精确匹配率: {:.2}%\n",
            result.exact_match_accuracy * 100.0
        ));
        report.push_str(&format!("- 失败数: {}\n\n", result.failures.len()));

        if !result.failures.is_empty() {
            report.push_str("## 失败详情\n\n");
            for failure in &result.failures {
                report.push_str(&format!(
                    "### 样本 #{}\n- NL: {}\n- 期望 SQL: {}\n- 生成 SQL: {}\n- 失败类型: {:?}\n\n",
                    failure.sample_index,
                    failure.nl_query,
                    failure.expected_sql,
                    failure.generated_sql,
                    failure.failure_type
                ));
            }
        }

        report
    }
}

impl Default for Nl2SqlEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_samples() -> Vec<EvalSample> {
        vec![
            EvalSample {
                nl_query: "查询所有用户".to_string(),
                expected_sql: "SELECT * FROM users".to_string(),
                expected_results: serde_json::json!({"sql": "SELECT * FROM users", "rows": []}),
            },
            EvalSample {
                nl_query: "查询订单数量".to_string(),
                expected_sql: "SELECT COUNT(*) FROM orders".to_string(),
                expected_results: serde_json::json!({"sql": "SELECT COUNT(*) FROM orders", "rows": []}),
            },
        ]
    }

    #[test]
    fn test_evaluate_exact_match() {
        let evaluator = Nl2SqlEvaluator::new();
        let samples = make_samples();
        let result = evaluator
            .evaluate(&samples, |nl| {
                if nl.contains("用户") {
                    Ok("SELECT * FROM users".to_string())
                } else {
                    Ok("SELECT COUNT(*) FROM orders".to_string())
                }
            })
            .unwrap();

        assert_eq!(result.exact_match_accuracy, 1.0);
        assert_eq!(result.execution_accuracy, 1.0);
        assert!(result.failures.is_empty());
    }

    #[test]
    fn test_evaluate_with_failures() {
        let evaluator = Nl2SqlEvaluator::new();
        let samples = make_samples();
        let result = evaluator
            .evaluate(&samples, |nl| {
                if nl.contains("用户") {
                    Ok("SELECT * FROM users".to_string())
                } else {
                    Ok("SELECT * FROM orders".to_string())
                }
            })
            .unwrap();

        assert!(result.exact_match_accuracy < 1.0);
        assert!(!result.failures.is_empty());
    }

    #[test]
    fn test_evaluate_generation_error() {
        let evaluator = Nl2SqlEvaluator::new();
        let samples = make_samples();
        let result = evaluator
            .evaluate(&samples, |_| Err("模型不可用".to_string()))
            .unwrap();

        assert_eq!(result.execution_accuracy, 0.0);
        assert_eq!(result.failures.len(), 2);
        assert!(result
            .failures
            .iter()
            .all(|f| f.failure_type == FailureType::GenerationFailed));
    }

    #[test]
    fn test_normalize_sql() {
        let a = Nl2SqlEvaluator::normalize_sql("SELECT  *   FROM  users");
        let b = Nl2SqlEvaluator::normalize_sql("select * from users");
        assert_eq!(a, b);
    }

    #[test]
    fn test_generate_report() {
        let evaluator = Nl2SqlEvaluator::new();
        let result = EvalResult {
            total_samples: 10,
            execution_accuracy: 0.8,
            exact_match_accuracy: 0.6,
            failures: vec![],
        };
        let report = evaluator.generate_report(&result);
        assert!(report.contains("执行准确率"));
        assert!(report.contains("精确匹配率"));
    }

    #[test]
    fn test_empty_samples() {
        let evaluator = Nl2SqlEvaluator::new();
        let result = evaluator
            .evaluate(&[], |_| Ok("SELECT 1".to_string()))
            .unwrap();
        assert_eq!(result.total_samples, 0);
        assert_eq!(result.execution_accuracy, 0.0);
    }
}
