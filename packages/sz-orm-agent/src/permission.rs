//! 权限边界运行时拦截（TASK-004）

use crate::tool::{AgentTool, RiskLevel};
use crate::types::AgentError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};


/// 权限边界配置
#[derive(Debug, Clone)]
pub struct PermissionBoundary {
    /// 允许的工具集合
    pub allowed_tools: HashSet<String>,
    /// 只读模式
    pub readonly: bool,
}

impl PermissionBoundary {
    pub fn new(allowed_tools: HashSet<String>, readonly: bool) -> Self {
        Self {
            allowed_tools,
            readonly,
        }
    }

    /// 全权限（允许所有工具，非只读）
    pub fn full() -> Self {
        Self {
            allowed_tools: HashSet::new(),
            readonly: false,
        }
    }

    /// 只读权限
    pub fn readonly() -> Self {
        Self {
            allowed_tools: HashSet::new(),
            readonly: true,
        }
    }

    pub fn allow_tool(&mut self, tool_name: &str) {
        self.allowed_tools.insert(tool_name.to_string());
    }
}

/// 越权事件记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionViolation {
    pub tool_name: String,
    pub reason: String,
    pub timestamp: DateTime<Utc>,
}

/// 工具权限守卫
pub struct ToolPermissionGuard {
    boundary: PermissionBoundary,
    violations: Vec<PermissionViolation>,
}

impl ToolPermissionGuard {
    pub fn new(boundary: PermissionBoundary) -> Self {
        Self {
            boundary,
            violations: Vec::new(),
        }
    }

    /// 检查工具调用是否被允许
    pub fn check(&mut self, tool_name: &str, tool: &dyn AgentTool) -> bool {
        if self.boundary.readonly && tool.risk_level() == RiskLevel::Dangerous {
            self.violations.push(PermissionViolation {
                tool_name: tool_name.to_string(),
                reason: "只读模式下禁止危险操作".to_string(),
                timestamp: Utc::now(),
            });
            return false;
        }

        if !self.boundary.allowed_tools.is_empty()
            && !self.boundary.allowed_tools.contains(tool_name)
        {
            self.violations.push(PermissionViolation {
                tool_name: tool_name.to_string(),
                reason: "工具不在允许列表中".to_string(),
                timestamp: Utc::now(),
            });
            return false;
        }

        true
    }

    /// 守卫式工具调用
    pub async fn guarded_call(
        &mut self,
        tool_name: &str,
        tool: &dyn AgentTool,
        params: &HashMap<String, String>,
    ) -> Result<String, AgentError> {
        if !self.check(tool_name, tool) {
            return Err(AgentError::PermissionDenied(format!(
                "工具 {tool_name} 被权限边界拦截"
            )));
        }
        tool.execute(params).await
    }

    /// 获取越权记录
    pub fn violations(&self) -> &[PermissionViolation] {
        &self.violations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::index_creation::IndexCreationTool;
    use crate::tool::query_execution::QueryExecutionTool;

    #[tokio::test]
    async fn test_readonly_blocks_dangerous() {
        let mut guard = ToolPermissionGuard::new(PermissionBoundary::readonly());
        let tool = IndexCreationTool;
        let params = HashMap::from([
            ("table".to_string(), "users".to_string()),
            ("columns".to_string(), "email".to_string()),
        ]);

        let result = guard.guarded_call("index_creation", &tool, &params).await;
        assert!(result.is_err());
        assert_eq!(guard.violations().len(), 1);
        assert!(guard.violations()[0].reason.contains("只读"));
    }

    #[tokio::test]
    async fn test_readonly_allows_safe() {
        let mut guard = ToolPermissionGuard::new(PermissionBoundary::readonly());
        let tool = QueryExecutionTool::new();
        let params = HashMap::from([("sql".to_string(), "SELECT 1".to_string())]);

        let result = guard.guarded_call("query_execution", &tool, &params).await;
        assert!(result.is_ok());
        assert!(guard.violations().is_empty());
    }

    #[tokio::test]
    async fn test_allowed_tools_filter() {
        let boundary =
            PermissionBoundary::new(HashSet::from(["query_execution".to_string()]), false);
        let mut guard = ToolPermissionGuard::new(boundary);

        let safe_tool = QueryExecutionTool::new();
        let dangerous_tool = IndexCreationTool;

        guard.check("query_execution", &safe_tool);
        guard.check("index_creation", &dangerous_tool);

        assert_eq!(guard.violations().len(), 1);
        assert_eq!(guard.violations()[0].tool_name, "index_creation");
    }

    #[tokio::test]
    async fn test_full_permission_allows_all() {
        let mut guard = ToolPermissionGuard::new(PermissionBoundary::full());
        let safe_tool = QueryExecutionTool::new();
        let dangerous_tool = IndexCreationTool;

        assert!(guard.check("query_execution", &safe_tool));
        assert!(guard.check("index_creation", &dangerous_tool));
        assert!(guard.violations().is_empty());
    }
}
