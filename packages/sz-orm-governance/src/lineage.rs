//! 字段级数据血缘构建与变更追踪（TASK-006 + TASK-022）

use crate::types::GovernanceError;
use serde::{Deserialize, Serialize};

/// 血缘节点
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LineageNode {
    pub node_id: String,
    pub table_name: String,
    pub column_name: String,
    pub source_columns: Vec<String>,
}

/// 血缘图
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LineageGraph {
    pub nodes: Vec<LineageNode>,
}

/// 血缘变更差异（TASK-022）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LineageDiff {
    pub added: Vec<LineageNode>,
    pub removed: Vec<LineageNode>,
    pub modified: Vec<LineageNode>,
}

/// 血缘变更检查点（TASK-022 断点续传）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LineageCheckpoint {
    pub version: u64,
    pub graph: LineageGraph,
    pub source_sql_hash: String,
}

/// 血缘构建器
pub struct LineageBuilder;

impl LineageBuilder {
    pub fn new() -> Self {
        Self
    }

    /// 从 SQL 构建血缘图（解析 SELECT 列引用）
    pub fn build_from_sql(&self, sql: &str) -> Result<LineageGraph, GovernanceError> {
        let nodes = Self::parse_select_columns(sql)?;
        Ok(LineageGraph { nodes })
    }

    /// 增量更新血缘并返回差异（TASK-022）
    pub fn update_lineage(
        &self,
        old: &LineageGraph,
        new_sql: &str,
    ) -> Result<(LineageGraph, LineageDiff), GovernanceError> {
        let new_graph = self.build_from_sql(new_sql)?;
        let diff = Self::compute_diff(old, &new_graph);
        Ok((new_graph, diff))
    }

    /// 从检查点恢复并继续构建（TASK-022 断点续传）
    pub fn resume_from_checkpoint(
        &self,
        checkpoint: &LineageCheckpoint,
        new_sql: &str,
    ) -> Result<(LineageGraph, LineageDiff, LineageCheckpoint), GovernanceError> {
        let (new_graph, diff) = self.update_lineage(&checkpoint.graph, new_sql)?;
        let new_checkpoint = LineageCheckpoint {
            version: checkpoint.version + 1,
            graph: new_graph.clone(),
            source_sql_hash: Self::hash_sql(new_sql),
        };
        Ok((new_graph, diff, new_checkpoint))
    }

    /// 创建初始检查点
    pub fn create_checkpoint(&self, graph: &LineageGraph, sql: &str) -> LineageCheckpoint {
        LineageCheckpoint {
            version: 1,
            graph: graph.clone(),
            source_sql_hash: Self::hash_sql(sql),
        }
    }

    fn compute_diff(old: &LineageGraph, new: &LineageGraph) -> LineageDiff {
        let old_map: std::collections::HashMap<&String, &LineageNode> =
            old.nodes.iter().map(|n| (&n.node_id, n)).collect();
        let new_map: std::collections::HashMap<&String, &LineageNode> =
            new.nodes.iter().map(|n| (&n.node_id, n)).collect();

        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut modified = Vec::new();

        for node in &new.nodes {
            match old_map.get(&node.node_id) {
                None => added.push(node.clone()),
                Some(old_node) if *old_node != node => modified.push(node.clone()),
                _ => {}
            }
        }
        for node in &old.nodes {
            if !new_map.contains_key(&node.node_id) {
                removed.push(node.clone());
            }
        }

        LineageDiff {
            added,
            removed,
            modified,
        }
    }

    fn parse_select_columns(sql: &str) -> Result<Vec<LineageNode>, GovernanceError> {
        let upper = sql.to_uppercase();
        if !upper.contains("SELECT") {
            return Err(GovernanceError::LineageBuildFailed(
                "SQL 缺少 SELECT".to_string(),
            ));
        }

        let select_start = upper.find("SELECT").unwrap() + 6;
        let from_pos = upper.find(" FROM ").unwrap_or(upper.len());
        let columns_part = sql[select_start..from_pos].trim();

        let table = Self::extract_table(sql);
        let mut nodes = Vec::new();

        for (idx, col) in columns_part.split(',').enumerate() {
            let col = col.trim();
            if col == "*" || col.is_empty() {
                continue;
            }
            let col_name = Self::extract_column_name(col);
            let source = Self::extract_source_column(col);
            nodes.push(LineageNode {
                node_id: format!("{}_{}", table, idx),
                table_name: table.clone(),
                column_name: col_name,
                source_columns: source,
            });
        }
        Ok(nodes)
    }

