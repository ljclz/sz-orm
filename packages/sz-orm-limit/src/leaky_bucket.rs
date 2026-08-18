//! 漏桶限流器（Leaky Bucket Limiter）
//!
//! 漏桶算法以恒定速率"漏出"请求，多余请求被拒绝或排队。
//! 与令牌桶对偶：令牌桶限制突发量，漏桶平滑输出速率。
//!
//! 适用于需要平滑输出速率的场景（如消息队列生产者）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::{now_timestamp, RateLimitError, RateLimitResult};

/// 漏桶限流器
///
/// 桶容量为 `capacity`，以 `leak_rate`（请求/秒）的速率漏出。
/// 请求到来时加入桶，如果桶满则拒绝。
pub struct LeakyBucketLimiter {
    capacity: u64,
    leak_rate: f64,
    buckets: Arc<RwLock<HashMap<String, LeakyBucketEntry>>>,
    max_keys: usize,
    total_allowed: AtomicU64,
    total_rejected: AtomicU64,
}

#[derive(Clone)]
struct LeakyBucketEntry {
    water: f64,
    last_leak: Instant,
}

impl LeakyBucketLimiter {
    /// 创建漏桶限流器
    ///
    /// - `capacity`：桶容量（最大排队请求数）
    /// - `leak_rate`：漏出速率（请求/秒）
    pub fn new(capacity: u64, leak_rate: f64) -> Self {
        Self {
            capacity,
            leak_rate,
            buckets: Arc::new(RwLock::new(HashMap::new())),
            max_keys: crate::DEFAULT_MAX_KEYS,
            total_allowed: AtomicU64::new(0),
            total_rejected: AtomicU64::new(0),
        }
    }

    /// 配置最大 key 数量
    pub fn with_max_keys(mut self, max_keys: usize) -> Self {
        self.max_keys = max_keys;
        self
    }

    /// 桶容量
    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    /// 漏出速率
    pub fn leak_rate(&self) -> f64 {
        self.leak_rate
    }

    /// 当前 key 数量
    pub fn key_count(&self) -> usize {
        self.buckets.read().map(|m| m.len()).unwrap_or(0)
    }

    /// 获取 key 的当前水位
    pub fn water_level(&self, key: &str) -> f64 {
        let buckets = self.buckets.read().map_err(|e| e.to_string());
        match buckets {
            Ok(map) => map.get(key).map(|e| e.water).unwrap_or(0.0),
            Err(_) => 0.0,
        }
    }

    fn leak(&self, entry: &mut LeakyBucketEntry) {
        let now = Instant::now();
        let elapsed = now.duration_since(entry.last_leak).as_secs_f64();
        let leaked = if self.leak_rate > 0.0 {
            elapsed * self.leak_rate
        } else {
            0.0
        };
        entry.water = (entry.water - leaked).max(0.0);
        entry.last_leak = now;
    }

    fn enforce_max_keys(&self, buckets: &mut HashMap<String, LeakyBucketEntry>) {
        while buckets.len() > self.max_keys {
            let oldest = buckets
                .iter()
                .min_by_key(|(_, e)| e.last_leak)
                .map(|(k, _)| k.clone());
            match oldest {
                Some(k) => {
                    buckets.remove(&k);
                }
                None => break,
            }
        }
    }

    /// 尝试加入一个请求
    pub fn acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        let mut buckets = self
            .buckets
            .write()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;

        if buckets.len() >= self.max_keys && !buckets.contains_key(key) {
            self.enforce_max_keys(&mut buckets);
        }

        let entry = buckets
            .entry(key.to_string())
            .or_insert_with(|| LeakyBucketEntry {
                water: 0.0,
                last_leak: Instant::now(),
            });

        self.leak(entry);

