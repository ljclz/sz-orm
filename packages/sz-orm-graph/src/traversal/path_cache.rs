//! 遍历路径缓存
//!
//! 缓存已编译的遍历路径，避免重复编译。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// 遍历路径缓存
pub struct TraversalPathCache {
    cache: HashMap<String, String>,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl Default for TraversalPathCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TraversalPathCache {
    /// 创建缓存
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// 获取缓存的 SQL
    pub fn get(&self, path: &str) -> Option<&String> {
        if let Some(sql) = self.cache.get(path) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            Some(sql)
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// 插入缓存
    pub fn insert(&mut self, path: String, sql: String) {
        self.cache.insert(path, sql);
    }

    /// 命中次数
    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    /// 未命中次数
    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }

    /// 缓存大小
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_get_insert() {
        let mut cache = TraversalPathCache::new();
        cache.insert(
            "users.posts".into(),
            "SELECT * FROM users LEFT JOIN posts...".into(),
        );
        assert_eq!(
            cache.get("users.posts"),
            Some(&"SELECT * FROM users LEFT JOIN posts...".to_string())
        );
        assert_eq!(cache.hits(), 1);
    }

    #[test]
    fn test_cache_miss() {
        let cache = TraversalPathCache::new();
        assert!(cache.get("missing").is_none());
        assert_eq!(cache.misses(), 1);
    }

    #[test]
    fn test_cache_len() {
        let mut cache = TraversalPathCache::new();
        cache.insert("a".into(), "sql1".into());
        cache.insert("b".into(), "sql2".into());
        assert_eq!(cache.len(), 2);
    }
}
