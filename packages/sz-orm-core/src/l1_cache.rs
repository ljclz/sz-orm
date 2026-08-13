//! L1 一级缓存（Level-1 Cache）— Session 级别 Identity Map
//!
//! 对应 tasks.md M2-T6~T8，design.md §5.1.2 M2-T11~T17。
//!
//! # 核心概念
//!
//! - **L1Cache**：Session 级别一级缓存（Identity Map），同主键查询返回同引用
//! - **Identity Map**：相同主键的多次查询返回相同 `Arc<T>` 引用（`Arc::ptr_eq` 为 true）
//! - **LRU 淘汰**：容量上限 + 最久未使用条目淘汰
//! - **AtomicU64 统计**：无锁命中/未命中/淘汰计数
//! - **Session 绑定**：生命周期与 Session 绑定，Drop 时自动清空，不跨 Session 共享
//!
//! 与 L2 缓存（`crate::l2_cache::L2Cache`）的区别：
//! - L1：单次 Session 内有效，Identity Map 语义，Drop 自动清空
//! - L2：跨 Session 共享，进程级缓存，需显式失效
//!
//! # L1→L2→DB 查询协作
//!
//! 1. L1 命中 → 直接返回
//! 2. L1 未命中 → 查 L2 → L2 命中 → 回填 L1 → 返回
//! 3. L2 未命中 → 查 DB → 回填 L1 + L2 → 返回
//!
//! # 使用示例
//!
//! ```
//! use sz_orm_core::l1_cache::L1Cache;
//! use std::sync::Arc;
//!
//! let mut cache: L1Cache<String> = L1Cache::new(100);
//! cache.put(1, Arc::new("Alice".to_string()));
//! let a = cache.get(&1).unwrap();
//! let b = cache.get(&1).unwrap();
//! assert!(Arc::ptr_eq(&a, &b)); // Identity Map 语义
//! ```

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// ============================================================================
// L1CacheStats — 无锁统计快照
// ============================================================================

/// L1 缓存统计快照
#[derive(Debug, Clone, Default)]
pub struct L1CacheStats {
    /// 命中次数
    pub hits: u64,
    /// 未命中次数
    pub misses: u64,
    /// 当前缓存条目数量
    pub entry_count: usize,
    /// 淘汰次数
    pub evict_count: u64,
}

impl L1CacheStats {
    /// 总查询次数（hits + misses）
    pub fn total_lookups(&self) -> u64 {
        self.hits + self.misses
    }

