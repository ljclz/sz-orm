//! M4 数据 lineage 集成测试

#![cfg(feature = "data-lineage")]

use std::sync::Arc;
use sz_orm_audit::{
    EdgeType, HashChainAuditor, LineageDialect, LineageExportFormat, LineageGraph, LineageNode,
    LineageNodeId, LineageTracker, NodeType,
};

#[test]
fn test_lineage_full_pipeline() {
    let tracker = LineageTracker::new(LineageDialect::PostgreSQL, None);

    tracker
        .track_sql("CREATE VIEW report AS SELECT users.name, orders.amount FROM users JOIN orders ON users.id = orders.user_id")
        .unwrap();
    tracker
        .track_sql("CREATE VIEW dashboard AS SELECT report.name FROM report")
        .unwrap();

    let impacted = tracker.impact_analysis(&LineageNodeId::new("users", "name"));
    assert!(impacted
        .iter()
        .any(|n| n.id == LineageNodeId::new("report", "name")));
    assert!(!impacted
        .iter()
        .any(|n| n.id == LineageNodeId::new("dashboard", "nameG")));
    assert!(impacted
        .iter()
        .any(|n| n.id == LineageNodeId::new("dashboard", "name")));
}

#[test]
fn test_lineage_with_audit_chain() {
    let auditor = Arc::new(HashChainAuditor::new());
    let tracker = LineageTracker::new(LineageDialect::PostgreSQL, Some(auditor.clone()));

    tracker
        .track_sql("CREATE VIEW v1 AS SELECT a FROM t1")
        .unwrap();
    tracker
        .track_sql("CREATE VIEW v2 AS SELECT a FROM t2")
        .unwrap();

    assert!(auditor.len() >= 2);
    assert!(auditor.verify().is_ok());
}

#[test]
fn test_lineage_export_all_formats() {
    let tracker = LineageTracker::new(LineageDialect::PostgreSQL, None);
    tracker
        .track_sql("CREATE VIEW v AS SELECT a, b FROM t")
        .unwrap();

    let dot = tracker.export(LineageExportFormat::Dot).unwrap();
    assert!(dot.contains("digraph lineage"));

    let json = tracker.export(LineageExportFormat::Json).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["nodes"].is_array());
    assert!(parsed["edges"].is_array());

    let xml = tracker.export(LineageExportFormat::GraphMl).unwrap();
    assert!(xml.contains("<graphml"));
}

#[test]
fn test_lineage_origin_and_impact() {
    let tracker = LineageTracker::new(LineageDialect::PostgreSQL, None);

    tracker
        .track_sql("INSERT INTO report (name, amount) SELECT users.name, orders.amount FROM users JOIN orders ON users.id = orders.user_id")
        .unwrap();

    let origins = tracker.origin_analysis(&LineageNodeId::new("report", "name"));
    assert!(origins
        .iter()
        .any(|n| n.id == LineageNodeId::new("users", "name")));

    let impacted = tracker.impact_analysis(&LineageNodeId::new("orders", "amount"));
    assert!(impacted
        .iter()
        .any(|n| n.id == LineageNodeId::new("report", "amount")));
}

#[test]
fn test_lineage_cycle_detection_in_tracker() {
    let tracker = LineageTracker::new(LineageDialect::PostgreSQL, None);

    tracker
        .track_sql("CREATE VIEW a AS SELECT b.x FROM b")
        .unwrap();
    let result = tracker.track_sql("CREATE VIEW b AS SELECT a.x FROM a");
    assert!(result.is_ok());

    let graph = tracker.graph_snapshot();
    assert_eq!(graph.edge_count(), 1);
}

#[test]
fn test_lineage_graph_direct_usage() {
    let mut graph = LineageGraph::new();
    graph.add_node(LineageNode::new(
        LineageNodeId::new("src", "col"),
        NodeType::Column,
    ));
    graph.add_node(LineageNode::new(
        LineageNodeId::new("dst", "col"),
        NodeType::View,
    ));

    let edge = sz_orm_audit::LineageEdge::new(
        LineageNodeId::new("src", "col"),
        LineageNodeId::new("dst", "col"),
        EdgeType::Derived,
    );
    assert!(graph.add_edge(edge).is_ok());
    assert_eq!(graph.node_count(), 2);
    assert_eq!(graph.edge_count(), 1);
}

#[test]
fn test_lineage_multiple_dialects() {
    for dialect in [
        LineageDialect::MySQL,
        LineageDialect::PostgreSQL,
        LineageDialect::SQLite,
        LineageDialect::Ansi,
        LineageDialect::Generic,
    ] {
        let tracker = LineageTracker::new(dialect, None);
        let result = tracker.track_sql("CREATE VIEW v AS SELECT a FROM t");
        assert!(result.is_ok(), "dialect {:?} should parse", dialect);
        assert!(
            tracker.edge_count() > 0,
            "dialect {:?} should have edges",
            dialect
        );
    }
}
