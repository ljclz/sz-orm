//! # 缓存预热与穿透防护
//!
//! 缓存预热器（`CacheWarmer`）+ 布隆过滤器（`BloomFilter`，穿透防护）+
//! 穿透防护器（`PenetrationGuard`，包装 BloomFilter + L1/L2 缓存）+
//! 击穿防护器（`SingleFlight`，singleflight 模式）。
//!
//! 复用 v4.6.0 `ProcessL1Cache`（`process_l1_cache.rs:169`）+ 既有 `L2Cache`（`l2_cache.rs:517`）。

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::process_l1_cache::ProcessL1Cache;
use crate::value::Value;

/// 预热策略
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WarmupStrategy {
    /// 热点表预热（加载整表到缓存）
    HotspotTable(String),
    /// 热点键预热（加载指定键）
    HotspotKey(Vec<String>),
    /// 自定义查询预热
    CustomQuery(String),
    /// 禁用预热
    #[default]
    Disabled,
}

/// 预热配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmupConfig {
    /// 预热策略
    pub strategy: WarmupStrategy,
    /// 目标表名
    pub table: String,
    /// 并行度
    pub parallelism: usize,
    /// 每批大小
    pub batch_size: usize,
    /// TTL（毫秒，0 = 永不过期）
    pub ttl_ms: u64,
}

impl Default for WarmupConfig {
    fn default() -> Self {
        Self {
            strategy: WarmupStrategy::default(),
            table: String::new(),
            parallelism: 4,
            batch_size: 100,
            ttl_ms: 0,
        }
    }
}

impl WarmupConfig {
    pub fn new(table: impl Into<String>, strategy: WarmupStrategy) -> Self {
        Self {
            strategy,
            table: table.into(),
            ..Default::default()
        }
    }
}

/// 预热结果
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WarmupResult {
    /// 预热的键数量
    pub warmed_keys: usize,
    /// 跳过的键数量（已存在）
    pub skipped_keys: usize,
    /// 失败的键数量
    pub failed_keys: usize,
    /// 耗时（毫秒）
    pub elapsed_ms: u64,
}

/// 缓存错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheError {
    /// 预热失败
    WarmupFailed(String),
    /// 布隆过滤器容量超限
    BloomFilterCapacityExceeded { capacity: usize, requested: usize },
    /// SingleFlight 超时
    SingleFlightTimeout(String),
    /// 预热数据过期
    WarmupDataStale(String),
    /// 缓存不可用
    CacheUnavailable(String),
}

/// v4.7.0 双实现合并：公共布隆过滤器错误转换
impl From<crate::bloom::BloomError> for CacheError {
    fn from(e: crate::bloom::BloomError) -> Self {
        match e {
            crate::bloom::BloomError::CapacityExceeded {
                capacity,
                requested,
            } => Self::BloomFilterCapacityExceeded {
                capacity,
                requested,
            },
        }
    }
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WarmupFailed(msg) => write!(f, "warmup failed: {msg}"),
            Self::BloomFilterCapacityExceeded {
                capacity,
                requested,
            } => {
                write!(
                    f,
                    "bloom filter capacity exceeded: capacity={capacity}, requested={requested}"
                )
            }
            Self::SingleFlightTimeout(msg) => write!(f, "singleflight timeout: {msg}"),
            Self::WarmupDataStale(msg) => write!(f, "warmup data stale: {msg}"),
            Self::CacheUnavailable(msg) => write!(f, "cache unavailable: {msg}"),
        }
    }
}

impl std::error::Error for CacheError {}

/// 布隆过滤器（穿透防护）
///
/// 不漏判：不存在的键一定返回 `false`（可能误判存在）。
/// 使用双哈希策略（k 个哈希函数 = 2 个基础哈希的线性组合）。
///
/// 公共实现（v4.7.0 双实现合并）：原自研实现已迁移至 `crate::bloom`
/// （并发安全 + 容量拒绝），此处保留公开路径兼容（架构债清零）。
pub use crate::bloom::BloomFilter;

/// 缓存预热器
///
/// 复用 v4.6.0 `ProcessL1Cache`（`process_l1_cache.rs:169`）+ 既有 `L2Cache`。
pub struct CacheWarmer<T: Clone + Send + Sync + 'static> {
    cache: Arc<ProcessL1Cache<T>>,
}

