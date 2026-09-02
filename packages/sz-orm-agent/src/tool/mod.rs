//! Tool Use 工具调用协议

pub mod index_creation;
pub mod parameter_query;
pub mod query_execution;
pub mod stats_collection;

use crate::types::AgentError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;

pub use query_execution::QueryExecutionTool;

/// 工具风险等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Safe,
    Dangerous,
}

/// Agent 工具 trait
#[async_trait]
pub trait AgentTool: Send + Sync {
    /// 工具名称
    fn name(&self) -> &str;

    /// 风险等级
    fn risk_level(&self) -> RiskLevel;

    /// 执行工具
    async fn execute(&self, params: &HashMap<String, String>) -> Result<String, AgentError>;
}

/// SQL 执行器接口
///
/// 用户可实现此 trait 注入真实数据库连接，使工具不仅生成 SQL 还直接执行。
/// 未注入执行器时，工具仅返回生成的 SQL 字符串（降级模式）。
#[async_trait]
pub trait SqlExecutor: Send + Sync {
    /// 执行 SQL，返回 JSON 格式的结果
    async fn execute_sql(&self, sql: &str) -> Result<String, AgentError>;
}

/// 工具注册表
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn AgentTool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// 注册默认 4 类工具
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(query_execution::QueryExecutionTool::new()));
        registry.register(Arc::new(index_creation::IndexCreationTool::new()));
        registry.register(Arc::new(stats_collection::StatsCollectionTool::new()));
        registry.register(Arc::new(parameter_query::ParameterQueryTool::new()));
        registry
    }

    /// 注册默认 4 类工具并注入 SQL 执行器
    ///
    /// 注入后所有工具将直接执行 SQL 返回 JSON 结果，
    /// 而非仅返回 SQL 字符串。
    pub fn with_defaults_and_executor(executor: Arc<dyn SqlExecutor>) -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(
            query_execution::QueryExecutionTool::with_executor(executor.clone()),
        ));
        registry.register(Arc::new(index_creation::IndexCreationTool::with_executor(
            executor.clone(),
        )));
        registry.register(Arc::new(
            stats_collection::StatsCollectionTool::with_executor(executor.clone()),
        ));
        registry.register(Arc::new(
            parameter_query::ParameterQueryTool::with_executor(executor),
        ));
        registry
    }

    pub fn register(&mut self, tool: Arc<dyn AgentTool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn AgentTool>> {
        self.tools.get(name)
    }

    pub fn list(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// 调用工具并记录审计日志
    pub async fn call(
        &self,
        name: &str,
        params: &HashMap<String, String>,
        audit: &mut AuditLog,
    ) -> Result<String, AgentError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| AgentError::ToolExecutionFailed(format!("工具不存在: {name}")))?;

        let result = tool.execute(params).await;
        audit.record(name, params, &result);
        result
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 审计日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: DateTime<Utc>,
    pub tool_name: String,
    pub params: HashMap<String, String>,
    pub success: bool,
    pub result_summary: String,
    pub prev_hash: String,
    pub hash: String,
}

/// 审计日志（SHA-256 哈希链防篡改）
#[derive(Debug, Default)]
pub struct AuditLog {
    entries: Vec<AuditEntry>,
    last_hash: String,
}

impl AuditLog {
    pub fn new() -> Self {
        Self::default()
    }

    fn record(
        &mut self,
        tool_name: &str,
        params: &HashMap<String, String>,
        result: &Result<String, AgentError>,
    ) {
        let timestamp = Utc::now();
        let prev_hash = self.last_hash.clone();
        let (success, result_summary) = match result {
            Ok(r) => (true, r.chars().take(200).collect::<String>()),
            Err(e) => (false, e.to_string()),
        };

        let mut hasher = Sha256::new();
        hasher.update(timestamp.to_rfc3339().as_bytes());
        hasher.update(tool_name.as_bytes());
        hasher.update(serde_json::to_string(params).unwrap_or_default().as_bytes());
        hasher.update(result_summary.as_bytes());
        hasher.update(prev_hash.as_bytes());
        let hash = format!("{:x}", hasher.finalize());

        self.entries.push(AuditEntry {
            timestamp,
            tool_name: tool_name.to_string(),
            params: params.clone(),
            success,
            result_summary,
            prev_hash,
            hash: hash.clone(),
        });
        self.last_hash = hash;
    }

    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }

    /// 验证哈希链完整性
    pub fn verify(&self) -> bool {
        let mut prev = "";
        for entry in &self.entries {
            if entry.prev_hash != prev {
                return false;
            }
            let mut hasher = Sha256::new();
            hasher.update(entry.timestamp.to_rfc3339().as_bytes());
            hasher.update(entry.tool_name.as_bytes());
            hasher.update(
                serde_json::to_string(&entry.params)
                    .unwrap_or_default()
                    .as_bytes(),
            );
            hasher.update(entry.result_summary.as_bytes());
            hasher.update(entry.prev_hash.as_bytes());
            let expected = format!("{:x}", hasher.finalize());
            if entry.hash != expected {
                return false;
            }
            prev = &entry.hash;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_defaults() {
        let registry = ToolRegistry::with_defaults();
        assert!(registry.get("query_execution").is_some());
        assert!(registry.get("index_creation").is_some());
        assert!(registry.get("stats_collection").is_some());
        assert!(registry.get("parameter_query").is_some());
    }

    #[tokio::test]
    async fn test_audit_hash_chain() {
        let registry = ToolRegistry::with_defaults();
        let mut audit = AuditLog::new();
        let params = HashMap::from([("sql".to_string(), "SELECT 1".to_string())]);

        registry
            .call("query_execution", &params, &mut audit)
            .await
            .unwrap();

        assert_eq!(audit.entries().len(), 1);
        assert!(audit.verify(), "哈希链完整");
    }
}
