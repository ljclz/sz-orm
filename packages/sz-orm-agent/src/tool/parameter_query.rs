//! 参数查询工具（Safe）— SQL 生成器 + 可选执行器
//!
//! 默认仅生成参数查询 SQL 语句（降级模式）。
//! 注入 `SqlExecutor` 后直接执行 SQL 返回结果。

use crate::tool::{AgentTool, RiskLevel, SqlExecutor};
use crate::types::AgentError;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// 参数查询工具
///
/// 生成参数查询 SQL。Safe 风险等级，无需审批。
/// 默认查询所有参数（name=all）。
/// 注入 `SqlExecutor` 后直接执行 SQL 返回结果。
pub struct ParameterQueryTool {
    executor: Option<Arc<dyn SqlExecutor>>,
}

impl ParameterQueryTool {
    pub fn new() -> Self {
        Self { executor: None }
    }

    pub fn with_executor(executor: Arc<dyn SqlExecutor>) -> Self {
        Self {
            executor: Some(executor),
        }
    }

    fn generate_sql(params: &HashMap<String, String>) -> Result<String, AgentError> {
        let default_name = "all".to_string();
        let param_name = params.get("name").unwrap_or(&default_name);

        if param_name.trim().is_empty() {
            return Err(AgentError::ToolExecutionFailed("name 不能为空".into()));
        }

        Ok(format!("SHOW PARAMETER {param_name}"))
    }
}

impl Default for ParameterQueryTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentTool for ParameterQueryTool {
    fn name(&self) -> &str {
        "parameter_query"
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Safe
    }

    async fn execute(&self, params: &HashMap<String, String>) -> Result<String, AgentError> {
        let sql = Self::generate_sql(params)?;

        if let Some(executor) = &self.executor {
            executor.execute_sql(&sql).await
        } else {
            Ok(sql)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_specific_param() {
        let tool = ParameterQueryTool::new();
        let params = HashMap::from([("name".to_string(), "max_connections".to_string())]);
        let result = tool.execute(&params).await.unwrap();
        assert_eq!(result, "SHOW PARAMETER max_connections");
    }

    #[tokio::test]
    async fn test_execute_default_all() {
        let tool = ParameterQueryTool::new();
        let params = HashMap::new();
        let result = tool.execute(&params).await.unwrap();
        assert_eq!(result, "SHOW PARAMETER all");
    }

    #[tokio::test]
    async fn test_execute_empty_name() {
        let tool = ParameterQueryTool::new();
        let params = HashMap::from([("name".to_string(), "  ".to_string())]);
        assert!(tool.execute(&params).await.is_err());
    }

    struct StubExecutor;

    #[async_trait]
    impl SqlExecutor for StubExecutor {
        async fn execute_sql(&self, sql: &str) -> Result<String, AgentError> {
            Ok(format!(r#"{{"executed":"{sql}"}}"#))
        }
    }

    #[tokio::test]
    async fn test_execute_with_executor() {
        let tool = ParameterQueryTool::with_executor(Arc::new(StubExecutor));
        let params = HashMap::from([("name".to_string(), "max_connections".to_string())]);
        let result = tool.execute(&params).await.unwrap();
        assert!(result.contains(r#""executed":"SHOW PARAMETER max_connections"#));
    }
}
