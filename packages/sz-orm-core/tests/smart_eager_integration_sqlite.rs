//! SmartEagerLoader SQLite 方言集成测试（v2.4.0 任务 2.3）
//!
//! 验证 SmartEagerLoader 与手动 EagerLoader 在 SQLite 下结果集等价。
//! 使用 rusqlite in-memory，无需外部依赖，默认执行（不标注 #[ignore]）。

mod common;

use common::equivalence;
use common::rusqlite_adapter::RusqliteConnection;
use common::schema_builder::{TestDialect, TestSchemaBuilder};
use std::collections::HashMap;
use sz_orm_core::eager_loader::{EagerLoader, NestedEagerResult};
use sz_orm_core::relation_trait::{RelationDef, RelationKind};
use sz_orm_core::smart_eager_loader::{LoadStrategy, SmartEagerLoader, StrategyResolver};
use sz_orm_core::Value;

/// 初始化 SQLite 测试数据库：建表 + 插入数据
fn setup_sqlite() -> RusqliteConnection {
    let conn = RusqliteConnection::open_in_memory();
    let builder = TestSchemaBuilder::new(TestDialect::Sqlite);
    for ddl in builder.build_ddl() {
        conn.execute_direct(&ddl);
    }
    for sql in builder.seed_data() {
        conn.execute_direct(&sql);
    }
    conn
}

/// 从 NestedEagerResult 提取关联行列表
fn extract_children_rows(nested: &[NestedEagerResult]) -> Vec<HashMap<String, Value>> {
    let mut result = Vec::new();
    for node in nested {
        for child in node.children() {
            result.push(child.row().clone());
        }
    }
    result
}

/// 从 EagerResult 提取关联行列表
fn extract_related_rows(
    eager_results: &[(HashMap<String, Value>, Vec<HashMap<String, Value>>)],
) -> Vec<HashMap<String, Value>> {
    let mut result = Vec::new();
    for (_, related) in eager_results {
        result.extend(related.iter().cloned());
    }
    result
}

// ============================================================================
// 关联类型等价性测试
// ============================================================================

#[tokio::test]
async fn test_hasone_equivalent_sqlite() {
    let mut conn = setup_sqlite();
    let relation = RelationDef::new(
        "profile",
        "users",
        "profiles",
        "id",
        "user_id",
        RelationKind::HasOne,
    );

    let smart_loader = SmartEagerLoader::new(relation.clone());
    let smart_results = smart_loader
        .load(&mut conn, "SELECT * FROM users")
        .await
        .unwrap();

    let manual_loader = EagerLoader::new(relation);
    let manual_results = manual_loader
        .load_many(&mut conn, "SELECT * FROM users")
        .await
        .unwrap();

    let smart_rows: Vec<HashMap<String, Value>> =
        smart_results.iter().map(|n| n.row().clone()).collect();
    let manual_rows: Vec<HashMap<String, Value>> =
        manual_results.iter().map(|(r, _)| r.clone()).collect();
    equivalence::assert_eager_equivalent(&smart_rows, &manual_rows, RelationKind::HasOne, "");

    let smart_related = extract_children_rows(&smart_results);
    let manual_related = extract_related_rows(&manual_results);
    equivalence::assert_eager_equivalent(
        &smart_related,
        &manual_related,
        RelationKind::HasOne,
        "user_id",
    );
}

#[tokio::test]
async fn test_hasmany_equivalent_sqlite() {
    let mut conn = setup_sqlite();
    let relation = RelationDef::new(
        "orders",
        "users",
        "orders",
        "id",
        "user_id",
        RelationKind::HasMany,
    );

    let smart_loader = SmartEagerLoader::new(relation.clone());
    let smart_results = smart_loader
        .load(&mut conn, "SELECT * FROM users")
        .await
        .unwrap();

    let manual_loader = EagerLoader::new(relation);
    let manual_results = manual_loader
        .load_many(&mut conn, "SELECT * FROM users")
        .await
        .unwrap();

    let smart_rows: Vec<HashMap<String, Value>> =
        smart_results.iter().map(|n| n.row().clone()).collect();
    let manual_rows: Vec<HashMap<String, Value>> =
        manual_results.iter().map(|(r, _)| r.clone()).collect();
    equivalence::assert_eager_equivalent(&smart_rows, &manual_rows, RelationKind::HasMany, "");

    let smart_related = extract_children_rows(&smart_results);
    let manual_related = extract_related_rows(&manual_results);
    equivalence::assert_eager_equivalent(
        &smart_related,
        &manual_related,
        RelationKind::HasMany,
        "user_id",
    );
}

