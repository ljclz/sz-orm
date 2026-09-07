//! v6.6.0 查询结果缓存（QueryResultCache）
//!
//! 缓存查询结果集（数据级别），带 TTL + LRU 淘汰 + 容量限制。
//! 与 v6.5.0 的 PreparedStatementCache（句柄级别）互补：
//! 查询流程：QueryResultCache → PreparedStatementCache → DB
//!
//! # 缓存键
//! `(sql_hash, params_hash, tenant_id)` 三元组，避免存储完整 SQL 字符串。
//!
//! # 失效策略
//! - 被动失效：TTL 过期
//! - 容量淘汰：LRU 策略
//! - 主动失效：表写入触发相关查询缓存失效

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::pool::QueryRows;

/// 默认缓存容量（条目数）
const DEFAULT_CAPACITY: usize = 1024;

/// 默认 TTL（秒）
const DEFAULT_TTL_SECS: u64 = 300;

/// 查询结果缓存配置
#[derive(Debug, Clone)]
pub struct QueryResultCacheConfig {
    /// 最大缓存条目数
    pub capacity: usize,
    /// 单条目 TTL
    pub ttl: Duration,
}

impl Default for QueryResultCacheConfig {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_CAPACITY,
            ttl: Duration::from_secs(DEFAULT_TTL_SECS),
        }
    }
}

/// 缓存键：SQL hash + 参数 hash + 租户 ID
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// SQL 文本的 hash（FxHash 兼容）
    pub sql_hash: u64,
    /// 参数绑定的 hash
    pub params_hash: u64,
    /// 租户 ID（多租户场景隔离）
    pub tenant_id: Option<i64>,
}

impl CacheKey {
    /// 从 SQL + 参数 + 租户 ID 构造缓存键
    pub fn new(sql: &str, params: &[crate::Value], tenant_id: Option<i64>) -> Self {
        let sql_hash = hash_str(sql);
        let params_hash = hash_params(params);
        Self {
            sql_hash,
            params_hash,
            tenant_id,
        }
    }

    /// 仅从 SQL hash + 参数 hash 构造（无租户）
    pub fn from_hashes(sql_hash: u64, params_hash: u64) -> Self {
        Self {
            sql_hash,
            params_hash,
            tenant_id: None,
        }
    }
}

/// 缓存的查询结果
#[derive(Debug, Clone)]
pub struct CachedResult {
    /// 结果行
    pub rows: QueryRows,
    /// 缓存写入时间
    pub cached_at: Instant,
    /// 过期时间
    pub expires_at: Instant,
    /// 依赖的表（用于失效通知）
    pub depends_on: HashSet<String>,
}

impl CachedResult {
    /// 创建缓存结果
    pub fn new(rows: QueryRows, ttl: Duration, depends_on: HashSet<String>) -> Self {
        let now = Instant::now();
        Self {
            rows,
            cached_at: now,
            expires_at: now + ttl,
            depends_on,
        }
    }

    /// 是否已过期
    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }
}

/// 缓存统计信息
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// 命中次数
    pub hits: u64,
    /// 未命中次数
    pub misses: u64,
    /// 淘汰次数（LRU 或 TTL）
    pub evictions: u64,
    /// 主动失效次数
    pub invalidations: u64,
    /// 当前缓存条目数
    pub entry_count: usize,
}

impl CacheStats {
    /// 命中率（0.0 ~ 1.0）
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    /// miss 率（0.0 ~ 1.0）
    pub fn miss_rate(&self) -> f64 {
        1.0 - self.hit_rate()
    }

    /// 总查询次数
    pub fn total_queries(&self) -> u64 {
        self.hits + self.misses
    }
}

/// 查询结果缓存
///
/// LRU + TTL 双策略淘汰，线程安全（Mutex 保护）。
pub struct QueryResultCache {
    /// LRU 缓存（键 → 缓存结果）
    entries: Mutex<HashMap<CacheKey, CachedResult>>,
    /// LRU 访问顺序（最近访问在尾部）
    access_order: Mutex<VecDeque<CacheKey>>,
    /// 表 → 缓存键反向索引（用于主动失效）
    table_index: Mutex<HashMap<String, HashSet<CacheKey>>>,
    /// 统计信息
    stats: Mutex<CacheStats>,
    /// 配置
    config: QueryResultCacheConfig,
}

