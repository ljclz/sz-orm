//! lineage 图导出：DOT/JSON/GraphML 格式。

use super::graph::{LineageEdge, LineageGraph, LineageNodeId};
use super::tracker::LineageTracker;

/// 导出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineageExportFormat {
    Dot,
    Json,
    GraphMl,
}

impl LineageTracker {
    /// 导出 lineage 图
    pub fn export(
        &self,
        format: LineageExportFormat,
    ) -> Result<String, super::graph::LineageError> {
        let graph = self.graph_snapshot();
        Ok(match format {
            LineageExportFormat::Dot => export_dot(&graph),
            LineageExportFormat::Json => export_json(&graph),
            LineageExportFormat::GraphMl => export_graphml(&graph),
        })
    }
}

fn node_id_str(id: &LineageNodeId) -> String {
    format!("{}.{}", id.table, id.column)
}

/// 导出 DOT 格式（可被 Graphviz 渲染）
pub fn export_dot(graph: &LineageGraph) -> String {
    let mut out = String::new();
    out.push_str("digraph lineage {\n");
    out.push_str("    rankdir=LR;\n");

    for (id, node) in &graph.nodes {
        let label = node_id_str(id);
        let node_type = match node.node_type {
            super::graph::NodeType::Table => "Table",
            super::graph::NodeType::Column => "Column",
            super::graph::NodeType::View => "View",
            super::graph::NodeType::MaterializedView => "MaterializedView",
        };
        out.push_str(&format!(
            "    \"{}\" [label=\"{}\", shape=box, type={}];\n",
            label, label, node_type
        ));
    }

    for edge in &graph.edges {
        let src = node_id_str(&edge.source);
        let tgt = node_id_str(&edge.target);
        let edge_type = edge_type_str(edge);
        out.push_str(&format!(
            "    \"{}\" -> \"{}\" [label=\"{}\"];\n",
            src, tgt, edge_type
        ));
    }

    out.push_str("}\n");
    out
}

/// 导出 JSON 格式（可被 D3.js 解析）
pub fn export_json(graph: &LineageGraph) -> String {
    let mut nodes = Vec::new();
    for (id, node) in &graph.nodes {
        let node_type = match node.node_type {
            super::graph::NodeType::Table => "Table",
            super::graph::NodeType::Column => "Column",
            super::graph::NodeType::View => "View",
            super::graph::NodeType::MaterializedView => "MaterializedView",
        };
        nodes.push(format!(
            r#"{{"id":{{"table":"{}","column":"{}"}},"type":"{}"}}"#,
            escape_json(&id.table),
            escape_json(&id.column),
            node_type
        ));
    }

    let mut edges = Vec::new();
    for edge in &graph.edges {
        edges.push(format!(
            r#"{{"source":{{"table":"{}","column":"{}"}},"target":{{"table":"{}","column":"{}"}},"edge_type":"{}"}}"#,
            escape_json(&edge.source.table),
            escape_json(&edge.source.column),
            escape_json(&edge.target.table),
            escape_json(&edge.target.column),
            edge_type_str(edge)
        ));
    }

    format!(
        r#"{{"nodes":[{}],"edges":[{}]}}"#,
        nodes.join(","),
        edges.join(",")
    )
}

/// 导出 GraphML XML 格式
pub fn export_graphml(graph: &LineageGraph) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">\n");
    out.push_str("    <graph edgedirected=\"true\">\n");

    for id in graph.nodes.keys() {
        let node_id = escape_xml(&node_id_str(id));
        out.push_str(&format!("        <node id=\"{}\"/>\n", node_id));
    }

    for edge in &graph.edges {
        let src = escape_xml(&node_id_str(&edge.source));
        let tgt = escape_xml(&node_id_str(&edge.target));
        let et = edge_type_str(edge);
        out.push_str(&format!(
            "        <edge source=\"{}\" target=\"{}\" type=\"{}\"/>\n",
            src, tgt, et
        ));
    }

    out.push_str("    </graph>\n");
    out.push_str("</graphml>\n");
    out
}

