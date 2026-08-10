//! M2-T8.2: L1→L2→DB 协作集成测试
//! 覆盖三级查询顺序、命中回填、写操作失效、跨 Session 隔离

#![cfg(feature = "l1-cache")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use sz_orm_core::l1_cache::L1L2Coordinator;

#[test]
fn test_three_level_query_order() {
    let mut coord: L1L2Coordinator<String> = L1L2Coordinator::new(10);
    let db_calls = Arc::new(AtomicUsize::new(0));

    // 首次：L1 miss → L2 miss → DB
    let c = Arc::clone(&db_calls);
    let r1 = coord.get_or_load("users", 1, || {
        c.fetch_add(1, Ordering::Relaxed);
        Some("Alice".to_string())
    });
    assert_eq!(*r1.unwrap(), "Alice");
    assert_eq!(db_calls.load(Ordering::Relaxed), 1);

    // 二次：L1 hit → 不查 DB
    let c2 = Arc::clone(&db_calls);
    let r2 = coord.get_or_load("users", 1, || {
        c2.fetch_add(1, Ordering::Relaxed);
        Some("X".to_string())
    });
    assert_eq!(*r2.unwrap(), "Alice");
    assert_eq!(db_calls.load(Ordering::Relaxed), 1);
}

#[test]
fn test_write_invalidates_l1() {
    let mut coord: L1L2Coordinator<String> = L1L2Coordinator::new(10);
    let _ = coord.get_or_load("users", 1, || Some("original".to_string()));
    coord.invalidate(1);
    let r = coord.get_or_load("users", 1, || Some("updated".to_string()));
    assert_eq!(*r.unwrap(), "updated");
}

#[test]
fn test_cross_session_isolation() {
    let mut s1: L1L2Coordinator<String> = L1L2Coordinator::new(10);
    let mut s2: L1L2Coordinator<String> = L1L2Coordinator::new(10);

    let _ = s1.get_or_load("users", 1, || Some("from_s1".to_string()));
    let _ = s2.get_or_load("users", 1, || Some("from_s2".to_string()));

    // s1 的缓存不影响 s2
    let r1 = s1.get_or_load("users", 1, || Some("fallback".to_string()));
    let r2 = s2.get_or_load("users", 1, || Some("fallback".to_string()));
    assert_eq!(*r1.unwrap(), "from_s1");
    assert_eq!(*r2.unwrap(), "from_s2");
}

#[test]
fn test_clear_resets_cache() {
    let mut coord: L1L2Coordinator<String> = L1L2Coordinator::new(10);
    let _ = coord.get_or_load("users", 1, || Some("Alice".to_string()));
    coord.clear();
    let stats = coord.l1_stats();
    assert_eq!(stats.entry_count, 0);
}
