//! 索引创建工具（Dangerous）— DDL 生成器 + 可选执行器
//!
//! 默认仅生成 CREATE INDEX DDL 语句（降级模式）。
//! 注入 `SqlExecutor` 后直接执行 DDL 返回结果。
//! Dangerous 风险等级，需经过审批门后执行。

use crate::tool::{AgentTool, RiskLevel, SqlExecutor};
use crate::types::AgentError;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// 索引创建工具
///
/// 生成 CREATE INDEX DDL。Dangerous 风险等级，需审批。
/// 注入 `SqlExecutor` 后直接执行 DDL 返回结果。
pub struct IndexCreationTool {
    executor: Option<Arc<dyn SqlExecutor>>,
}

impl IndexCreationTool {
    pub fn new() -> Self {
        Self { executor: None }
    }

    pub fn with_executor(executor: Arc<dyn SqlExecutor>) -> Self {
        Self {
            executor: Some(executor),
        }
    }

    fn generate_ddl(params: &HashMap<String, String>) -> Result<String, AgentError> {
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

impl Default for IndexCreationTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentTool for IndexCreationTool {
    fn name(&self) -> &str {
        "index_creation"
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Dangerous
    }

    async fn execute(&self, params: &HashMap<String, String>) -> Result<String, AgentError> {
        let ddl = Self::generate_ddl(params)?;

        if let Some(executor) = &self.executor {
            executor.execute_sql(&ddl).await
        } else {
            Ok(ddl)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_generates_ddl() {
        let tool = IndexCreationTool::new();
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
        let tool = IndexCreationTool::new();
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
        let tool = IndexCreationTool::new();
        assert!(tool.execute(&HashMap::new()).await.is_err());
        let params = HashMap::from([("table".to_string(), "users".to_string())]);
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
        let tool = IndexCreationTool::with_executor(Arc::new(StubExecutor));
        let params = HashMap::from([
            ("table".to_string(), "users".to_string()),
            ("columns".to_string(), "email".to_string()),
        ]);
        let result = tool.execute(&params).await.unwrap();
        assert!(result.contains(r#""executed":"CREATE INDEX"#));
    }
}
