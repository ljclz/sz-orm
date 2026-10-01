//! v9.2.0 M7-T39：CacheKey::by_pk/by_query/by_relation/to_string_key（4 tests）

use sz_orm_core::l2_cache::{CacheKey, CacheKeyKind};

#[test]
fn test_cache_key_by_pk_constructs_correctly() {
    let key = CacheKey::by_pk("users", 42);
    assert_eq!(key.table, "users");
    assert_eq!(key.kind, CacheKeyKind::ByPk);
    assert_eq!(key.identifier, "42");
}

#[test]
fn test_cache_key_by_query_constructs_correctly() {
    let key = CacheKey::by_query("orders", 0xdeadbeef_u64);
    assert_eq!(key.table, "orders");
    assert_eq!(key.kind, CacheKeyKind::ByQuery);
    assert_eq!(key.identifier, "3735928559");
}

#[test]
fn test_cache_key_by_relation_constructs_correctly() {
    let key = CacheKey::by_relation("posts", "author_id=1");
    assert_eq!(key.table, "posts");
    assert_eq!(key.kind, CacheKeyKind::ByRelation);
    assert_eq!(key.identifier, "author_id=1");
}

#[test]
fn test_cache_key_to_string_key_format() {
    let pk_key = CacheKey::by_pk("users", 1);
    assert_eq!(pk_key.to_string_key(), "l2:users:pk:1");

    let query_key = CacheKey::by_query("orders", 100);
    assert_eq!(query_key.to_string_key(), "l2:orders:q:100");

    let rel_key = CacheKey::by_relation("posts", "author=1");
    assert_eq!(rel_key.to_string_key(), "l2:posts:rel:author=1");

    let pk1 = CacheKey::by_pk("users", 1);
    let pk2 = CacheKey::by_pk("users", 1);
    assert_eq!(pk1.to_string_key(), pk2.to_string_key());
}
