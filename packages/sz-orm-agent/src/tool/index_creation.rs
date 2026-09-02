//! 索引创建工具（Dangerous）— DDL 生成器
//!
//! 生成 CREATE INDEX DDL 语句，不直接执行。
//! Dangerous 风险等级，需经过审批门后由 `ActionExecutor` 执行。

use crate::tool::{AgentTool, RiskLevel};
use crate::types::AgentError;
use async_trait::async_trait;
use std::collections::HashMap;

/// 索引创建工具
///
/// 生成 CREATE INDEX DDL。Dangerous 风险等级，需审批。
pub struct IndexCreationTool;

#[async_trait]
impl AgentTool for IndexCreationTool {
    fn name(&self) -> &str {
        "index_creation"
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Dangerous
    }

    async fn execute(&self, params: &HashMap<String, String>) -> Result<String, AgentError> {
        let table = params
            .get("table")
            .ok_or_else(|| AgentError::ToolExecutionFailed("缺少 table 参数".into()))?;
        let columns = params
            .get("columns")
            .ok_or_else(|| AgentError::ToolExecutionFailed("缺少 columns 参数".into()))?;

        if table.trim().is_empty() {
            return Err(AgentError::ToolExecutionFailed("table 不能为空".into()));
        }
        if columns.trim().is_empty() {
            return Err(AgentError::ToolExecutionFailed("columns 不能为空".into()));
        }

        let index_name = format!("idx_{table}_{}", columns.replace(',', "_"));
        Ok(format!("CREATE INDEX {index_name} ON {table} ({columns})"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_generates_ddl() {
        let tool = IndexCreationTool;
        let params = HashMap::from([
            ("table".to_string(), "users".to_string()),
            ("columns".to_string(), "email".to_string()),
        ]);
        let result = tool.execute(&params).await.unwrap();
        assert!(result.starts_with("CREATE INDEX"));
        assert!(result.contains("idx_users_email"));
        assert!(result.contains("ON users (email)"));
    }

    #[tokio::test]
    async fn test_execute_multi_columns() {
        let tool = IndexCreationTool;
        let params = HashMap::from([
            ("table".to_string(), "orders".to_string()),
            ("columns".to_string(), "user_id,status".to_string()),
        ]);
        let result = tool.execute(&params).await.unwrap();
        assert!(result.contains("idx_orders_user_id_status"));
        assert!(result.contains("user_id,status"));
    }

    #[tokio::test]
    async fn test_execute_missing_params() {
        let tool = IndexCreationTool;
        assert!(tool.execute(&HashMap::new()).await.is_err());
        let params = HashMap::from([("table".to_string(), "users".to_string())]);
        assert!(tool.execute(&params).await.is_err());
    }
}