impl QueryResultCache {
    /// 创建查询结果缓存
    pub fn new(config: QueryResultCacheConfig) -> Arc<Self> {
        Arc::new(Self {
            entries: Mutex::new(HashMap::with_capacity(config.capacity)),
            access_order: Mutex::new(VecDeque::with_capacity(config.capacity)),
            table_index: Mutex::new(HashMap::new()),
            stats: Mutex::new(CacheStats::default()),
            config,
        })
    }

    /// 创建默认配置的缓存
    pub fn with_default() -> Arc<Self> {
        Self::new(QueryResultCacheConfig::default())
    }

    /// 查询缓存
    ///
    /// 返回 `Some(rows)` 表示命中（未过期），`None` 表示未命中。
    pub fn get(&self, key: &CacheKey) -> Option<QueryRows> {
        let entries = self.entries.lock().unwrap();
        let entry = match entries.get(key) {
            Some(e) => e,
            None => {
                drop(entries);
                self.record_miss();
                return None;
            }
        };
        if entry.is_expired() {
            drop(entries);
            self.remove_expired(key);
            self.record_miss();
            return None;
        }
        let rows = entry.rows.clone();
        drop(entries);
        self.touch_access(key);
        self.record_hit();
        Some(rows)
    }

    /// 写入缓存
    ///
    /// `depends_on`：该查询依赖的表名列表（用于主动失效）。
    pub fn put(&self, key: CacheKey, rows: QueryRows, depends_on: HashSet<String>) {
        let result = CachedResult::new(rows, self.config.ttl, depends_on.clone());
        {
            let mut entries = self.entries.lock().unwrap();
            if entries.len() >= self.config.capacity && !entries.contains_key(&key) {
                self.evict_lru(&mut entries);
            }
            entries.insert(key.clone(), result);
        }
        {
            let mut order = self.access_order.lock().unwrap();
            order.retain(|k| k != &key);
            order.push_back(key.clone());
        }
        {
            let mut idx = self.table_index.lock().unwrap();
            for table in depends_on {
                idx.entry(table).or_default().insert(key.clone());
            }
        }
        self.update_entry_count();
    }

    /// 主动失效：使依赖指定表的所有缓存失效
    ///
    /// INSERT/UPDATE/DELETE 后调用，避免返回脏数据。
    pub fn invalidate_table(&self, table: &str) -> usize {
        let keys_to_remove: Vec<CacheKey> = {
            let idx = self.table_index.lock().unwrap();
            idx.get(table)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect()
        };
        let count = keys_to_remove.len();
        if count == 0 {
            return 0;
        }
        {
            let mut entries = self.entries.lock().unwrap();
            for key in &keys_to_remove {
                entries.remove(key);
            }
        }
        {
            let mut order = self.access_order.lock().unwrap();
            order.retain(|k| !keys_to_remove.contains(k));
        }
        {
            let mut idx = self.table_index.lock().unwrap();
            if let Some(keys) = idx.get_mut(table) {
                keys.clear();
            }
        }
        {
            let mut stats = self.stats.lock().unwrap();
            stats.invalidations += count as u64;
        }
        self.update_entry_count();
        count
    }

    /// 清空所有缓存
    pub fn clear(&self) {
        {
            let mut entries = self.entries.lock().unwrap();
            entries.clear();
        }
        {
            let mut order = self.access_order.lock().unwrap();
            order.clear();
        }
        {
            let mut idx = self.table_index.lock().unwrap();
            idx.clear();
        }
        self.update_entry_count();
    }

    /// 获取统计信息
    pub fn stats(&self) -> CacheStats {
        self.stats.lock().unwrap().clone()
    }

