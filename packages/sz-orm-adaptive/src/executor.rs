//! 自适应执行器：决策（decide）+ 统计记录（record）+ 缓存执行（execute_cached）
//!
//! 设计说明：
//! - 本模块**只做决策层**，不重写分页/缓存实现——调用方按 [`ExecutionPath`]
//!   复用既有 `sz-orm-core` 的 `cursor_stream` / `paginator` / `l2_cache`
//! - 自动缓存默认关闭（[`AdaptiveConfig::cache_enabled`]，防脏读，需显式开启）
//! - 慢查询不做强行中断（Rust 同步闭包无法安全取消），以结构化标记输出

use crate::stats::QueryStats;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// 决策出的执行路径
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionPath {
    /// 正常执行
    Normal,
    /// 建议游标分页（大结果集）
    Paginated,
    /// 使用缓存结果（热点查询）
    Cached,
}

/// 自适应配置
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdaptiveConfig {
    /// 平均行数阈值：超过 → 建议分页（默认 1000）
    pub row_threshold: u64,
    /// 热点缓存 TTL（毫秒，默认 60_000）
    pub cache_time_ms: u64,
    /// 缓存决策最少执行次数（默认 100）
    pub min_executions: u64,
    /// 慢查询阈值（毫秒，默认 100）
    pub slow_ms: u64,
    /// 自动缓存开关（默认关闭，防脏读）
    pub cache_enabled: bool,
}

impl Default for AdaptiveConfig {
    fn default() -> Self {
        Self {
            row_threshold: 1000,
            cache_time_ms: 60_000,
            min_executions: 100,
            slow_ms: 100,
            cache_enabled: false,
        }
    }
}

/// 结果缓存抽象（调用方可将既有 `l2_cache` 适配到此 trait）
pub trait ResultCache: Send + Sync {
    /// 读取缓存（未命中返回 `None`）
    fn get(&self, key: &str) -> Option<Vec<u8>>;
    /// 写入缓存（TTL 毫秒）
    fn set(&self, key: &str, value: Vec<u8>, ttl_ms: u64);
}

/// 进程内 TTL 内存缓存（测试/轻量场景；生产可替换为 L2 缓存适配器）
pub struct MemoryTtlCache {
    inner: Mutex<HashMap<String, (Vec<u8>, Instant)>>,
}

impl MemoryTtlCache {
    /// 创建空缓存
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for MemoryTtlCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ResultCache for MemoryTtlCache {
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        let inner = self.inner.lock().ok()?;
        let (value, deadline) = inner.get(key)?;
        if Instant::now() > *deadline {
            None
        } else {
            Some(value.clone())
        }
    }

    fn set(&self, key: &str, value: Vec<u8>, ttl_ms: u64) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.insert(
                key.to_string(),
                (
                    value,
                    Instant::now() + std::time::Duration::from_millis(ttl_ms),
                ),
            );
        }
    }
}

/// 一次查询的执行结果
#[derive(Debug, Clone)]
pub struct QueryOutcome<T> {
    /// 查询结果
    pub value: T,
    /// 返回行数
    pub rows: u64,
    /// 执行耗时（毫秒）
    pub elapsed_ms: u64,
    /// 是否命中缓存
    pub from_cache: bool,
    /// 是否慢查询（超过 `slow_ms`）
    pub slow: bool,
}

/// 自适应执行器（线程安全，按 query_key 独立统计）
pub struct AdaptiveExecutor {
    stats: Mutex<HashMap<String, Arc<QueryStats>>>,
    config: AdaptiveConfig,
    cache: Option<Arc<dyn ResultCache>>,
}

impl AdaptiveExecutor {
    /// 创建执行器
    pub fn new(config: AdaptiveConfig) -> Self {
        Self {
            stats: Mutex::new(HashMap::new()),
            config,
            cache: None,
        }
    }

    /// 挂载结果缓存（自动缓存路径使用；需同时开启 `config.cache_enabled`）
    pub fn with_cache(mut self, cache: Arc<dyn ResultCache>) -> Self {
        self.cache = Some(cache);
        self
    }

    /// 当前配置
    pub fn config(&self) -> &AdaptiveConfig {
        &self.config
    }

    /// 获取（或创建）某 query_key 的统计
    pub fn stats_for(&self, query_key: &str) -> Arc<QueryStats> {
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        stats
            .entry(query_key.to_string())
            .or_insert_with(|| Arc::new(QueryStats::new()))
            .clone()
    }

    /// 决策：按当前统计选择执行路径
    pub fn decide(&self, query_key: &str) -> ExecutionPath {
        let s = self.stats_for(query_key);
        if self.config.cache_enabled
            && self.cache.is_some()
            && s.should_cache(self.config.slow_ms, self.config.min_executions)
        {
            return ExecutionPath::Cached;
        }
        if s.should_paginate(self.config.row_threshold) {
            return ExecutionPath::Paginated;
        }
        ExecutionPath::Normal
    }

    /// 记录一次执行结果，返回是否慢查询
    pub fn record(&self, query_key: &str, rows: u64, elapsed_ms: u64) -> bool {
        self.stats_for(query_key)
            .record(rows, elapsed_ms.saturating_mul(1000));
        elapsed_ms >= self.config.slow_ms
    }

