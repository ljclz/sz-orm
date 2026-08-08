//! SmartEagerLoader PostgreSQL 方言集成测试（v2.4.0 任务 2.2）
//!
//! 验证 SmartEagerLoader 与手动 EagerLoader 在 PostgreSQL 下结果集等价。
//! 需真实 PostgreSQL 服务，标注 #[ignore]，通过 `cargo test -- --ignored` 触发。

mod common;

use common::equivalence;
use common::schema_builder::{TestDialect, TestSchemaBuilder};
use common::sqlx_pg_adapter::SqlxPgAdapter;
use sqlx::postgres::PgPoolOptions;
use std::collections::HashMap;
use sz_orm_core::eager_loader::{EagerLoader, NestedEagerResult};
use sz_orm_core::relation_trait::{RelationDef, RelationKind};
use sz_orm_core::smart_eager_loader::{LoadStrategy, SmartEagerLoader, StrategyResolver};
use sz_orm_core::{Connection, Value};

const PG_URL: &str = "postgres://postgres:szormtestpwd@127.0.0.1:5432/sz_orm_test";

async fn setup_pg() -> SqlxPgAdapter {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(PG_URL)
        .await
        .expect("PostgreSQL 不可用: postgres://postgres:szormtestpwd@127.0.0.1:5432/sz_orm_test");
    let mut conn = SqlxPgAdapter::new(pool);
    let builder = TestSchemaBuilder::new(TestDialect::Postgres);
    for ddl in builder.build_ddl() {
        conn.execute(&ddl).await.unwrap();
    }
    for sql in builder.seed_data() {
        conn.execute(&sql).await.unwrap();
    }
    conn
}

async fn teardown_pg(conn: &mut SqlxPgAdapter) {
    let builder = TestSchemaBuilder::new(TestDialect::Postgres);
    for ddl in builder.teardown_ddl() {
        let _ = conn.execute(&ddl).await;
    }
}

fn extract_children_rows(nested: &[NestedEagerResult]) -> Vec<HashMap<String, Value>> {
    let mut result = Vec::new();
    for node in nested {
        for child in node.children() {
            result.push(child.row().clone());
        }
    }
    result
}

fn extract_related_rows(
    eager_results: &[(HashMap<String, Value>, Vec<HashMap<String, Value>>)],
) -> Vec<HashMap<String, Value>> {
    let mut result = Vec::new();
    for (_, related) in eager_results {
        result.extend(related.iter().cloned());
    }
    result
}

#[tokio::test]
#[ignore]
async fn test_hasone_equivalent_pg() {
    let mut conn = setup_pg().await;
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

    let smart_rows: Vec<_> = smart_results.iter().map(|n| n.row().clone()).collect();
    let manual_rows: Vec<_> = manual_results.iter().map(|(r, _)| r.clone()).collect();
    equivalence::assert_eager_equivalent(&smart_rows, &manual_rows, RelationKind::HasOne, "");

    let smart_related = extract_children_rows(&smart_results);
    let manual_related = extract_related_rows(&manual_results);
    equivalence::assert_eager_equivalent(
        &smart_related,
        &manual_related,
        RelationKind::HasOne,
        "user_id",
    );

    teardown_pg(&mut conn).await;
}

#[tokio::test]
#[ignore]
async fn test_hasmany_equivalent_pg() {
    let mut conn = setup_pg().await;
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

    let smart_rows: Vec<_> = smart_results.iter().map(|n| n.row().clone()).collect();
    let manual_rows: Vec<_> = manual_results.iter().map(|(r, _)| r.clone()).collect();
    equivalence::assert_eager_equivalent(&smart_rows, &manual_rows, RelationKind::HasMany, "");

    let smart_related = extract_children_rows(&smart_results);
    let manual_related = extract_related_rows(&manual_results);
    equivalence::assert_eager_equivalent(
        &smart_related,
        &manual_related,
        RelationKind::HasMany,
        "user_id",
    );

    teardown_pg(&mut conn).await;
}

#[tokio::test]
#[ignore]
async fn test_many_to_many_equivalent_pg() {
    let mut conn = setup_pg().await;
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

    let smart_rows: Vec<_> = smart_results.iter().map(|n| n.row().clone()).collect();
    let manual_rows: Vec<_> = manual_results.iter().map(|(r, _)| r.clone()).collect();
    equivalence::assert_eager_equivalent(&smart_rows, &manual_rows, RelationKind::ManyToMany, "");

    teardown_pg(&mut conn).await;
}

#[tokio::test]
#[ignore]
async fn test_join_strategy_pg() {
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
#[ignore]
async fn test_dataloader_strategy_pg() {
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
#[ignore]
async fn test_intermediate_strategy_pg() {
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

#[tokio::test]
#[ignore]
async fn test_nested_depth_pg() {
    let mut conn = setup_pg().await;
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
            "子记录数量应一致"
        );
    }

    teardown_pg(&mut conn).await;
}
