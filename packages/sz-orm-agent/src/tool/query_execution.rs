//! 查询执行工具（Safe）— SQL 生成器 + 可选执行器
//!
//! 默认仅生成只读查询 SQL 语句（降级模式）。
//! 注入 `SqlExecutor` 后直接执行 SQL 返回 JSON 结果。
//! 实际执行由审批门通过后的 `ActionExecutor` 完成。

use crate::tool::{AgentTool, RiskLevel, SqlExecutor};
use crate::types::AgentError;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// 查询执行工具
///
/// 生成只读查询 SQL。Safe 风险等级，无需审批。
/// 注入 `SqlExecutor` 后直接执行 SQL 返回 JSON 结果。
pub struct QueryExecutionTool {
    executor: Option<Arc<dyn SqlExecutor>>,
}

impl QueryExecutionTool {
    /// 创建无执行器的实例（降级模式，仅返回 SQL 字符串）
    pub fn new() -> Self {
        Self { executor: None }
    }

    /// 创建注入执行器的实例（执行 SQL 返回 JSON 结果）
    pub fn with_executor(executor: Arc<dyn SqlExecutor>) -> Self {
        Self {
            executor: Some(executor),
        }
    }
}

impl Default for QueryExecutionTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentTool for QueryExecutionTool {
    fn name(&self) -> &str {
        "query_execution"
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Safe
    }

    async fn execute(&self, params: &HashMap<String, String>) -> Result<String, AgentError> {
        let sql = params
            .get("sql")
            .ok_or_else(|| AgentError::ToolExecutionFailed("缺少 sql 参数".into()))?;

        if sql.trim().is_empty() {
            return Err(AgentError::ToolExecutionFailed("SQL 不能为空".into()));
        }

        if let Some(executor) = &self.executor {
            executor.execute_sql(sql).await
        } else {
            Ok(sql.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_returns_sql() {
        let tool = QueryExecutionTool::new();
        let params = HashMap::from([("sql".to_string(), "SELECT * FROM users".to_string())]);
        let result = tool.execute(&params).await.unwrap();
        assert_eq!(result, "SELECT * FROM users");
    }

    #[tokio::test]
    async fn test_execute_missing_sql() {
        let tool = QueryExecutionTool::new();
        let params = HashMap::new();
        assert!(tool.execute(&params).await.is_err());
    }

    #[tokio::test]
    async fn test_execute_empty_sql() {
        let tool = QueryExecutionTool::new();
        let params = HashMap::from([("sql".to_string(), "  ".to_string())]);
        assert!(tool.execute(&params).await.is_err());
    }

    /// 桩执行器：返回固定 JSON 结果
    struct StubExecutor;

    #[async_trait]
    impl SqlExecutor for StubExecutor {
        async fn execute_sql(&self, sql: &str) -> Result<String, AgentError> {
            Ok(format!(r#"{{"sql":"{sql}","rows":[]}}"#))
        }
    }

    #[tokio::test]
    async fn test_execute_with_executor_returns_json() {
        let tool = QueryExecutionTool::with_executor(Arc::new(StubExecutor));
        let params = HashMap::from([("sql".to_string(), "SELECT 1".to_string())]);
        let result = tool.execute(&params).await.unwrap();
        assert!(result.contains(r#""sql":"SELECT 1""#));
        assert!(result.contains(r#""rows":[]"#));
    }
}