fn edge_type_str(edge: &LineageEdge) -> &'static str {
    match edge.edge_type {
        super::graph::EdgeType::DirectDependency => "DirectDependency",
        super::graph::EdgeType::Derived => "Derived",
        super::graph::EdgeType::Join => "Join",
        super::graph::EdgeType::Filter => "Filter",
        super::graph::EdgeType::Projection => "Projection",
    }
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lineage::{LineageDialect, LineageGraph, LineageNode, LineageNodeId, NodeType};
    use crate::HashChainAuditor;
    use std::sync::Arc;

    fn sample_graph() -> LineageGraph {
        let mut graph = LineageGraph::new();
        graph.add_node(LineageNode::new(
            LineageNodeId::new("users", "name"),
            NodeType::Column,
        ));
        graph.add_node(LineageNode::new(
            LineageNodeId::new("report", "name"),
            NodeType::View,
        ));
        graph
            .add_edge(super::super::graph::LineageEdge::new(
                LineageNodeId::new("users", "name"),
                LineageNodeId::new("report", "name"),
                super::super::graph::EdgeType::Derived,
            ))
            .unwrap();
        graph
    }

    #[test]
    fn test_export_dot() {
        let graph = sample_graph();
        let dot = export_dot(&graph);

        assert!(dot.contains("digraph lineage"));
        assert!(dot.contains("\"users.name\""));
        assert!(dot.contains("\"report.name\""));
        assert!(dot.contains("\"users.name\" -> \"report.name\""));
    }

    #[test]
    fn test_export_json() {
        let graph = sample_graph();
        let json = export_json(&graph);

        assert!(json.contains("\"nodes\""));
        assert!(json.contains("\"edges\""));
        assert!(json.contains("\"table\":\"users\""));
        assert!(json.contains("\"column\":\"name\""));
        assert!(json.contains("\"edge_type\":\"Derived\""));

        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["nodes"].is_array());
        assert!(parsed["edges"].is_array());
        assert_eq!(parsed["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["edges"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_export_graphml() {
        let graph = sample_graph();
        let xml = export_graphml(&graph);

        assert!(xml.contains("<graphml"));
        assert!(xml.contains("<graph"));
        assert!(xml.contains("<node"));
        assert!(xml.contains("<edge"));
        assert!(xml.contains("users.name"));
        assert!(xml.contains("report.name"));
        assert!(xml.contains("</graphml>"));
    }

    #[test]
    fn test_export_empty_graph() {
        let graph = LineageGraph::new();

        let dot = export_dot(&graph);
        assert!(dot.contains("digraph lineage"));

        let json = export_json(&graph);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["nodes"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["edges"].as_array().unwrap().len(), 0);

        let xml = export_graphml(&graph);
        assert!(xml.contains("<graphml"));
    }

    #[test]
    fn test_tracker_export_dot() {
        let tracker = LineageTracker::new(LineageDialect::PostgreSQL, None);
        tracker
            .track_sql("CREATE VIEW v AS SELECT a FROM t")
            .unwrap();

        let dot = tracker.export(LineageExportFormat::Dot).unwrap();
        assert!(dot.contains("digraph lineage"));
        assert!(dot.contains("t.a"));
        assert!(dot.contains("v.a"));
    }

    #[test]
    fn test_tracker_export_json() {
        let tracker = LineageTracker::new(LineageDialect::PostgreSQL, None);
        tracker
            .track_sql("CREATE VIEW v AS SELECT a FROM t")
            .unwrap();

        let json = tracker.export(LineageExportFormat::Json).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["nodes"].as_array().unwrap().len() >= 2);
        assert!(!parsed["edges"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_tracker_export_graphml() {
        let tracker = LineageTracker::new(LineageDialect::PostgreSQL, None);
        tracker
            .track_sql("CREATE VIEW v AS SELECT a FROM t")
            .unwrap();

        let xml = tracker.export(LineageExportFormat::GraphMl).unwrap();
        assert!(xml.contains("<graphml"));
        assert!(xml.contains("<node"));
        assert!(xml.contains("<edge"));
    }

    #[test]
    fn test_export_with_auditor() {
        let auditor = Arc::new(HashChainAuditor::new());
        let tracker = LineageTracker::new(LineageDialect::PostgreSQL, Some(auditor.clone()));
        tracker
            .track_sql("CREATE VIEW v AS SELECT a FROM t")
            .unwrap();

        let dot = tracker.export(LineageExportFormat::Dot).unwrap();
        assert!(dot.contains("digraph lineage"));
        assert!(auditor.verify().is_ok());
    }

    #[test]
    fn test_export_json_special_chars() {
        let mut graph = LineageGraph::new();
        graph.add_node(LineageNode::new(
            LineageNodeId::new("table\"with\"quotes", "col"),
            NodeType::Column,
        ));

        let json = export_json(&graph);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["nodes"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_export_graphml_special_chars() {
        let mut graph = LineageGraph::new();
        graph.add_node(LineageNode::new(
            LineageNodeId::new("table<with>lt", "col"),
            NodeType::Column,
        ));

        let xml = export_graphml(&graph);
        assert!(xml.contains("&lt;"));
        assert!(xml.contains("&gt;"));
    }
}
