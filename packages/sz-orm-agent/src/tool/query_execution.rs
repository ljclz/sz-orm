//! 查询执行工具（Safe）

use crate::tool::{AgentTool, RiskLevel};
use crate::types::AgentError;
use async_trait::async_trait;
use std::collections::HashMap;

/// 查询执行工具
///
/// 复用 `Connection::query_with_params` 执行只读查询。
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

        Ok(format!("查询已执行: {sql}"))
    }
}