        if entry.water + 1.0 <= self.capacity as f64 {
            entry.water += 1.0;
            let remaining = (self.capacity as f64 - entry.water).floor() as u64;
            self.total_allowed.fetch_add(1, Ordering::Relaxed);
            Ok(RateLimitResult::allowed(remaining, now_timestamp() + 1000))
        } else {
            self.total_rejected.fetch_add(1, Ordering::Relaxed);
            Ok(RateLimitResult::rejected(0, now_timestamp() + 1000))
        }
    }

    /// 重置 key
    pub fn reset(&self, key: &str) -> Result<(), RateLimitError> {
        let mut buckets = self
            .buckets
            .write()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;
        buckets.remove(key);
        Ok(())
    }

    /// 统计信息
    pub fn stats(&self) -> LeakyBucketStats {
        LeakyBucketStats {
            capacity: self.capacity,
            leak_rate: self.leak_rate,
            key_count: self.key_count(),
            total_allowed: self.total_allowed.load(Ordering::Relaxed),
            total_rejected: self.total_rejected.load(Ordering::Relaxed),
        }
    }
}

/// 漏桶统计信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct LeakyBucketStats {
    pub capacity: u64,
    pub leak_rate: f64,
    pub key_count: usize,
    pub total_allowed: u64,
    pub total_rejected: u64,
}

/// 滑动窗口日志算法（Sliding Window Log）
///
/// 与滑动窗口计数器不同，日志算法记录每个请求的精确时间戳，
/// 提供更精确的限流（无边界突刺），但内存占用更高。
pub struct SlidingWindowLogLimiter {
    max_requests: u64,
    window_size: Duration,
    entries: Arc<RwLock<HashMap<String, Vec<Instant>>>>,
    max_keys: usize,
}

impl SlidingWindowLogLimiter {
    /// 创建滑动窗口日志限流器
    pub fn new(max_requests: u64, window_size: Duration) -> Self {
        Self {
            max_requests,
            window_size,
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_keys: crate::DEFAULT_MAX_KEYS,
        }
    }

    /// 配置最大 key 数量
    pub fn with_max_keys(mut self, max_keys: usize) -> Self {
        self.max_keys = max_keys;
        self
    }

    /// 最大请求数
    pub fn max_requests(&self) -> u64 {
        self.max_requests
    }

    /// 窗口大小
    pub fn window_size(&self) -> Duration {
        self.window_size
    }

    /// 当前 key 数量
    pub fn key_count(&self) -> usize {
        self.entries.read().map(|m| m.len()).unwrap_or(0)
    }

    /// 当前窗口内请求数
    pub fn current_count(&self, key: &str) -> usize {
        let entries = self.entries.read().map_err(|e| e.to_string());
        match entries {
            Ok(map) => {
                if let Some(log) = map.get(key) {
                    let now = Instant::now();
                    log.iter()
                        .filter(|&&t| now.duration_since(t) < self.window_size)
                        .count()
                } else {
                    0
                }
            }
            Err(_) => 0,
        }
    }

    fn enforce_max_keys(&self, entries: &mut HashMap<String, Vec<Instant>>) {
        while entries.len() > self.max_keys {
            let now = Instant::now();
            let oldest = entries
                .iter()
                .min_by_key(|(_, log)| log.first().copied().unwrap_or(now))
                .map(|(k, _)| k.clone());
            match oldest {
                Some(k) => {
                    entries.remove(&k);
                }
                None => break,
            }
        }
    }

    /// 尝试获取一个请求
    pub fn acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;

        if entries.len() >= self.max_keys && !entries.contains_key(key) {
            self.enforce_max_keys(&mut entries);
        }

        let log = entries.entry(key.to_string()).or_insert_with(Vec::new);

        let now = Instant::now();
        log.retain(|&t| now.duration_since(t) < self.window_size);

