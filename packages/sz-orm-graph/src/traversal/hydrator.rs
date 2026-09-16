//! 图遍历结果水合器
//!
//! 将扁平 JOIN 结果转换为嵌套结构。

use std::collections::HashMap;

/// 嵌套结果节点
#[derive(Debug, Clone)]
pub struct NestedNode {
    /// 表名
    pub table: String,
    /// 字段值
    pub fields: HashMap<String, String>,
    /// 子节点（按关系名分组）
    pub children: HashMap<String, Vec<NestedNode>>,
}

impl NestedNode {
    /// 创建节点
    pub fn new(table: &str) -> Self {
        Self {
            table: table.to_string(),
            fields: HashMap::new(),
            children: HashMap::new(),
        }
    }

    /// 设置字段
    pub fn set_field(&mut self, key: &str, value: &str) {
        self.fields.insert(key.to_string(), value.to_string());
    }

    /// 添加子节点
    pub fn add_child(&mut self, relation: &str, node: NestedNode) {
        self.children
            .entry(relation.to_string())
            .or_default()
            .push(node);
    }
}

/// 图遍历结果水合器
pub struct GraphResultHydrator {
    /// 表名列表（按 JOIN 顺序）
    tables: Vec<String>,
}

impl GraphResultHydrator {
    /// 创建水合器
    pub fn new(tables: Vec<String>) -> Self {
        Self { tables }
    }

    /// 将扁平行水合为嵌套结构
    pub fn hydrate(&self, rows: &[HashMap<String, String>]) -> Vec<NestedNode> {
        if self.tables.is_empty() || rows.is_empty() {
            return Vec::new();
        }

        let root_table = &self.tables[0];
        let mut root_nodes: Vec<NestedNode> = Vec::new();
        let mut seen_root_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        for row in rows {
            let root_id = row
                .get(&format!("{}_id", root_table))
                .or_else(|| row.get("id"))
                .cloned()
                .unwrap_or_default();

            if !seen_root_ids.insert(root_id.clone()) {
                continue;
            }

            let mut node = NestedNode::new(root_table);
            for (key, value) in row {
                if !key.contains('_') || key.starts_with(&format!("{}_", root_table)) {
                    node.set_field(key, value);
                }
            }

            for (i, table) in self.tables.iter().enumerate().skip(1) {
                let prefix = format!("t{}_", i);
                let mut child = NestedNode::new(table);
                for (key, value) in row {
                    if key.starts_with(&prefix) {
                        let field = key.strip_prefix(&prefix).unwrap_or(key);
                        child.set_field(field, value);
                    }
                }
                if !child.fields.is_empty() {
                    node.add_child(table, child);
                }
            }

            root_nodes.push(node);
        }

        root_nodes
    }

    /// 水合为 JSON 字符串
    pub fn to_json(&self, nodes: &[NestedNode]) -> String {
        if nodes.is_empty() {
            return "[]".to_string();
        }
        format!(
            "[{}]",
            nodes
                .iter()
                .map(Self::node_to_json)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    fn node_to_json(node: &NestedNode) -> String {
        let mut parts = vec![format!("\"table\": \"{}\"", node.table)];
        let fields: Vec<String> = node
            .fields
            .iter()
            .map(|(k, v)| format!("\"{}\": \"{}\"", k, v))
            .collect();
        if !fields.is_empty() {
            parts.push(format!("\"fields\": {{{}}}", fields.join(", ")));
        }
        let children: Vec<String> = node
            .children
            .iter()
            .map(|(rel, nodes)| {
                format!(
                    "\"{}\": [{}]",
                    rel,
                    nodes
                        .iter()
                        .map(Self::node_to_json)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .collect();
        if !children.is_empty() {
            parts.push(format!("\"children\": {{{}}}", children.join(", ")));
        }
        format!("{{{}}}", parts.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hydrate_single_table() {
        let hydrator = GraphResultHydrator::new(vec!["users".into()]);
        let mut row = HashMap::new();
        row.insert("id".into(), "1".into());
        row.insert("name".into(), "Alice".into());
        let nodes = hydrator.hydrate(&[row]);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].table, "users");
        assert_eq!(nodes[0].fields.get("name"), Some(&"Alice".to_string()));
    }

    #[test]
    fn test_hydrate_with_children() {
        let hydrator = GraphResultHydrator::new(vec!["users".into(), "posts".into()]);
        let mut row = HashMap::new();
        row.insert("id".into(), "1".into());
        row.insert("name".into(), "Alice".into());
        row.insert("t1_title".into(), "Hello".into());
        let nodes = hydrator.hydrate(&[row]);
        assert_eq!(nodes.len(), 1);
        assert!(nodes[0].children.contains_key("posts"));
    }

    #[test]
    fn test_hydrate_deduplicates() {
        let hydrator = GraphResultHydrator::new(vec!["users".into()]);
        let mut row1 = HashMap::new();
        row1.insert("id".into(), "1".into());
        row1.insert("name".into(), "Alice".into());
        let mut row2 = HashMap::new();
        row2.insert("id".into(), "1".into());
        row2.insert("name".into(), "Alice".into());
        let nodes = hydrator.hydrate(&[row1, row2]);
        assert_eq!(nodes.len(), 1);
    }

    #[test]
    fn test_hydrate_empty() {
        let hydrator = GraphResultHydrator::new(vec!["users".into()]);
        let nodes = hydrator.hydrate(&[]);
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_to_json() {
        let mut node = NestedNode::new("users");
        node.set_field("name", "Alice");
        let json = GraphResultHydrator::new(vec!["users".into()]).to_json(&[node]);
        assert!(json.contains("Alice"));
        assert!(json.starts_with("["));
    }
}
