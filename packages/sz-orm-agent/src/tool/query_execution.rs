//! 查询执行工具（Safe）— SQL 生成器
//!
//! 生成只读查询 SQL 语句，不直接执行数据库操作。
//! 实际执行由审批门通过后的 `ActionExecutor` 完成。

use crate::tool::{AgentTool, RiskLevel};
use crate::types::AgentError;
use async_trait::async_trait;
use std::collections::HashMap;

/// 查询执行工具
///
/// 生成只读查询 SQL。Safe 风险等级，无需审批。
pub struct QueryExecutionTool;

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

        Ok(sql.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_returns_sql() {
        let tool = QueryExecutionTool;
        let params = HashMap::from([("sql".to_string(), "SELECT * FROM users".to_string())]);
        let result = tool.execute(&params).await.unwrap();
        assert_eq!(result, "SELECT * FROM users");
    }

    #[tokio::test]
    async fn test_execute_missing_sql() {
        let tool = QueryExecutionTool;
        let params = HashMap::new();
        assert!(tool.execute(&params).await.is_err());
    }

    #[tokio::test]
    async fn test_execute_empty_sql() {
        let tool = QueryExecutionTool;
        let params = HashMap::from([("sql".to_string(), "  ".to_string())]);
        assert!(tool.execute(&params).await.is_err());
    }
}
