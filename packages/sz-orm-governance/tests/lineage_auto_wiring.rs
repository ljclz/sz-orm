use sz_orm_governance::lineage::*;

#[test]
fn auto_collector_creates_with_builder_and_rules() {
    let builder = LineageBuilder::new();
    let rules = vec![LineageRule {
        name: "rule1".to_string(),
        priority: 1,
        source_pattern: "users".to_string(),
        sink_pattern: "report".to_string(),
    }];
    let collector = LineageAutoCollector::new(builder, rules);
    assert_eq!(collector.edges().len(), 0);
}

#[test]
fn on_query_executed_records_lineage_edges() {
    let builder = LineageBuilder::new();
    let collector = LineageAutoCollector::new(builder, vec![]);
    let mut collector = collector;
    collector
        .on_query_executed(
            "SELECT u.id AS user_id, u.name AS user_name FROM users u",
            vec!["users".to_string()],
            vec!["report".to_string()],
        )
        .unwrap();
    let edges = collector.edges();
    assert!(!edges.is_empty());
    assert!(edges
        .iter()
        .any(|e| e.from_table == "users" && e.to_table == "report"));
}

#[test]
fn on_query_executed_join_creates_multiple_edges() {
    let builder = LineageBuilder::new();
    let collector = LineageAutoCollector::new(builder, vec![]);
    let mut collector = collector;
    collector
        .on_query_executed(
            "SELECT a.id AS aid, b.name AS bname FROM table_a a",
            vec!["table_a".to_string(), "table_b".to_string()],
            vec!["view_c".to_string()],
        )
        .unwrap();
    let edges = collector.edges();
    assert!(edges.iter().any(|e| e.to_table == "view_c"));
}

#[test]
fn on_cdc_event_records_edge() {
    let builder = LineageBuilder::new();
    let collector = LineageAutoCollector::new(builder, vec![]);
    let mut collector = collector;
    collector
        .on_cdc_event(&CdcEventRef {
            source_table: "orders".to_string(),
            downstream: "analytics".to_string(),
        })
        .unwrap();
    let edges = collector.edges();
    assert!(edges.iter().any(|e| e.from_table == "orders"
        && e.to_table == "analytics"
        && e.edge_type == LineageEdgeType::Cdc));
}

#[test]
fn on_cdc_event_multiple_downstreams() {
    let builder = LineageBuilder::new();
    let collector = LineageAutoCollector::new(builder, vec![]);
    let mut collector = collector;
    collector
        .on_cdc_event(&CdcEventRef {
            source_table: "users".to_string(),
            downstream: "cache".to_string(),
        })
        .unwrap();
    collector
        .on_cdc_event(&CdcEventRef {
            source_table: "users".to_string(),
            downstream: "search_index".to_string(),
        })
        .unwrap();
    let edges = collector.edges();
    let user_edges: Vec<_> = edges.iter().filter(|e| e.from_table == "users").collect();
    assert!(user_edges.len() >= 2);
}

#[test]
fn query_lineage_returns_upstream_edges() {
    let builder = LineageBuilder::new();
    let collector = LineageAutoCollector::new(builder, vec![]);
    let mut collector = collector;
    collector
        .on_query_executed(
            "SELECT u.id AS user_id FROM users u",
            vec!["users".to_string()],
            vec!["report".to_string()],
        )
        .unwrap();
    let path = collector.query_lineage("report", "user_id");
    assert!(!path.upstream.is_empty());
    assert!(path.upstream.iter().any(|e| e.from_table == "users"));
}

#[test]
fn query_lineage_returns_downstream_edges() {
    let builder = LineageBuilder::new();
    let collector = LineageAutoCollector::new(builder, vec![]);
    let mut collector = collector;
    collector
        .on_query_executed(
            "SELECT u.id AS user_id FROM users u",
            vec!["users".to_string()],
            vec!["report".to_string()],
        )
        .unwrap();
    let path = collector.query_lineage("users", "id");
    assert!(!path.downstream.is_empty());
    assert!(path.downstream.iter().any(|e| e.to_table == "report"));
}

#[test]
fn resolve_rule_conflicts_keeps_highest_priority() {
    let builder = LineageBuilder::new();
    let rules = vec![
        LineageRule {
            name: "rule_a".to_string(),
            priority: 1,
            source_pattern: "users".to_string(),
            sink_pattern: "report".to_string(),
        },
        LineageRule {
            name: "rule_a".to_string(),
            priority: 5,
            source_pattern: "users".to_string(),
            sink_pattern: "dashboard".to_string(),
        },
    ];
    let collector = LineageAutoCollector::new(builder, rules);
    let resolved = collector.resolve_rule_conflicts();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].priority, 5);
}
