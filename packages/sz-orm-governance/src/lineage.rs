//! 字段级数据血缘构建（TASK-006 占位，后续实现）
#![allow(dead_code)]

use crate::types::GovernanceError;
use serde::{Deserialize, Serialize};

/// 血缘节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageNode {
    pub node_id: String,
    pub table_name: String,
    pub column_name: String,
    pub source_columns: Vec<String>,
}

/// 血缘图
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageGraph {
    pub nodes: Vec<LineageNode>,
}

/// 血缘构建器
pub struct LineageBuilder;

impl LineageBuilder {
    pub fn new() -> Self {
        Self
    }

    pub fn build_from_sql(&self, _sql: &str) -> Result<LineageGraph, GovernanceError> {
        Ok(LineageGraph { nodes: Vec::new() })
    }
}

impl Default for LineageBuilder {
    fn default() -> Self {
        Self::new()
    }
}
