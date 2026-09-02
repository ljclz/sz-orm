//! 参数查询工具（Safe）

use crate::tool::{AgentTool, RiskLevel};
use crate::types::AgentError;
use async_trait::async_trait;
use std::collections::HashMap;

/// 参数查询工具
///
/// 查询数据库参数配置。
pub struct ParameterQueryTool;

#[async_trait]
impl AgentTool for ParameterQueryTool {
    fn name(&self) -> &str {
        "parameter_query"
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Safe
    }

    async fn execute(&self, params: &HashMap<String, String>) -> Result<String, AgentError> {
        let default_name = "all".to_string();
        let param_name = params.get("name").unwrap_or(&default_name);

        Ok(format!("参数 {param_name} 查询完成"))
    }
}
