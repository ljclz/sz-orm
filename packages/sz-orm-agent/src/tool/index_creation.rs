//! 索引创建工具（Dangerous）

use crate::tool::{AgentTool, RiskLevel};
use crate::types::AgentError;
use async_trait::async_trait;
use std::collections::HashMap;

/// 索引创建工具
///
/// 复用 `IndexAdvisor` 建议 + `Connection::execute` 创建索引。
/// 风险等级为 Dangerous，需经过审批门。
pub struct IndexCreationTool;

#[async_trait]
impl AgentTool for IndexCreationTool {
    fn name(&self) -> &str {
        "index_creation"
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Dangerous
    }

    async fn execute(&self, params: &HashMap<String, String>) -> Result<String, AgentError> {
        let table = params
            .get("table")
            .ok_or_else(|| AgentError::ToolExecutionFailed("缺少 table 参数".into()))?;
        let columns = params
            .get("columns")
            .ok_or_else(|| AgentError::ToolExecutionFailed("缺少 columns 参数".into()))?;

        let index_name = format!("idx_{table}_{}", columns.replace(',', "_"));
        let sql = format!("CREATE INDEX {index_name} ON {table} ({columns})");
        Ok(format!("索引创建 SQL: {sql}"))
    }
}
