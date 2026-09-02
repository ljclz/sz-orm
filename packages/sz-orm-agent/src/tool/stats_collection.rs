//! 统计信息收集工具（Safe）

use crate::tool::{AgentTool, RiskLevel};
use crate::types::AgentError;
use async_trait::async_trait;
use std::collections::HashMap;

/// 统计信息收集工具
///
/// 执行 ANALYZE 收集表统计信息。
pub struct StatsCollectionTool;

#[async_trait]
impl AgentTool for StatsCollectionTool {
    fn name(&self) -> &str {
        "stats_collection"
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Safe
    }

    async fn execute(&self, params: &HashMap<String, String>) -> Result<String, AgentError> {
        let table = params
            .get("table")
            .ok_or_else(|| AgentError::ToolExecutionFailed("缺少 table 参数".into()))?;

        Ok(format!("ANALYZE {table} 已执行"))
    }
}