impl<T: Clone + Send + Sync + 'static> CacheWarmer<T> {
    pub fn new(cache: Arc<ProcessL1Cache<T>>) -> Self {
        Self { cache }
    }

    /// 异步预热
    ///
    /// 按 `WarmupStrategy` 预热缓存，返回预热结果。
    /// 实际数据加载由调用方提供 `loader` 函数。
    pub async fn warmup<F, Fut>(
        &self,
        config: &WarmupConfig,
        loader: F,
    ) -> Result<WarmupResult, CacheError>
    where
        F: Fn(&str) -> Fut,
        Fut: Future<Output = Result<Vec<(Value, T)>, CacheError>>,
    {
        let start = std::time::Instant::now();

        if matches!(config.strategy, WarmupStrategy::Disabled) {
            return Ok(WarmupResult::default());
        }

        let keys = match &config.strategy {
            WarmupStrategy::HotspotTable(table) => vec![table.clone()],
            WarmupStrategy::HotspotKey(keys) => keys.clone(),
            WarmupStrategy::CustomQuery(query) => vec![query.clone()],
            WarmupStrategy::Disabled => return Ok(WarmupResult::default()),
        };

        let mut result = WarmupResult::default();

        for key in &keys {
            match loader(key).await {
                Ok(entries) => {
                    for (pk, value) in entries {
                        let table = &config.table;
                        let existing = self.cache.get(table, &pk).await;
                        if existing.is_some() {
                            result.skipped_keys += 1;
                        } else {
                            self.cache.put(table, pk.clone(), Arc::new(value)).await;
                            result.warmed_keys += 1;
                        }
                    }
                }
                Err(_) => {
                    result.failed_keys += 1;
                }
            }
        }

        result.elapsed_ms = start.elapsed().as_millis() as u64;
        Ok(result)
    }
}

impl<T: Clone + Send + Sync + 'static> std::fmt::Debug for CacheWarmer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CacheWarmer").finish_non_exhaustive()
    }
}

/// 穿透防护器
///
/// 包装 `BloomFilter` + `ProcessL1Cache`，不存在返回 `None` 不查 DB。
pub struct PenetrationGuard<T: Clone + Send + Sync + 'static> {
    bloom: Mutex<BloomFilter>,
    cache: Arc<ProcessL1Cache<T>>,
}

impl<T: Clone + Send + Sync + 'static> PenetrationGuard<T> {
    pub fn new(cache: Arc<ProcessL1Cache<T>>, bloom_capacity: usize) -> Self {
        Self {
            bloom: Mutex::new(BloomFilter::new(bloom_capacity, 0.01)),
            cache,
        }
    }

    /// 注册存在的键（预热时调用）
    pub fn register(&self, key: &str) -> Result<(), CacheError> {
        self.bloom.lock().unwrap().add(key)?;
        Ok(())
    }

    /// 布隆过滤器判断（穿透检查）：key 不存在返回 `false`（不漏判）
    ///
    /// 与 `get` 的区别：`get` 将"穿透"与"缓存未命中"合并返回 `None`；
    /// 本方法仅做穿透判断，供组合防护区分"不查 DB"与"查 DB 后未命中"。
    pub fn might_contain(&self, table: &str, pk: &Value) -> bool {
        let bloom_key = format!("{table}:{pk:?}");
        self.bloom.lock().unwrap().might_contain(&bloom_key)
    }

    /// 查询缓存（穿透防护）
    ///
    /// 布隆过滤器判断不存在则直接返回 `None`，不查 DB。
    /// 布隆过滤器判断可能存在则查缓存，缓存未命中返回 `None`（调用方可查 DB）。
    pub async fn get(&self, table: &str, pk: &Value) -> Option<Arc<T>> {
        let bloom_key = format!("{table}:{pk:?}");
        {
            let bloom = self.bloom.lock().unwrap();
            if !bloom.might_contain(&bloom_key) {
                return None;
            }
        }
        self.cache.get(table, pk).await
    }

    /// 写入缓存并注册到布隆过滤器
    pub async fn put(&self, table: &str, pk: Value, value: T) -> Result<(), CacheError> {
        let bloom_key = format!("{table}:{pk:?}");
        {
            let bloom = self.bloom.lock().unwrap();
            bloom.add(&bloom_key)?;
        }
        self.cache.put(table, pk, Arc::new(value)).await;
        Ok(())
    }

    /// 布隆过滤器中的元素数量
    pub fn bloom_count(&self) -> usize {
        self.bloom.lock().unwrap().count()
    }
}

