//! M1.2 验证：#[derive(RelationTrait)] 宏生成 RelationTrait 实现
//!
//! 测试宏展开后 `all_relations()` 返回正确的 `RelationDef` 静态切片

use sz_orm_core::relation_trait::{RelationKind, RelationTrait};
use sz_orm_core::RelationTrait as RelationTraitMacro;

#[derive(RelationTraitMacro)]
#[table(name = "users")]
#[relation(has_many = "orders", fk = "user_id", pk = "id")]
struct User {
    id: i64,
    name: String,
}

#[derive(RelationTraitMacro)]
#[table(name = "orders")]
#[relation(belongs_to = "users", fk = "user_id", pk = "id")]
struct Order {
    id: i64,
    user_id: i64,
}

#[derive(RelationTraitMacro)]
#[table(name = "user_profiles")]
#[relation(has_one = "profiles", fk = "user_id", pk = "id")]
struct UserProfile {
    id: i64,
    name: String,
}

#[test]
fn test_derive_relation_trait_has_many() {
    let relations = User::all_relations();
    assert_eq!(relations.len(), 1);
    let def = &relations[0];
    assert_eq!(def.name, "orders");
    assert_eq!(def.from_entity, "users");
    assert_eq!(def.to_entity, "orders");
    assert_eq!(def.from_key, "id");
    assert_eq!(def.to_key, "user_id");
    assert_eq!(def.kind, RelationKind::HasMany);
}

#[test]
fn test_derive_relation_trait_belongs_to() {
    let relations = Order::all_relations();
    assert_eq!(relations.len(), 1);
    let def = &relations[0];
    assert_eq!(def.name, "users");
    assert_eq!(def.kind, RelationKind::BelongsTo);
    assert_eq!(def.from_key, "user_id");
    assert_eq!(def.to_key, "id");
}

#[test]
fn test_derive_relation_trait_has_one() {
    let relations = UserProfile::all_relations();
    assert_eq!(relations.len(), 1);
    let def = &relations[0];
    assert_eq!(def.name, "profiles");
    assert_eq!(def.kind, RelationKind::HasOne);
}

#[test]
fn test_derive_relation_trait_def() {
    let user = User { id: 1, name: "test".into() };
    let def = user.def();
    assert_eq!(def.name, "orders");
}

#[test]
fn test_derive_relation_trait_relation_by_name() {
    let found = User::relation_by_name("orders");
    assert!(found.is_some());
    assert_eq!(found.unwrap().to_entity, "orders");

    let not_found = User::relation_by_name("unknown");
    assert!(not_found.is_none());
}

#[test]
fn test_derive_relation_trait_default_join_type() {
    let user_relations = User::all_relations();
    assert_eq!(
        user_relations[0].kind.default_join_type(),
        sz_orm_core::relation_trait::JoinKind::Left
    );

    let order_relations = Order::all_relations();
    assert_eq!(
        order_relations[0].kind.default_join_type(),
        sz_orm_core::relation_trait::JoinKind::Inner
    );
}