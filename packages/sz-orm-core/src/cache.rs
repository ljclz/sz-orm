//! Cache abstraction layer
//!
//! Provides multi-level caching with memory support

use crate::error::CacheError;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

pub trait Cache: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, CacheError>;
    fn set(&self, key: &str, value: Vec<u8>, ttl: Option<Duration>) -> Result<(), CacheError>;
    fn delete(&self, key: &str) -> Result<(), CacheError>;
    fn clear(&self) -> Result<(), CacheError>;
    fn exists(&self, key: &str) -> Result<bool, CacheError>;
    fn expire(&self, key: &str, ttl: Duration) -> Result<(), CacheError>;
    fn ttl(&self, key: &str) -> Result<Option<Duration>, CacheError>;
}

#[derive(Clone)]
pub struct MemoryCache {
    data: Arc<RwLock<HashMap<String, CacheEntry>>>,
    default_ttl: Option<Duration>,
}

struct CacheEntry {
    value: Vec<u8>,
    expires_at: Option<Instant>,
}

impl MemoryCache {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
            default_ttl: None,
        }
    }

    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
            default_ttl: Some(ttl),
        }
    }
}

impl Default for MemoryCache {
    fn default() -> Self {
        Self::new()
    }
}

impl Cache for MemoryCache {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, CacheError> {
        let data = self.data.read()?;
        if let Some(entry) = data.get(key) {
            if let Some(expires_at) = entry.expires_at {
                if expires_at <= Instant::now() {
                    return Ok(None);
                }
            }
            Ok(Some(entry.value.clone()))
        } else {
            Ok(None)
        }
    }

    fn set(&self, key: &str, value: Vec<u8>, ttl: Option<Duration>) -> Result<(), CacheError> {
        let expires_at = ttl.or(self.default_ttl).map(|d| Instant::now() + d);
        let mut data = self.data.write()?;
        data.insert(key.to_string(), CacheEntry { value, expires_at });
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), CacheError> {
        let mut data = self.data.write()?;
        data.remove(key);
        Ok(())
    }

    fn clear(&self) -> Result<(), CacheError> {
        let mut data = self.data.write()?;
        data.clear();
        Ok(())
    }

    fn exists(&self, key: &str) -> Result<bool, CacheError> {
        let data = self.data.read()?;
        if let Some(entry) = data.get(key) {
            if let Some(expires_at) = entry.expires_at {
                if expires_at <= Instant::now() {
                    return Ok(false);
                }
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn expire(&self, key: &str, ttl: Duration) -> Result<(), CacheError> {
        let mut data = self.data.write()?;
        if let Some(entry) = data.get_mut(key) {
            entry.expires_at = Some(Instant::now() + ttl);
            Ok(())
        } else {
            Err(CacheError::NotFound(key.to_string()))
        }
    }

    fn ttl(&self, key: &str) -> Result<Option<Duration>, CacheError> {
        let data = self.data.read()?;
        if let Some(entry) = data.get(key) {
            if let Some(expires_at) = entry.expires_at {
                if expires_at <= Instant::now() {
                    return Ok(None);
                }
                let remaining = expires_at.duration_since(Instant::now());
                Ok(Some(remaining))
            } else {
                Ok(None)
            }
        } else {
            Err(CacheError::NotFound(key.to_string()))
        }
    }
}

pub struct MultiLevelCache {
    caches: Vec<Box<dyn Cache>>,
}

impl MultiLevelCache {
    pub fn new() -> Self {
        Self { caches: Vec::new() }
    }

    pub fn add_cache(mut self, cache: Box<dyn Cache>) -> Self {
        self.caches.push(cache);
        self
    }
}

impl Default for MultiLevelCache {
    fn default() -> Self {
        Self::new()
    }
}

impl Cache for MultiLevelCache {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, CacheError> {
        for (i, cache) in self.caches.iter().enumerate() {
            if let Ok(Some(value)) = cache.get(key) {
                // 保留原始 TTL 信息：从命中的缓存层查询剩余 TTL，写回低层时使用
                let ttl = cache.ttl(key).ok().flatten();
                for j in 0..i {
                    let _ = self.caches[j].set(key, value.clone(), ttl);
                }
                return Ok(Some(value));
            }
        }
        Ok(None)
    }

    fn set(&self, key: &str, value: Vec<u8>, ttl: Option<Duration>) -> Result<(), CacheError> {
        for cache in &self.caches {
            cache.set(key, value.clone(), ttl)?;
        }
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), CacheError> {
        for cache in &self.caches {
            cache.delete(key)?;
        }
        Ok(())
    }

    fn clear(&self) -> Result<(), CacheError> {
        for cache in &self.caches {
            cache.clear()?;
        }
        Ok(())
    }

    fn exists(&self, key: &str) -> Result<bool, CacheError> {
        for cache in &self.caches {
            if cache.exists(key)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn expire(&self, key: &str, ttl: Duration) -> Result<(), CacheError> {
        for cache in &self.caches {
            cache.expire(key, ttl)?;
        }
        Ok(())
    }

    fn ttl(&self, key: &str) -> Result<Option<Duration>, CacheError> {
        if let Some(cache) = self.caches.first() {
            cache.ttl(key)
        } else {
            Err(CacheError::NotFound("No caches configured".to_string()))
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub sets: u64,
    pub deletes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_cache_set_get() {
        let cache = MemoryCache::new();
        cache.set("key1", b"value1".to_vec(), None).unwrap();
        let val = cache.get("key1").unwrap();
        assert_eq!(val, Some(b"value1".to_vec()));
    }

    #[test]
    fn test_memory_cache_delete() {
        let cache = MemoryCache::new();
        cache.set("key1", b"value1".to_vec(), None).unwrap();
        cache.delete("key1").unwrap();
        let val = cache.get("key1").unwrap();
        assert_eq!(val, None);
    }

    #[test]
    fn test_memory_cache_exists() {
        let cache = MemoryCache::new();
        cache.set("key1", b"value1".to_vec(), None).unwrap();
        let exists = cache.exists("key1").unwrap();
        assert!(exists);
        let exists2 = cache.exists("nonexistent").unwrap();
        assert!(!exists2);
    }

    #[test]
    fn test_memory_cache_clear() {
        let cache = MemoryCache::new();
        cache.set("key1", b"value1".to_vec(), None).unwrap();
        cache.set("key2", b"value2".to_vec(), None).unwrap();
        cache.clear().unwrap();
        let val = cache.get("key1").unwrap();
        assert_eq!(val, None);
    }

    #[test]
    fn test_memory_cache_with_ttl() {
        let cache = MemoryCache::with_ttl(Duration::from_secs(1));
        cache.set("key1", b"value1".to_vec(), None).unwrap();
        let val = cache.get("key1").unwrap();
        assert!(val.is_some());
    }

    #[test]
    fn test_cache_stats() {
        let stats = CacheStats::default();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
    }

    #[test]
    fn test_multi_level_cache() {
        let cache1 = MemoryCache::new();
        let cache2 = MemoryCache::new();
        let multi = MultiLevelCache::new()
            .add_cache(Box::new(cache1))
            .add_cache(Box::new(cache2));

        multi.set("key1", b"value1".to_vec(), None).unwrap();
        let val = multi.get("key1").unwrap();
        assert_eq!(val, Some(b"value1".to_vec()));

        multi.delete("key1").unwrap();
        let val = multi.get("key1").unwrap();
        assert_eq!(val, None);
    }
}