impl<T: Clone + Send + Sync + 'static> std::fmt::Debug for PenetrationGuard<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PenetrationGuard")
            .field("bloom_count", &self.bloom.lock().unwrap().count())
            .finish_non_exhaustive()
    }
}

/// 击穿防护器（SingleFlight 模式）
///
/// 同一键的并发请求只执行一次重建，其他请求等待结果。
pub struct SingleFlight {
    in_flight: Mutex<HashMap<String, Arc<tokio::sync::Notify>>>,
}

impl SingleFlight {
    pub fn new() -> Self {
        Self {
            in_flight: Mutex::new(HashMap::new()),
        }
    }

    /// 获取或重建
    ///
    /// 如果键正在重建，等待已有重建完成；否则执行 `rebuild`。
    pub async fn get_or_rebuild<F, Fut, V>(&self, key: &str, rebuild: F) -> Result<V, CacheError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<V, CacheError>>,
        V: Clone,
    {
        let notify = {
            let mut in_flight = self.in_flight.lock().unwrap();
            if let Some(existing) = in_flight.get(key) {
                Arc::clone(existing)
            } else {
                let notify = Arc::new(tokio::sync::Notify::new());
                in_flight.insert(key.to_string(), Arc::clone(&notify));
                notify
            }
        };

        let is_leader = {
            let in_flight = self.in_flight.lock().unwrap();
            Arc::ptr_eq(in_flight.get(key).unwrap_or(&notify), &notify)
        };

        if is_leader {
            let result = rebuild().await;
            {
                let mut in_flight = self.in_flight.lock().unwrap();
                in_flight.remove(key);
            }
            notify.notify_waiters();
            result
        } else {
            notify.notified().await;
            Err(CacheError::SingleFlightTimeout(format!(
                "key {key} rebuild by another task, retry recommended"
            )))
        }
    }

    /// 当前在途的键数量
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.lock().unwrap().len()
    }
}

impl Default for SingleFlight {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SingleFlight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SingleFlight")
            .field("in_flight_count", &self.in_flight.lock().unwrap().len())
            .finish()
    }
}

// ============================================================================
// CacheProtection — 组合防护门面（v4.7.0 真剩余接入）
//
// 将穿透防护（PenetrationGuard：BloomFilter 不存在不查 DB）与击穿防护
// （SingleFlight：并发重建只执行一次）组合为缓存查询的统一入口，
// 形成生产可用的"防穿透 + 防击穿"缓存加载路径。
// ============================================================================

/// 组合防护门面：穿透防护 + 击穿防护
///
/// 查询路径：
///   ① 布隆过滤器判断 key 不存在 → 直接 `None`（穿透防护，不查 DB）
///   ② 缓存命中 → 返回
///   ③ 未命中 → SingleFlight 并发重建（击穿防护，同一 key 只重建一次）
///   ④ 重建结果写入缓存并注册布隆过滤器
pub struct CacheProtection<T: Clone + Send + Sync + 'static> {
    penetration: PenetrationGuard<T>,
    single_flight: SingleFlight,
}

