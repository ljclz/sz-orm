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

// ===================== Read-Through / Write-Through 辅助方法 =====================

/// Read-through（同步版）：缓存未命中时通过 `loader` 回源加载，写入缓存后返回
///
/// # 参数
/// - `cache`：缓存实例
/// - `key`：缓存键
/// - `ttl`：写入缓存时的 TTL（`None` 表示永不过期）
/// - `loader`：回源加载闭包，返回 `Ok(None)` 表示数据源也无此键
///
/// # 返回
/// - `Ok(Some(v))`：命中缓存或回源加载成功
/// - `Ok(None)`：缓存与数据源均无此键
/// - `Err(_)`：缓存或加载过程出错
pub fn read_through<F>(
    cache: &dyn Cache,
    key: &str,
    ttl: Option<Duration>,
    loader: F,
) -> Result<Option<Vec<u8>>, CacheError>
where
    F: FnOnce() -> Result<Option<Vec<u8>>, CacheError>,
{
    // 1. 先查缓存
    if let Some(v) = cache.get(key)? {
        return Ok(Some(v));
    }
    // 2. 缓存未命中，回源加载
    let value = loader()?;
    // 3. 加载到值则写入缓存
    if let Some(ref v) = value {
        cache.set(key, v.clone(), ttl)?;
    }
    Ok(value)
}

/// Read-through（异步版）：缓存未命中时通过异步 `loader` 回源加载，写入缓存后返回
///
/// 用于数据库等异步数据源的回源加载场景。
pub async fn read_through_async<F, Fut>(
    cache: &dyn Cache,
    key: &str,
    ttl: Option<Duration>,
    loader: F,
) -> Result<Option<Vec<u8>>, CacheError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<Option<Vec<u8>>, CacheError>>,
{
    // 1. 先查缓存（同步操作）
    if let Some(v) = cache.get(key)? {
        return Ok(Some(v));
    }
    // 2. 缓存未命中，异步回源加载
    let value = loader().await?;
    // 3. 加载到值则写入缓存
    if let Some(ref v) = value {
        cache.set(key, v.clone(), ttl)?;
    }
    Ok(value)
}

/// Write-through（同步版）：同时写入缓存和后端存储（通过 `writer` 回调）
///
/// 写入顺序：先写后端存储，再写缓存。若后端存储写入失败，不写缓存。
///
/// # 参数
/// - `cache`：缓存实例
/// - `key`：缓存键
/// - `value`：值
/// - `ttl`：缓存 TTL
/// - `writer`：后端存储写入闭包，接收 `(key, value)`
pub fn write_through<F>(
    cache: &dyn Cache,
    key: &str,
    value: Vec<u8>,
    ttl: Option<Duration>,
    writer: F,
) -> Result<(), CacheError>
where
    F: FnOnce(&str, &[u8]) -> Result<(), CacheError>,
{
    // 1. 先写后端存储
    writer(key, &value)?;
    // 2. 再写缓存
    cache.set(key, value, ttl)?;
    Ok(())
}

/// Write-through（异步版）：同时写入缓存和异步后端存储
///
/// 写入顺序：先写后端存储，再写缓存。若后端存储写入失败，不写缓存。
pub async fn write_through_async<F, Fut>(
    cache: &dyn Cache,
    key: &str,
    value: Vec<u8>,
    ttl: Option<Duration>,
    writer: F,
) -> Result<(), CacheError>
where
    F: FnOnce(&str, Vec<u8>) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>, CacheError>>,
{
    // 1. 先写后端存储（writer 消费 value，返回写入后的值用于缓存）
    let stored = writer(key, value).await?;
    // 2. 再写缓存
    cache.set(key, stored, ttl)?;
    Ok(())
}

/// Write-around（写旁路）：仅写后端存储，同时失效缓存中的旧值
///
/// 适用于写多读少场景，避免频繁写入导致缓存频繁失效。
pub fn write_around<F>(
    cache: &dyn Cache,
    key: &str,
    writer: F,
) -> Result<(), CacheError>
where
    F: FnOnce(&str) -> Result<(), CacheError>,
{
    writer(key)?;
    cache.delete(key)?;
    Ok(())
}

// ===================== 负缓存支持 =====================

