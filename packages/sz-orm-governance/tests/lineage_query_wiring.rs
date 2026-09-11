use sz_orm_governance::lineage::*;

#[test]
fn query_lineage_empty_for_unknown_table() {
    let builder = LineageBuilder::new();
    let collector = LineageAutoCollector::new(builder, vec![]);
    let path = collector.query_lineage("nonexistent", "");
    assert!(path.upstream.is_empty());
    assert!(path.downstream.is_empty());
}

#[test]
fn query_lineage_by_table_returns_all_edges() {
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
    let path = collector.query_lineage("users", "");
    assert!(!path.downstream.is_empty());
    let path_report = collector.query_lineage("report", "");
    assert!(!path_report.upstream.is_empty());
}

#[test]
fn query_lineage_by_specific_field_filters() {
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
    let path = collector.query_lineage("report", "user_id");
    assert!(path.upstream.iter().all(|e| e.to_field == "user_id"));
}

#[test]
fn query_lineage_cdc_edges_included() {
    let builder = LineageBuilder::new();
    let collector = LineageAutoCollector::new(builder, vec![]);
    let mut collector = collector;
    collector
        .on_cdc_event(&CdcEventRef {
            source_table: "orders".to_string(),
            downstream: "warehouse".to_string(),
        })
        .unwrap();
    let path = collector.query_lineage("orders", "");
    assert!(!path.downstream.is_empty());
    assert!(path
        .downstream
        .iter()
        .any(|e| e.to_table == "warehouse" && e.edge_type == LineageEdgeType::Cdc));
}

#[test]
fn query_lineage_both_upstream_and_downstream() {
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
    collector
        .on_query_executed(
            "SELECT r.user_id AS rid FROM report r",
            vec!["report".to_string()],
            vec!["dashboard".to_string()],
        )
        .unwrap();
    let path = collector.query_lineage("report", "");
    assert!(!path.upstream.is_empty());
    assert!(!path.downstream.is_empty());
}

#[test]
fn query_lineage_field_level_resolution() {
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
    let upstream_field = &path.upstream[0].from_field;
    assert!(!upstream_field.is_empty());
}
