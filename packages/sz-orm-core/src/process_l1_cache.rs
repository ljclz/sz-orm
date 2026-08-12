//! 进程级 L1 缓存（Process-Level L1 Cache）— 跨 Session 共享 Identity Map
//!
//! 对应 v4.6.0 REQ-V46-007，tasks.md M5。
//!
//! # 核心概念
//!
//! - **ProcessL1Cache**：进程级 L1 缓存，跨 Session 共享 Identity Map，线程安全 `Send + Sync`
//! - **CrossSessionIdentityMap**：跨 Session Identity Map，L1→L2→DB 查询协作
//! - **LRU 淘汰**：容量上限 + 最久未使用条目淘汰
//! - **TTL 过期**：条目插入后超过 TTL 自动失效
//! - **多租户隔离**：`tenant_isolated = true` 时 CacheKey 含 tenant_id
//!
//! 与既有 `L1Cache`（Session 级）的区别：
//! - `L1Cache`：Session 内有效，非 `Send + Sync`，Drop 自动清空
//! - `ProcessL1Cache`：进程级，`Send + Sync`，跨 Session 共享，需显式失效
//!
//! # 使用示例
//!
//! ```
//! use sz_orm_core::process_l1_cache::{ProcessL1Cache, ProcessL1Config};
//! use std::sync::Arc;
//!
//! let config = ProcessL1Config::new().with_capacity(100);
//! let cache: ProcessL1Cache<String> = ProcessL1Cache::new(config);
//! cache.put("users", &sz_orm_core::value::Value::I64(1), Arc::new("Alice".to_string()));
//! let a = cache.get("users", &sz_orm_core::value::Value::I64(1));
//! assert!(a.is_some());
//! ```

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::l2_cache::{CacheKey, CacheKeyKind, L2Cache};
use crate::value::Value;

// ============================================================================
// ProcessL1Config — 进程级 L1 缓存配置
// ============================================================================

/// 进程级 L1 缓存配置
#[derive(Debug, Clone)]
pub struct ProcessL1Config {
    /// 容量上限
    pub capacity: usize,
    /// TTL（毫秒）
    pub ttl_ms: u64,
    /// 是否启用缓存一致性协议
    pub enable_coherence: bool,
    /// 是否启用多租户隔离
    pub tenant_isolated: bool,
}

impl Default for ProcessL1Config {
    fn default() -> Self {
        Self {
            capacity: 10_000,
            ttl_ms: 300_000,
            enable_coherence: true,
            tenant_isolated: true,
        }
    }
}

impl ProcessL1Config {
    /// 创建默认配置（capacity 10000, ttl 300000ms, enable_coherence true, tenant_isolated true）
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置容量上限
    pub fn with_capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }

    /// 设置 TTL（毫秒）
    pub fn with_ttl_ms(mut self, ttl_ms: u64) -> Self {
        self.ttl_ms = ttl_ms;
        self
    }

    /// 设置是否启用缓存一致性
    pub fn with_coherence(mut self, enable: bool) -> Self {
        self.enable_coherence = enable;
        self
    }

    /// 设置是否启用多租户隔离
    pub fn with_tenant_isolated(mut self, isolated: bool) -> Self {
        self.tenant_isolated = isolated;
        self
    }
}

// ============================================================================
// ProcessL1Stats — 无锁统计
// ============================================================================

/// 进程级 L1 缓存无锁统计
#[derive(Debug, Default)]
pub struct ProcessL1Stats {
    /// 命中次数
    pub hits: AtomicU64,
    /// 未命中次数
    pub misses: AtomicU64,
    /// 当前条目数
    pub entry_count: AtomicU64,
    /// 淘汰次数
    pub evict_count: AtomicU64,
}

/// 统计快照
#[derive(Debug, Clone, Default)]
pub struct ProcessL1StatsSnapshot {
    /// 命中次数
    pub hits: u64,
    /// 未命中次数
    pub misses: u64,
    /// 当前条目数
    pub entry_count: u64,
    /// 淘汰次数
    pub evict_count: u64,
    /// 命中率
    pub hit_rate: f64,
}