/// 缓存查找结果
///
/// 区分"缓存未命中"（需要回源）与"明确不存在"（负缓存命中，不回源），
/// 避免缓存穿透（大量请求查询不存在的 key 导致回源压力）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheLookup {
    /// 找到值
    Found(Vec<u8>),
    /// 缓存未命中（正缓存无值，需回源查询）
    Miss,
    /// 明确不存在（负缓存命中，在 TTL 期内不回源）
    NotFound,
}

/// 带负缓存的缓存包装器
///
/// 在正缓存（`inner`）之上叠加负缓存层：当 `get` 返回 `None` 时，将 key 记录到
/// 负缓存，在 `negative_ttl` 期内再次查询直接返回 `NotFound`，避免缓存穿透。
///
/// 写入（`set`）或删除（`delete`）key 时自动清除对应的负缓存条目。
pub struct NegativeCache<C: Cache> {
    /// 内部正缓存
    inner: C,
    /// 负缓存：key → 过期时刻
    negatives: RwLock<HashMap<String, Instant>>,
    /// 负缓存 TTL
    negative_ttl: Duration,
}

impl<C: Cache> NegativeCache<C> {
    /// 创建负缓存包装器，默认负缓存 TTL 为 60 秒
    pub fn new(inner: C) -> Self {
        Self {
            inner,
            negatives: RwLock::new(HashMap::new()),
            negative_ttl: Duration::from_secs(60),
        }
    }

    /// 设置负缓存 TTL
    pub fn with_negative_ttl(mut self, ttl: Duration) -> Self {
        self.negative_ttl = ttl;
        self
    }

    /// 获取值，区分"未命中"和"明确不存在"
    ///
    /// 查找顺序：负缓存 → 正缓存。
    /// - 负缓存命中且未过期 → `NotFound`（不回源）
    /// - 正缓存命中 → `Found`
    /// - 正缓存未命中 → 记录到负缓存，返回 `Miss`
    pub fn get_or_negative(&self, key: &str) -> Result<CacheLookup, CacheError> {
        // 先查负缓存
        let now = Instant::now();
        {
            let neg = self.negatives.read()?;
            if let Some(expires_at) = neg.get(key) {
                if *expires_at > now {
                    return Ok(CacheLookup::NotFound);
                }
            }
        }
        // 再查正缓存
        match self.inner.get(key)? {
            Some(v) => Ok(CacheLookup::Found(v)),
            None => {
                // 记录到负缓存（覆盖已过期的旧条目）
                let mut neg = self.negatives.write()?;
                neg.insert(key.to_string(), now + self.negative_ttl);
                Ok(CacheLookup::Miss)
            }
        }
    }

    /// 清理所有已过期的负缓存条目
    pub fn purge_expired(&self) -> Result<usize, CacheError> {
        let now = Instant::now();
        let mut neg = self.negatives.write()?;
        let before = neg.len();
        neg.retain(|_, expires_at| *expires_at > now);
        Ok(before - neg.len())
    }
}

impl<C: Cache> Cache for NegativeCache<C> {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, CacheError> {
        match self.get_or_negative(key)? {
            CacheLookup::Found(v) => Ok(Some(v)),
            CacheLookup::Miss | CacheLookup::NotFound => Ok(None),
        }
    }

    fn set(&self, key: &str, value: Vec<u8>, ttl: Option<Duration>) -> Result<(), CacheError> {
        // 写入正缓存时清除负缓存（key 现在有值了）
        {
            let mut neg = self.negatives.write()?;
            neg.remove(key);
        }
        self.inner.set(key, value, ttl)
    }

    fn delete(&self, key: &str) -> Result<(), CacheError> {
        {
            let mut neg = self.negatives.write()?;
            neg.remove(key);
        }
        self.inner.delete(key)
    }

    fn clear(&self) -> Result<(), CacheError> {
        {
            let mut neg = self.negatives.write()?;
            neg.clear();
        }
        self.inner.clear()
    }

    fn exists(&self, key: &str) -> Result<bool, CacheError> {
        self.inner.exists(key)
    }

    fn expire(&self, key: &str, ttl: Duration) -> Result<(), CacheError> {
        self.inner.expire(key, ttl)
    }

    fn ttl(&self, key: &str) -> Result<Option<Duration>, CacheError> {
        self.inner.ttl(key)
    }
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