impl<T: Clone + Send + Sync + 'static> CacheProtection<T> {
    /// 创建组合防护（`bloom_capacity` 布隆过滤器容量）
    pub fn new(cache: Arc<ProcessL1Cache<T>>, bloom_capacity: usize) -> Self {
        Self {
            penetration: PenetrationGuard::new(cache, bloom_capacity),
            single_flight: SingleFlight::new(),
        }
    }

    /// 防护查询：存在返回缓存值，不存在返回 `None`（不查 DB）
    pub async fn get(&self, table: &str, pk: &Value) -> Option<Arc<T>> {
        self.penetration.get(table, pk).await
    }

    /// 防护加载：缓存命中 → SingleFlight 并发重建 → 写入缓存并注册布隆
    ///
    /// 穿透防护分层语义：
    ///   - `get`（查询热路径）：bloom 不存在的 key 被拦截，不查 DB（防穿透风暴）
    ///   - `get_or_load`（加载路径）：不受 bloom 限制，负责首次加载并注册——
    ///     否则首次访问会被短路，数据永远加载不进来
    ///
    /// `loader` 返回 `Ok(Some(v))` 表示数据存在，`Ok(None)` 表示数据不存在
    /// （不存在也经 SingleFlight 去重，避免穿透风暴）。
    pub async fn get_or_load<F, Fut>(
        &self,
        table: &str,
        pk: Value,
        loader: F,
    ) -> Result<Option<Arc<T>>, CacheError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Option<T>, CacheError>>,
    {
        // ① 缓存命中（bloom 存在且缓存有值）
        if let Some(hit) = self.penetration.get(table, &pk).await {
            return Ok(Some(hit));
        }
        // ② 击穿防护：并发重建只执行一次（loader 调用次数由 SingleFlight 去重），
        //    成功后 put 写入缓存并注册布隆过滤器（后续查询走穿透防护）
        let key = format!("{table}:{pk:?}");
        let value: Option<T> = self
            .single_flight
            .get_or_rebuild(&key, || async {
                match loader().await {
                    Ok(Some(v)) => {
                        self.penetration.put(table, pk.clone(), v.clone()).await?;
                        Ok(Some(v))
                    }
                    Ok(None) => Ok(None),
                    Err(e) => Err(e),
                }
            })
            .await?;
        Ok(value.map(Arc::new))
    }

    /// 布隆过滤器元素数量
    pub fn bloom_count(&self) -> usize {
        self.penetration.bloom_count()
    }
}

