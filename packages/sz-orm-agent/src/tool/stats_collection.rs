//! 统计信息收集工具（Safe）— SQL 生成器 + 可选执行器
//!
//! 默认仅生成 ANALYZE SQL 语句（降级模式）。
//! 注入 `SqlExecutor` 后直接执行 SQL 返回结果。

use crate::tool::{AgentTool, RiskLevel, SqlExecutor};
use crate::types::AgentError;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// 统计信息收集工具
///
/// 生成 ANALYZE SQL。Safe 风险等级，无需审批。
/// 注入 `SqlExecutor` 后直接执行 SQL 返回结果。
pub struct StatsCollectionTool {
    executor: Option<Arc<dyn SqlExecutor>>,
}

impl StatsCollectionTool {
    pub fn new() -> Self {
        Self { executor: None }
    }

    pub fn with_executor(executor: Arc<dyn SqlExecutor>) -> Self {
        Self {
            executor: Some(executor),
        }
    }

    fn generate_sql(params: &HashMap<String, String>) -> Result<String, AgentError> {
        let table = params
            .get("table")
            .ok_or_else(|| AgentError::ToolExecutionFailed("缺少 table 参数".into()))?;

        if table.trim().is_empty() {
            return Err(AgentError::ToolExecutionFailed("table 不能为空".into()));
        }

        Ok(format!("ANALYZE {table}"))
    }
}

impl Default for StatsCollectionTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentTool for StatsCollectionTool {
    fn name(&self) -> &str {
        "stats_collection"
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
    async fn test_execute_generates_analyze() {
        let tool = StatsCollectionTool::new();
        let params = HashMap::from([("table".to_string(), "orders".to_string())]);
        let result = tool.execute(&params).await.unwrap();
        assert_eq!(result, "ANALYZE orders");
    }

    #[tokio::test]
    async fn test_execute_missing_table() {
        let tool = StatsCollectionTool::new();
        assert!(tool.execute(&HashMap::new()).await.is_err());
    }

    #[tokio::test]
    async fn test_execute_empty_table() {
        let tool = StatsCollectionTool::new();
        let params = HashMap::from([("table".to_string(), "  ".to_string())]);
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
        let tool = StatsCollectionTool::with_executor(Arc::new(StubExecutor));
        let params = HashMap::from([("table".to_string(), "orders".to_string())]);
        let result = tool.execute(&params).await.unwrap();
        assert!(result.contains(r#""executed":"ANALYZE orders"#));
    }
}
