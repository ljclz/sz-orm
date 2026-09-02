//! 方向1 集成测试：Agent 工具接入真实 MySQL 数据库
//!
//! 验证 QueryExecutionTool 注入 SqlExecutor 后执行真实 SQL 返回 JSON 结果。
//! 需要 MySQL 运行在 127.0.0.1:3306，数据库 shop。
//! 通过环境变量 SZ_ORM_MYSQL_DSN 指定 DSN，未设置时跳过。

use async_trait::async_trait;
use sz_orm_agent::tool::{AgentTool, QueryExecutionTool, SqlExecutor, ToolRegistry};
use sz_orm_agent::types::AgentError;
use sz_orm_sqlx::any_driver::AnyPool;
use sz_orm_sqlx::sz_orm_core::Connection;
use std::collections::HashMap;
use std::sync::Arc;

/// MySQL SQL 执行器：通过 sz-orm-sqlx AnyPool 执行 SQL
struct MysqlExecutor {
    pool: AnyPool,
}

#[async_trait]
impl SqlExecutor for MysqlExecutor {
    async fn execute_sql(&self, sql: &str) -> Result<String, AgentError> {
        let mut conn = self
            .pool
            .create()
            .await
            .map_err(|e| AgentError::ToolExecutionFailed(format!("连接失败: {e}")))?;

        let rows = conn
            .query(sql)
            .await
            .map_err(|e| AgentError::ToolExecutionFailed(format!("查询失败: {e}")))?;

        let json_rows: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                let obj: serde_json::Map<String, serde_json::Value> = row
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::Value::String(format!("{v:?}"))))
                    .collect();
                serde_json::Value::Object(obj)
            })
            .collect();

        Ok(serde_json::to_string(&json_rows).unwrap_or_default())
    }
}

fn get_dsn() -> Option<String> {
    std::env::var("SZ_ORM_MYSQL_DSN").ok()
}

#[tokio::test]
async fn test_query_execution_tool_with_real_mysql() {
    let dsn = match get_dsn() {
        Some(d) => d,
        None => {
            eprintln!("跳过：未设置 SZ_ORM_MYSQL_DSN");
            return;
        }
    };

    let pool = AnyPool::connect(&dsn).await.expect("连接 MySQL 失败");
    let executor = Arc::new(MysqlExecutor { pool });
    let tool = QueryExecutionTool::with_executor(executor);

    let params = HashMap::from([(
        "sql".to_string(),
        "SELECT COUNT(*) as cnt FROM sz_user".to_string(),
    )]);

    let result = tool.execute(&params).await.expect("查询失败");
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("结果应为 JSON");
    let rows = parsed.as_array().expect("rows 应为数组");
    assert!(!rows.is_empty(), "应返回真实数据行");
    println!("QueryExecutionTool MySQL 结果: {}", result);
}

#[tokio::test]
async fn test_tool_registry_with_executor() {
    let dsn = match get_dsn() {
        Some(d) => d,
        None => {
            eprintln!("跳过：未设置 SZ_ORM_MYSQL_DSN");
            return;
        }
    };

    let pool = AnyPool::connect(&dsn).await.expect("连接 MySQL 失败");
    let executor = Arc::new(MysqlExecutor { pool });
    let registry = ToolRegistry::with_defaults_and_executor(executor);

    let params = HashMap::from([(
        "sql".to_string(),
        "SELECT user_id, nickname FROM sz_user LIMIT 3".to_string(),
    )]);

    let tool = registry.get("query_execution").expect("工具应存在");
    let result = tool.execute(&params).await.expect("查询失败");
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("结果应为 JSON");
    let rows = parsed.as_array().expect("rows 应为数组");
    assert_eq!(rows.len(), 3, "应返回 3 行用户数据");
    println!("ToolRegistry MySQL 结果: {} 行", rows.len());
}

#[tokio::test]
async fn test_query_execution_tool_without_executor_returns_sql() {
    let tool = QueryExecutionTool::new();
    let params = HashMap::from([(
        "sql".to_string(),
        "SELECT 1".to_string(),
    )]);
    let result = tool.execute(&params).await.unwrap();
    assert_eq!(result, "SELECT 1", "未注入执行器时应返回 SQL 字符串");
}