impl<T: Clone + Send + Sync + 'static> std::fmt::Debug for CacheProtection<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CacheProtection")
            .field("bloom_count", &self.bloom_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process_l1_cache::ProcessL1Config;
    use crate::value::Value;

    fn make_cache() -> Arc<ProcessL1Cache<String>> {
        Arc::new(ProcessL1Cache::new(ProcessL1Config::default()))
    }

    #[test]
    fn test_warmup_strategy_default() {
        assert_eq!(WarmupStrategy::default(), WarmupStrategy::Disabled);
    }

    #[test]
    fn test_warmup_config_default() {
        let config = WarmupConfig::default();
        assert_eq!(config.strategy, WarmupStrategy::Disabled);
        assert_eq!(config.parallelism, 4);
        assert_eq!(config.batch_size, 100);
    }

    #[test]
    fn test_warmup_config_new() {
        let config = WarmupConfig::new("users", WarmupStrategy::HotspotTable("users".to_string()));
        assert_eq!(config.table, "users");
        assert_eq!(
            config.strategy,
            WarmupStrategy::HotspotTable("users".to_string())
        );
    }

    #[test]
    fn test_warmup_result_default() {
        let result = WarmupResult::default();
        assert_eq!(result.warmed_keys, 0);
        assert_eq!(result.skipped_keys, 0);
        assert_eq!(result.failed_keys, 0);
    }

    #[test]
    fn test_cache_error_display() {
        let err = CacheError::WarmupFailed("db error".to_string());
        assert!(err.to_string().contains("db error"));

        let err = CacheError::BloomFilterCapacityExceeded {
            capacity: 100,
            requested: 101,
        };
        assert!(err.to_string().contains("100"));
        assert!(err.to_string().contains("101"));

        let err = CacheError::SingleFlightTimeout("timeout".to_string());
        assert!(err.to_string().contains("timeout"));

        let err = CacheError::WarmupDataStale("stale".to_string());
        assert!(err.to_string().contains("stale"));

        let err = CacheError::CacheUnavailable("down".to_string());
        assert!(err.to_string().contains("down"));
    }

    #[test]
    fn test_bloom_filter_new() {
        let bf = BloomFilter::new(1000, 0.01);
        assert!(bf.is_empty());
        assert_eq!(bf.capacity(), 1000);
        assert_eq!(bf.count(), 0);
    }

    #[test]
    fn test_bloom_filter_add_and_check() {
        let bf = BloomFilter::new(1000, 0.01);
        bf.add("key1").unwrap();
        bf.add("key2").unwrap();
        bf.add("key3").unwrap();
        assert!(bf.might_contain("key1"));
        assert!(bf.might_contain("key2"));
        assert!(bf.might_contain("key3"));
        assert_eq!(bf.count(), 3);
    }

    #[test]
    fn test_bloom_filter_not_contain() {
        let bf = BloomFilter::new(100, 0.01);
        bf.add("existing").unwrap();
        assert!(!bf.might_contain("nonexistent_key_12345"));
    }

    #[test]
    fn test_bloom_filter_capacity_exceeded() {
        let bf = BloomFilter::new(2, 0.01);
        assert!(bf.add("key1").is_ok());
        assert!(bf.add("key2").is_ok());
        let result = bf.add("key3");
        assert!(result.is_err());
        match result {
            Err(crate::bloom::BloomError::CapacityExceeded { capacity, .. }) => {
                assert_eq!(capacity, 2);
            }
            _ => panic!("wrong error"),
        }
    }

    #[test]
    fn test_bloom_filter_clear() {
        let bf = BloomFilter::new(100, 0.01);
        bf.add("key1").unwrap();
        bf.clear();
        assert!(bf.is_empty());
        assert_eq!(bf.count(), 0);
    }

    #[test]
    fn test_bloom_filter_no_false_negatives() {
        let bf = BloomFilter::new(10000, 0.01);
        let keys: Vec<String> = (0..1000).map(|i| format!("key_{i}")).collect();
        for key in &keys {
            bf.add(key).unwrap();
        }
        for key in &keys {
            assert!(bf.might_contain(key), "false negative for {key}");
        }
    }

    #[tokio::test]
    async fn test_cache_warmer_disabled() {
        let cache = make_cache();
        let warmer = CacheWarmer::new(cache);
        let config = WarmupConfig::default();
        let result = warmer
            .warmup(&config, |_| async { Ok(Vec::<(Value, String)>::new()) })
            .await
            .unwrap();
        assert_eq!(result.warmed_keys, 0);
    }

    #[tokio::test]
    async fn test_cache_warmer_hotspot_keys() {
        let cache = make_cache();
        let warmer = CacheWarmer::new(Arc::clone(&cache));
        let config = WarmupConfig::new("users", WarmupStrategy::HotspotKey(vec!["k1".to_string()]));
        let result = warmer
            .warmup(&config, |_| async {
                Ok(vec![
                    (Value::I64(1), "Alice".to_string()),
                    (Value::I64(2), "Bob".to_string()),
                ])
            })
            .await
            .unwrap();
        assert_eq!(result.warmed_keys, 2);
        assert_eq!(result.skipped_keys, 0);
    }

    #[tokio::test]
    async fn test_cache_warmer_skip_existing() {
        let cache = make_cache();
        cache
            .put("users", Value::I64(1), Arc::new("Alice".to_string()))
            .await;
        let warmer = CacheWarmer::new(Arc::clone(&cache));
        let config = WarmupConfig::new("users", WarmupStrategy::HotspotKey(vec!["k1".to_string()]));
        let result = warmer
            .warmup(&config, |_| async {
                Ok(vec![
                    (Value::I64(1), "Alice".to_string()),
                    (Value::I64(2), "Bob".to_string()),
                ])
            })
            .await
            .unwrap();
        assert_eq!(result.warmed_keys, 1);
        assert_eq!(result.skipped_keys, 1);
    }

    #[tokio::test]
    async fn test_cache_warmer_failed_keys() {
        let cache = make_cache();
        let warmer = CacheWarmer::new(cache);
        let config = WarmupConfig::new("users", WarmupStrategy::HotspotKey(vec!["k1".to_string()]));
        let result = warmer
            .warmup(&config, |_| async {
                Err(CacheError::WarmupFailed("db down".to_string()))
            })
            .await
            .unwrap();
        assert_eq!(result.failed_keys, 1);
        assert_eq!(result.warmed_keys, 0);
    }

    #[tokio::test]
    async fn test_penetration_guard_not_registered() {
        let cache = make_cache();
        let guard = PenetrationGuard::new(cache, 1000);
        let result = guard.get("users", &Value::I64(1)).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_penetration_guard_registered_and_cached() {
        let cache = make_cache();
        let guard = PenetrationGuard::new(Arc::clone(&cache), 1000);
        guard
            .put("users", Value::I64(1), "Alice".to_string())
            .await
            .unwrap();
        let result = guard.get("users", &Value::I64(1)).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().as_ref(), "Alice");
    }

    #[tokio::test]
    async fn test_penetration_guard_bloom_miss() {
        let cache = make_cache();
        let guard = PenetrationGuard::new(cache, 1000);
        guard.register("users:1").unwrap();
        assert!(guard.get("users", &Value::I64(1)).await.is_none());
        assert!(guard.get("users", &Value::I64(999)).await.is_none());
    }

    #[tokio::test]
    async fn test_penetration_guard_bloom_count() {
        let cache = make_cache();
        let guard = PenetrationGuard::new(cache, 1000);
        guard
            .put("users", Value::I64(1), "a".to_string())
            .await
            .unwrap();
        guard
            .put("users", Value::I64(2), "b".to_string())
            .await
            .unwrap();
        assert_eq!(guard.bloom_count(), 2);
    }

    #[tokio::test]
    async fn test_single_flight_leader_executes() {
        let sf = SingleFlight::new();
        let result = sf
            .get_or_rebuild("key1", || async { Ok(42_i32) })
            .await
            .unwrap();
        assert_eq!(result, 42);
        assert_eq!(sf.in_flight_count(), 0);
    }

    #[tokio::test]
    async fn test_single_flight_error_propagates() {
        let sf = SingleFlight::new();
        let result: Result<i32, _> = sf
            .get_or_rebuild("key1", || async {
                Err(CacheError::WarmupFailed("fail".to_string()))
            })
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_single_flight_concurrent() {
        let sf = Arc::new(SingleFlight::new());
        let sf1 = Arc::clone(&sf);
        let sf2 = Arc::clone(&sf);

        let h1 = tokio::spawn(async move {
            sf1.get_or_rebuild("shared_key", || async {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                Ok(100_i32)
            })
            .await
        });

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let h2 = tokio::spawn(async move {
            sf2.get_or_rebuild("shared_key", || async { Ok(200_i32) })
                .await
        });

        let r1 = h1.await.unwrap();
        let _r2 = h2.await.unwrap();

        assert!(r1.is_ok());
        assert_eq!(r1.unwrap(), 100);
    }

    #[test]
    fn test_single_flight_default() {
        let sf = SingleFlight::default();
        assert_eq!(sf.in_flight_count(), 0);
    }

    #[test]
    fn test_single_flight_debug() {
        let sf = SingleFlight::new();
        let debug = format!("{:?}", sf);
        assert!(debug.contains("SingleFlight"));
    }

    // ========================================================================
    // 集成测试：ProcessL1Cache → CacheWarmer 生产调用链（v4.7.0 幻影交付修复）
    // ========================================================================

    #[tokio::test]
    async fn test_process_l1_cache_warmup_integration() {
        let cache: Arc<ProcessL1Cache<String>> = make_cache();
        let config = WarmupConfig::new("users", WarmupStrategy::HotspotKey(vec!["k1".to_string()]));
        let result = cache
            .warmup(&config, |_| async {
                Ok(vec![
                    (Value::I64(1), "Alice".to_string()),
                    (Value::I64(2), "Bob".to_string()),
                ])
            })
            .await
            .unwrap();
        assert_eq!(result.warmed_keys, 2);
        assert_eq!(result.skipped_keys, 0);
        let val = cache.get("users", &Value::I64(1)).await;
        assert!(val.is_some());
        assert_eq!(val.unwrap().as_str(), "Alice");
    }

    #[tokio::test]
    async fn test_process_l1_cache_warmup_disabled() {
        let cache: Arc<ProcessL1Cache<String>> = make_cache();
        let config = WarmupConfig::default();
        let result = cache
            .warmup(&config, |_| async { Ok(Vec::<(Value, String)>::new()) })
            .await
            .unwrap();
        assert_eq!(result.warmed_keys, 0);
    }

    #[tokio::test]
    async fn test_process_l1_cache_warmup_skip_existing() {
        let cache: Arc<ProcessL1Cache<String>> = make_cache();
        cache
            .put("users", Value::I64(1), Arc::new("Alice".to_string()))
            .await;
        let config = WarmupConfig::new("users", WarmupStrategy::HotspotKey(vec!["k1".to_string()]));
        let result = cache
            .warmup(&config, |_| async {
                Ok(vec![
                    (Value::I64(1), "Alice".to_string()),
                    (Value::I64(2), "Bob".to_string()),
                ])
            })
            .await
            .unwrap();
        assert_eq!(result.warmed_keys, 1);
        assert_eq!(result.skipped_keys, 1);
    }

    // ========================================================================
    // CacheProtection 组合防护测试（v4.7.0 真剩余接入）
    // ========================================================================

    #[tokio::test]
    async fn test_cache_protection_penetration_short_circuit() {
        // 穿透防护（查询热路径）：注册 key A 后，未注册 key B 的 get 被 bloom 拦截
        let cache: Arc<ProcessL1Cache<String>> = make_cache();
        let protection = CacheProtection::new(Arc::clone(&cache), 1000);

        // 注册 key 1（模拟数据写入）
        protection
            .get_or_load("users", Value::I64(1), || async {
                Ok(Some("Alice".to_string()))
            })
            .await
            .unwrap();

        // 未注册 key（穿透攻击）：get 直接 None，不查 DB（无 loader 概念，纯 bloom 拦截）
        let miss = protection.get("users", &Value::I64(999)).await;
        assert!(miss.is_none(), "未注册 key 的查询必须被穿透防护拦截");
        // 已注册 key：get 命中
        let hit = protection.get("users", &Value::I64(1)).await;
        assert!(hit.is_some(), "已注册 key 的查询必须命中");
    }

    #[tokio::test]
    async fn test_cache_protection_load_and_hit() {
        // 加载路径：get_or_load 不受 bloom 短路（负责注册），二次访问命中不重复加载
        let cache: Arc<ProcessL1Cache<String>> = make_cache();
        let protection = CacheProtection::new(Arc::clone(&cache), 1000);
        let loader_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let calls = Arc::clone(&loader_calls);
        let first = protection
            .get_or_load("users", Value::I64(1), move || {
                calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                async { Ok(Some("Alice".to_string())) }
            })
            .await
            .unwrap();
        assert_eq!(first.unwrap().as_str(), "Alice");
        assert_eq!(protection.bloom_count(), 1, "加载后必须注册布隆过滤器");

        // 二次加载：缓存命中，loader 不再执行
        let calls2 = Arc::clone(&loader_calls);
        let second = protection
            .get_or_load("users", Value::I64(1), move || {
                calls2.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                async { Ok(Some("should-not-reload".to_string())) }
            })
            .await
            .unwrap();
        assert_eq!(second.unwrap().as_str(), "Alice", "二次访问应命中缓存");
        assert_eq!(
            loader_calls.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "缓存命中时 loader 不应重复执行"
        );
    }

    #[tokio::test]
    async fn test_cache_protection_single_flight_dedup() {
        // 击穿防护：并发加载同一 key，loader 只执行一次
        let cache: Arc<ProcessL1Cache<String>> = make_cache();
        let protection = Arc::new(CacheProtection::new(Arc::clone(&cache), 1000));
        let loader_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let mut handles = vec![];
        for _ in 0..8 {
            let p = Arc::clone(&protection);
            let calls = Arc::clone(&loader_calls);
            handles.push(tokio::spawn(async move {
                p.get_or_load("users", Value::I64(7), move || {
                    calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    async { Ok(Some("Bob".to_string())) }
                })
                .await
                .unwrap()
            }));
        }
        for h in handles {
            assert!(h.await.unwrap().is_some());
        }
        assert_eq!(
            loader_calls.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "并发加载同一 key 时 loader 只应执行一次（击穿防护）"
        );
        assert_eq!(protection.bloom_count(), 1);
    }
}
