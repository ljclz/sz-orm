//! 方向1 集成测试：评估器注入真实 MySQL 执行函数
//!
//! 验证 Nl2SqlEvaluator::evaluate_with_executor 通过真实数据库执行 SQL 比较结果。
//! 需要 MySQL 运行在 127.0.0.1:3306，数据库 shop。
//! 通过环境变量 SZ_ORM_MYSQL_DSN 指定 DSN，未设置时跳过。

use async_trait::async_trait;
use serde_json::Value;
use sz_orm_model_ops::evaluator::{EvalSample, Nl2SqlEvaluator, SqlExecutor};
use sz_orm_sqlx::any_driver::AnyPool;
use sz_orm_sqlx::sz_orm_core::Connection;
use std::sync::Arc;

struct MysqlExecutor {
    pool: AnyPool,
}

#[async_trait]
impl SqlExecutor for MysqlExecutor {
    async fn execute(&self, sql: &str) -> Result<Value, String> {
        let mut conn = self.pool.create().await.map_err(|e| format!("连接失败: {e}"))?;
        let rows = conn.query(sql).await.map_err(|e| format!("查询失败: {e}"))?;

        let json_rows: Vec<Value> = rows
            .iter()
            .map(|row| {
                let obj: serde_json::Map<String, Value> = row
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::String(format!("{v:?}"))))
                    .collect();
                Value::Object(obj)
            })
            .collect();

        Ok(Value::Array(json_rows))
    }
}

fn get_dsn() -> Option<String> {
    std::env::var("SZ_ORM_MYSQL_DSN").ok()
}

#[tokio::test]
async fn test_evaluator_with_real_mysql() {
    let dsn = match get_dsn() {
        Some(d) => d,
        None => {
            eprintln!("跳过：未设置 SZ_ORM_MYSQL_DSN");
            return;
        }
    };

    let pool = AnyPool::connect(&dsn).await.expect("连接 MySQL 失败");
    let executor = Arc::new(MysqlExecutor { pool });
    let evaluator = Nl2SqlEvaluator::new();

    let samples = vec![
        EvalSample {
            nl_query: "查询用户数量".to_string(),
            expected_sql: "SELECT COUNT(*) as cnt FROM sz_user".to_string(),
            expected_results: Value::Null,
        },
        EvalSample {
            nl_query: "查询前3个用户".to_string(),
            expected_sql: "SELECT user_id FROM sz_user LIMIT 3".to_string(),
            expected_results: Value::Null,
        },
    ];

    let result = evaluator
        .evaluate_with_executor(&samples, |nl| {
            if nl.contains("数量") {
                Ok("SELECT COUNT(*) as cnt FROM sz_user".to_string())
            } else {
                Ok("SELECT user_id FROM sz_user LIMIT 3".to_string())
            }
        }, executor)
        .await
        .expect("评估失败");

    assert_eq!(result.total_samples, 2);
    assert_eq!(result.exact_match_accuracy, 1.0, "两个样本应精确匹配");
    assert!(result.failures.is_empty());
    println!("评估结果: 精确匹配率 {:.2}%", result.exact_match_accuracy * 100.0);
}

#[tokio::test]
async fn test_evaluator_with_executor_result_mismatch() {
    let dsn = match get_dsn() {
        Some(d) => d,
        None => {
            eprintln!("跳过：未设置 SZ_ORM_MYSQL_DSN");
            return;
        }
    };

    let pool = AnyPool::connect(&dsn).await.expect("连接 MySQL 失败");
    let executor = Arc::new(MysqlExecutor { pool });
    let evaluator = Nl2SqlEvaluator::new();

    let samples = vec![EvalSample {
        nl_query: "查询用户数量".to_string(),
        expected_sql: "SELECT COUNT(*) as cnt FROM sz_user".to_string(),
        expected_results: Value::Null,
    }];

    let result = evaluator
        .evaluate_with_executor(&samples, |_| {
            Ok("SELECT user_id FROM sz_user LIMIT 5".to_string())
        }, executor)
        .await
        .expect("评估失败");

    assert_eq!(result.total_samples, 1);
    assert!(result.exact_match_accuracy < 1.0, "SQL 不匹配");
    assert!(!result.failures.is_empty(), "应有失败记录");
    println!(
        "评估结果: 精确匹配率 {:.2}%, 失败数 {}",
        result.exact_match_accuracy * 100.0,
        result.failures.len()
    );
}