//! 意图缓存
//!
//! 缓存自然语言意图分析结果，避免重复 LLM 调用。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// 缓存条目
struct CacheEntry<V> {
    value: V,
    inserted_at: Instant,
}

/// 意图缓存
pub struct IntentCache<V> {
    entries: HashMap<String, CacheEntry<V>>,
    ttl: Duration,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl<V: Clone> IntentCache<V> {
    /// 创建缓存
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            ttl,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// 获取缓存值
    pub fn get(&mut self, key: &str) -> Option<V> {
        if let Some(entry) = self.entries.get(key) {
            if entry.inserted_at.elapsed() < self.ttl {
                self.hits.fetch_add(1, Ordering::Relaxed);
                return Some(entry.value.clone());
            }
            self.entries.remove(key);
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// 插入缓存值
    pub fn insert(&mut self, key: String, value: V) {
        self.entries.insert(
            key,
            CacheEntry {
                value,
                inserted_at: Instant::now(),
            },
        );
    }

    /// 清除过期条目
    pub fn evict_expired(&mut self) -> usize {
        let before = self.entries.len();
        self.entries
            .retain(|_, entry| entry.inserted_at.elapsed() < self.ttl);
        before - self.entries.len()
    }

    /// 缓存命中次数
    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    /// 缓存未命中次数
    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }

    /// 命中率
    pub fn hit_rate(&self) -> f64 {
        let h = self.hits() as f64;
        let m = self.misses() as f64;
        if h + m == 0.0 {
            0.0
        } else {
            h / (h + m)
        }
    }

    /// 当前条目数
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<V: Clone> Default for IntentCache<V> {
    fn default() -> Self {
        Self::new(Duration::from_secs(300))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_insert_get() {
        let mut cache: IntentCache<String> = IntentCache::default();
        cache.insert("query1".into(), "SELECT * FROM users".into());
        assert_eq!(cache.get("query1"), Some("SELECT * FROM users".into()));
        assert_eq!(cache.hits(), 1);
    }

    #[test]
    fn test_cache_miss() {
        let mut cache: IntentCache<i32> = IntentCache::default();
        assert_eq!(cache.get("missing"), None);
        assert_eq!(cache.misses(), 1);
    }

    #[test]
    fn test_cache_ttl_expiry() {
        let mut cache: IntentCache<String> = IntentCache::new(Duration::from_millis(1));
        cache.insert("key".into(), "value".into());
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(cache.get("key"), None);
    }

    #[test]
    fn test_hit_rate() {
        let mut cache: IntentCache<i32> = IntentCache::default();
        cache.insert("a".into(), 1);
        cache.get("a");
        cache.get("missing");
        let rate = cache.hit_rate();
        assert!((rate - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_evict_expired() {
        let mut cache: IntentCache<String> = IntentCache::new(Duration::from_millis(1));
        cache.insert("a".into(), "1".into());
        cache.insert("b".into(), "2".into());
        std::thread::sleep(Duration::from_millis(10));
        let evicted = cache.evict_expired();
        assert_eq!(evicted, 2);
        assert!(cache.is_empty());
    }
}