    /// 缓存执行：先查缓存（命中直接返回），否则执行闭包并回填缓存。
    ///
    /// 仅在 `adaptive-query` feature 下可用（需要 serde 序列化）。
    /// 闭包返回 `Result<(结果, 行数), 错误>`；执行耗时由本方法计时，
    /// 闭包失败时错误原样返回（不 panic）。
    #[cfg(feature = "adaptive-query")]
    pub fn execute_cached<T, F>(&self, query_key: &str, f: F) -> Result<QueryOutcome<T>, String>
    where
        T: serde::Serialize + serde::de::DeserializeOwned + Clone,
        F: FnOnce() -> Result<(T, u64), String>,
    {
        let cache_key = format!("sz-orm-adaptive:{query_key}");
        // 1. 缓存命中（仅当缓存开启且决策为 Cached）
        if self.config.cache_enabled {
            if let Some(cache) = &self.cache {
                if let Some(bytes) = cache.get(&cache_key) {
                    if let Ok(value) = serde_json::from_slice::<T>(&bytes) {
                        self.stats_for(query_key).record(0, 1);
                        return Ok(QueryOutcome {
                            value,
                            rows: 0,
                            elapsed_ms: 0,
                            from_cache: true,
                            slow: false,
                        });
                    }
                }
            }
        }

        // 2. 正常执行
        let start = Instant::now();
        let (value, rows) = f()?;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        let slow = self.record(query_key, rows, elapsed_ms);

        // 3. 回填缓存（慢查询 + 开启缓存时）
        if self.config.cache_enabled && slow {
            if let Some(cache) = &self.cache {
                if let Ok(bytes) = serde_json::to_vec(&value) {
                    cache.set(&cache_key, bytes, self.config.cache_time_ms);
                }
            }
        }

        Ok(QueryOutcome {
            value,
            rows,
            elapsed_ms,
            from_cache: false,
            slow,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn decide_starts_normal() {
        let ex = AdaptiveExecutor::new(AdaptiveConfig::default());
        assert_eq!(ex.decide("q1"), ExecutionPath::Normal);
    }

    #[test]
    fn decide_flips_to_paginated_after_high_rows() {
        let ex = AdaptiveExecutor::new(AdaptiveConfig::default());
        for _ in 0..3 {
            ex.record("q1", 5000, 10); // 平均 5000 > 1000
        }
        assert_eq!(ex.decide("q1"), ExecutionPath::Paginated);
    }

    #[test]
    fn decide_cached_requires_enabled_and_slow_and_frequent() {
        let config = AdaptiveConfig {
            cache_enabled: true,
            min_executions: 5,
            slow_ms: 100,
            ..AdaptiveConfig::default()
        };
        let ex = AdaptiveExecutor::new(config).with_cache(Arc::new(MemoryTtlCache::new()));
        // 慢且高频
        for _ in 0..6 {
            ex.record("q1", 1, 500);
        }
        assert_eq!(ex.decide("q1"), ExecutionPath::Cached);
    }

    #[test]
    fn decide_cache_disabled_by_default() {
        let ex = AdaptiveExecutor::new(AdaptiveConfig::default());
        for _ in 0..10 {
            ex.record("q1", 1, 500);
        }
        // cache_enabled = false → 不会返回 Cached
        assert_eq!(ex.decide("q1"), ExecutionPath::Normal);
    }

    #[test]
    fn record_reports_slow_query() {
        let ex = AdaptiveExecutor::new(AdaptiveConfig::default());
        assert!(!ex.record("q1", 1, 10));
        assert!(ex.record("q1", 1, 500));
        let s = ex.stats_for("q1");
        assert_eq!(s.total_executions(), 2);
    }

    #[test]
    fn memory_ttl_cache_expires() {
        let cache = MemoryTtlCache::new();
        cache.set("k", b"v".to_vec(), 20);
        assert_eq!(cache.get("k"), Some(b"v".to_vec()));
        thread::sleep(Duration::from_millis(40));
        assert_eq!(cache.get("k"), None);
    }

    #[test]
    fn stats_are_isolated_per_query_key() {
        let ex = AdaptiveExecutor::new(AdaptiveConfig::default());
        ex.record("a", 5000, 1);
        ex.record("b", 1, 1);
        assert_eq!(ex.decide("a"), ExecutionPath::Paginated);
        assert_eq!(ex.decide("b"), ExecutionPath::Normal);
    }

    #[cfg(feature = "adaptive-query")]
    #[test]
    fn execute_cached_roundtrip() {
        let config = AdaptiveConfig {
            cache_enabled: true,
            min_executions: 0,
            slow_ms: 0,
            ..AdaptiveConfig::default()
        };
        let ex = AdaptiveExecutor::new(config).with_cache(Arc::new(MemoryTtlCache::new()));
        let mut calls = 0;
        let outcome = ex
            .execute_cached("q", || {
                calls += 1;
                Ok((vec![1u32, 2, 3], 3u64))
            })
            .expect("first run ok");
        assert_eq!(outcome.value, vec![1, 2, 3]);
        assert!(!outcome.from_cache);
        // 第二次命中缓存
        let second = ex
            .execute_cached::<Vec<u32>, _>("q", || {
                calls += 1;
                Ok((vec![], 0u64))
            })
            .expect("cached run ok");
        assert_eq!(second.value, vec![1, 2, 3]);
        assert!(second.from_cache);
        assert_eq!(calls, 1, "closure must run exactly once");
    }

    #[cfg(feature = "adaptive-query")]
    #[test]
    fn execute_cached_propagates_closure_error() {
        let ex = AdaptiveExecutor::new(AdaptiveConfig::default());
        let err = ex
            .execute_cached::<u32, _>("q", || Err("db down".to_string()))
            .unwrap_err();
        assert_eq!(err, "db down");
    }

    #[cfg(feature = "adaptive-query")]
    #[test]
    fn execute_cached_cache_disabled_runs_every_time() {
        let ex = AdaptiveExecutor::new(AdaptiveConfig::default()); // cache_enabled = false
        let mut calls = 0;
        for _ in 0..3 {
            let _ = ex
                .execute_cached("q", || {
                    calls += 1;
                    Ok((42u32, 1u64))
                })
                .expect("run ok");
        }
        assert_eq!(calls, 3);
    }
}
