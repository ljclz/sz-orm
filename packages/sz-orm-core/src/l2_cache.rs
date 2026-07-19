//! L2 二级缓存（Level-2 Cache）
//!
//! 对应文档 6.8 节改进项 21（L2 二级缓存）。
//!
//! # 核心概念
//!
//! - **L2Cache**：跨 Session 共享的二级缓存（与 Hibernate L2 Cache / MyBatis 二级缓存对应）
//! - **CacheKey**：统一缓存键构造（table + pk 或 table + query_hash）
//! - **L2CacheStats**：命中率统计（hits/misses/evictions/sets）
//! - **表级失效**：`invalidate_table(table)` 一次失效某表的所有缓存项
//!
//! 与 L1 缓存（Session 级别）的区别：
//! - L1：单次 Session/请求 内有效，事务结束自动清空
//! - L2：跨 Session 共享，进程级缓存，需显式失效
//!
//! # 设计灵感
//!
//! - Hibernate L2 Cache（`@Cache` / `@Cacheable`）
//! - MyBatis 二级缓存（`<cache>` 标签）
//! - Rails `Rails.cache`
//! - Django cache framework
//!
//! # 使用示例
//!
//! ```no_run
//! use sz_orm_core::l2_cache::{L2Cache, CacheKey};
//! use sz_orm_core::Value;
//!
//! // 1. 创建 L2 缓存
//! let cache = L2Cache::new();
//!
//! // 2. 缓存单行（pk 维度）
//! let key = CacheKey::by_pk("users", 1);
//! cache.put(&key, Value::String("Alice".to_string()), None);
//!
//! // 3. 读取
//! let val = cache.get(&key);
//! assert!(val.is_some());
//!
//! // 4. 表级失效（用户表更新后）
//! cache.invalidate_table("users");
//! assert!(cache.get(&key).is_none());
//!
//! // 5. 命中率统计
//! let stats = cache.stats();
//! println!("hit rate: {:.2}%", stats.hit_rate() * 100.0);
//! ```

use crate::value::Value;
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

// ============================================================================
// CacheKey — 统一缓存键
// ============================================================================

/// 统一缓存键
///
/// 通过 `table` + `kind` + `identifier` 三元组唯一标识一个缓存项：
/// - `table`：表名（用于表级失效）
/// - `kind`：缓存类型（ByPk / ByQuery / ByRelation）
/// - `identifier`：具体标识（pk 值 / 查询哈希 / 关联键）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// 表名
    pub table: String,
    /// 缓存类型
    pub kind: CacheKeyKind,
    /// 具体标识
    pub identifier: String,
}

/// 缓存键类型
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CacheKeyKind {
    /// 按主键缓存
    ByPk,
    /// 按查询条件缓存
    ByQuery,
    /// 按关联关系缓存
    ByRelation,
}

impl CacheKey {
    /// 构造主键维度的缓存键
    pub fn by_pk(table: impl Into<String>, pk: impl std::fmt::Display) -> Self {
        Self {
            table: table.into(),
            kind: CacheKeyKind::ByPk,
            identifier: pk.to_string(),
        }
    }

    /// 构造查询维度的缓存键（identifier 通常是 SQL + params 的哈希）
    pub fn by_query(table: impl Into<String>, query_hash: impl std::fmt::Display) -> Self {
        Self {
            table: table.into(),
            kind: CacheKeyKind::ByQuery,
            identifier: query_hash.to_string(),
        }
    }

    /// 构造关联维度的缓存键
    pub fn by_relation(table: impl Into<String>, relation: impl std::fmt::Display) -> Self {
        Self {
            table: table.into(),
            kind: CacheKeyKind::ByRelation,
            identifier: relation.to_string(),
        }
    }

    /// 序列化为字符串（用于底层存储键）
    pub fn to_string_key(&self) -> String {
        let kind_str = match self.kind {
            CacheKeyKind::ByPk => "pk",
            CacheKeyKind::ByQuery => "q",
            CacheKeyKind::ByRelation => "rel",
        };
        format!("l2:{}:{}:{}", self.table, kind_str, self.identifier)
    }
}

impl std::fmt::Display for CacheKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string_key())
    }
}

// ============================================================================
// L2CacheStats — 命中率统计
// ============================================================================

