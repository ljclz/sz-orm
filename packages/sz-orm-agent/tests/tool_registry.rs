//! TASK-002 验证测试：工具注册与调用

use std::collections::HashMap;
use std::sync::Arc;
use sz_orm_agent::tool::{AgentTool, RiskLevel, ToolRegistry};

#[tokio::test]
async fn test_default_tools_registered() {
    let registry = ToolRegistry::with_defaults();
    assert_eq!(registry.list().len(), 4);
    assert!(registry.get("query_execution").is_some());
    assert!(registry.get("index_creation").is_some());
    assert!(registry.get("stats_collection").is_some());
    assert!(registry.get("parameter_query").is_some());
}

#[tokio::test]
async fn test_tool_risk_levels() {
    let registry = ToolRegistry::with_defaults();

    let query_tool = registry.get("query_execution").unwrap();
    assert_eq!(query_tool.risk_level(), RiskLevel::Safe);

    let index_tool = registry.get("index_creation").unwrap();
    assert_eq!(index_tool.risk_level(), RiskLevel::Dangerous);

    let stats_tool = registry.get("stats_collection").unwrap();
    assert_eq!(stats_tool.risk_level(), RiskLevel::Safe);

    let param_tool = registry.get("parameter_query").unwrap();
    assert_eq!(param_tool.risk_level(), RiskLevel::Safe);
}

#[tokio::test]
async fn test_query_execution_tool() {
    let registry = ToolRegistry::with_defaults();
    let tool = registry.get("query_execution").unwrap();
    let params = HashMap::from([("sql".to_string(), "SELECT 1".to_string())]);
    let result = tool.execute(&params).await.unwrap();
    assert!(result.contains("SELECT 1"));
}

#[tokio::test]
async fn test_index_creation_tool() {
    let registry = ToolRegistry::with_defaults();
    let tool = registry.get("index_creation").unwrap();
    let params = HashMap::from([
        ("table".to_string(), "users".to_string()),
        ("columns".to_string(), "email".to_string()),
    ]);
    let result = tool.execute(&params).await.unwrap();
    assert!(result.contains("CREATE INDEX"));
    assert!(result.contains("users"));
}

#[tokio::test]
async fn test_stats_collection_tool() {
    let registry = ToolRegistry::with_defaults();
    let tool = registry.get("stats_collection").unwrap();
    let params = HashMap::from([("table".to_string(), "orders".to_string())]);
    let result = tool.execute(&params).await.unwrap();
    assert!(result.contains("ANALYZE"));
}

#[tokio::test]
async fn test_parameter_query_tool() {
    let registry = ToolRegistry::with_defaults();
    let tool = registry.get("parameter_query").unwrap();
    let params = HashMap::from([("name".to_string(), "max_connections".to_string())]);
    let result = tool.execute(&params).await.unwrap();
    assert!(result.contains("max_connections"));
}

#[tokio::test]
async fn test_custom_tool_registration() {
    use sz_orm_agent::types::AgentError;

    struct CustomTool;

    #[async_trait::async_trait]
    impl AgentTool for CustomTool {
        fn name(&self) -> &str {
            "custom"
        }
        fn risk_level(&self) -> RiskLevel {
            RiskLevel::Safe
        }
        async fn execute(&self, _params: &HashMap<String, String>) -> Result<String, AgentError> {
            Ok("custom result".to_string())
        }
    }

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(CustomTool));
    assert!(registry.get("custom").is_some());
}
