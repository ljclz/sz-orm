//! v7.6.0 方向 4 端到端测试：端到端血缘追踪 + 影响分析。

#![cfg(feature = "end-to-end-lineage")]

use std::sync::Arc;

use sz_orm_audit::lineage::{
    EndToEndLineage, EndToEndLineageTracker, ImpactAnalyzer, SchemaChange, SchemaChangeType,
};

#[test]
fn v760_e2e_end_to_end_lineage_tracking() {
    let tracker = EndToEndLineageTracker::new();

    tracker.track(EndToEndLineage::new("users.id", "orders.uid", "join"));
    tracker.track(EndToEndLineage::new(
        "orders.uid",
        "report.user_id",
        "project",
    ));
    tracker.track(EndToEndLineage::new("orders.total", "report.amount", "sum"));

    assert_eq!(tracker.len(), 3);

    let result = tracker.verify_completeness();
    assert!(result.is_all_complete(), "所有链路应完整");
}

#[test]
fn v760_e2e_impact_analysis_drop_column() {
    let tracker = Arc::new(EndToEndLineageTracker::new());

    tracker.track(EndToEndLineage::new("users.id", "orders.uid", "join"));
    tracker.track(EndToEndLineage::new(
        "orders.uid",
        "report.user_id",
        "project",
    ));

    let analyzer = ImpactAnalyzer::new(tracker);
    let change =
        SchemaChange::new(SchemaChangeType::DropColumn, "users", "drop id").with_column("id");
    let result = analyzer.analyze(change);

    assert!(result.is_critical(), "DropColumn 应为 Critical");
    assert!(result.affected_count() > 0, "应有受影响节点");
    assert!(result.recommendation.contains("高危"), "建议应包含高危提示");
}

#[test]
fn v760_e2e_impact_analysis_batch() {
    let tracker = Arc::new(EndToEndLineageTracker::new());
    tracker.track(EndToEndLineage::new("users.id", "orders.uid", "join"));

    let analyzer = ImpactAnalyzer::new(tracker);
    let changes = vec![
        SchemaChange::new(SchemaChangeType::DropColumn, "users", "drop").with_column("id"),
        SchemaChange::new(SchemaChangeType::AddColumn, "users", "add").with_column("new"),
        SchemaChange::new(SchemaChangeType::AlterType, "orders", "alter").with_column("uid"),
    ];

    let results = analyzer.analyze_batch(changes);
    assert_eq!(results.len(), 3);

    let critical = ImpactAnalyzer::critical_changes(&results);
    assert_eq!(critical.len(), 1, "应有 1 个 Critical 变更");
}

#[test]
fn v760_e2e_completeness_check_with_incomplete() {
    let tracker = EndToEndLineageTracker::new();
    tracker.track(EndToEndLineage::new("a.x", "b.y", "direct"));

    let mut incomplete = EndToEndLineage::new("c.z", "d.w", "direct");
    incomplete.path = vec!["c.z".to_string()];
    tracker.track(incomplete);

    let result = tracker.verify_completeness();
    assert!(!result.is_all_complete());
    assert_eq!(result.incomplete_count(), 1);
}
