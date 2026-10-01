//! SmartEagerLoader 测试基础设施验证（v2.4.0 任务 1）
//!
//! 引用 common::equivalence 和 common::schema_builder 模块，
//! 验证等价性断言工具与测试数据构造器的正确性。

mod common;

use common::equivalence;
use common::schema_builder;

// ============================================================================
// equivalence 模块测试重导出
// ============================================================================

#[test]
fn test_assert_eager_equivalent_equal() {
    let mut smart_row = std::collections::HashMap::new();
    smart_row.insert("id".to_string(), sz_orm_core::Value::I64(1));
    smart_row.insert(
        "name".to_string(),
        sz_orm_core::Value::String("a".to_string()),
    );
    let mut manual_row = std::collections::HashMap::new();
    manual_row.insert("id".to_string(), sz_orm_core::Value::I64(1));
    manual_row.insert(
        "name".to_string(),
        sz_orm_core::Value::String("a".to_string()),
    );
    equivalence::assert_eager_equivalent(
        &[smart_row],
        &[manual_row],
        sz_orm_core::relation_trait::RelationKind::HasOne,
        "",
    );
}

#[test]
fn test_assert_strategy_selected_join() {
    use sz_orm_core::smart_eager_loader::{LoadStrategy, StrategyDecision};
    let decision = StrategyDecision {
        relation_name: "profile".to_string(),
        relation_kind: sz_orm_core::relation_trait::RelationKind::HasOne,
        strategy: LoadStrategy::Join,
        reason: "HasOne → Join".to_string(),
        estimated_query_count: 1,
    };
    equivalence::assert_strategy_selected(&decision, LoadStrategy::Join);
}

#[test]
fn test_assert_strategy_selected_dataloader() {
    use sz_orm_core::smart_eager_loader::{LoadStrategy, StrategyDecision};
    let decision = StrategyDecision {
        relation_name: "orders".to_string(),
        relation_kind: sz_orm_core::relation_trait::RelationKind::HasMany,
        strategy: LoadStrategy::DataLoader,
        reason: "HasMany → DataLoader".to_string(),
        estimated_query_count: 2,
    };
    equivalence::assert_strategy_selected(&decision, LoadStrategy::DataLoader);
}

#[test]
fn test_assert_strategy_selected_intermediate() {
    use sz_orm_core::smart_eager_loader::{LoadStrategy, StrategyDecision};
    let decision = StrategyDecision {
        relation_name: "roles".to_string(),
        relation_kind: sz_orm_core::relation_trait::RelationKind::ManyToMany,
        strategy: LoadStrategy::IntermediateTableBatch,
        reason: "ManyToMany → IntermediateTableBatch".to_string(),
        estimated_query_count: 2,
    };
    equivalence::assert_strategy_selected(&decision, LoadStrategy::IntermediateTableBatch);
}

#[test]
fn test_assert_nested_depth_equal_leaf() {
    use sz_orm_core::eager_loader::NestedEagerResult;
    let mut row = std::collections::HashMap::new();
    row.insert("id".to_string(), sz_orm_core::Value::I64(1));
    let smart = NestedEagerResult::Leaf(row.clone());
    let manual = NestedEagerResult::Leaf(row);
    equivalence::assert_nested_depth_equal(&smart, &manual);
}

// ============================================================================
// schema_builder 模块测试
// ============================================================================

#[test]
fn test_schema_builder_ddl_all_dialects() {
    use schema_builder::{TestDialect, TestSchemaBuilder};
    for dialect in [
        TestDialect::MySql,
        TestDialect::Postgres,
        TestDialect::Sqlite,
        TestDialect::Oracle,
        TestDialect::MsSql,
    ] {
        let builder = TestSchemaBuilder::new(dialect);
        let ddl = builder.build_ddl();
        assert_eq!(ddl.len(), 5, "{} 应生成 5 表 DDL", dialect.as_str());
        assert!(ddl[0].contains("users"));
        assert!(ddl[4].contains("user_roles"));
    }
}

#[test]
fn test_schema_builder_teardown_reverse_order() {
    use schema_builder::{TestDialect, TestSchemaBuilder};
    let builder = TestSchemaBuilder::new(TestDialect::MySql);
    let teardown = builder.teardown_ddl();
    assert_eq!(teardown.len(), 5);
    assert!(teardown[0].contains("user_roles"));
    assert!(teardown[4].contains("users"));
}

#[test]
fn test_schema_builder_seed_data_counts() {
    use schema_builder::{TestDialect, TestSchemaBuilder};
    let builder = TestSchemaBuilder::new(TestDialect::MySql);
    let seed = builder.seed_data();
    let counts = TestSchemaBuilder::expected_counts();
    assert_eq!(
        seed.iter()
            .filter(|s| s.contains("INSERT INTO users"))
            .count(),
        counts.users
    );
    assert_eq!(
        seed.iter()
            .filter(|s| s.contains("INSERT INTO orders"))
            .count(),
        counts.orders
    );
    assert_eq!(
        seed.iter()
            .filter(|s| s.contains("INSERT INTO profiles"))
            .count(),
        counts.profiles
    );
    assert_eq!(
        seed.iter()
            .filter(|s| s.contains("INSERT INTO roles"))
            .count(),
        counts.roles
    );
    assert_eq!(
        seed.iter()
            .filter(|s| s.contains("INSERT INTO user_roles"))
            .count(),
        counts.user_roles
    );
}

#[test]
fn test_schema_builder_user3_no_relations() {
    use schema_builder::{TestDialect, TestSchemaBuilder};
    let builder = TestSchemaBuilder::new(TestDialect::MySql);
    let seed = builder.seed_data();
    let user3_orders = seed
        .iter()
        .any(|s| s.contains("INSERT INTO orders") && s.contains("3, 3,"));
    let user3_profiles = seed
        .iter()
        .any(|s| s.contains("INSERT INTO profiles") && s.contains("3, 3,"));
    let user3_roles = seed
        .iter()
        .any(|s| s.contains("INSERT INTO user_roles'") && s.contains("3, 3,"));
    assert!(!user3_orders, "user3 不应有 orders");
    assert!(!user3_profiles, "user3 不应有 profiles");
    assert!(!user3_roles, "user3 不应有 roles");
}