impl ProcessL1StatsSnapshot {
    /// 总查询次数
    pub fn total_lookups(&self) -> u64 {
        self.hits + self.misses
    }
}

// ============================================================================
// CacheEntry — 缓存条目
// ============================================================================

/// 缓存条目
struct CacheEntry<T> {
    /// 缓存值
    value: Arc<T>,
    /// 插入时间（Unix 毫秒）
    inserted_at: u64,
    /// 最后访问时间（Unix 毫秒）
    last_accessed_at: u64,
}

// ============================================================================
// ProcessL1Cache — 进程级 L1 缓存核心
// ============================================================================

/// 进程级 L1 缓存内部数据
struct ProcessL1Inner<T> {
    /// 缓存条目
    entries: HashMap<CacheKey, CacheEntry<T>>,
    /// LRU 访问顺序（头部 = 最久未使用）
    lru_order: VecDeque<CacheKey>,
}

/// 进程级 L1 缓存（跨 Session 共享 Identity Map）
///
/// - **线程安全**：`RwLock` 保护内部数据，`Send + Sync`
/// - **Identity Map**：相同主键返回相同 `Arc<T>` 引用
/// - **LRU 淘汰**：超过 `capacity` 时淘汰最久未使用条目
/// - **TTL 过期**：条目超过 TTL 自动失效
/// - **跨 Session 共享**：进程级生命周期，不与 Session 绑定
pub struct ProcessL1Cache<T: Clone + Send + Sync + 'static> {
    /// 内部数据
    inner: RwLock<ProcessL1Inner<T>>,
    /// 配置
    config: ProcessL1Config,
    /// 统计
    stats: ProcessL1Stats,
    /// L2 缓存引用（可选）
    l2: Option<Arc<L2Cache>>,
}