    /// 清理所有过期条目
    ///
    /// 返回清理的条目数。
    pub fn purge_expired(&self) -> usize {
        let now = Instant::now();
        let expired_keys: Vec<CacheKey> = {
            let entries = self.entries.lock().unwrap();
            entries
                .iter()
                .filter(|(_, v)| now >= v.expires_at)
                .map(|(k, _)| k.clone())
                .collect()
        };
        let count = expired_keys.len();
        for key in &expired_keys {
            self.remove_expired(key);
        }
        count
    }

    fn touch_access(&self, key: &CacheKey) {
        let mut order = self.access_order.lock().unwrap();
        order.retain(|k| k != key);
        order.push_back(key.clone());
    }

    fn evict_lru(&self, entries: &mut HashMap<CacheKey, CachedResult>) {
        let key_to_evict = {
            let order = self.access_order.lock().unwrap();
            order.front().cloned()
        };
        if let Some(key) = key_to_evict {
            entries.remove(&key);
            let mut order = self.access_order.lock().unwrap();
            order.pop_front();
            let mut idx = self.table_index.lock().unwrap();
            for keys in idx.values_mut() {
                keys.remove(&key);
            }
            let mut stats = self.stats.lock().unwrap();
            stats.evictions += 1;
        }
    }

    fn remove_expired(&self, key: &CacheKey) {
        {
            let mut entries = self.entries.lock().unwrap();
            entries.remove(key);
        }
        {
            let mut order = self.access_order.lock().unwrap();
            order.retain(|k| k != key);
        }
        {
            let mut idx = self.table_index.lock().unwrap();
            for keys in idx.values_mut() {
                keys.remove(key);
            }
        }
        {
            let mut stats = self.stats.lock().unwrap();
            stats.evictions += 1;
        }
        self.update_entry_count();
    }

    fn record_hit(&self) {
        let mut stats = self.stats.lock().unwrap();
        stats.hits += 1;
    }

    fn record_miss(&self) {
        let mut stats = self.stats.lock().unwrap();
        stats.misses += 1;
    }

    fn update_entry_count(&self) {
        let entries = self.entries.lock().unwrap();
        let mut stats = self.stats.lock().unwrap();
        stats.entry_count = entries.len();
    }
}