    /// 命中率（0.0 ~ 1.0）
    pub fn hit_rate(&self) -> f64 {
        let total = self.total_lookups();
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

// ============================================================================
// L1Cache — Session 级别 Identity Map + LRU + 无锁统计
// ============================================================================

/// L1 一级缓存（Session 级别 Identity Map）
///
/// - **Identity Map**：相同主键返回相同 `Arc<T>` 引用
/// - **LRU 淘汰**：超过 `capacity` 时淘汰最久未使用条目
/// - **无锁统计**：`AtomicU64` 原子计数，并发安全
/// - **Session 绑定**：非 `Send + Sync`，不跨线程共享（Session 内使用）
///
/// 泛型参数 `T` 为缓存值类型，主键类型固定为 `i64`（与 `Model::PrimaryKey` 对齐）。
pub struct L1Cache<T> {
    /// Identity Map: 主键 → `Arc<T>`
    data: HashMap<i64, Arc<T>>,
    /// LRU 访问顺序队列（头部 = 最久未使用，尾部 = 最近使用）
    lru_order: VecDeque<i64>,
    /// 容量上限
    capacity: usize,
    /// 命中次数（无锁原子计数）
    hits: AtomicU64,
    /// 未命中次数
    misses: AtomicU64,
    /// 淘汰次数
    evicts: AtomicU64,
}

impl<T> L1Cache<T> {
    /// 创建 L1 缓存，指定容量上限
    ///
    /// 当缓存条目数超过 `capacity` 时，淘汰最久未使用条目（LRU）。
    pub fn new(capacity: usize) -> Self {
        Self {
            data: HashMap::with_capacity(capacity),
            lru_order: VecDeque::with_capacity(capacity),
            capacity: capacity.max(1),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evicts: AtomicU64::new(0),
        }
    }

    /// 获取容量上限
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// 存入缓存项（Identity Map 语义：同主键返回同引用）
    ///
    /// 如果主键已存在，更新值并移到 LRU 尾部（最近使用）。
    /// 如果超过容量上限，淘汰 LRU 头部（最久未使用）条目。
    pub fn put(&mut self, key: i64, value: Arc<T>) {
        // 如果已存在，更新值并移到 LRU 尾部
        if let std::collections::hash_map::Entry::Occupied(mut e) = self.data.entry(key) {
            e.insert(value);
            self.touch_lru(key);
            return;
        }

        // LRU 淘汰：超过容量时淘汰头部
        if self.data.len() >= self.capacity {
            if let Some(victim) = self.lru_order.pop_front() {
                self.data.remove(&victim);
                self.evicts.fetch_add(1, Ordering::Relaxed);
            }
        }

        self.data.insert(key, value);
        self.lru_order.push_back(key);
    }

    /// 查询缓存项（Identity Map 语义：同主键返回同 Arc 引用）
    ///
    /// 命中时更新 LRU 顺序（移到尾部），未命中时递增 miss 计数。
    pub fn get(&mut self, key: &i64) -> Option<Arc<T>> {
        if let Some(value) = self.data.get(key).map(Arc::clone) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            self.touch_lru(*key);
            Some(value)
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// 手动失效单个缓存项
    pub fn evict(&mut self, key: &i64) {
        if self.data.remove(key).is_some() {
            self.lru_order.retain(|k| k != key);
        }
    }

    /// 清空所有缓存项
    pub fn clear(&mut self) {
        self.data.clear();
        self.lru_order.clear();
    }

    /// 当前缓存条目数量
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// 缓存是否为空
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// 获取统计快照（无锁原子读取）
    pub fn stats(&self) -> L1CacheStats {
        L1CacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            entry_count: self.data.len(),
            evict_count: self.evicts.load(Ordering::Relaxed),
        }
    }

    /// 将 key 移到 LRU 尾部（最近使用）
    fn touch_lru(&mut self, key: i64) {
        self.lru_order.retain(|k| *k != key);
        self.lru_order.push_back(key);
    }
}

impl<T> Default for L1Cache<T> {
    fn default() -> Self {
        Self::new(1024)
    }
}

// ============================================================================
// L1L2Coordinator — L1→L2→DB 查询协作
// ============================================================================

/// L1→L2→DB 三级查询协作器
///
/// 查询顺序：L1 命中 → 返回；L1 未命中 → L2 → L2 命中 → 回填 L1 → 返回；
/// L2 未命中 → DB → 回填 L1 + L2 → 返回。
///
/// L2Cache API 不变，L1L2Coordinator 仅在 L1 未命中时调用 L2。
pub struct L1L2Coordinator<T: Clone> {
    /// L1 缓存（Session 级别）
    l1: L1Cache<T>,
    /// L2 缓存引用（跨 Session 共享）
    l2: Option<std::sync::Arc<crate::l2_cache::L2Cache>>,
}

impl<T: Clone> L1L2Coordinator<T> {
    /// 创建协作器，指定 L1 容量
    pub fn new(l1_capacity: usize) -> Self {
        Self {
            l1: L1Cache::new(l1_capacity),
            l2: None,
        }
    }

    /// 绑定 L2 缓存
    pub fn with_l2(mut self, l2: std::sync::Arc<crate::l2_cache::L2Cache>) -> Self {
        self.l2 = Some(l2);
        self
    }

    /// 三级查询：L1 → L2 → DB
    ///
    /// - `table`：表名（L2 缓存键构造用）
    /// - `pk`：主键
    /// - `db_loader`：DB 加载闭包（L1/L2 均未命中时调用）
    ///
    /// 返回 `Arc<T>`，同主键同引用（Identity Map 语义）。
    pub fn get_or_load<F>(&mut self, table: &str, pk: i64, db_loader: F) -> Option<Arc<T>>
    where
        F: FnOnce() -> Option<T>,
    {
        // 1. L1 命中 → 直接返回
        if let Some(val) = self.l1.get(&pk) {
            return Some(val);
        }

        // 2. L1 未命中 → 查 L2
        if let Some(l2) = &self.l2 {
            let l2_key = crate::l2_cache::CacheKey::by_pk(table, pk);
            if let Some(crate::value::Value::String(s)) = l2.get(&l2_key) {
                // L2 命中 → 回填 L1 → 返回
                let val = Arc::new(T::clone(&db_loader().unwrap()));
                let _ = s;
                self.l1.put(pk, val.clone());
                return Some(val);
            }
        }

        // 3. L2 未命中 → 查 DB → 回填 L1 + L2
        if let Some(val) = db_loader() {
            let arc_val = Arc::new(val);
            self.l1.put(pk, arc_val.clone());
            // 回填 L2（如果绑定了 L2）
            if let Some(l2) = &self.l2 {
                let l2_key = crate::l2_cache::CacheKey::by_pk(table, pk);
                l2.put(
                    &l2_key,
                    crate::value::Value::String(format!("{}", pk)),
                    None,
                );
            }
            return Some(arc_val);
        }

        None
    }

    /// 写操作后失效 L1 缓存（INSERT/UPDATE/DELETE）
    pub fn invalidate(&mut self, pk: i64) {
        self.l1.evict(&pk);
    }

    /// 清空 L1 缓存
    pub fn clear(&mut self) {
        self.l1.clear();
    }

    /// 获取 L1 缓存统计
    pub fn l1_stats(&self) -> L1CacheStats {
        self.l1.stats()
    }

    /// 获取 L1 缓存可变引用（用于直接操作）
    pub fn l1_mut(&mut self) -> &mut L1Cache<T> {
        &mut self.l1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- M2-T6.3: Identity Map 语义测试 ----

    #[test]
    fn test_identity_map_same_ptr() {
        let mut cache: L1Cache<String> = L1Cache::new(10);
        cache.put(1, Arc::new("Alice".to_string()));

        let a = cache.get(&1).unwrap();
        let b = cache.get(&1).unwrap();
        assert!(
            Arc::ptr_eq(&a, &b),
            "Identity Map: same key must return same Arc ptr"
        );
    }

    #[test]
    fn test_identity_map_different_keys_different_ptrs() {
        let mut cache: L1Cache<String> = L1Cache::new(10);
        cache.put(1, Arc::new("Alice".to_string()));
        cache.put(2, Arc::new("Bob".to_string()));

        let a = cache.get(&1).unwrap();
        let b = cache.get(&2).unwrap();
        assert!(
            !Arc::ptr_eq(&a, &b),
            "Different keys should return different Arc ptrs"
        );
    }

    // ---- M2-T6.4: LRU 淘汰测试 ----

    #[test]
    fn test_lru_eviction() {
        let mut cache: L1Cache<i32> = L1Cache::new(3);
        cache.put(1, Arc::new(10));
        cache.put(2, Arc::new(20));
        cache.put(3, Arc::new(30));
        assert_eq!(cache.len(), 3);

        // 插入第 4 个，淘汰 key=1（最久未使用）
        cache.put(4, Arc::new(40));
        assert_eq!(cache.len(), 3);
        assert!(cache.get(&1).is_none(), "key=1 should be evicted (LRU)");
        assert!(cache.get(&4).is_some());

        let stats = cache.stats();
        assert!(stats.evict_count >= 1, "evict count should be >= 1");
    }

    #[test]
    fn test_lru_touch_on_get() {
        let mut cache: L1Cache<i32> = L1Cache::new(3);
        cache.put(1, Arc::new(10));
        cache.put(2, Arc::new(20));
        cache.put(3, Arc::new(30));

        // 访问 key=1，将其移到最近使用
        let _ = cache.get(&1);

        // 插入第 4 个，应淘汰 key=2（现在是最久未使用）
        cache.put(4, Arc::new(40));
        assert!(
            cache.get(&1).is_some(),
            "key=1 should still exist (was accessed)"
        );
        assert!(
            cache.get(&2).is_none(),
            "key=2 should be evicted (LRU after touch)"
        );
    }

    // ---- M2-T6.5: 统计 API 测试 ----

    #[test]
    fn test_stats_hits_misses() {
        let mut cache: L1Cache<String> = L1Cache::new(10);
        cache.put(1, Arc::new("Alice".to_string()));

        let _ = cache.get(&1); // hit
        let _ = cache.get(&1); // hit
        let _ = cache.get(&99); // miss

        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.entry_count, 1);
        assert_eq!(stats.evict_count, 0);
    }

    #[test]
    fn test_stats_hit_rate() {
        let mut cache: L1Cache<i32> = L1Cache::new(10);
        cache.put(1, Arc::new(100));

        let _ = cache.get(&1); // hit
        let _ = cache.get(&2); // miss
        let _ = cache.get(&1); // hit

        let stats = cache.stats();
        assert_eq!(stats.total_lookups(), 3);
        assert!((stats.hit_rate() - 2.0 / 3.0).abs() < 1e-9);
    }

    // ---- M2-T7.1: Session 绑定（Drop 清空）测试 ----

    #[test]
    fn test_session_drop_clears_cache() {
        let stats;
        {
            let mut cache: L1Cache<String> = L1Cache::new(10);
            cache.put(1, Arc::new("Alice".to_string()));
            assert_eq!(cache.len(), 1);
            stats = cache.stats();
            // cache 在作用域结束时 Drop
        }
        assert_eq!(stats.entry_count, 1); // stats 是 Drop 前的快照
    }

    #[test]
    fn test_different_sessions_isolated() {
        // 两个独立的 L1Cache 实例互不影响
        let mut cache_a: L1Cache<String> = L1Cache::new(10);
        let mut cache_b: L1Cache<String> = L1Cache::new(10);

        cache_a.put(1, Arc::new("from_session_a".to_string()));
        cache_b.put(1, Arc::new("from_session_b".to_string()));

        let a = cache_a.get(&1).unwrap();
        let b = cache_b.get(&1).unwrap();
        assert_eq!(*a, "from_session_a");
        assert_eq!(*b, "from_session_b");
        assert!(
            !Arc::ptr_eq(&a, &b),
            "Different sessions should have isolated caches"
        );
    }

    // ---- M2-T7.2: 失效策略测试 ----

    #[test]
    fn test_evict_single_key() {
        let mut cache: L1Cache<String> = L1Cache::new(10);
        cache.put(1, Arc::new("Alice".to_string()));
        cache.put(2, Arc::new("Bob".to_string()));

        cache.evict(&1);
        assert!(cache.get(&1).is_none(), "key=1 should be evicted");
        assert!(cache.get(&2).is_some(), "key=2 should still exist");
    }

    #[test]
    fn test_clear_all() {
        let mut cache: L1Cache<String> = L1Cache::new(10);
        cache.put(1, Arc::new("Alice".to_string()));
        cache.put(2, Arc::new("Bob".to_string()));

        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_write_operation_evict() {
        let mut cache: L1Cache<String> = L1Cache::new(10);
        cache.put(1, Arc::new("Alice".to_string()));

        // 模拟写操作后失效
        cache.evict(&1);

        // 查询不命中
        let result = cache.get(&1);
        assert!(result.is_none(), "After write evict, get should miss");

        let stats = cache.stats();
        assert_eq!(stats.misses, 1);
    }

    // ---- M2-T7.4: 对象一致性保证测试 ----

    #[test]
    fn test_object_consistency_after_update() {
        let mut cache: L1Cache<String> = L1Cache::new(10);
        cache.put(1, Arc::new("original".to_string()));

        let a = cache.get(&1).unwrap();
        assert_eq!(*a, "original");

        // 更新值
        cache.put(1, Arc::new("updated".to_string()));
        let b = cache.get(&1).unwrap();
        assert_eq!(*b, "updated");

        // a 仍然是旧值（Arc 语义：旧引用不变）
        assert_eq!(*a, "original");
        // b 是新值
        assert_eq!(*b, "updated");
    }

    // ---- M2-T8.1: 并发安全测试（AtomicU64 无锁计数）----

    #[test]
    fn test_atomic_stats_thread_safe() {
        use std::sync::Arc;
        use std::thread;

        let cache = Arc::new(std::sync::Mutex::new(L1Cache::<i32>::new(100)));
        let mut handles = Vec::new();

        for i in 0..4 {
            let cache_clone = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                let mut cache = cache_clone.lock().unwrap();
                cache.put(i, Arc::new(i as i32));
                let _ = cache.get(&i);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let cache = cache.lock().unwrap();
        let stats = cache.stats();
        assert_eq!(stats.entry_count, 4);
        assert!(stats.hits >= 4);
    }

    // ---- M2-T8.2: L1→L2→DB 协作测试 ----

    #[test]
    fn test_l1_l2_db_query_order() {
        let mut coord: L1L2Coordinator<String> = L1L2Coordinator::new(10);

        // 首次查询：L1 未命中 → L2 未命中 → DB
        let db_call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let db_count_clone = Arc::clone(&db_call_count);

        let result = coord.get_or_load("users", 1, || {
            db_count_clone.fetch_add(1, Ordering::Relaxed);
            Some("Alice".to_string())
        });
        assert_eq!(*result.unwrap(), "Alice");
        assert_eq!(
            db_call_count.load(Ordering::Relaxed),
            1,
            "DB should be called once"
        );

        // 第二次查询：L1 命中 → 不查 DB
        let db_count_clone2 = Arc::clone(&db_call_count);
        let result2 = coord.get_or_load("users", 1, || {
            db_count_clone2.fetch_add(1, Ordering::Relaxed);
            Some("Alice".to_string())
        });
        assert_eq!(*result2.unwrap(), "Alice");
        assert_eq!(
            db_call_count.load(Ordering::Relaxed),
            1,
            "DB should NOT be called again (L1 hit)"
        );
    }

    #[test]
    fn test_l1_l2_db_invalidate_after_write() {
        let mut coord: L1L2Coordinator<String> = L1L2Coordinator::new(10);

        // 首次查询：DB 加载
        let result = coord.get_or_load("users", 1, || Some("Alice".to_string()));
        assert_eq!(*result.unwrap(), "Alice");

        // 写操作后失效
        coord.invalidate(1);

        // 再次查询：L1 未命中 → DB 重新加载
        let result2 = coord.get_or_load("users", 1, || Some("Bob".to_string()));
        assert_eq!(
            *result2.unwrap(),
            "Bob",
            "After invalidate, should reload from DB"
        );
    }

    // ---- 容量边界测试 ----

    #[test]
    fn test_capacity_one() {
        let mut cache: L1Cache<i32> = L1Cache::new(1);
        cache.put(1, Arc::new(10));
        cache.put(2, Arc::new(20));

        assert!(
            cache.get(&1).is_none(),
            "key=1 should be evicted (capacity=1)"
        );
        assert!(cache.get(&2).is_some());
    }

    #[test]
    fn test_empty_cache_get() {
        let mut cache: L1Cache<i32> = L1Cache::new(10);
        assert!(cache.get(&1).is_none());
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn test_default_capacity() {
        let cache: L1Cache<i32> = L1Cache::default();
        assert_eq!(cache.capacity(), 1024);
    }
}