/// L2 缓存命中率统计
#[derive(Debug, Clone, Default)]
pub struct L2CacheStats {
    /// 命中次数
    pub hits: u64,
    /// 未命中次数
    pub misses: u64,
    /// 设置次数
    pub sets: u64,
    /// 失效次数（含单键和表级失效）
    pub evictions: u64,
    /// 当前缓存项数量
    pub size: usize,
}

impl L2CacheStats {
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

    /// 未命中率（0.0 ~ 1.0）
    pub fn miss_rate(&self) -> f64 {
        1.0 - self.hit_rate()
    }

    /// 合并两个统计（用于多分片汇总）
    pub fn merge(&mut self, other: &L2CacheStats) {
        self.hits += other.hits;
        self.misses += other.misses;
        self.sets += other.sets;
        self.evictions += other.evictions;
        self.size += other.size;
    }
}

// ============================================================================
// CacheEntry — 缓存项
// ============================================================================

/// 缓存项（值 + 过期时间）
#[derive(Debug, Clone)]
struct CacheEntry {
    /// 缓存值
    value: Value,
    /// 过期时间（None 表示永不过期）
    expires_at: Option<Instant>,
}

impl CacheEntry {
    fn new(value: Value, ttl: Option<Duration>) -> Self {
        // Duration::MAX 会导致 Instant::now() + Duration::MAX 溢出
        // 将其视为永不过期（expires_at = None），与 None 语义一致
        let expires_at = ttl.and_then(|d| {
            if d == Duration::MAX {
                None
            } else {
                Some(Instant::now() + d)
            }
        });
        Self { value, expires_at }
    }

    fn is_expired(&self) -> bool {
        self.expires_at
            .map(|t| t <= Instant::now())
            .unwrap_or(false)
    }
}

// ============================================================================
// L2Cache — 跨 Session 共享的二级缓存
// ============================================================================

/// L2 二级缓存 — 跨 Session 共享
///
/// 线程安全：内部使用 RwLock，可在多线程环境下共享。
///
/// # 示例
///
/// ```
/// use sz_orm_core::l2_cache::{L2Cache, CacheKey};
/// use sz_orm_core::Value;
/// use std::time::Duration;
///
/// let cache = L2Cache::new();
///
/// // 缓存单行
/// let key = CacheKey::by_pk("users", 1);
/// cache.put(&key, Value::String("Alice".to_string()), None);
///
/// // 读取
/// assert!(cache.get(&key).is_some());
///
/// // 表级失效
/// cache.invalidate_table("users");
/// assert!(cache.get(&key).is_none());
/// ```
pub struct L2Cache {
    /// 缓存数据
    data: RwLock<HashMap<String, CacheEntry>>,
    /// 表名索引（用于表级失效）— table -> Vec<key_string>（去重）
    table_index: RwLock<HashMap<String, Vec<String>>>,
    /// LRU 访问顺序（尾部为最近访问，头部为最久未访问）
    ///
    /// # 锁顺序约定
    ///
    /// 跨字段持锁时遵循：`data` → `access_order` → `table_index` → `stats`，
    /// 避免死锁。本字段不允许在持 `data` 写锁时获取其他写锁。
    access_order: RwLock<Vec<String>>,
    /// 统计信息
    stats: RwLock<L2CacheStats>,
    /// 默认 TTL（`put` 传 `None` 时使用，要"永不失效"请传 `Some(Duration::MAX)`）
    default_ttl: Option<Duration>,
    /// 最大容量（LRU 淘汰）
    max_size: usize,
}

impl Default for L2Cache {
    fn default() -> Self {
        Self::new()
    }
}

