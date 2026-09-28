use sz_orm_core::relation_trait::{RelationDef, RelationKind, JoinKind};
use sz_orm_core::smart_eager_loader::{LoadStrategy, StrategyResolver};

#[test]
fn test_load_strategy_estimated_query_count() {
    assert_eq!(LoadStrategy::Join.estimated_query_count(), 1);
    assert_eq!(LoadStrategy::DataLoader.estimated_query_count(), 2);
    assert_eq!(LoadStrategy::IntermediateTableBatch.estimated_query_count(), 2);
}

#[test]
fn test_relation_kind_default_join_type() {
    assert_eq!(RelationKind::HasOne.default_join_type(), JoinKind::Inner);
    assert_eq!(RelationKind::BelongsTo.default_join_type(), JoinKind::Inner);
    assert_eq!(RelationKind::HasMany.default_join_type(), JoinKind::Left);
    assert_eq!(RelationKind::ManyToMany.default_join_type(), JoinKind::Left);
}

#[test]
fn test_join_kind_as_sql() {
    assert_eq!(JoinKind::Inner.as_sql(), "INNER JOIN");
    assert_eq!(JoinKind::Left.as_sql(), "LEFT JOIN");
}

#[test]
fn test_relation_def_new() {
    let rel = RelationDef::new("orders", "users", "orders", "id", "user_id", RelationKind::HasMany);
    assert_eq!(rel.name, "orders");
    assert_eq!(rel.from_entity, "users");
    assert_eq!(rel.to_entity, "orders");
    assert_eq!(rel.from_key, "id");
    assert_eq!(rel.to_key, "user_id");
    assert_eq!(rel.kind, RelationKind::HasMany);
    assert!(rel.join_table.is_none());
}

#[test]
fn test_relation_def_new_many_to_many() {
    let rel = RelationDef::new_many_to_many(
        "roles", "users", "roles", "id", "id",
        "user_roles", "user_id", "role_id",
    );
    assert_eq!(rel.kind, RelationKind::ManyToMany);
    assert_eq!(rel.join_table, Some("user_roles"));
    assert_eq!(rel.join_from_key, Some("user_id"));
    assert_eq!(rel.join_to_key, Some("role_id"));
}

#[test]
fn test_strategy_resolver_new() {
    let resolver = StrategyResolver::new();
    let _ = resolver;
}

#[test]
fn test_strategy_resolver_has_one() {
    let resolver = StrategyResolver::new();
    let rel = RelationDef::new("profile", "users", "profiles", "id", "user_id", RelationKind::HasOne);
    let decision = resolver.resolve(&rel);
    assert_eq!(decision.strategy, LoadStrategy::Join);
    assert_eq!(decision.estimated_query_count, 1);
    assert_eq!(decision.relation_name, "profile");
}

#[test]
fn test_strategy_resolver_belongs_to() {
    let resolver = StrategyResolver::new();
    let rel = RelationDef::new("user", "orders", "users", "user_id", "id", RelationKind::BelongsTo);
    let decision = resolver.resolve(&rel);
    assert_eq!(decision.strategy, LoadStrategy::Join);
}

#[test]
fn test_strategy_resolver_has_many() {
    let resolver = StrategyResolver::new();
    let rel = RelationDef::new("orders", "users", "orders", "id", "user_id", RelationKind::HasMany);
    let decision = resolver.resolve(&rel);
    assert_eq!(decision.strategy, LoadStrategy::DataLoader);
    assert_eq!(decision.estimated_query_count, 2);
}

#[test]
fn test_strategy_resolver_many_to_many_with_join_table() {
    let resolver = StrategyResolver::new();
    let rel = RelationDef::new_many_to_many(
        "roles", "users", "roles", "id", "id",
        "user_roles", "user_id", "role_id",
    );
    let decision = resolver.resolve(&rel);
    assert_eq!(decision.strategy, LoadStrategy::IntermediateTableBatch);
    assert_eq!(decision.estimated_query_count, 2);
}

#[test]
fn test_strategy_resolver_many_to_many_without_join_table() {
    let resolver = StrategyResolver::new();
    let rel = RelationDef::new("roles", "users", "roles", "id", "id", RelationKind::ManyToMany);
    let decision = resolver.resolve(&rel);
    assert_eq!(decision.strategy, LoadStrategy::DataLoader);
}

#[test]
fn test_strategy_resolver_resolve_chain() {
    let resolver = StrategyResolver::new();
    let rels = vec![
        RelationDef::new("profile", "users", "profiles", "id", "user_id", RelationKind::HasOne),
        RelationDef::new("orders", "users", "orders", "id", "user_id", RelationKind::HasMany),
    ];
    let decisions = resolver.resolve_chain(&rels);
    assert_eq!(decisions.len(), 2);
    assert_eq!(decisions[0].strategy, LoadStrategy::Join);
    assert_eq!(decisions[1].strategy, LoadStrategy::DataLoader);
}

#[test]
fn test_strategy_resolver_resolve_chain_empty() {
    let resolver = StrategyResolver::new();
    let decisions = resolver.resolve_chain(&[]);
    assert!(decisions.is_empty());
}

#[test]
fn test_strategy_decision_relation_kind() {
    let resolver = StrategyResolver::new();
    let rel = RelationDef::new("profile", "users", "profiles", "id", "user_id", RelationKind::HasOne);
    let decision = resolver.resolve(&rel);
    assert_eq!(decision.relation_kind, RelationKind::HasOne);
}

#[test]
fn test_strategy_decision_reason_not_empty() {
    let resolver = StrategyResolver::new();
    let rel = RelationDef::new("orders", "users", "orders", "id", "user_id", RelationKind::HasMany);
    let decision = resolver.resolve(&rel);
    assert!(!decision.reason.is_empty());
}