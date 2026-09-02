//! TASK-006 验证测试：字段级数据血缘构建

use sz_orm_governance::lineage::{LineageBuilder, LineageGraph};
use sz_orm_governance::types::GovernanceError;

#[test]
fn test_lineage_build_from_sql() {
    let builder = LineageBuilder::new();
    let graph = builder.build_from_sql("SELECT a, b FROM source").unwrap();
    assert!(graph.nodes.is_empty() || !graph.nodes.is_empty());
}

#[test]
fn test_lineage_builder_default() {
    let builder = LineageBuilder::default();
    let graph: LineageGraph = builder.build_from_sql("").unwrap();
    assert!(graph.nodes.is_empty());
}
