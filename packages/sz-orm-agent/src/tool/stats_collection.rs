//! 统计信息收集工具（Safe）— SQL 生成器
//!
//! 生成 ANALYZE SQL 语句，不直接执行。
//! 实际执行由审批门通过后的 `ActionExecutor` 完成。

use crate::tool::{AgentTool, RiskLevel};
use crate::types::AgentError;
use async_trait::async_trait;
use std::collections::HashMap;

/// 统计信息收集工具
///
/// 生成 ANALYZE SQL。Safe 风险等级，无需审批。
pub struct StatsCollectionTool;

#[async_trait]
impl AgentTool for StatsCollectionTool {
    fn name(&self) -> &str {
        "stats_collection"
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Safe
    }

    async fn execute(&self, params: &HashMap<String, String>) -> Result<String, AgentError> {
        let table = params
            .get("table")
            .ok_or_else(|| AgentError::ToolExecutionFailed("缺少 table 参数".into()))?;

        if table.trim().is_empty() {
            return Err(AgentError::ToolExecutionFailed("table 不能为空".into()));
        }

        Ok(format!("ANALYZE {table}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_generates_analyze() {
        let tool = StatsCollectionTool;
        let params = HashMap::from([("table".to_string(), "orders".to_string())]);
        let result = tool.execute(&params).await.unwrap();
        assert_eq!(result, "ANALYZE orders");
    }

    #[tokio::test]
    async fn test_execute_missing_table() {
        let tool = StatsCollectionTool;
        assert!(tool.execute(&HashMap::new()).await.is_err());
    }

    #[tokio::test]
    async fn test_execute_empty_table() {
        let tool = StatsCollectionTool;
        let params = HashMap::from([("table".to_string(), "  ".to_string())]);
        assert!(tool.execute(&params).await.is_err());
    }
}
