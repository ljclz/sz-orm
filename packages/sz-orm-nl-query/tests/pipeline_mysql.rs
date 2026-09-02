//! 方向1 集成测试：NlQueryPipeline 接入真实 MySQL 数据库
//!
//! 验证 NL→SQL→执行→返回真实 rows 闭环。
//! 需要 MySQL 9.6 运行在 127.0.0.1:3306，数据库 shop。
//! 通过环境变量 SZ_ORM_MYSQL_DSN 指定 DSN，未设置时跳过。

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use sz_orm_nl_query::pipeline::{NlQueryPipeline, SqlExecutor};
use sz_orm_nl_query::types::NlQueryError;
use sz_orm_sqlx::any_driver::AnyPool;
use sz_orm_sqlx::sz_orm_core::Connection;

/// MySQL SQL 执行器：通过 sz-orm-sqlx AnyPool 执行 SQL
struct MysqlExecutor {
    pool: AnyPool,
}

#[async_trait]
impl SqlExecutor for MysqlExecutor {
    async fn execute(&self, sql: &str) -> Result<Value, NlQueryError> {
        let mut conn = self
            .pool
            .create()
            .await
            .map_err(|e| NlQueryError::Nl2SqlFailed(format!("连接失败: {e}")))?;

        let rows = conn
            .query(sql)
            .await
            .map_err(|e| NlQueryError::Nl2SqlFailed(format!("查询失败: {e}")))?;

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
async fn test_pipeline_with_real_mysql() {
    let dsn = match get_dsn() {
        Some(d) => d,
        None => {
            eprintln!("跳过：未设置 SZ_ORM_MYSQL_DSN");
            return;
        }
    };

    let pool = AnyPool::connect(&dsn).await.expect("连接 MySQL 失败");
    let executor = Arc::new(MysqlExecutor { pool });
    let pipeline = NlQueryPipeline::new().with_executor(executor);

    // 查询 sz_user 表（shop 数据库中有 806 条用户记录）
    let response = pipeline
        .execute_sql("SELECT COUNT(*) as cnt FROM sz_user")
        .await
        .expect("查询失败");

    assert!(response.sql.contains("SELECT"));
    let rows = response.rows.as_array().expect("rows 应为数组");
    assert!(!rows.is_empty(), "应返回真实数据行，而非空数组");
    println!("查询结果: {} 行", rows.len());
    println!("第一行: {:?}", rows[0]);
}

#[tokio::test]
async fn test_pipeline_without_executor_returns_empty() {
    let pipeline = NlQueryPipeline::new();
    let response = pipeline.query("SELECT 1").await.expect("查询失败");
    let rows = response.rows.as_array().expect("rows 应为数组");
    assert!(rows.is_empty(), "未注入执行器时 rows 应为空");
}

#[tokio::test]
async fn test_pipeline_mysql_real_data() {
    let dsn = match get_dsn() {
        Some(d) => d,
        None => {
            eprintln!("跳过：未设置 SZ_ORM_MYSQL_DSN");
            return;
        }
    };

    let pool = AnyPool::connect(&dsn).await.expect("连接 MySQL 失败");
    let executor = Arc::new(MysqlExecutor { pool });
    let pipeline = NlQueryPipeline::new().with_executor(executor);

    // 查询真实用户数据
    let response = pipeline
        .execute_sql("SELECT user_id, nickname FROM sz_user LIMIT 5")
        .await
        .expect("查询失败");

    let rows = response.rows.as_array().expect("rows 应为数组");
    assert_eq!(rows.len(), 5, "应返回 5 行用户数据");
    println!("用户数据: {} 行", rows.len());
    for row in rows {
        println!("  {:?}", row);
    }
}