    // ----- NegativeCache -----
    #[test]
    fn test_negative_cache_found() {
        let inner = MemoryCache::new();
        inner.set("key1", b"value1".to_vec(), None).unwrap();
        let cache = NegativeCache::new(inner);
        let result = cache.get_or_negative("key1").unwrap();
        assert_eq!(result, CacheLookup::Found(b"value1".to_vec()));
    }

    #[test]
    fn test_negative_cache_miss_then_not_found() {
        let inner = MemoryCache::new();
        let cache = NegativeCache::new(inner);
        // 第一次查询：正缓存未命中 → Miss，记录到负缓存
        let result = cache.get_or_negative("missing").unwrap();
        assert_eq!(result, CacheLookup::Miss);
        // 第二次查询：负缓存命中 → NotFound（不回源）
        let result = cache.get_or_negative("missing").unwrap();
        assert_eq!(result, CacheLookup::NotFound);
    }

    #[test]
    fn test_negative_cache_set_clears_negative() {
        let inner = MemoryCache::new();
        let cache = NegativeCache::new(inner);
        // 查询不存在的 key → Miss，记录负缓存
        cache.get_or_negative("key1").unwrap();
        assert_eq!(
            cache.get_or_negative("key1").unwrap(),
            CacheLookup::NotFound
        );
        // 写入值后负缓存应清除 → Found
        cache.set("key1", b"value1".to_vec(), None).unwrap();
        assert_eq!(
            cache.get_or_negative("key1").unwrap(),
            CacheLookup::Found(b"value1".to_vec())
        );
    }

    #[test]
    fn test_negative_cache_delete_clears_negative() {
        let inner = MemoryCache::new();
        inner.set("key1", b"value1".to_vec(), None).unwrap();
        let cache = NegativeCache::new(inner);
        // 删除后重新查询 → Miss（删除时清除负缓存，允许回源）
        cache.delete("key1").unwrap();
        let result = cache.get_or_negative("key1").unwrap();
        assert_eq!(result, CacheLookup::Miss);
    }

    #[test]
    fn test_negative_cache_ttl_expiry() {
        let inner = MemoryCache::new();
        let cache = NegativeCache::new(inner).with_negative_ttl(Duration::from_millis(50));
        // 查询 → Miss，记录负缓存
        cache.get_or_negative("key1").unwrap();
        assert_eq!(
            cache.get_or_negative("key1").unwrap(),
            CacheLookup::NotFound
        );
        // 等待 TTL 过期
        std::thread::sleep(Duration::from_millis(60));
        // 过期后应再次 Miss（回源）
        let result = cache.get_or_negative("key1").unwrap();
        assert_eq!(result, CacheLookup::Miss);
    }

    #[test]
    fn test_negative_cache_clear() {
        let inner = MemoryCache::new();
        let cache = NegativeCache::new(inner);
        cache.get_or_negative("key1").unwrap();
        cache.get_or_negative("key2").unwrap();
        cache.clear().unwrap();
        // clear 后应全部回源 → Miss
        assert_eq!(cache.get_or_negative("key1").unwrap(), CacheLookup::Miss);
        assert_eq!(cache.get_or_negative("key2").unwrap(), CacheLookup::Miss);
    }

    #[test]
    fn test_negative_cache_purge_expired() {
        let inner = MemoryCache::new();
        let cache = NegativeCache::new(inner).with_negative_ttl(Duration::from_millis(50));
        cache.get_or_negative("key1").unwrap();
        cache.get_or_negative("key2").unwrap();
        std::thread::sleep(Duration::from_millis(60));
        let purged = cache.purge_expired().unwrap();
        assert_eq!(purged, 2);
    }

    #[test]
    fn test_negative_cache_as_cache_trait() {
        let inner = MemoryCache::new();
        let cache = NegativeCache::new(inner);
        // 通过 Cache trait 的 get 方法访问：未命中返回 None
        let val = cache.get("missing").unwrap();
        assert_eq!(val, None);
        // 负缓存命中后仍返回 None（但内部不回源）
        let val = cache.get("missing").unwrap();
        assert_eq!(val, None);
        // 写入后返回值
        cache.set("key1", b"value1".to_vec(), None).unwrap();
        let val = cache.get("key1").unwrap();
        assert_eq!(val, Some(b"value1".to_vec()));
    }
}