#[tokio::test]
async fn test_many_to_many_equivalent_sqlite() {
    let mut conn = setup_sqlite();
    let relation = RelationDef::new_many_to_many(
        "roles",
        "users",
        "roles",
        "id",
        "id",
        "user_roles",
        "user_id",
        "role_id",
    );

    let smart_loader = SmartEagerLoader::new(relation.clone());
    let smart_results = smart_loader
        .load(&mut conn, "SELECT * FROM users")
        .await
        .unwrap();

    let manual_loader = EagerLoader::new(relation);
    let manual_results = manual_loader
        .load_many(&mut conn, "SELECT * FROM users")
        .await
        .unwrap();

    let smart_rows: Vec<HashMap<String, Value>> =
        smart_results.iter().map(|n| n.row().clone()).collect();
    let manual_rows: Vec<HashMap<String, Value>> =
        manual_results.iter().map(|(r, _)| r.clone()).collect();
    equivalence::assert_eager_equivalent(&smart_rows, &manual_rows, RelationKind::ManyToMany, "");
}

// ============================================================================
// 策略选择测试
// ============================================================================

#[tokio::test]
async fn test_join_strategy_sqlite() {
    let relation = RelationDef::new(
        "profile",
        "users",
        "profiles",
        "id",
        "user_id",
        RelationKind::HasOne,
    );
    let resolver = StrategyResolver::new();
    let decision = resolver.resolve(&relation);
    equivalence::assert_strategy_selected(&decision, LoadStrategy::Join);
}

#[tokio::test]
async fn test_dataloader_strategy_sqlite() {
    let relation = RelationDef::new(
        "orders",
        "users",
        "orders",
        "id",
        "user_id",
        RelationKind::HasMany,
    );
    let resolver = StrategyResolver::new();
    let decision = resolver.resolve(&relation);
    equivalence::assert_strategy_selected(&decision, LoadStrategy::DataLoader);
}

#[tokio::test]
async fn test_intermediate_strategy_sqlite() {
    let relation = RelationDef::new_many_to_many(
        "roles",
        "users",
        "roles",
        "id",
        "id",
        "user_roles",
        "user_id",
        "role_id",
    );
    let resolver = StrategyResolver::new();
    let decision = resolver.resolve(&relation);
    equivalence::assert_strategy_selected(&decision, LoadStrategy::IntermediateTableBatch);
}

// ============================================================================
// 嵌套深度测试
// ============================================================================

#[tokio::test]
async fn test_nested_depth_sqlite() {
    let mut conn = setup_sqlite();
    let relation = RelationDef::new(
        "orders",
        "users",
        "orders",
        "id",
        "user_id",
        RelationKind::HasMany,
    );

    let smart_loader = SmartEagerLoader::new(relation.clone());
    let smart_results = smart_loader
        .load(&mut conn, "SELECT * FROM users")
        .await
        .unwrap();

    let manual_loader = EagerLoader::new(relation);
    let manual_results = manual_loader
        .load_many(&mut conn, "SELECT * FROM users")
        .await
        .unwrap();

    assert_eq!(smart_results.len(), manual_results.len(), "主表行数应一致");

    for (smart_node, (_manual_row, manual_related)) in
        smart_results.iter().zip(manual_results.iter())
    {
        let smart_children = smart_node.children();
        assert_eq!(
            smart_children.len(),
            manual_related.len(),
            "子记录数量应一致: smart={} manual={}",
            smart_children.len(),
            manual_related.len()
        );
    }
}