fn hash_str(s: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

fn hash_params(params: &[crate::Value]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for param in params {
        format!("{param:?}").hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;

    fn make_rows(n: usize) -> QueryRows {
        (0..n)
            .map(|i| {
                let mut row = HashMap::new();
                row.insert("id".to_string(), Value::I64(i as i64));
                row
            })
            .collect()
    }

    #[test]
    fn test_cache_put_get_hit() {
        let cache = QueryResultCache::with_default();
        let key = CacheKey::new("SELECT * FROM users", &[], None);
        let rows = make_rows(3);
        cache.put(key.clone(), rows.clone(), HashSet::new());
        let got = cache.get(&key);
        assert!(got.is_some());
        assert_eq!(got.unwrap().len(), 3);
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 0);
    }

    #[test]
    fn test_cache_miss() {
        let cache = QueryResultCache::with_default();
        let key = CacheKey::new("SELECT * FROM users", &[], None);
        let got = cache.get(&key);
        assert!(got.is_none());
        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 1);
    }

    #[test]
    fn test_cache_ttl_expiry() {
        let config = QueryResultCacheConfig {
            capacity: 10,
            ttl: Duration::from_millis(10),
        };
        let cache = QueryResultCache::new(config);
        let key = CacheKey::new("SELECT 1", &[], None);
        cache.put(key.clone(), make_rows(1), HashSet::new());
        std::thread::sleep(Duration::from_millis(20));
        let got = cache.get(&key);
        assert!(got.is_none());
        let stats = cache.stats();
        assert!(stats.evictions >= 1);
    }

    #[test]
    fn test_cache_lru_eviction() {
        let config = QueryResultCacheConfig {
            capacity: 2,
            ttl: Duration::from_secs(60),
        };
        let cache = QueryResultCache::new(config);
        let k1 = CacheKey::from_hashes(1, 0);
        let k2 = CacheKey::from_hashes(2, 0);
        let k3 = CacheKey::from_hashes(3, 0);
        cache.put(k1.clone(), make_rows(1), HashSet::new());
        cache.put(k2.clone(), make_rows(1), HashSet::new());
        let _ = cache.get(&k1);
        cache.put(k3.clone(), make_rows(1), HashSet::new());
        assert!(cache.get(&k2).is_none());
        assert!(cache.get(&k1).is_some());
        assert!(cache.get(&k3).is_some());
    }

    #[test]
    fn test_cache_key_with_tenant() {
        let k1 = CacheKey::new("SELECT 1", &[], Some(1));
        let k2 = CacheKey::new("SELECT 1", &[], Some(2));
        assert_ne!(k1, k2);
        let k3 = CacheKey::new("SELECT 1", &[], Some(1));
        assert_eq!(k1, k3);
    }

    #[test]
    fn test_cache_key_params_hash() {
        let k1 = CacheKey::new("SELECT * FROM t WHERE id = ?", &[Value::I64(1)], None);
        let k2 = CacheKey::new("SELECT * FROM t WHERE id = ?", &[Value::I64(2)], None);
        assert_ne!(k1, k2);
        let k3 = CacheKey::new("SELECT * FROM t WHERE id = ?", &[Value::I64(1)], None);
        assert_eq!(k1, k3);
    }

    #[test]
    fn test_invalidate_table() {
        let cache = QueryResultCache::with_default();
        let key = CacheKey::new("SELECT * FROM users", &[], None);
        let mut deps = HashSet::new();
        deps.insert("users".to_string());
        cache.put(key.clone(), make_rows(1), deps);
        assert!(cache.get(&key).is_some());
        let count = cache.invalidate_table("users");
        assert_eq!(count, 1);
        assert!(cache.get(&key).is_none());
        let stats = cache.stats();
        assert!(stats.invalidations >= 1);
    }

    #[test]
    fn test_invalidate_unrelated_table() {
        let cache = QueryResultCache::with_default();
        let key = CacheKey::new("SELECT * FROM users", &[], None);
        let mut deps = HashSet::new();
        deps.insert("users".to_string());
        cache.put(key.clone(), make_rows(1), deps);
        let count = cache.invalidate_table("orders");
        assert_eq!(count, 0);
        assert!(cache.get(&key).is_some());
    }

    #[test]
    fn test_cache_stats_hit_rate() {
        let cache = QueryResultCache::with_default();
        let key = CacheKey::new("SELECT 1", &[], None);
        cache.put(key.clone(), make_rows(1), HashSet::new());
        let _ = cache.get(&key);
        let _ = cache.get(&CacheKey::from_hashes(999, 0));
        let stats = cache.stats();
        assert_eq!(stats.total_queries(), 2);
        assert!((stats.hit_rate() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_purge_expired() {
        let config = QueryResultCacheConfig {
            capacity: 10,
            ttl: Duration::from_millis(10),
        };
        let cache = QueryResultCache::new(config);
        let k1 = CacheKey::from_hashes(1, 0);
        let k2 = CacheKey::from_hashes(2, 0);
        cache.put(k1.clone(), make_rows(1), HashSet::new());
        cache.put(k2.clone(), make_rows(1), HashSet::new());
        std::thread::sleep(Duration::from_millis(20));
        let purged = cache.purge_expired();
        assert_eq!(purged, 2);
        assert!(cache.get(&k1).is_none());
        assert!(cache.get(&k2).is_none());
    }

    #[test]
    fn test_clear() {
        let cache = QueryResultCache::with_default();
        let k1 = CacheKey::from_hashes(1, 0);
        cache.put(k1.clone(), make_rows(1), HashSet::new());
        assert!(cache.get(&k1).is_some());
        cache.clear();
        assert!(cache.get(&k1).is_none());
    }

    #[test]
    fn test_capacity_limit() {
        let config = QueryResultCacheConfig {
            capacity: 3,
            ttl: Duration::from_secs(60),
        };
        let cache = QueryResultCache::new(config);
        for i in 0..5 {
            let key = CacheKey::from_hashes(i, 0);
            cache.put(key, make_rows(1), HashSet::new());
        }
        let stats = cache.stats();
        assert!(stats.entry_count <= 3);
    }
}
