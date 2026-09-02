//! 危险操作人工确认（TASK-003）

use crate::tool::{AgentTool, RiskLevel};
use crate::types::AgentError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// 审批请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub task_id: String,
    pub step_number: usize,
    pub tool_name: String,
    pub params: HashMap<String, String>,
    pub risk_level: RiskLevel,
    pub impact_summary: String,
    pub created_at: DateTime<Utc>,
    pub timeout_secs: u64,
}

/// 审批决策
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDecision {
    Approved,
    Rejected,
    Timeout,
}

/// 审批门
pub struct ApprovalGate {
    registry: Arc<HashMap<String, Arc<dyn AgentTool>>>,
    pending: Mutex<HashMap<String, ApprovalRequest>>,
    timeout: Duration,
}

impl ApprovalGate {
    pub fn new(timeout: Duration) -> Self {
        Self {
            registry: Arc::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            timeout,
        }
    }

    pub fn with_tools(tools: HashMap<String, Arc<dyn AgentTool>>, timeout: Duration) -> Self {
        Self {
            registry: Arc::new(tools),
            pending: Mutex::new(HashMap::new()),
            timeout,
        }
    }

    /// 检查操作是否需要审批
    pub fn needs_approval(&self, tool_name: &str) -> bool {
        self.registry
            .get(tool_name)
            .map(|t| t.risk_level() == RiskLevel::Dangerous)
            .unwrap_or(false)
    }

    /// 请求审批危险操作
    pub async fn request_approval(
        &self,
        task_id: &str,
        step_number: usize,
        tool_name: &str,
        params: HashMap<String, String>,
    ) -> Result<ApprovalRequest, AgentError> {
        let tool = self
            .registry
            .get(tool_name)
            .ok_or_else(|| AgentError::ToolExecutionFailed(format!("工具不存在: {tool_name}")))?;

        if tool.risk_level() != RiskLevel::Dangerous {
            return Err(AgentError::ToolExecutionFailed("非危险操作无需审批".into()));
        }

        let request = ApprovalRequest {
            task_id: task_id.to_string(),
            step_number,
            tool_name: tool_name.to_string(),
            params: params.clone(),
            risk_level: RiskLevel::Dangerous,
            impact_summary: format!("工具 {tool_name} 将执行危险操作"),
            created_at: Utc::now(),
            timeout_secs: self.timeout.as_secs(),
        };

        let key = format!("{task_id}:{step_number}");
        self.pending.lock().await.insert(key, request.clone());

        Ok(request)
    }

    /// 处理审批决策
    pub async fn resolve(
        &self,
        task_id: &str,
        step_number: usize,
        decision: ApprovalDecision,
    ) -> Result<Option<String>, AgentError> {
        let key = format!("{task_id}:{step_number}");
        let request = {
            let pending = self.pending.lock().await;
            pending
                .get(&key)
                .cloned()
                .ok_or_else(|| AgentError::TaskNotFound(key.clone()))?
        };

        match decision {
            ApprovalDecision::Approved => {
                let tool = self
                    .registry
                    .get(&request.tool_name)
                    .ok_or_else(|| AgentError::ToolExecutionFailed("工具不存在".into()))?;
                let result = tool.execute(&request.params).await?;
                Ok(Some(result))
            }
            ApprovalDecision::Rejected => Ok(None),
            ApprovalDecision::Timeout => Err(AgentError::ApprovalTimeout(key)),
        }
    }

    /// 检查是否有超时的审批请求
    pub async fn check_timeouts(&self) -> Vec<String> {
        let now = Utc::now();
        let timeout_secs = self.timeout.as_secs();
        let pending = self.pending.lock().await;

        pending
            .iter()
            .filter(|(_, req)| (now - req.created_at).num_seconds() as u64 > timeout_secs)
            .map(|(key, _)| key.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::index_creation::IndexCreationTool;
    use crate::tool::query_execution::QueryExecutionTool;

    #[tokio::test]
    async fn test_dangerous_needs_approval() {
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
        let gate = ApprovalGate::with_tools(tools, Duration::from_secs(300));

        assert!(!gate.needs_approval("query_execution"));
        assert!(gate.needs_approval("index_creation"));
    }

    #[tokio::test]
    async fn test_approval_flow() {
        let tools: HashMap<String, Arc<dyn AgentTool>> = HashMap::from([(
            "index_creation".to_string(),
            Arc::new(IndexCreationTool) as Arc<dyn AgentTool>,
        )]);
        let gate = ApprovalGate::with_tools(tools, Duration::from_secs(300));

        let params = HashMap::from([
            ("table".to_string(), "users".to_string()),
            ("columns".to_string(), "email".to_string()),
        ]);

        let request = gate
            .request_approval("task-1", 1, "index_creation", params)
            .await
            .unwrap();
        assert_eq!(request.risk_level, RiskLevel::Dangerous);

        let result = gate
            .resolve("task-1", 1, ApprovalDecision::Approved)
            .await
            .unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("CREATE INDEX"));
    }

    #[tokio::test]
    async fn test_rejection_skips_execution() {
        let tools: HashMap<String, Arc<dyn AgentTool>> = HashMap::from([(
            "index_creation".to_string(),
            Arc::new(IndexCreationTool) as Arc<dyn AgentTool>,
        )]);
        let gate = ApprovalGate::with_tools(tools, Duration::from_secs(300));

        let params = HashMap::from([
            ("table".to_string(), "users".to_string()),
            ("columns".to_string(), "id".to_string()),
        ]);

        gate.request_approval("task-2", 1, "index_creation", params)
            .await
            .unwrap();

        let result = gate
            .resolve("task-2", 1, ApprovalDecision::Rejected)
            .await
            .unwrap();
        assert!(result.is_none(), "拒绝时不执行操作");
    }
}
