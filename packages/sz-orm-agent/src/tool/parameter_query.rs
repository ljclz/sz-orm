//! 参数查询工具（Safe）— SQL 生成器
//!
//! 生成数据库参数查询 SQL 语句，不直接执行。
//! 实际执行由审批门通过后的 `ActionExecutor` 完成。

use crate::tool::{AgentTool, RiskLevel};
use crate::types::AgentError;
use async_trait::async_trait;
use std::collections::HashMap;

/// 参数查询工具
///
/// 生成参数查询 SQL。Safe 风险等级，无需审批。
/// 默认查询所有参数（name=all）。
pub struct ParameterQueryTool;

#[async_trait]
impl AgentTool for ParameterQueryTool {
    fn name(&self) -> &str {
        "parameter_query"
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Safe
    }

    async fn execute(&self, params: &HashMap<String, String>) -> Result<String, AgentError> {
        let default_name = "all".to_string();
        let param_name = params.get("name").unwrap_or(&default_name);

        if param_name.trim().is_empty() {
            return Err(AgentError::ToolExecutionFailed("name 不能为空".into()));
        }

        Ok(format!("SHOW PARAMETER {param_name}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_specific_param() {
        let tool = ParameterQueryTool;
        let params = HashMap::from([("name".to_string(), "max_connections".to_string())]);
        let result = tool.execute(&params).await.unwrap();
        assert_eq!(result, "SHOW PARAMETER max_connections");
    }

    #[tokio::test]
    async fn test_execute_default_all() {
        let tool = ParameterQueryTool;
        let params = HashMap::new();
        let result = tool.execute(&params).await.unwrap();
        assert_eq!(result, "SHOW PARAMETER all");
    }

    #[tokio::test]
    async fn test_execute_empty_name() {
        let tool = ParameterQueryTool;
        let params = HashMap::from([("name".to_string(), "  ".to_string())]);
        assert!(tool.execute(&params).await.is_err());
    }
}
