//! NL Query 核心类型

use serde::{Deserialize, Serialize};

/// NL 查询响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NlQueryResponse {
    pub sql: String,
    pub sql_explanation: String,
    pub rows: serde_json::Value,
    pub visualization: Option<VisualizationSpec>,
    pub insight: Option<String>,
    pub truncated: bool,
}

/// 可视化规格
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualizationSpec {
    pub chart_type: String,
    pub data: serde_json::Value,
}

/// NL Query 错误
#[derive(Debug, thiserror::Error)]
pub enum NlQueryError {
    #[error("NL2SQL 转换失败: {0}")]
    Nl2SqlFailed(String),
    #[error("SQL 注入检测失败")]
    SqlInjectionDetected,
    #[error("权限拒绝: {0}")]
    PermissionDenied(String),
    #[error("查询超时")]
    Timeout,
    #[error("行数超限")]
    RowLimitExceeded,
    #[error("DML 默认拒绝")]
    DmlDenied,
}
