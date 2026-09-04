//! v6.0.x nl2sql 端到端 demo：NL→SQL→执行→可视化→洞察 完整闭环

#![cfg(feature = "nl-query")]

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use sz_orm_nl_query::pipeline::{NlQueryPipeline, SqlExecutor};
use sz_orm_nl_query::types::NlQueryError;
use sz_orm_sqlx::any_driver::AnyPool;
use sz_orm_sqlx::sz_orm_core::Connection;

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

#[tokio::test]
#[ignore = "需要真实 MySQL"]
async fn demo_nl2sql_e2e_full_pipeline() {
    let pool = AnyPool::connect("mysql://root:test123@127.0.0.1:3306/shop")
        .await
        .unwrap();
    let executor = MysqlExecutor { pool };
    let pipeline = NlQueryPipeline::new().with_executor(Arc::new(executor));

    // 直接执行 SQL（绕过 NL2SQL 规则引擎，验证执行器闭环）
    let response = pipeline.execute_sql("SELECT * FROM sz_user LIMIT 5").await.unwrap();

    assert!(response.sql.contains("SELECT"));
    let rows = response.rows.as_array().expect("rows 应为数组");
    assert!(!rows.is_empty(), "应返回真实用户数据");
    println!("demo: SQL → {} 行数据", rows.len());

    // 同时验证 NL→SQL 规则引擎（降级模式，不执行）
    let pipeline2 = NlQueryPipeline::new();
    let nl_response = pipeline2.query("查询所有用户").await.unwrap();
    assert!(nl_response.sql.contains("SELECT"));
    assert!(nl_response.sql.contains("users"));
    println!("demo: NL → SQL '{}'", nl_response.sql);
}

#[tokio::test]
#[ignore = "需要真实 MySQL"]
async fn demo_nl2sql_e2e_with_llm_generator() {
    use sz_orm_nl_query::pipeline::Nl2SqlGenerator;

    struct FixedGenerator;
    #[async_trait]
    impl Nl2SqlGenerator for FixedGenerator {
        async fn generate(&self, nl: &str) -> Result<String, NlQueryError> {
            Ok(format!("SELECT * FROM sz_user LIMIT 5 -- {}", nl))
        }
    }

    let pool = AnyPool::connect("mysql://root:test123@127.0.0.1:3306/shop")
        .await
        .unwrap();
    let executor = MysqlExecutor { pool };
    let pipeline = NlQueryPipeline::new()
        .with_executor(Arc::new(executor))
        .with_llm_generator(Arc::new(FixedGenerator));

    let response = pipeline.query("查询用户").await.unwrap();
    assert!(response.sql.contains("sz_user"));
    let rows = response.rows.as_array().unwrap();
    assert!(!rows.is_empty());
    println!("demo: LLM → SQL '{}' → {} 行", response.sql, rows.len());
}

#[tokio::test]
async fn demo_nl2sql_e2e_degraded_mode() {
    let pipeline = NlQueryPipeline::new();

    let response = pipeline.query("查询所有订单").await.unwrap();
    assert!(response.sql.contains("SELECT"));
    assert!(response.sql.contains("orders"));
    let rows = response.rows.as_array().unwrap();
    assert!(rows.is_empty(), "未注入执行器时应返回空 rows");
    println!("demo: 降级模式 → SQL '{}' → 空 rows", response.sql);
}