impl L2Cache {
    /// 创建 L2 缓存（默认容量 10000，无 TTL）
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
            table_index: RwLock::new(HashMap::new()),
            access_order: RwLock::new(Vec::new()),
            stats: RwLock::new(L2CacheStats::default()),
            default_ttl: None,
            max_size: 10_000,
        }
    }

    /// 设置默认 TTL
    pub fn with_default_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = Some(ttl);
        self
    }

    /// 设置最大容量
    pub fn with_max_size(mut self, max_size: usize) -> Self {
        self.max_size = max_size;
        self
    }

    /// 存入缓存项
    ///
    /// # TTL 语义
    ///
    /// - `ttl = Some(d)`：使用 `d` 作为过期时间
    /// - `ttl = None`：使用 `default_ttl`（若未设置则永不过期）
    /// - 要显式表示"永不失效"，请传 `Some(Duration::MAX)`
    pub fn put(&self, key: &CacheKey, value: Value, ttl: Option<Duration>) {
        let actual_ttl = ttl.or(self.default_ttl);
        let entry = CacheEntry::new(value, actual_ttl);
        let key_str = key.to_string_key();

        // 1. 写入数据 + LRU 淘汰
        let is_new_key = {
            let mut data = self.data.write().unwrap();
            let exists = data.contains_key(&key_str);
            if !exists && data.len() >= self.max_size {
                // LRU 淘汰：优先淘汰已过期的 key，否则淘汰 access_order 头部
                let victim = {
                    // 不在持 data 写锁时获取 access_order 写锁，先读 access_order
                    let order = self.access_order.read().unwrap();
                    // 优先找已过期的 key
                    order
                        .iter()
                        .find(|k| data.get(*k).map(|e| e.is_expired()).unwrap_or(false))
                        .cloned()
                        .or_else(|| order.first().cloned())
                };
                if let Some(victim) = victim {
                    data.remove(&victim);
                    // 同步清理 access_order
                    let mut order = self.access_order.write().unwrap();
                    order.retain(|k| k != &victim);
                }
            }
            data.insert(key_str.clone(), entry);
            !exists
        };

        // 2. 更新 LRU 访问顺序（key 移到尾部）
        {
            let mut order = self.access_order.write().unwrap();
            if is_new_key {
                order.push(key_str.clone());
            } else {
                // 已存在的 key：移到尾部
                order.retain(|k| k != &key_str);
                order.push(key_str.clone());
            }
        }

        // 3. 更新表索引（去重，避免重复 push 导致 invalidate_table 统计错误）
        {
            let mut idx = self.table_index.write().unwrap();
            let keys = idx.entry(key.table.clone()).or_default();
            if !keys.contains(&key_str) {
                keys.push(key_str);
            }
        }

        // 4. 更新统计（不在此处读取 data.len()，避免锁顺序敏感）
        {
            let mut stats = self.stats.write().unwrap();
            stats.sets += 1;
        }
    }

    /// 读取缓存项（不存在或已过期返回 None）
    ///
    /// 命中时会更新 LRU 访问顺序（移到尾部）。
    pub fn get(&self, key: &CacheKey) -> Option<Value> {
        let key_str = key.to_string_key();
        let result = {
            let data = self.data.read().ok()?;
            if let Some(entry) = data.get(&key_str) {
                if entry.is_expired() {
                    None
                } else {
                    Some(entry.value.clone())
                }
            } else {
                None
            }
        };

        // 命中时更新 LRU 顺序（移到尾部）
        if result.is_some() {
            let mut order = self.access_order.write().unwrap();
            order.retain(|k| k != &key_str);
            order.push(key_str);
        }

        // 更新统计
        if let Ok(mut stats) = self.stats.write() {
            if result.is_some() {
                stats.hits += 1;
            } else {
                stats.misses += 1;
            }
        }

        result
    }

    /// 失效单个缓存项
    pub fn invalidate(&self, key: &CacheKey) {
        let key_str = key.to_string_key();
        let removed = {
            let mut data = self.data.write().unwrap();
            data.remove(&key_str).is_some()
        };
        if removed {
            let mut order = self.access_order.write().unwrap();
            order.retain(|k| k != &key_str);
        }
        if removed {
            let mut stats = self.stats.write().unwrap();
            stats.evictions += 1;
        }
    }

    /// 失效整张表的所有缓存项
    ///
    /// 仅统计实际从缓存中删除的 key 数量，避免 evictions 偏大。
    pub fn invalidate_table(&self, table: &str) {
        let keys_to_remove: Vec<String> = {
            let idx = match self.table_index.read() {
                Ok(i) => i,
                Err(_) => return,
            };
            idx.get(table).cloned().unwrap_or_default()
        };

        let mut actually_removed: usize = 0;
        {
            let mut data = self.data.write().unwrap();
            for k in &keys_to_remove {
                if data.remove(k).is_some() {
                    actually_removed += 1;
                }
            }
        }

        if actually_removed > 0 {
            let mut order = self.access_order.write().unwrap();
            order.retain(|k| !keys_to_remove.contains(k));
        }

        if let Ok(mut idx) = self.table_index.write() {
            idx.remove(table);
        }
        if actually_removed > 0 {
            let mut stats = self.stats.write().unwrap();
            stats.evictions += actually_removed as u64;
        }
    }

    /// 清空所有缓存
    pub fn clear(&self) {
        let removed = {
            let mut data = self.data.write().unwrap();
            let n = data.len();
            data.clear();
            n
        };
        if let Ok(mut order) = self.access_order.write() {
            order.clear();
        }
        if let Ok(mut idx) = self.table_index.write() {
            idx.clear();
        }
        if removed > 0 {
            let mut stats = self.stats.write().unwrap();
            stats.evictions += removed as u64;
            stats.size = 0;
        }
    }

    /// 获取当前缓存项数量
    pub fn size(&self) -> usize {
        self.data.read().map(|d| d.len()).unwrap_or(0)
    }

    /// 获取统计信息
    pub fn stats(&self) -> L2CacheStats {
        let mut s = self.stats.read().map(|s| s.clone()).unwrap_or_default();
        // 实时同步 size 字段（不写入 stats，避免持锁读 data）
        s.size = self.size();
        s
    }

    /// 重置统计信息
    pub fn reset_stats(&self) {
        if let Ok(mut stats) = self.stats.write() {
            *stats = L2CacheStats::default();
        }
    }

    /// 检查缓存项是否存在（不更新统计与 LRU 顺序）
    pub fn contains(&self, key: &CacheKey) -> bool {
        let key_str = key.to_string_key();
        self.data
            .read()
            .map(|d| d.get(&key_str).map(|e| !e.is_expired()).unwrap_or(false))
            .unwrap_or(false)
    }

    /// 手动清理所有过期项
    pub fn evict_expired(&self) -> usize {
        let expired_keys: Vec<String> = {
            let data = self.data.read().unwrap();
            data.iter()
                .filter(|(_, e)| e.is_expired())
                .map(|(k, _)| k.clone())
                .collect()
        };

        let mut removed = 0;
        if !expired_keys.is_empty() {
            let mut data = self.data.write().unwrap();
            for k in &expired_keys {
                if data.remove(k).is_some() {
                    removed += 1;
                }
            }
        }

        if removed > 0 {
            let mut order = self.access_order.write().unwrap();
            order.retain(|k| !expired_keys.contains(k));
            let mut stats = self.stats.write().unwrap();
            stats.evictions += removed as u64;
        }
        removed
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;
    use std::thread;
    use std::time::Duration;

    // ===== CacheKey 测试 =====

    #[test]
    fn test_cache_key_by_pk() {
        let key = CacheKey::by_pk("users", 1);
        assert_eq!(key.table, "users");
        assert_eq!(key.kind, CacheKeyKind::ByPk);
        assert_eq!(key.identifier, "1");
        assert_eq!(key.to_string_key(), "l2:users:pk:1");
    }

    #[test]
    fn test_cache_key_by_query() {
        let key = CacheKey::by_query("orders", "abc123");
        assert_eq!(key.kind, CacheKeyKind::ByQuery);
        assert_eq!(key.to_string_key(), "l2:orders:q:abc123");
    }

    #[test]
    fn test_cache_key_by_relation() {
        let key = CacheKey::by_relation("users", "posts:1");
        assert_eq!(key.kind, CacheKeyKind::ByRelation);
        assert_eq!(key.to_string_key(), "l2:users:rel:posts:1");
    }

    #[test]
    fn test_cache_key_equality() {
        let k1 = CacheKey::by_pk("users", 1);
        let k2 = CacheKey::by_pk("users", 1);
        let k3 = CacheKey::by_pk("users", 2);
        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
    }

    #[test]
    fn test_cache_key_display() {
        let key = CacheKey::by_pk("users", 42);
        assert_eq!(format!("{}", key), "l2:users:pk:42");
    }

    // ===== L2CacheStats 测试 =====

    #[test]
    fn test_stats_hit_rate_empty() {
        let stats = L2CacheStats::default();
        assert_eq!(stats.hit_rate(), 0.0);
        assert_eq!(stats.total_lookups(), 0);
    }

    #[test]
    fn test_stats_hit_rate_calculation() {
        let stats = L2CacheStats {
            hits: 80,
            misses: 20,
            ..Default::default()
        };
        assert_eq!(stats.total_lookups(), 100);
        assert!((stats.hit_rate() - 0.8).abs() < 0.001);
        assert!((stats.miss_rate() - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_stats_merge() {
        let mut s1 = L2CacheStats {
            hits: 10,
            misses: 5,
            sets: 15,
            evictions: 2,
            size: 100,
        };
        let s2 = L2CacheStats {
            hits: 20,
            misses: 10,
            sets: 30,
            evictions: 5,
            size: 200,
        };
        s1.merge(&s2);
        assert_eq!(s1.hits, 30);
        assert_eq!(s1.misses, 15);
        assert_eq!(s1.sets, 45);
        assert_eq!(s1.evictions, 7);
        assert_eq!(s1.size, 300);
    }

    // ===== L2Cache 基本操作 =====

    #[test]
    fn test_put_and_get() {
        let cache = L2Cache::new();
        let key = CacheKey::by_pk("users", 1);

        cache.put(&key, Value::String("Alice".to_string()), None);
        let val = cache.get(&key);
        assert_eq!(val, Some(Value::String("Alice".to_string())));
    }

    #[test]
    fn test_get_missing_returns_none() {
        let cache = L2Cache::new();
        let key = CacheKey::by_pk("users", 999);
        assert_eq!(cache.get(&key), None);
    }

    #[test]
    fn test_overwrite_existing_key() {
        let cache = L2Cache::new();
        let key = CacheKey::by_pk("users", 1);

        cache.put(&key, Value::String("Alice".to_string()), None);
        cache.put(&key, Value::String("Bob".to_string()), None);
        assert_eq!(cache.get(&key), Some(Value::String("Bob".to_string())));
    }

    #[test]
    fn test_invalidate_single_key() {
        let cache = L2Cache::new();
        let key = CacheKey::by_pk("users", 1);

        cache.put(&key, Value::I64(42), None);
        assert!(cache.get(&key).is_some());

        cache.invalidate(&key);
        assert!(cache.get(&key).is_none());
    }

    // ===== 表级失效 =====

    #[test]
    fn test_invalidate_table_removes_all_entries_for_table() {
        let cache = L2Cache::new();

        let k1 = CacheKey::by_pk("users", 1);
        let k2 = CacheKey::by_pk("users", 2);
        let k3 = CacheKey::by_query("users", "hash1");
        let k4 = CacheKey::by_pk("orders", 1); // 不同表

        cache.put(&k1, Value::I64(1), None);
        cache.put(&k2, Value::I64(2), None);
        cache.put(&k3, Value::I64(3), None);
        cache.put(&k4, Value::I64(4), None);

        cache.invalidate_table("users");

        // users 表的所有缓存项应被失效
        assert!(cache.get(&k1).is_none());
        assert!(cache.get(&k2).is_none());
        assert!(cache.get(&k3).is_none());
        // orders 表的缓存项应保留
        assert!(cache.get(&k4).is_some());
    }

    #[test]
    fn test_invalidate_table_no_op_for_unknown_table() {
        let cache = L2Cache::new();
        let k1 = CacheKey::by_pk("users", 1);
        cache.put(&k1, Value::I64(1), None);

        cache.invalidate_table("nonexistent");
        assert!(cache.get(&k1).is_some());
    }

    // ===== TTL 测试 =====

    #[test]
    fn test_ttl_expiration() {
        let cache = L2Cache::new();
        let key = CacheKey::by_pk("users", 1);

        cache.put(&key, Value::I64(42), Some(Duration::from_millis(50)));
        assert!(cache.get(&key).is_some());

        // 等待 TTL 过期
        thread::sleep(Duration::from_millis(100));
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_default_ttl_applied_when_no_explicit_ttl() {
        let cache = L2Cache::new().with_default_ttl(Duration::from_millis(50));
        let key = CacheKey::by_pk("users", 1);

        cache.put(&key, Value::I64(42), None); // 不显式传 TTL
        assert!(cache.get(&key).is_some());

        thread::sleep(Duration::from_millis(100));
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_explicit_ttl_overrides_default() {
        // 语义验证：ttl=Some(Duration::MAX) 表示永不失效，覆盖默认 TTL
        let cache = L2Cache::new().with_default_ttl(Duration::from_millis(50));
        let key = CacheKey::by_pk("users", 1);

        // 显式传 Some(Duration::MAX) 覆盖默认 TTL（永不失效）
        cache.put(&key, Value::I64(42), Some(Duration::MAX));

        // 等待默认 TTL 已过期的时间
        thread::sleep(Duration::from_millis(100));
        // 由于显式传 Some(Duration::MAX)，应仍然有效
        assert!(cache.get(&key).is_some());
    }

    #[test]
    fn test_none_ttl_uses_default_ttl() {
        // 语义验证：ttl=None 时使用 default_ttl
        let cache = L2Cache::new().with_default_ttl(Duration::from_millis(50));
        let key = CacheKey::by_pk("users", 1);

        cache.put(&key, Value::I64(42), None);
        assert!(cache.get(&key).is_some());

        thread::sleep(Duration::from_millis(100));
        // None 使用了 default_ttl，应已过期
        assert!(cache.get(&key).is_none());
    }

    // ===== 命中率统计 =====

    #[test]
    fn test_stats_tracks_hits_and_misses() {
        let cache = L2Cache::new();

        let k1 = CacheKey::by_pk("users", 1);
        let k2 = CacheKey::by_pk("users", 2);

        cache.put(&k1, Value::I64(1), None);

        // 1 次命中
        cache.get(&k1);
        // 2 次未命中
        cache.get(&k2);
        cache.get(&k2);

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.sets, 1);
    }

    #[test]
    fn test_stats_tracks_evictions() {
        let cache = L2Cache::new();
        let k1 = CacheKey::by_pk("users", 1);
        let k2 = CacheKey::by_pk("users", 2);

        cache.put(&k1, Value::I64(1), None);
        cache.put(&k2, Value::I64(2), None);

        cache.invalidate(&k1); // evictions = 1
        cache.invalidate_table("users"); // 仅 k2 实际被删除，evictions = 2

        let stats = cache.stats();
        // invalidate(k1) 删除 1 项；invalidate_table("users") 仅删除 k2（k1 已不存在）
        assert_eq!(stats.evictions, 2);
    }

    #[test]
    fn test_stats_reset() {
        let cache = L2Cache::new();
        let k1 = CacheKey::by_pk("users", 1);

        cache.put(&k1, Value::I64(1), None);
        cache.get(&k1);
        cache.get(&k1);

        let stats_before = cache.stats();
        assert!(stats_before.hits > 0);

        cache.reset_stats();
        let stats_after = cache.stats();
        assert_eq!(stats_after.hits, 0);
        assert_eq!(stats_after.misses, 0);
        assert_eq!(stats_after.sets, 0);
    }

    // ===== 容量管理 =====

    #[test]
    fn test_max_size_eviction() {
        let cache = L2Cache::new().with_max_size(3);

        for i in 0..5 {
            let k = CacheKey::by_pk("users", i);
            cache.put(&k, Value::I64(i), None);
        }

        // 真正的 LRU：容量严格不超过 max_size
        let size = cache.size();
        assert_eq!(
            size, 3,
            "size should be exactly max_size after LRU eviction, got {}",
            size
        );
    }

    #[test]
    fn test_lru_eviction_order() {
        // 验证 LRU 顺序：访问 k0 后，下次淘汰应跳过 k0 而淘汰 k1
        let cache = L2Cache::new().with_max_size(3);

        let k0 = CacheKey::by_pk("users", 0);
        let k1 = CacheKey::by_pk("users", 1);
        let k2 = CacheKey::by_pk("users", 2);
        let k3 = CacheKey::by_pk("users", 3);

        cache.put(&k0, Value::I64(0), None);
        cache.put(&k1, Value::I64(1), None);
        cache.put(&k2, Value::I64(2), None);

        // 访问 k0，使其成为最近使用
        let _ = cache.get(&k0);

        // 插入 k3，应淘汰 k1（最久未访问）
        cache.put(&k3, Value::I64(3), None);

        assert!(
            cache.get(&k0).is_some(),
            "k0 should survive (recently accessed)"
        );
        assert!(
            cache.get(&k1).is_none(),
            "k1 should be evicted (LRU victim)"
        );
        assert!(cache.get(&k2).is_some(), "k2 should survive");
        assert!(
            cache.get(&k3).is_some(),
            "k3 should survive (just inserted)"
        );
    }

    #[test]
    fn test_clear_all() {
        let cache = L2Cache::new();
        cache.put(&CacheKey::by_pk("users", 1), Value::I64(1), None);
        cache.put(&CacheKey::by_pk("users", 2), Value::I64(2), None);
        cache.put(&CacheKey::by_pk("orders", 1), Value::I64(3), None);

        assert_eq!(cache.size(), 3);
        cache.clear();
        assert_eq!(cache.size(), 0);
    }

    // ===== contains（不更新统计）=====

    #[test]
    fn test_contains_does_not_update_stats() {
        let cache = L2Cache::new();
        let k1 = CacheKey::by_pk("users", 1);
        cache.put(&k1, Value::I64(1), None);

        let exists = cache.contains(&k1);
        assert!(exists);

        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
    }

    #[test]
    fn test_contains_returns_false_for_missing() {
        let cache = L2Cache::new();
        let k = CacheKey::by_pk("users", 999);
        assert!(!cache.contains(&k));
    }

    #[test]
    fn test_contains_returns_false_for_expired() {
        let cache = L2Cache::new();
        let k = CacheKey::by_pk("users", 1);
        cache.put(&k, Value::I64(1), Some(Duration::from_millis(10)));

        thread::sleep(Duration::from_millis(50));
        assert!(!cache.contains(&k));
    }

    // ===== evict_expired 手动清理 =====

    #[test]
    fn test_evict_expired_removes_only_expired_entries() {
        let cache = L2Cache::new();

        let k1 = CacheKey::by_pk("users", 1);
        let k2 = CacheKey::by_pk("users", 2);

        cache.put(&k1, Value::I64(1), Some(Duration::from_millis(10)));
        cache.put(&k2, Value::I64(2), None); // 永不过期

        thread::sleep(Duration::from_millis(50));
        let removed = cache.evict_expired();

        assert_eq!(removed, 1);
        assert!(cache.get(&k1).is_none());
        assert!(cache.get(&k2).is_some());
    }

    #[test]
    fn test_evict_expired_returns_zero_if_no_expired() {
        let cache = L2Cache::new();
        let k1 = CacheKey::by_pk("users", 1);
        cache.put(&k1, Value::I64(1), None);

        let removed = cache.evict_expired();
        assert_eq!(removed, 0);
    }

    // ===== 多线程测试 =====

    #[test]
    fn test_concurrent_access() {
        let cache = std::sync::Arc::new(L2Cache::new());
        let mut handles = Vec::new();

        // 多线程写入
        for i in 0..4 {
            let c = cache.clone();
            handles.push(thread::spawn(move || {
                for j in 0..10 {
                    let k = CacheKey::by_pk("users", i * 10 + j);
                    c.put(&k, Value::I64(i * 10 + j), None);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(cache.size(), 40);

        // 多线程读取
        let mut handles = Vec::new();
        for i in 0..4 {
            let c = cache.clone();
            handles.push(thread::spawn(move || {
                for j in 0..10 {
                    let k = CacheKey::by_pk("users", i * 10 + j);
                    let v = c.get(&k);
                    assert!(v.is_some());
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        let stats = cache.stats();
        assert_eq!(stats.hits, 40);
    }

    // ===== Default 测试 =====

    #[test]
    fn test_default() {
        let cache = L2Cache::default();
        assert_eq!(cache.size(), 0);
    }

    // ===== 综合场景 =====

    #[test]
    fn test_realistic_scenario() {
        let cache = L2Cache::new();

        // 1. 缓存用户表数据
        for i in 1..=5 {
            cache.put(
                &CacheKey::by_pk("users", i),
                Value::String(format!("user_{}", i)),
                None,
            );
        }

        // 2. 缓存查询结果
        cache.put(
            &CacheKey::by_query("users", "active_users_hash"),
            Value::I64(5),
            None,
        );

        // 3. 读取（部分命中、部分未命中）
        for i in 1..=10 {
            let _ = cache.get(&CacheKey::by_pk("users", i));
        }

        let stats = cache.stats();
        assert_eq!(stats.hits, 5); // 1-5 命中
        assert_eq!(stats.misses, 5); // 6-10 未命中
        assert_eq!(stats.sets, 6); // 5 pk + 1 query

        // 4. 用户表更新，失效所有缓存
        cache.invalidate_table("users");

        // 5. 再次读取应全部未命中
        cache.reset_stats();
        for i in 1..=5 {
            let _ = cache.get(&CacheKey::by_pk("users", i));
        }
        let stats2 = cache.stats();
        assert_eq!(stats2.hits, 0);
        assert_eq!(stats2.misses, 5);
    }
}
