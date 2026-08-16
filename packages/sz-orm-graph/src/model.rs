//! # Model — 声明式建模
//!
//! GraphNodeModel + GraphRelationModel + GraphPropertyDef

use crate::query::GraphResult;

/// 属性值类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphValueType {
    String,
    Integer,
    Float,
    Boolean,
    DateTime,
    Duration,
}

/// 图属性定义
#[derive(Debug, Clone)]
pub struct GraphPropertyDef {
    pub name: String,
    pub value_type: GraphValueType,
    pub nullable: bool,
    pub default: Option<serde_json::Value>,
}

impl GraphPropertyDef {
    pub fn new(name: &str, value_type: GraphValueType) -> Self {
        Self {
            name: name.to_string(),
            value_type,
            nullable: false,
            default: None,
        }
    }

    pub fn nullable(mut self) -> Self {
        self.nullable = true;
        self
    }

    pub fn with_default(mut self, value: serde_json::Value) -> Self {
        self.default = Some(value);
        self
    }
}

/// 关系方向
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationDirection {
    Outgoing,
    Incoming,
    Undirected,
}

/// 图节点模型（声明式建模）
#[derive(Debug, Clone)]
pub struct GraphNodeModel {
    pub label: String,
    pub properties: Vec<GraphPropertyDef>,
}

impl GraphNodeModel {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            properties: Vec::new(),
        }
    }

    pub fn property(mut self, name: &str, value_type: GraphValueType) -> Self {
        self.properties
            .push(GraphPropertyDef::new(name, value_type));
        self
    }

    /// 生成 MATCH 子句
    pub fn match_clause(&self, alias: &str) -> String {
        format!("MATCH ({}:{})", alias, self.label)
    }

    /// 生成 CREATE 子句
    pub fn create_clause(&self, alias: &str) -> String {
        let prop_names: Vec<String> = self
            .properties
            .iter()
            .map(|p| format!("{}: ${}", p.name, p.name))
            .collect();
        if prop_names.is_empty() {
            format!("CREATE ({}:{})", alias, self.label)
        } else {
            format!(
                "CREATE ({}:{} {{{}}})",
                alias,
                self.label,
                prop_names.join(", ")
            )
        }
    }
}

/// 图关系模型
#[derive(Debug, Clone)]
pub struct GraphRelationModel {
    pub rel_type: String,
    pub direction: RelationDirection,
    pub from_label: String,
    pub to_label: String,
    pub properties: Vec<GraphPropertyDef>,
}

impl GraphRelationModel {
    pub fn new(rel_type: &str, from_label: &str, to_label: &str) -> Self {
        Self {
            rel_type: rel_type.to_string(),
            direction: RelationDirection::Outgoing,
            from_label: from_label.to_string(),
            to_label: to_label.to_string(),
            properties: Vec::new(),
        }
    }

    pub fn direction(mut self, dir: RelationDirection) -> Self {
        self.direction = dir;
        self
    }

    pub fn property(mut self, name: &str, value_type: GraphValueType) -> Self {
        self.properties
            .push(GraphPropertyDef::new(name, value_type));
        self
    }

    /// 生成 MATCH 关系子句
    pub fn match_clause(&self, from: &str, rel: &str, to: &str) -> String {
        match self.direction {
            RelationDirection::Outgoing => format!(
                "MATCH ({from}:{from_label})-[{rel}:{rel_type}]->({to}:{to_label})",
                from = from,
                from_label = self.from_label,
                rel = rel,
                rel_type = self.rel_type,
                to = to,
                to_label = self.to_label
            ),
            RelationDirection::Incoming => format!(
                "MATCH ({from}:{from_label})<-[{rel}:{rel_type}]-({to}:{to_label})",
                from = from,
                from_label = self.from_label,
                rel = rel,
                rel_type = self.rel_type,
                to = to,
                to_label = self.to_label
            ),
            RelationDirection::Undirected => format!(
                "MATCH ({from}:{from_label})-[{rel}:{rel_type}]-({to}:{to_label})",
                from = from,
                from_label = self.from_label,
                rel = rel,
                rel_type = self.rel_type,
                to = to,
                to_label = self.to_label
            ),
        }
    }
}

/// 从 GraphResult 提取节点属性
pub fn extract_node_properties(result: &GraphResult) -> Option<&serde_json::Value> {
    result.as_node().map(|n| &n.properties)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::{GraphNode, GraphResult};

    #[test]
    fn test_graph_property_def_builders() {
        let prop = GraphPropertyDef::new("name", GraphValueType::String)
            .nullable()
            .with_default(serde_json::json!("unknown"));
        assert_eq!(prop.name, "name");
        assert!(prop.nullable);
        assert_eq!(prop.default, Some(serde_json::json!("unknown")));

        let non_nullable = GraphPropertyDef::new("age", GraphValueType::Integer);
        assert!(!non_nullable.nullable);
        assert!(non_nullable.default.is_none());
    }

    #[test]
    fn test_node_model_match_clause() {
        let model = GraphNodeModel::new("Person")
            .property("name", GraphValueType::String)
            .property("age", GraphValueType::Integer);
        let clause = model.match_clause("n");
        assert_eq!(clause, "MATCH (n:Person)");
    }

    #[test]
    fn test_node_model_create_clause_with_props() {
        let model = GraphNodeModel::new("Person")
            .property("name", GraphValueType::String)
            .property("age", GraphValueType::Integer);
        let clause = model.create_clause("n");
        assert!(clause.contains("CREATE (n:Person"));
        assert!(clause.contains("name: $name"));
        assert!(clause.contains("age: $age"));
    }

    #[test]
    fn test_node_model_create_clause_no_props() {
        let model = GraphNodeModel::new("EmptyLabel");
        let clause = model.create_clause("n");
        assert_eq!(clause, "CREATE (n:EmptyLabel)");
    }

    #[test]
    fn test_relation_model_match_outgoing() {
        let model = GraphRelationModel::new("KNOWS", "Person", "Person");
        let clause = model.match_clause("a", "r", "b");
        assert!(clause.contains("MATCH (a:Person)-[r:KNOWS]->(b:Person)"));
    }

    #[test]
    fn test_relation_model_match_incoming_and_undirected() {
        let incoming = GraphRelationModel::new("KNOWS", "Person", "Person")
            .direction(RelationDirection::Incoming);
        let clause = incoming.match_clause("a", "r", "b");
        assert!(clause.contains("<-[r:KNOWS]-"));

        let undirected = GraphRelationModel::new("KNOWS", "Person", "Person")
            .direction(RelationDirection::Undirected);
        let clause = undirected.match_clause("a", "r", "b");
        assert!(clause.contains("-[r:KNOWS]-"));
        assert!(!clause.contains("->"));
        assert!(!clause.contains("<-"));
    }

    #[test]
    fn test_extract_node_properties() {
        let node = GraphNode {
            id: "1".into(),
            labels: vec![],
            properties: serde_json::json!({"name": "Alice"}),
        };
        let result = GraphResult::Node { node };
        let props = extract_node_properties(&result);
        assert!(props.is_some());
        assert_eq!(props.unwrap()["name"], serde_json::json!("Alice"));

        let scalar = GraphResult::Scalar {
            value: serde_json::json!(42),
        };
        assert!(extract_node_properties(&scalar).is_none());
    }
}
