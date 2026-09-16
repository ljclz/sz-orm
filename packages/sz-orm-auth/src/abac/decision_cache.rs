//! ABAC 决策缓存
//!
//! 缓存授权决策结果，避免重复计算。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::policy_engine::Effect;

/// 缓存键
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    user_id: String,
    action: String,
    resource: String,
}

/// 缓存条目
struct CacheEntry {
    effect: Effect,
    expires_at: Instant,
}

/// 决策缓存
pub struct DecisionCache {
    entries: HashMap<CacheKey, CacheEntry>,
    ttl: Duration,
    hits: u64,
    misses: u64,
}

impl DecisionCache {
    /// 创建缓存
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            entries: HashMap::new(),
            ttl: Duration::from_secs(ttl_secs),
            hits: 0,
            misses: 0,
        }
    }

    /// 获取缓存的决策
    pub fn get(&mut self, user_id: &str, action: &str, resource: &str) -> Option<Effect> {
        let key = CacheKey {
            user_id: user_id.to_string(),
            action: action.to_string(),
            resource: resource.to_string(),
        };
        match self.entries.get(&key) {
            Some(entry) if entry.expires_at > Instant::now() => {
                self.hits += 1;
                Some(entry.effect.clone())
            }
            Some(_) => {
                self.entries.remove(&key);
                self.misses += 1;
                None
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    /// 存入决策
    pub fn put(&mut self, user_id: &str, action: &str, resource: &str, effect: Effect) {
        let key = CacheKey {
            user_id: user_id.to_string(),
            action: action.to_string(),
            resource: resource.to_string(),
        };
        self.entries.insert(
            key,
            CacheEntry {
                effect,
                expires_at: Instant::now() + self.ttl,
            },
        );
    }

    /// 清除缓存
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// 命中次数
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// 未命中次数
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// 缓存大小
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_put_get() {
        let mut cache = DecisionCache::new(60);
        cache.put("user1", "read", "data", Effect::Allow);
        assert_eq!(cache.get("user1", "read", "data"), Some(Effect::Allow));
        assert_eq!(cache.hits(), 1);
    }

    #[test]
    fn test_cache_miss() {
        let mut cache = DecisionCache::new(60);
        assert_eq!(cache.get("user1", "read", "data"), None);
        assert_eq!(cache.misses(), 1);
    }

    #[test]
    fn test_cache_clear() {
        let mut cache = DecisionCache::new(60);
        cache.put("user1", "read", "data", Effect::Allow);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_multiple_entries() {
        let mut cache = DecisionCache::new(60);
        cache.put("user1", "read", "data1", Effect::Allow);
        cache.put("user2", "write", "data2", Effect::Deny);
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get("user1", "read", "data1"), Some(Effect::Allow));
        assert_eq!(cache.get("user2", "write", "data2"), Some(Effect::Deny));
    }

    #[test]
    fn test_cache_ttl_expiration() {
        let mut cache = DecisionCache::new(1);
        cache.put("user1", "read", "data", Effect::Allow);
        std::thread::sleep(Duration::from_secs(2));
        assert_eq!(cache.get("user1", "read", "data"), None);
    }
}
