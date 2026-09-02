//! TASK-003 验证测试：危险操作人工确认

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use sz_orm_agent::approval::{ApprovalDecision, ApprovalGate};
use sz_orm_agent::tool::{
    index_creation::IndexCreationTool, query_execution::QueryExecutionTool, AgentTool,
};

fn make_gate() -> ApprovalGate {
    let tools: HashMap<String, Arc<dyn AgentTool>> = HashMap::from([
        (
            "query_execution".to_string(),
            Arc::new(QueryExecutionTool::new()) as Arc<dyn AgentTool>,
        ),
        (
            "index_creation".to_string(),
            Arc::new(IndexCreationTool) as Arc<dyn AgentTool>,
        ),
    ]);
    ApprovalGate::with_tools(tools, Duration::from_secs(300))
}

#[tokio::test]
async fn test_dangerous_operation_paused() {
    let gate = make_gate();
    let params = HashMap::from([
        ("table".to_string(), "orders".to_string()),
        ("columns".to_string(), "user_id".to_string()),
    ]);

    let request = gate
        .request_approval("task-1", 1, "index_creation", params)
        .await
        .unwrap();

    assert_eq!(request.tool_name, "index_creation");
    assert!(!request.impact_summary.is_empty());
}

#[tokio::test]
async fn test_rejected_operation_not_executed() {
    let gate = make_gate();
    let params = HashMap::from([
        ("table".to_string(), "orders".to_string()),
        ("columns".to_string(), "user_id".to_string()),
    ]);

    gate.request_approval("task-2", 1, "index_creation", params)
        .await
        .unwrap();

    let result = gate
        .resolve("task-2", 1, ApprovalDecision::Rejected)
        .await
        .unwrap();

    assert!(result.is_none(), "拒绝时操作不执行");
}

#[tokio::test]
async fn test_approved_operation_executed() {
    let gate = make_gate();
    let params = HashMap::from([
        ("table".to_string(), "orders".to_string()),
        ("columns".to_string(), "user_id".to_string()),
    ]);

    gate.request_approval("task-3", 1, "index_creation", params)
        .await
        .unwrap();

    let result = gate
        .resolve("task-3", 1, ApprovalDecision::Approved)
        .await
        .unwrap();

    assert!(result.is_some());
    assert!(result.unwrap().contains("CREATE INDEX"));
}

#[tokio::test]
async fn test_timeout_auto_cancel() {
    let gate = make_gate();
    let params = HashMap::from([
        ("table".to_string(), "orders".to_string()),
        ("columns".to_string(), "user_id".to_string()),
    ]);

    gate.request_approval("task-4", 1, "index_creation", params)
        .await
        .unwrap();

    let result = gate.resolve("task-4", 1, ApprovalDecision::Timeout).await;

    assert!(result.is_err(), "超时返回错误");
}
