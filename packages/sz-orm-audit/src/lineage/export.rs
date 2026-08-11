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

// ---------------------------------------------------------------------------
// v4.3.0 M3-T1：血缘可视化导出（Mermaid / HTML 报告，`lineage-viz` feature）
// ---------------------------------------------------------------------------

/// 导出 Mermaid 流程图格式（可被 GitHub / Mermaid Live 渲染）
///
/// ```text
/// graph LR
///     users_id["users.id"] --> orders_user_id["orders.user_id"]
/// ```
#[cfg(feature = "lineage-viz")]
pub fn export_mermaid(graph: &LineageGraph) -> String {
    let mut out = String::new();
    out.push_str("graph LR\n");
    for edge in &graph.edges {
        let src = node_id_str(&edge.source);
        let tgt = node_id_str(&edge.target);
        out.push_str(&format!(
            "    {}[\"{}\"] --> {}[\"{}\"]\n",
            escape_mermaid_id(&src),
            escape_mermaid(&src),
            escape_mermaid_id(&tgt),
            escape_mermaid(&tgt)
        ));
    }
    out
}

/// 导出独立 HTML 血缘报告（内联样式，无外部依赖）
#[cfg(feature = "lineage-viz")]
pub fn export_html_report(graph: &LineageGraph) -> String {
    let mut rows = String::new();
    let mut seen = std::collections::HashSet::new();
    for edge in &graph.edges {
        let src = node_id_str(&edge.source);
        let tgt = node_id_str(&edge.target);
        // 去重（同源同目标仅展示一次）
        let key = format!("{src}->{tgt}");
        if !seen.insert(key) {
            continue;
        }
        rows.push_str(&format!(
            "<tr><td>{}</td><td>→</td><td>{}</td><td>{}</td></tr>\n",
            escape_html(&src),
            escape_html(&tgt),
            edge_type_str(edge)
        ));
    }
    format!(
        r#"<!DOCTYPE html>
<html lang="zh">
<head><meta charset="utf-8"><title>数据血缘报告</title>
<style>
body {{ font-family: monospace; margin: 2rem; }}
table {{ border-collapse: collapse; }}
td, th {{ border: 1px solid #999; padding: 4px 12px; }}
th {{ background: #eee; }}
</style></head>
<body>
<h2>数据血缘报告</h2>
<p>节点数: {nodes} | 边数: {edges}</p>
<table>
<tr><th>来源</th><th></th><th>目标</th><th>依赖类型</th></tr>
{rows}</table>
</body>
</html>"#,
        nodes = graph.node_count(),
        edges = graph.edge_count(),
        rows = rows
    )
}

/// 转义 Mermaid 节点 ID（仅保留字母数字与下划线）
#[cfg(feature = "lineage-viz")]
fn escape_mermaid_id(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// 转义 Mermaid 标签文本（引号与方括号）
#[cfg(feature = "lineage-viz")]
fn escape_mermaid(s: &str) -> String {
    s.replace('"', "&quot;").replace('[', "(").replace(']', ")")
}

/// 转义 HTML 文本
#[cfg(feature = "lineage-viz")]
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
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

    #[cfg(feature = "lineage-viz")]
    #[test]
    fn test_export_mermaid() {
        let graph = sample_graph();
        let mermaid = export_mermaid(&graph);
        assert!(mermaid.starts_with("graph LR"));
        assert!(mermaid.contains("users_name"));
        assert!(mermaid.contains("report_name"));
        assert!(mermaid.contains("-->"));
        // 节点 ID 转义：`.` → `_`
        assert!(mermaid.contains("users_name[\"users.name\"]"));
    }

    #[cfg(feature = "lineage-viz")]
    #[test]
    fn test_export_html_report() {
        let graph = sample_graph();
        let html = export_html_report(&graph);
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("users.name"));
        assert!(html.contains("report.name"));
        assert!(html.contains("Derived"));
        assert!(html.contains("节点数: 2"));
        // 无外部脚本
        assert!(!html.contains("<script"));
    }

    #[cfg(feature = "lineage-viz")]
    #[test]
    fn test_export_mermaid_escapes_special_chars() {
        let mut graph = LineageGraph::new();
        graph.add_node(LineageNode::new(
            LineageNodeId::new("a\"b", "c"),
            NodeType::Column,
        ));
        graph.add_node(LineageNode::new(
            LineageNodeId::new("target", "col"),
            NodeType::Column,
        ));
        graph
            .add_edge(super::super::graph::LineageEdge::new(
                LineageNodeId::new("a\"b", "c"),
                LineageNodeId::new("target", "col"),
                super::super::graph::EdgeType::Derived,
            ))
            .unwrap();
        let mermaid = export_mermaid(&graph);
        // 引号被转义，ID 中非法字符转 `_`
        assert!(mermaid.contains("&quot;"));
    }
}