        if log.len() < self.max_requests as usize {
            log.push(now);
            let remaining = self.max_requests - log.len() as u64;
            Ok(RateLimitResult::allowed(remaining, now_timestamp() + 1000))
        } else {
            Ok(RateLimitResult::rejected(0, now_timestamp() + 1000))
        }
    }

    /// 重置 key
    pub fn reset(&self, key: &str) -> Result<(), RateLimitError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;
        entries.remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leaky_bucket_basic() {
        let limiter = LeakyBucketLimiter::new(5, 1.0);
        let r = limiter.acquire("k").unwrap();
        assert!(r.allowed);
    }

    #[test]
    fn test_leaky_bucket_full() {
        let limiter = LeakyBucketLimiter::new(2, 0.0);
        assert!(limiter.acquire("k").unwrap().allowed);
        assert!(limiter.acquire("k").unwrap().allowed);
        assert!(!limiter.acquire("k").unwrap().allowed);
    }

    #[test]
    fn test_leaky_bucket_different_keys() {
        let limiter = LeakyBucketLimiter::new(1, 0.0);
        assert!(limiter.acquire("a").unwrap().allowed);
        assert!(limiter.acquire("b").unwrap().allowed);
    }

    #[test]
    fn test_leaky_bucket_water_level() {
        let limiter = LeakyBucketLimiter::new(5, 0.0);
        assert_eq!(limiter.water_level("k"), 0.0);
        limiter.acquire("k").unwrap();
        assert!(limiter.water_level("k") > 0.0);
    }

    #[test]
    fn test_leaky_bucket_reset() {
        let limiter = LeakyBucketLimiter::new(1, 0.0);
        limiter.acquire("k").unwrap();
        assert!(!limiter.acquire("k").unwrap().allowed);
        limiter.reset("k").unwrap();
        assert!(limiter.acquire("k").unwrap().allowed);
    }

    #[test]
    fn test_leaky_bucket_stats() {
        let limiter = LeakyBucketLimiter::new(3, 1.0);
        limiter.acquire("k").unwrap();
        limiter.acquire("k").unwrap();
        let stats = limiter.stats();
        assert_eq!(stats.capacity, 3);
        assert_eq!(stats.total_allowed, 2);
        assert_eq!(stats.total_rejected, 0);
    }

    #[test]
    fn test_leaky_bucket_key_count() {
        let limiter = LeakyBucketLimiter::new(5, 1.0);
        assert_eq!(limiter.key_count(), 0);
        limiter.acquire("a").unwrap();
        limiter.acquire("b").unwrap();
        assert_eq!(limiter.key_count(), 2);
    }

    #[test]
    fn test_sliding_window_log_basic() {
        let limiter = SlidingWindowLogLimiter::new(5, Duration::from_secs(60));
        let r = limiter.acquire("k").unwrap();
        assert!(r.allowed);
    }

    #[test]
    fn test_sliding_window_log_full() {
        let limiter = SlidingWindowLogLimiter::new(2, Duration::from_secs(60));
        assert!(limiter.acquire("k").unwrap().allowed);
        assert!(limiter.acquire("k").unwrap().allowed);
        assert!(!limiter.acquire("k").unwrap().allowed);
    }

    #[test]
    fn test_sliding_window_log_different_keys() {
        let limiter = SlidingWindowLogLimiter::new(1, Duration::from_secs(60));
        assert!(limiter.acquire("a").unwrap().allowed);
        assert!(limiter.acquire("b").unwrap().allowed);
    }

    #[test]
    fn test_sliding_window_log_current_count() {
        let limiter = SlidingWindowLogLimiter::new(5, Duration::from_secs(60));
        assert_eq!(limiter.current_count("k"), 0);
        limiter.acquire("k").unwrap();
        limiter.acquire("k").unwrap();
        assert_eq!(limiter.current_count("k"), 2);
    }

    #[test]
    fn test_sliding_window_log_reset() {
        let limiter = SlidingWindowLogLimiter::new(1, Duration::from_secs(60));
        limiter.acquire("k").unwrap();
        assert!(!limiter.acquire("k").unwrap().allowed);
        limiter.reset("k").unwrap();
        assert!(limiter.acquire("k").unwrap().allowed);
    }

    #[test]
    fn test_sliding_window_log_key_count() {
        let limiter = SlidingWindowLogLimiter::new(5, Duration::from_secs(60));
        assert_eq!(limiter.key_count(), 0);
        limiter.acquire("a").unwrap();
        assert_eq!(limiter.key_count(), 1);
    }

    #[test]
    fn test_sliding_window_log_remaining() {
        let limiter = SlidingWindowLogLimiter::new(3, Duration::from_secs(60));
        let r1 = limiter.acquire("k").unwrap();
        assert_eq!(r1.remaining, 2);
        let r2 = limiter.acquire("k").unwrap();
        assert_eq!(r2.remaining, 1);
    }
}
