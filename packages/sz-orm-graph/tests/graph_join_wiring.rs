//! W3-4 GRAPH-JOIN-01：图查询联合投影接线测试
//!
//! 验证 `JointProjection::query_with_projection` 端到端接线：
//! - 图查询返回节点主键 + 关系字段投影 → 断言结果含图结构 + 关系字段值
//!
//! 生产入口：`JointProjection::query_with_projection`（packages/sz-orm-graph/src/joint_projection.rs）

use std::collections::HashMap;
use std::time::Duration;
use sz_orm_graph::joint_projection::{
    FieldProjection, GraphEdge, GraphNode, JointProjection, JointResult, RelationalFieldLoader,
    RelationalFields,
};

/// 内存 mock 关系字段加载器
struct MockRelationalLoader {
    /// 表 → (节点 ID → (字段名 → 字段值))
    data: HashMap<String, HashMap<String, HashMap<String, String>>>,
}

impl MockRelationalLoader {
    fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    fn with_row(mut self, table: &str, id: &str, fields: Vec<(&str, &str)>) -> Self {
        let row: HashMap<String, String> = fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        self.data
            .entry(table.to_string())
            .or_default()
            .insert(id.to_string(), row);
        self
    }
}

#[async_trait::async_trait]
impl RelationalFieldLoader for MockRelationalLoader {
    async fn load_fields(
        &self,
        table: &str,
        fields: &[String],
        ids: &[String],
    ) -> Result<RelationalFields, String> {
        let table_data = self.data.get(table).cloned().unwrap_or_default();
        let mut result = RelationalFields::new();
        for id in ids {
            if let Some(row) = table_data.get(id) {
                let mut filtered = HashMap::new();
                for field in fields {
                    if let Some(val) = row.get(field) {
                        filtered.insert(field.clone(), val.clone());
                    }
                }
                if !filtered.is_empty() {
                    result.insert(id.clone(), filtered);
                }
            }
        }
        Ok(result)
    }
}

fn make_graph() -> JointProjection {
    let mut graph = JointProjection::new(10, Duration::from_secs(1));
    graph.add_node(GraphNode {
        id: "A".to_string(),
        label: "Person".to_string(),
    });
    graph.add_node(GraphNode {
        id: "B".to_string(),
        label: "Person".to_string(),
    });
    graph.add_node(GraphNode {
        id: "C".to_string(),
        label: "Company".to_string(),
    });
    graph.add_edge(GraphEdge {
        from: "A".to_string(),
        to: "B".to_string(),
        relation: "KNOWS".to_string(),
    });
    graph.add_edge(GraphEdge {
        from: "B".to_string(),
        to: "C".to_string(),
        relation: "WORKS_AT".to_string(),
    });
    graph
}

#[tokio::test]
async fn test_query_with_projection_returns_graph_structure() {
    let graph = make_graph();
    let loader = MockRelationalLoader::new();
    let projections = vec![FieldProjection::new("users", vec!["name".to_string()])];

    let result: JointResult = graph
        .query_with_projection("A", 1, &projections, &loader)
        .await
        .unwrap();

    assert!(!result.nodes.is_empty());
    assert!(result.nodes.iter().any(|n| n.id == "A"));
    assert!(result.nodes.iter().any(|n| n.id == "B"));
    assert!(!result.edges.is_empty());
}

#[tokio::test]
async fn test_query_with_projection_loads_relational_fields() {
    let graph = make_graph();
    let loader = MockRelationalLoader::new()
        .with_row("users", "A", vec![("name", "Alice"), ("age", "30")])
        .with_row("users", "B", vec![("name", "Bob"), ("age", "25")]);
    let projections = vec![FieldProjection::new(
        "users",
        vec!["name".to_string(), "age".to_string()],
    )];

    let result = graph
        .query_with_projection("A", 1, &projections, &loader)
        .await
        .unwrap();

    let a_fields = result.relational_fields.get("A").unwrap();
    assert_eq!(a_fields.get("name").unwrap(), "Alice");
    assert_eq!(a_fields.get("age").unwrap(), "30");

    let b_fields = result.relational_fields.get("B").unwrap();
    assert_eq!(b_fields.get("name").unwrap(), "Bob");
}

#[tokio::test]
async fn test_query_with_projection_multiple_tables() {
    let graph = make_graph();
    let loader = MockRelationalLoader::new()
        .with_row("users", "A", vec![("name", "Alice")])
        .with_row("users", "B", vec![("name", "Bob")])
        .with_row("companies", "C", vec![("cname", "Acme Corp")]);
    let projections = vec![
        FieldProjection::new("users", vec!["name".to_string()]),
        FieldProjection::new("companies", vec!["cname".to_string()]),
    ];

    let result = graph
        .query_with_projection("A", 2, &projections, &loader)
        .await
        .unwrap();

    assert!(result.relational_fields.contains_key("A"));
    assert!(result.relational_fields.contains_key("B"));
    assert!(result.relational_fields.contains_key("C"));
    assert_eq!(
        result
            .relational_fields
            .get("C")
            .unwrap()
            .get("cname")
            .unwrap(),
        "Acme Corp"
    );
}

#[tokio::test]
async fn test_query_with_projection_no_matching_fields() {
    let graph = make_graph();
    let loader = MockRelationalLoader::new();
    let projections = vec![FieldProjection::new("nonexistent", vec!["x".to_string()])];

    let result = graph
        .query_with_projection("A", 1, &projections, &loader)
        .await
        .unwrap();

    assert!(result.relational_fields.is_empty());
    assert!(!result.nodes.is_empty());
}

#[tokio::test]
async fn test_query_with_projection_empty_projections() {
    let graph = make_graph();
    let loader = MockRelationalLoader::new();

    let result = graph
        .query_with_projection("A", 1, &[], &loader)
        .await
        .unwrap();

    assert!(result.relational_fields.is_empty());
    assert!(!result.nodes.is_empty());
    assert!(!result.edges.is_empty());
}

#[tokio::test]
async fn test_query_with_projection_truncated_propagates() {
    let mut graph = JointProjection::new(1, Duration::from_secs(1));
    graph.add_node(GraphNode {
        id: "A".to_string(),
        label: "X".to_string(),
    });
    graph.add_node(GraphNode {
        id: "B".to_string(),
        label: "X".to_string(),
    });
    graph.add_node(GraphNode {
        id: "C".to_string(),
        label: "X".to_string(),
    });
    graph.add_edge(GraphEdge {
        from: "A".to_string(),
        to: "B".to_string(),
        relation: "R".to_string(),
    });
    graph.add_edge(GraphEdge {
        from: "B".to_string(),
        to: "C".to_string(),
        relation: "R".to_string(),
    });

    let loader = MockRelationalLoader::new();
    let result = graph
        .query_with_projection("A", 5, &[], &loader)
        .await
        .unwrap();

    assert!(result.truncated);
}

#[tokio::test]
async fn test_field_projection_construction() {
    let proj = FieldProjection::new("users", vec!["name".to_string(), "age".to_string()]);
    assert_eq!(proj.table, "users");
    assert_eq!(proj.fields.len(), 2);
    assert_eq!(proj.fields[0], "name");
    assert_eq!(proj.fields[1], "age");
}
