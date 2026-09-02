//! TASK-022 集成测试：血缘变更追踪+断点续传端到端验证

use sz_orm_governance::lineage::{LineageBuilder, LineageCheckpoint, LineageDiff};

#[test]
fn test_update_lineage_detects_added_columns() {
    let builder = LineageBuilder::new();
    let old = builder
        .build_from_sql("SELECT id, name FROM users")
        .unwrap();
    let (new_graph, diff) = builder
        .update_lineage(&old, "SELECT id, name, email, phone FROM users")
        .unwrap();

    assert_eq!(new_graph.nodes.len(), 4);
    assert!(!diff.added.is_empty(), "新增列应出现在 diff.added");
    assert!(diff.removed.is_empty(), "无删除列");
}

#[test]
fn test_update_lineage_detects_removed_columns() {
    let builder = LineageBuilder::new();
    let old = builder
        .build_from_sql("SELECT id, name, email FROM users")
        .unwrap();
    let (_, diff) = builder
        .update_lineage(&old, "SELECT id FROM users")
        .unwrap();

    assert!(!diff.removed.is_empty(), "删除列应出现在 diff.removed");
    assert!(diff.added.is_empty(), "无新增列");
}

#[test]
fn test_update_lineage_no_change() {
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
fn test_checkpoint_create_and_resume() {
    let builder = LineageBuilder::new();
    let graph = builder.build_from_sql("SELECT id FROM users").unwrap();
    let checkpoint: LineageCheckpoint = builder.create_checkpoint(&graph, "SELECT id FROM users");

    assert_eq!(checkpoint.version, 1);
    assert!(!checkpoint.source_sql_hash.is_empty());

    let (new_graph, diff, new_checkpoint) = builder
        .resume_from_checkpoint(&checkpoint, "SELECT id, name, email FROM users")
        .unwrap();

    assert_eq!(new_checkpoint.version, 2, "版本应递增");
    assert!(!diff.added.is_empty(), "应检测到新增列");
    assert_eq!(new_graph.nodes.len(), 3);
}

#[test]
fn test_checkpoint_multi_iteration() {
    let builder = LineageBuilder::new();
    let graph = builder.build_from_sql("SELECT id FROM users").unwrap();
    let mut checkpoint = builder.create_checkpoint(&graph, "SELECT id FROM users");

    let sqls = [
        "SELECT id, name FROM users",
        "SELECT id, name, email FROM users",
        "SELECT id, name, email, phone FROM users",
    ];

    for (i, sql) in sqls.iter().enumerate() {
        let (_, _, new_cp) = builder.resume_from_checkpoint(&checkpoint, sql).unwrap();
        assert_eq!(new_cp.version, (i + 2) as u64, "版本应递增至 {}", i + 2);
        checkpoint = new_cp;
    }
}

#[test]
fn test_lineage_diff_serialization() {
    let diff = LineageDiff {
        added: vec![],
        removed: vec![],
        modified: vec![],
    };
    let json = serde_json::to_string(&diff).unwrap();
    let restored: LineageDiff = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, diff);
}