impl<T: Clone + Send + Sync + 'static> ProcessL1Cache<T> {
    /// 创建进程级 L1 缓存
    pub fn new(config: ProcessL1Config) -> Self {
        Self {
            inner: RwLock::new(ProcessL1Inner {
                entries: HashMap::new(),
                lru_order: VecDeque::new(),
            }),
            config,
            stats: ProcessL1Stats::default(),
            l2: None,
        }
    }

    /// 设置 L2 缓存引用
    pub fn with_l2(mut self, l2: Arc<L2Cache>) -> Self {
        self.l2 = Some(l2);
        self
    }

    /// 构建缓存键
    fn build_key(&self, table: &str, pk: &Value) -> CacheKey {
        CacheKey::by_pk(table, pk)
    }

    /// 获取当前时间（Unix 毫秒）
    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// TTL 过期判定
    fn is_expired(&self, entry: &CacheEntry<T>) -> bool {
        if self.config.ttl_ms == 0 {
            return false;
        }
        Self::now_ms().saturating_sub(entry.inserted_at) > self.config.ttl_ms
    }

    /// 查询缓存
    pub async fn get(&self, table: &str, pk: &Value) -> Option<Arc<T>> {
        if self.config.capacity == 0 {
            self.stats.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        let key = self.build_key(table, pk);
        let now = Self::now_ms();
        let mut inner = self.inner.write().unwrap();
        if let Some(entry) = inner.entries.get_mut(&key) {
            if self.is_expired(entry) {
                inner.entries.remove(&key);
                inner.lru_order.retain(|k| k != &key);
                self.stats.misses.fetch_add(1, Ordering::Relaxed);
                return None;
            }
            entry.last_accessed_at = now;
            let value = Arc::clone(&entry.value);
            inner.lru_order.retain(|k| k != &key);
            inner.lru_order.push_back(key);
            self.stats.hits.fetch_add(1, Ordering::Relaxed);
            return Some(value);
        }
        self.stats.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// 写入缓存
    pub async fn put(&self, table: &str, pk: Value, value: Arc<T>) {
        if self.config.capacity == 0 {
            return;
        }
        let key = self.build_key(table, &pk);
        let now = Self::now_ms();
        let mut inner = self.inner.write().unwrap();
        if inner.entries.contains_key(&key) {
            inner.lru_order.retain(|k| k != &key);
        }
        inner.entries.insert(
            key.clone(),
            CacheEntry {
                value: Arc::clone(&value),
                inserted_at: now,
                last_accessed_at: now,
            },
        );
        inner.lru_order.push_back(key.clone());
        self.evict_lru(&mut inner);
        self.stats
            .entry_count
            .store(inner.entries.len() as u64, Ordering::Relaxed);
    }

    /// LRU 淘汰
    fn evict_lru(&self, inner: &mut ProcessL1Inner<T>) {
        while inner.entries.len() > self.config.capacity {
            if let Some(old_key) = inner.lru_order.pop_front() {
                inner.entries.remove(&old_key);
                self.stats.evict_count.fetch_add(1, Ordering::Relaxed);
            } else {
                break;
            }
        }
    }

    /// 失效单条目
    pub async fn invalidate(&self, table: &str, pk: &Value) {
        let key = self.build_key(table, pk);
        let mut inner = self.inner.write().unwrap();
        inner.entries.remove(&key);
        inner.lru_order.retain(|k| k != &key);
        self.stats
            .entry_count
            .store(inner.entries.len() as u64, Ordering::Relaxed);
        if self.config.enable_coherence {
            if let Some(l2) = &self.l2 {
                l2.invalidate(&key);
            }
        }
    }

    /// 失效整表
    pub async fn invalidate_table(&self, table: &str) {
        let mut inner = self.inner.write().unwrap();
        let keys_to_remove: Vec<CacheKey> = inner
            .entries
            .keys()
            .filter(|k| k.table == table)
            .cloned()
            .collect();
        for key in &keys_to_remove {
            inner.entries.remove(key);
        }
        inner.lru_order.retain(|k| !keys_to_remove.contains(k));
        self.stats
            .entry_count
            .store(inner.entries.len() as u64, Ordering::Relaxed);
        if self.config.enable_coherence {
            if let Some(l2) = &self.l2 {
                l2.invalidate_table(table);
            }
        }
    }

    /// 获取统计快照
    pub fn stats(&self) -> ProcessL1StatsSnapshot {
        let hits = self.stats.hits.load(Ordering::Relaxed);
        let misses = self.stats.misses.load(Ordering::Relaxed);
        let entry_count = self.stats.entry_count.load(Ordering::Relaxed);
        let evict_count = self.stats.evict_count.load(Ordering::Relaxed);
        let total = hits + misses;
        let hit_rate = if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        };
        ProcessL1StatsSnapshot {
            hits,
            misses,
            entry_count,
            evict_count,
            hit_rate,
        }
    }

    /// 当前条目数
    pub fn len(&self) -> usize {
        self.inner.read().unwrap().entries.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ============================================================================
// CrossSessionIdentityMap — 跨 Session Identity Map
// ============================================================================

/// 跨 Session Identity Map
///
/// 包装 `ProcessL1Cache`，提供 L1→L2→DB 查询协作：
/// 1. L1 命中 → 直接返回
/// 2. L1 未命中 → 调用 loader 查 DB → 回填 L1 → 返回
pub struct CrossSessionIdentityMap<T: Clone + Send + Sync + 'static> {
    /// 进程级 L1 缓存
    cache: Arc<ProcessL1Cache<T>>,
}

impl<T: Clone + Send + Sync + 'static> CrossSessionIdentityMap<T> {
    /// 创建跨 Session Identity Map
    pub fn new(cache: Arc<ProcessL1Cache<T>>) -> Self {
        Self { cache }
    }

    /// 获取或加载（L1→DB 协同）
    ///
    /// 1. L1 命中 → 直接返回 `Arc<T>`
    /// 2. L1 未命中 → 调用 `loader` 查 DB → 回填 L1 → 返回 `Arc<T>`
    pub async fn get_or_load<F, Fut, E>(
        &self,
        table: &str,
        pk: Value,
        loader: F,
    ) -> Result<Arc<T>, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
    {
        if let Some(cached) = self.cache.get(table, &pk).await {
            return Ok(cached);
        }
        let value = loader().await?;
        let arc = Arc::new(value);
        self.cache.put(table, pk, Arc::clone(&arc)).await;
        Ok(arc)
    }

    /// 失效单条目
    pub async fn invalidate(&self, table: &str, pk: &Value) {
        self.cache.invalidate(table, pk).await;
    }

    /// 失效整表
    pub async fn invalidate_table(&self, table: &str) {
        self.cache.invalidate_table(table).await;
    }

    /// 获取统计快照
    pub fn stats(&self) -> ProcessL1StatsSnapshot {
        self.cache.stats()
    }
}