    fn extract_table(sql: &str) -> String {
        let upper = sql.to_uppercase();
        if let Some(pos) = upper.find(" FROM ") {
            let after = sql[pos + 6..].trim();
            let table: String = after
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != ';' && *c != ')')
                .collect();
            table
        } else {
            "unknown".to_string()
        }
    }

    fn extract_column_name(col: &str) -> String {
        if let Some(as_pos) = col.to_uppercase().rfind(" AS ") {
            col[as_pos + 4..].trim().to_string()
        } else {
            col.split('.').next_back().unwrap_or(col).trim().to_string()
        }
    }

    fn extract_source_column(col: &str) -> Vec<String> {
        let base = if let Some(as_pos) = col.to_uppercase().rfind(" AS ") {
            col[..as_pos].trim()
        } else {
            col.trim()
        };
        if base.contains('.') {
            vec![base.to_string()]
        } else {
            Vec::new()
        }
    }

    fn hash_sql(sql: &str) -> String {
        let mut hash: u64 = 0;
        for byte in sql.as_bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(*byte as u64);
        }
        format!("{hash:016x}")
    }

    /// 敏感字段集合
    fn sensitive_fields() -> &'static [&'static str] {
        &[
            "password",
            "pwd",
            "secret",
            "token",
            "api_key",
            "id_card",
            "idcard",
            "identity",
            "phone",
            "mobile",
            "tel",
            "email",
            "mail",
            "ssn",
            "credit_card",
        ]
    }

    /// 校验血缘图中不泄露敏感数据（TASK-037）
    pub fn verify_no_sensitive_leak(
        &self,
        graph: &LineageGraph,
        allowed_destinations: &[&str],
    ) -> Result<(), GovernanceError> {
        let sensitive = Self::sensitive_fields();
        let allowed: std::collections::HashSet<&str> =
            allowed_destinations.iter().copied().collect();

        for node in &graph.nodes {
            let col_lower = node.column_name.to_lowercase();
            let is_sensitive = sensitive.iter().any(|s| col_lower.contains(s));

            if is_sensitive && !allowed.contains(node.table_name.as_str()) {
                return Err(GovernanceError::ComplianceAuditFailed(format!(
                    "敏感字段 '{}' 出现在非授权目标 '{}' 中",
                    node.column_name, node.table_name
                )));
            }

            for source in &node.source_columns {
                let source_lower = source.to_lowercase();
                let source_is_sensitive = sensitive.iter().any(|s| source_lower.contains(s));
                if source_is_sensitive && !allowed.contains(node.table_name.as_str()) {
                    return Err(GovernanceError::ComplianceAuditFailed(format!(
                        "敏感源字段 '{}' 流向非授权目标 '{}'",
                        source, node.table_name
                    )));
                }
            }
        }

        Ok(())
    }
}

impl Default for LineageBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_from_select() {
        let builder = LineageBuilder::new();
        let graph = builder
            .build_from_sql("SELECT id, name FROM users")
            .unwrap();
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.nodes[0].table_name, "users");
        assert_eq!(graph.nodes[0].column_name, "id");
    }

    #[test]
    fn test_build_with_alias() {
        let builder = LineageBuilder::new();
        let graph = builder
            .build_from_sql("SELECT u.id AS user_id FROM users u")
            .unwrap();
        assert_eq!(graph.nodes[0].column_name, "user_id");
        assert_eq!(graph.nodes[0].source_columns, vec!["u.id".to_string()]);
    }

    #[test]
    fn test_update_lineage_diff() {
        let builder = LineageBuilder::new();
        let old = builder
            .build_from_sql("SELECT id, name FROM users")
            .unwrap();
        let (new, diff) = builder
            .update_lineage(&old, "SELECT id, name, email FROM users")
            .unwrap();
        assert!(!diff.added.is_empty(), "email 列应被新增");
        assert_eq!(new.nodes.len(), 3);
    }

    #[test]
    fn test_checkpoint_resume() {
        let builder = LineageBuilder::new();
        let graph = builder.build_from_sql("SELECT id FROM users").unwrap();
        let checkpoint = builder.create_checkpoint(&graph, "SELECT id FROM users");
        let (new_graph, diff, new_checkpoint) = builder
            .resume_from_checkpoint(&checkpoint, "SELECT id, name FROM users")
            .unwrap();
        assert!(!diff.added.is_empty());
        assert_eq!(new_checkpoint.version, 2);
        assert_eq!(new_graph.nodes.len(), 2);
    }

    #[test]
    fn test_diff_empty_on_same_sql() {
        let builder = LineageBuilder::new();
        let old = builder
            .build_from_sql("SELECT id, name FROM users")
            .unwrap();
        let (_, diff) = builder
            .update_lineage(&old, "SELECT id, name FROM users")
            .unwrap();
        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
        assert!(diff.modified.is_empty());
    }

    #[test]
    fn test_verify_no_sensitive_leak_pass() {
        let builder = LineageBuilder::new();
        let graph = builder
            .build_from_sql("SELECT id, name FROM users")
            .unwrap();
        let result = builder.verify_no_sensitive_leak(&graph, &["users"]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_sensitive_leak_detected() {
        let graph = LineageGraph {
            nodes: vec![LineageNode {
                node_id: "1".to_string(),
                table_name: "report".to_string(),
                column_name: "password".to_string(),
                source_columns: vec![],
            }],
        };
        let builder = LineageBuilder::new();
        let result = builder.verify_no_sensitive_leak(&graph, &["users"]);
        assert!(result.is_err(), "password 出现在非授权表应报错");
    }

    #[test]
    fn test_verify_sensitive_source_leak() {
        let graph = LineageGraph {
            nodes: vec![LineageNode {
                node_id: "1".to_string(),
                table_name: "report".to_string(),
                column_name: "data".to_string(),
                source_columns: vec!["users.email".to_string()],
            }],
        };
        let builder = LineageBuilder::new();
        let result = builder.verify_no_sensitive_leak(&graph, &["users"]);
        assert!(result.is_err(), "email 流向非授权表应报错");
    }

    #[test]
    fn test_verify_sensitive_allowed_destination() {
        let graph = LineageGraph {
            nodes: vec![LineageNode {
                node_id: "1".to_string(),
                table_name: "secure_store".to_string(),
                column_name: "phone".to_string(),
                source_columns: vec![],
            }],
        };
        let builder = LineageBuilder::new();
        let result = builder.verify_no_sensitive_leak(&graph, &["secure_store"]);
        assert!(result.is_ok(), "phone 在授权表应通过");
    }
}
