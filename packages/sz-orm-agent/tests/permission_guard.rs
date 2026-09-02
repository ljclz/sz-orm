//! TASK-004 验证测试：权限边界运行时拦截

use std::collections::{HashMap, HashSet};
use sz_orm_agent::permission::{PermissionBoundary, ToolPermissionGuard};
use sz_orm_agent::tool::{index_creation::IndexCreationTool, query_execution::QueryExecutionTool};

#[tokio::test]
async fn test_readonly_blocks_dangerous() {
    let mut guard = ToolPermissionGuard::new(PermissionBoundary::readonly());
    let tool = IndexCreationTool::new();
    let params = HashMap::from([
        ("table".to_string(), "users".to_string()),
        ("columns".to_string(), "email".to_string()),
    ]);

    let result = guard.guarded_call("index_creation", &tool, &params).await;
    assert!(result.is_err(), "只读模式拦截危险操作");
    assert_eq!(guard.violations().len(), 1);
    assert!(guard.violations()[0].reason.contains("只读"));
}

#[tokio::test]
async fn test_readonly_allows_safe() {
    let mut guard = ToolPermissionGuard::new(PermissionBoundary::readonly());
    let tool = QueryExecutionTool::new();
    let params = HashMap::from([("sql".to_string(), "SELECT 1".to_string())]);

    let result = guard.guarded_call("query_execution", &tool, &params).await;
    assert!(result.is_ok(), "只读模式允许安全操作");
    assert!(guard.violations().is_empty());
}

#[tokio::test]
async fn test_allowed_tools_filter() {
    let boundary = PermissionBoundary::new(HashSet::from(["query_execution".to_string()]), false);
    let mut guard = ToolPermissionGuard::new(boundary);

    let safe_tool = QueryExecutionTool::new();
    let dangerous_tool = IndexCreationTool::new();


    guard.check("query_execution", &safe_tool);
    guard.check("index_creation", &dangerous_tool);

    assert_eq!(guard.violations().len(), 1);
    assert_eq!(guard.violations()[0].tool_name, "index_creation");
}