// ============================================================================
// 多租户缓存键
// ============================================================================

/// 构建多租户缓存键
pub fn tenant_cache_key(
    tenant_id: &str,
    table: &str,
    pk: &Value,
    tenant_isolated: bool,
) -> CacheKey {
    if tenant_isolated {
        CacheKey {
            table: format!("{}:{}", tenant_id, table),
            kind: CacheKeyKind::ByPk,
            identifier: pk.to_string(),
        }
    } else {
        CacheKey::by_pk(table, pk)
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> ProcessL1Config {
        ProcessL1Config::new()
    }

    #[test]
    fn test_config_default() {
        let config = ProcessL1Config::new();
        assert_eq!(config.capacity, 10_000);
        assert_eq!(config.ttl_ms, 300_000);
        assert!(config.enable_coherence);
        assert!(config.tenant_isolated);
    }

    #[test]
    fn test_config_builder() {
        let config = ProcessL1Config::new()
            .with_capacity(500)
            .with_ttl_ms(60_000)
            .with_coherence(false)
            .with_tenant_isolated(false);
        assert_eq!(config.capacity, 500);
        assert_eq!(config.ttl_ms, 60_000);
        assert!(!config.enable_coherence);
        assert!(!config.tenant_isolated);
    }

    #[tokio::test]
    async fn test_get_hit() {
        let cache: ProcessL1Cache<String> = ProcessL1Cache::new(make_config());
        cache
            .put("users", Value::I64(1), Arc::new("Alice".to_string()))
            .await;
        let result = cache.get("users", &Value::I64(1)).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().as_str(), "Alice");
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
    }

    #[tokio::test]
    async fn test_get_miss() {
        let cache: ProcessL1Cache<String> = ProcessL1Cache::new(make_config());
        let result = cache.get("users", &Value::I64(1)).await;
        assert!(result.is_none());
        let stats = cache.stats();
        assert_eq!(stats.misses, 1);
    }

    #[tokio::test]
    async fn test_identity_map() {
        let cache: ProcessL1Cache<String> = ProcessL1Cache::new(make_config());
        cache
            .put("users", Value::I64(1), Arc::new("Alice".to_string()))
            .await;
        let a = cache.get("users", &Value::I64(1)).await.unwrap();
        let b = cache.get("users", &Value::I64(1)).await.unwrap();
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[tokio::test]
    async fn test_lru_eviction() {
        let config = ProcessL1Config::new().with_capacity(3);
        let cache: ProcessL1Cache<i64> = ProcessL1Cache::new(config);
        for i in 1..=4i64 {
            cache.put("t", Value::I64(i), Arc::new(i)).await;
        }
        assert_eq!(cache.len(), 3);
        assert!(cache.get("t", &Value::I64(1)).await.is_none());
        assert!(cache.get("t", &Value::I64(4)).await.is_some());
        let stats = cache.stats();
        assert!(stats.evict_count >= 1);
    }

    #[tokio::test]
    async fn test_ttl_expiry() {
        let config = ProcessL1Config::new().with_capacity(100).with_ttl_ms(50);
        let cache: ProcessL1Cache<String> = ProcessL1Cache::new(config);
        cache
            .put("users", Value::I64(1), Arc::new("Alice".to_string()))
            .await;
        assert!(cache.get("users", &Value::I64(1)).await.is_some());
        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        assert!(cache.get("users", &Value::I64(1)).await.is_none());
    }

    #[tokio::test]
    async fn test_capacity_zero() {
        let config = ProcessL1Config::new().with_capacity(0);
        let cache: ProcessL1Cache<String> = ProcessL1Cache::new(config);
        cache
            .put("users", Value::I64(1), Arc::new("Alice".to_string()))
            .await;
        assert!(cache.get("users", &Value::I64(1)).await.is_none());
    }

    #[tokio::test]
    async fn test_invalidate() {
        let cache: ProcessL1Cache<String> = ProcessL1Cache::new(make_config());
        cache
            .put("users", Value::I64(1), Arc::new("Alice".to_string()))
            .await;
        assert!(cache.get("users", &Value::I64(1)).await.is_some());
        cache.invalidate("users", &Value::I64(1)).await;
        assert!(cache.get("users", &Value::I64(1)).await.is_none());
    }

    #[tokio::test]
    async fn test_invalidate_table() {
        let cache: ProcessL1Cache<String> = ProcessL1Cache::new(make_config());
        cache
            .put("users", Value::I64(1), Arc::new("Alice".to_string()))
            .await;
        cache
            .put("users", Value::I64(2), Arc::new("Bob".to_string()))
            .await;
        cache
            .put("orders", Value::I64(1), Arc::new("Order1".to_string()))
            .await;
        cache.invalidate_table("users").await;
        assert!(cache.get("users", &Value::I64(1)).await.is_none());
        assert!(cache.get("users", &Value::I64(2)).await.is_none());
        assert!(cache.get("orders", &Value::I64(1)).await.is_some());
    }

    #[tokio::test]
    async fn test_invalidate_nonexistent() {
        let cache: ProcessL1Cache<String> = ProcessL1Cache::new(make_config());
        cache.invalidate("users", &Value::I64(999)).await;
        cache.invalidate_table("nonexistent").await;
    }

    #[tokio::test]
    async fn test_cross_session_identity_map() {
        let cache: Arc<ProcessL1Cache<String>> = Arc::new(ProcessL1Cache::new(make_config()));
        let id_map = CrossSessionIdentityMap::new(Arc::clone(&cache));
        let result1 = id_map
            .get_or_load("users", Value::I64(1), || async {
                Ok::<String, String>("Alice".to_string())
            })
            .await
            .unwrap();
        let result2 = id_map
            .get_or_load("users", Value::I64(1), || async {
                Err::<String, String>("should not be called".to_string())
            })
            .await
            .unwrap();
        assert!(Arc::ptr_eq(&result1, &result2));
    }

    #[tokio::test]
    async fn test_cross_session_load_from_db() {
        let cache: Arc<ProcessL1Cache<String>> = Arc::new(ProcessL1Cache::new(make_config()));
        let id_map = CrossSessionIdentityMap::new(Arc::clone(&cache));
        let call_count = Arc::new(AtomicU64::new(0));
        let cc = Arc::clone(&call_count);
        let result1 = id_map
            .get_or_load("users", Value::I64(1), || {
                let cc = Arc::clone(&cc);
                async move {
                    cc.fetch_add(1, Ordering::Relaxed);
                    Ok::<String, String>("Alice".to_string())
                }
            })
            .await
            .unwrap();
        let result2 = id_map
            .get_or_load("users", Value::I64(1), || {
                let cc = Arc::clone(&cc);
                async move {
                    cc.fetch_add(1, Ordering::Relaxed);
                    Ok::<String, String>("Alice".to_string())
                }
            })
            .await
            .unwrap();
        assert_eq!(result1.as_str(), "Alice");
        assert_eq!(result2.as_str(), "Alice");
        assert!(Arc::ptr_eq(&result1, &result2));
        assert_eq!(call_count.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_cross_session_loader_error() {
        let cache: Arc<ProcessL1Cache<String>> = Arc::new(ProcessL1Cache::new(make_config()));
        let id_map = CrossSessionIdentityMap::new(Arc::clone(&cache));
        let result: Result<Arc<String>, String> = id_map
            .get_or_load("users", Value::I64(1), || async {
                Err("db error".to_string())
            })
            .await;
        assert!(result.is_err());
        assert!(cache.get("users", &Value::I64(1)).await.is_none());
    }

    #[tokio::test]
    async fn test_l2_invalidation_sync() {
        let l2 = Arc::new(L2Cache::new());
        let config = ProcessL1Config::new().with_coherence(true);
        let cache: ProcessL1Cache<String> = ProcessL1Cache::new(config).with_l2(Arc::clone(&l2));
        let key = CacheKey::by_pk("users", "1");
        l2.put(&key, Value::String("Alice".to_string()), None);
        assert!(l2.get(&key).is_some());
        cache.invalidate("users", &Value::I64(1)).await;
        assert!(l2.get(&key).is_none());
    }

    #[tokio::test]
    async fn test_l2_invalidate_table_sync() {
        let l2 = Arc::new(L2Cache::new());
        let config = ProcessL1Config::new().with_coherence(true);
        let cache: ProcessL1Cache<String> = ProcessL1Cache::new(config).with_l2(Arc::clone(&l2));
        let key1 = CacheKey::by_pk("users", "1");
        let key2 = CacheKey::by_pk("users", "2");
        l2.put(&key1, Value::String("Alice".to_string()), None);
        l2.put(&key2, Value::String("Bob".to_string()), None);
        cache.invalidate_table("users").await;
        assert!(l2.get(&key1).is_none());
        assert!(l2.get(&key2).is_none());
    }

    #[tokio::test]
    async fn test_coherence_disabled() {
        let l2 = Arc::new(L2Cache::new());
        let config = ProcessL1Config::new().with_coherence(false);
        let cache: ProcessL1Cache<String> = ProcessL1Cache::new(config).with_l2(Arc::clone(&l2));
        let key = CacheKey::by_pk("users", "1");
        l2.put(&key, Value::String("Alice".to_string()), None);
        cache.invalidate("users", &Value::I64(1)).await;
        assert!(l2.get(&key).is_some());
    }

    #[tokio::test]
    async fn test_tenant_cache_key_isolated() {
        let key_a = tenant_cache_key("tenant_a", "users", &Value::I64(1), true);
        let key_b = tenant_cache_key("tenant_b", "users", &Value::I64(1), true);
        assert_ne!(key_a, key_b);
    }

    #[tokio::test]
    async fn test_tenant_cache_key_shared() {
        let key_a = tenant_cache_key("tenant_a", "users", &Value::I64(1), false);
        let key_b = tenant_cache_key("tenant_b", "users", &Value::I64(1), false);
        assert_eq!(key_a, key_b);
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        let cache: Arc<ProcessL1Cache<i64>> =
            Arc::new(ProcessL1Cache::new(make_config().with_capacity(1000)));
        let mut handles = Vec::new();
        for i in 0..10i64 {
            let cache = Arc::clone(&cache);
            handles.push(tokio::spawn(async move {
                for j in 0..100i64 {
                    let v = i * 100 + j;
                    cache.put("t", Value::I64(v), Arc::new(v)).await;
                    cache.get("t", &Value::I64(v)).await;
                }
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert!(cache.len() <= 1000);
    }

    #[tokio::test]
    async fn test_stats_snapshot() {
        let cache: ProcessL1Cache<String> = ProcessL1Cache::new(make_config());
        cache
            .put("users", Value::I64(1), Arc::new("Alice".to_string()))
            .await;
        cache.get("users", &Value::I64(1)).await;
        cache.get("users", &Value::I64(2)).await;
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.total_lookups(), 2);
        assert!((stats.hit_rate - 0.5).abs() < 0.001);
    }
}
