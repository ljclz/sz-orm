//! M2-T8.1: L1Cache 单元测试
//! 覆盖 Identity Map 语义、LRU 淘汰、生命周期、失效策略、统计 API、并发安全

#![cfg(feature = "l1-cache")]

use std::sync::atomic::Ordering;
use std::sync::Arc;
use sz_orm_core::l1_cache::{L1Cache, L1L2Coordinator};

#[test]
fn test_identity_map_same_ptr() {
    let mut cache: L1Cache<String> = L1Cache::new(10);
    cache.put(1, Arc::new("Alice".to_string()));
    let a = cache.get(&1).unwrap();
    let b = cache.get(&1).unwrap();
    assert!(Arc::ptr_eq(&a, &b));
}

#[test]
fn test_lru_eviction_over_capacity() {
    let mut cache: L1Cache<i32> = L1Cache::new(3);
    cache.put(1, Arc::new(10));
    cache.put(2, Arc::new(20));
    cache.put(3, Arc::new(30));
    cache.put(4, Arc::new(40));
    assert_eq!(cache.len(), 3);
    assert!(cache.get(&1).is_none());
    let stats = cache.stats();
    assert!(stats.evict_count >= 1);
}

#[test]
fn test_stats_accuracy() {
    let mut cache: L1Cache<String> = L1Cache::new(10);
    cache.put(1, Arc::new("Alice".to_string()));
    let _ = cache.get(&1);
    let _ = cache.get(&99);
    let stats = cache.stats();
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.entry_count, 1);
}

#[test]
fn test_evict_and_clear() {
    let mut cache: L1Cache<String> = L1Cache::new(10);
    cache.put(1, Arc::new("a".to_string()));
    cache.put(2, Arc::new("b".to_string()));
    cache.evict(&1);
    assert!(cache.get(&1).is_none());
    assert!(cache.get(&2).is_some());
    cache.clear();
    assert!(cache.is_empty());
}

#[test]
fn test_l1_l2_db_coordinator() {
    let mut coord: L1L2Coordinator<String> = L1L2Coordinator::new(10);
    let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let c = Arc::clone(&counter);
    let r1 = coord.get_or_load("users", 1, || {
        c.fetch_add(1, Ordering::Relaxed);
        Some("Alice".to_string())
    });
    assert_eq!(*r1.unwrap(), "Alice");
    let c2 = Arc::clone(&counter);
    let r2 = coord.get_or_load("users", 1, || {
        c2.fetch_add(1, Ordering::Relaxed);
        Some("Alice".to_string())
    });
    assert_eq!(*r2.unwrap(), "Alice");
    assert_eq!(
        counter.load(Ordering::Relaxed),
        1,
        "DB should be called only once (L1 hit second time)"
    );
}

#[test]
fn test_coordinator_invalidate() {
    let mut coord: L1L2Coordinator<String> = L1L2Coordinator::new(10);
    let _ = coord.get_or_load("users", 1, || Some("Alice".to_string()));
    coord.invalidate(1);
    let r = coord.get_or_load("users", 1, || Some("Bob".to_string()));
    assert_eq!(*r.unwrap(), "Bob");
}

#[test]
fn test_concurrent_stats_thread_safe() {
    let cache = Arc::new(std::sync::Mutex::new(L1Cache::<i32>::new(100)));
    let mut handles = Vec::new();
    for i in 0..8 {
        let cc = Arc::clone(&cache);
        handles.push(std::thread::spawn(move || {
            let mut c = cc.lock().unwrap();
            c.put(i, Arc::new(i as i32));
            let _ = c.get(&i);
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let c = cache.lock().unwrap();
    let stats = c.stats();
    assert_eq!(stats.entry_count, 8);
    assert!(stats.hits >= 8);
}
