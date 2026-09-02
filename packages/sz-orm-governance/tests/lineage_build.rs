//! TASK-006 验证测试：字段级数据血缘构建

use sz_orm_governance::lineage::{LineageBuilder, LineageGraph};

#[test]
fn test_lineage_build_from_sql() {
    let builder = LineageBuilder::new();
    let graph = builder.build_from_sql("SELECT a, b FROM source").unwrap();
    assert_eq!(graph.nodes.len(), 2);
}

#[test]
fn test_lineage_builder_default() {
    let builder = LineageBuilder::default();
    let graph: LineageGraph = builder.build_from_sql("SELECT id FROM users").unwrap();
    assert!(!graph.nodes.is_empty());
}
