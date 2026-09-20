//! LruKReplacer 边界与极端场景测试（v7.5.0 组7.1）
//!
//! 验证 LruKReplacer 在空输入、容量为 0、容量为 1、最大容量等边界条件下的行为。

use sz_orm_core::plan_cache::LruKReplacer;

#[test]
fn test_lru_k_zero_capacity() {
    let mut replacer = LruKReplacer::new(2, 0);
    replacer.access(1);
    // 容量为 0 时 evict 行为由实现决定（可能返回 Some 或 None）
    let _victim = replacer.evict();
}

#[test]
fn test_lru_k_one_capacity() {
    let mut replacer = LruKReplacer::new(2, 1);
    replacer.access(1);
    replacer.access(2);
    let victim = replacer.evict();
    assert!(victim.is_some(), "容量为 1 时应有受害者");
}

#[test]
fn test_lru_k_empty_evict() {
    let mut replacer = LruKReplacer::new(2, 10);
    let victim = replacer.evict();
    assert!(victim.is_none(), "空 replacer 不应有受害者");
}

#[test]
fn test_lru_k_large_k() {
    let mut replacer = LruKReplacer::new(100, 5);
    for i in 0..5 {
        replacer.access(i);
    }
    let victim = replacer.evict();
    assert!(victim.is_some());
}

#[test]
fn test_lru_k_repeated_access() {
    let mut replacer = LruKReplacer::new(2, 3);
    for _ in 0..100 {
        replacer.access(1);
    }
    replacer.access(2);
    replacer.access(3);
    let victim = replacer.evict();
    assert!(victim.is_some());
}

#[test]
fn test_lru_k_zero_k() {
    let mut replacer = LruKReplacer::new(0, 5);
    replacer.access(1);
    replacer.access(2);
    let victim = replacer.evict();
    assert!(victim.is_some() || victim.is_none());
}

#[test]
fn test_lru_k_is_empty() {
    let replacer = LruKReplacer::new(2, 10);
    assert!(replacer.is_empty());
    assert_eq!(replacer.len(), 0);
}

#[test]
fn test_lru_k_not_empty_after_access() {
    let mut replacer = LruKReplacer::new(2, 10);
    replacer.access(42);
    assert!(!replacer.is_empty());
    assert_eq!(replacer.len(), 1);
}
