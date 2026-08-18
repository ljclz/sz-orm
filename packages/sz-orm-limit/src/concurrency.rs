//! 并发限流器（Concurrency Limiter）
//!
//! 限制同时进行的请求数量，而非时间窗口内的请求总数。
//! 适用于保护下游资源（如数据库连接池、外部 API）不被压垮。
//!
//! 与令牌桶/滑动窗口的区别：
//! - 令牌桶/滑动窗口：限制 QPS（每秒请求数）
//! - 并发限流器：限制并发数（同时进行的请求数）

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use crate::{now_timestamp, RateLimitError, RateLimitResult};

/// 并发限流器
///
/// 限制每个 key 同时进行的请求数量。请求完成后必须调用 `release` 释放。
/// 适用于保护下游资源不被压垮。
///
/// # 示例
///
/// ```rust
/// use sz_orm_limit::concurrency::ConcurrencyLimiter;
/// use std::time::Duration;
///
/// let limiter = ConcurrencyLimiter::new(10);
/// let r = limiter.acquire("user-1").unwrap();
/// assert!(r.allowed);
/// limiter.release("user-1");
/// ```
pub struct ConcurrencyLimiter {
    max_concurrent: u64,
    counters: Arc<RwLock<HashMap<String, AtomicI64>>>,
    /// 全局并发上限（跨所有 key）
    global_limit: AtomicI64,
    /// 当前全局并发数
    global_current: AtomicI64,
    /// 最大 key 数量（OOM 防护）
    max_keys: usize,
    /// 总获取次数
    total_acquires: AtomicU64,
    /// 总释放次数
    total_releases: AtomicU64,
    /// 总拒绝次数
    total_rejections: AtomicU64,
}

impl ConcurrencyLimiter {
    /// 创建并发限流器
    ///
    /// - `max_concurrent`：每个 key 允许的最大并发数
    pub fn new(max_concurrent: u64) -> Self {
        Self {
            max_concurrent,
            counters: Arc::new(RwLock::new(HashMap::new())),
            global_limit: AtomicI64::new(max_concurrent as i64 * 100),
            global_current: AtomicI64::new(0),
            max_keys: crate::DEFAULT_MAX_KEYS,
            total_acquires: AtomicU64::new(0),
            total_releases: AtomicU64::new(0),
            total_rejections: AtomicU64::new(0),
        }
    }

    /// 配置全局并发上限
    pub fn with_global_limit(mut self, limit: u64) -> Self {
        self.global_limit = AtomicI64::new(limit as i64);
        self
    }

    /// 配置最大 key 数量
    pub fn with_max_keys(mut self, max_keys: usize) -> Self {
        self.max_keys = max_keys;
        self
    }

    /// 获取 key 的当前并发数
    pub fn current_concurrent(&self, key: &str) -> i64 {
        let counters = self.counters.read().map_err(|e| e.to_string());
        match counters {
            Ok(map) => map.get(key).map(|a| a.load(Ordering::Relaxed)).unwrap_or(0),
            Err(_) => 0,
        }
    }

    /// 获取全局当前并发数
    pub fn global_current(&self) -> i64 {
        self.global_current.load(Ordering::Relaxed)
    }

    /// 获取最大并发数
    pub fn max_concurrent(&self) -> u64 {
        self.max_concurrent
    }

    /// 获取全局并发上限
    pub fn global_limit(&self) -> i64 {
        self.global_limit.load(Ordering::Relaxed)
    }

    /// 获取当前 key 数量
    pub fn key_count(&self) -> usize {
        self.counters.read().map(|m| m.len()).unwrap_or(0)
    }

    /// 尝试获取一个并发槽位
    ///
    /// 如果当前 key 的并发数未超过 `max_concurrent` 且全局并发数未超过
    /// `global_limit`，则并发数 +1 并返回 allowed。
    /// 否则返回 rejected。
    pub fn acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        self.total_acquires.fetch_add(1, Ordering::Relaxed);
        let mut counters = self
            .counters
            .write()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;

        // OOM 防护：超出 max_keys 时淘汰一个
        if counters.len() >= self.max_keys && !counters.contains_key(key) {
            let oldest = counters.keys().next().cloned();
            if let Some(k) = oldest {
                counters.remove(&k);
            }
        }

        let counter = counters
            .entry(key.to_string())
            .or_insert_with(|| AtomicI64::new(0));

        let current = counter.load(Ordering::Relaxed);
        let global = self.global_current.load(Ordering::Relaxed);
        let global_limit = self.global_limit.load(Ordering::Relaxed);

        if current >= self.max_concurrent as i64 {
            self.total_rejections.fetch_add(1, Ordering::Relaxed);
            return Ok(RateLimitResult::rejected(0, now_timestamp() + 1000));
        }
        if global >= global_limit {
            self.total_rejections.fetch_add(1, Ordering::Relaxed);
            return Ok(RateLimitResult::rejected(0, now_timestamp() + 1000));
        }

        counter.fetch_add(1, Ordering::Relaxed);
        self.global_current.fetch_add(1, Ordering::Relaxed);
        let remaining = self.max_concurrent - counter.load(Ordering::Relaxed) as u64;
        Ok(RateLimitResult::allowed(remaining, now_timestamp() + 60000))
    }

    /// 释放一个并发槽位
    ///
    /// 请求完成后必须调用此方法释放槽位，否则并发数会持续增长。
    pub fn release(&self, key: &str) -> Result<(), RateLimitError> {
        self.total_releases.fetch_add(1, Ordering::Relaxed);
        let counters = self
            .counters
            .read()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;
        if let Some(counter) = counters.get(key) {
            let current = counter.load(Ordering::Relaxed);
            if current > 0 {
                counter.fetch_sub(1, Ordering::Relaxed);
                self.global_current.fetch_sub(1, Ordering::Relaxed);
            }
        }
        Ok(())
    }

    /// 重置 key 的并发计数
    pub fn reset(&self, key: &str) -> Result<(), RateLimitError> {
        let counters = self
            .counters
            .read()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;
        if let Some(counter) = counters.get(key) {
            let current = counter.load(Ordering::Relaxed);
            if current > 0 {
                self.global_current.fetch_sub(current, Ordering::Relaxed);
                counter.store(0, Ordering::Relaxed);
            }
        }
        Ok(())
    }

    /// 获取统计信息
    pub fn stats(&self) -> ConcurrencyStats {
        ConcurrencyStats {
            max_concurrent: self.max_concurrent,
            global_limit: self.global_limit.load(Ordering::Relaxed),
            global_current: self.global_current.load(Ordering::Relaxed),
            key_count: self.key_count(),
            total_acquires: self.total_acquires.load(Ordering::Relaxed),
            total_releases: self.total_releases.load(Ordering::Relaxed),
            total_rejections: self.total_rejections.load(Ordering::Relaxed),
        }
    }
}

/// 并发限流统计信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct ConcurrencyStats {
    pub max_concurrent: u64,
    pub global_limit: i64,
    pub global_current: i64,
    pub key_count: usize,
    pub total_acquires: u64,
    pub total_releases: u64,
    pub total_rejections: u64,
}

/// 带超时的并发槽位守卫
///
/// 获取后自动占用槽位，drop 时自动释放。
/// 适用于 `with` 模式确保资源释放。
pub struct ConcurrencyGuard<'a> {
    limiter: &'a ConcurrencyLimiter,
    key: String,
    acquired: bool,
}

impl<'a> ConcurrencyGuard<'a> {
    /// 尝试获取并发槽位
    pub fn acquire(limiter: &'a ConcurrencyLimiter, key: &str) -> Result<Self, RateLimitError> {
        let result = limiter.acquire(key)?;
        Ok(Self {
            limiter,
            key: key.to_string(),
            acquired: result.allowed,
        })
    }

    /// 是否成功获取
    pub fn is_acquired(&self) -> bool {
        self.acquired
    }

    /// 手动释放（drop 时也会自动释放）
    pub fn release(mut self) {
        if self.acquired {
            let _ = self.limiter.release(&self.key);
            self.acquired = false;
        }
    }
}

impl<'a> Drop for ConcurrencyGuard<'a> {
    fn drop(&mut self) {
        if self.acquired {
            let _ = self.limiter.release(&self.key);
        }
    }
}

/// 带等待超时的并发限流器
///
/// 在 `acquire` 时如果并发已满，会等待最多 `wait_timeout` 时间，
/// 期间不断重试。超时后返回 rejected。
pub struct TimedConcurrencyLimiter {
    inner: ConcurrencyLimiter,
    wait_timeout: Duration,
    retry_interval: Duration,
}

impl TimedConcurrencyLimiter {
    /// 创建带超时的并发限流器
    ///
    /// - `max_concurrent`：每个 key 的最大并发数
    /// - `wait_timeout`：获取槽位的最长等待时间
    pub fn new(max_concurrent: u64, wait_timeout: Duration) -> Self {
        Self {
            inner: ConcurrencyLimiter::new(max_concurrent),
            wait_timeout,
            retry_interval: Duration::from_millis(10),
        }
    }

    /// 配置重试间隔
    pub fn with_retry_interval(mut self, interval: Duration) -> Self {
        self.retry_interval = interval;
        self
    }

    /// 配置全局并发上限
    pub fn with_global_limit(self, limit: u64) -> Self {
        Self {
            inner: self.inner.with_global_limit(limit),
            ..self
        }
    }

    /// 尝试获取槽位，最多等待 `wait_timeout`
    ///
    /// 注意：此方法会忙等重试，不适用于高并发场景。
    /// 生产环境建议使用异步版本或消息队列。
    pub fn acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        let start = std::time::Instant::now();
        loop {
            let result = self.inner.acquire(key)?;
            if result.allowed {
                return Ok(result);
            }
            if start.elapsed() >= self.wait_timeout {
                return Ok(result);
            }
            std::thread::sleep(self.retry_interval);
        }
    }

    /// 释放槽位
    pub fn release(&self, key: &str) -> Result<(), RateLimitError> {
        self.inner.release(key)
    }

    /// 获取内部限流器
    pub fn inner(&self) -> &ConcurrencyLimiter {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concurrency_limiter_basic_acquire() {
        let limiter = ConcurrencyLimiter::new(5);
        let r = limiter.acquire("k").unwrap();
        assert!(r.allowed);
        assert_eq!(r.remaining, 4);
    }

    #[test]
    fn test_concurrency_limiter_max_concurrent() {
        let limiter = ConcurrencyLimiter::new(2);
        assert!(limiter.acquire("k").unwrap().allowed);
        assert!(limiter.acquire("k").unwrap().allowed);
        let r3 = limiter.acquire("k").unwrap();
        assert!(!r3.allowed);
    }

    #[test]
    fn test_concurrency_limiter_release() {
        let limiter = ConcurrencyLimiter::new(1);
        assert!(limiter.acquire("k").unwrap().allowed);
        assert!(!limiter.acquire("k").unwrap().allowed);
        limiter.release("k").unwrap();
        assert!(limiter.acquire("k").unwrap().allowed);
    }

    #[test]
    fn test_concurrency_limiter_different_keys() {
        let limiter = ConcurrencyLimiter::new(1);
        assert!(limiter.acquire("a").unwrap().allowed);
        assert!(limiter.acquire("b").unwrap().allowed);
    }

    #[test]
    fn test_concurrency_limiter_current_concurrent() {
        let limiter = ConcurrencyLimiter::new(5);
        assert_eq!(limiter.current_concurrent("k"), 0);
        limiter.acquire("k").unwrap();
        assert_eq!(limiter.current_concurrent("k"), 1);
        limiter.acquire("k").unwrap();
        assert_eq!(limiter.current_concurrent("k"), 2);
    }

    #[test]
    fn test_concurrency_limiter_global_current() {
        let limiter = ConcurrencyLimiter::new(5);
        assert_eq!(limiter.global_current(), 0);
        limiter.acquire("a").unwrap();
        limiter.acquire("b").unwrap();
        assert_eq!(limiter.global_current(), 2);
        limiter.release("a").unwrap();
        assert_eq!(limiter.global_current(), 1);
    }

    #[test]
    fn test_concurrency_limiter_global_limit() {
        let limiter = ConcurrencyLimiter::new(10).with_global_limit(2);
        assert!(limiter.acquire("a").unwrap().allowed);
        assert!(limiter.acquire("b").unwrap().allowed);
        let r3 = limiter.acquire("c").unwrap();
        assert!(!r3.allowed);
    }

    #[test]
    fn test_concurrency_limiter_reset() {
        let limiter = ConcurrencyLimiter::new(5);
        limiter.acquire("k").unwrap();
        limiter.acquire("k").unwrap();
        assert_eq!(limiter.current_concurrent("k"), 2);
        limiter.reset("k").unwrap();
        assert_eq!(limiter.current_concurrent("k"), 0);
    }

    #[test]
    fn test_concurrency_limiter_stats() {
        let limiter = ConcurrencyLimiter::new(3);
        limiter.acquire("k").unwrap();
        limiter.acquire("k").unwrap();
        limiter.release("k").unwrap();
        limiter.acquire("k").unwrap();
        let stats = limiter.stats();
        assert_eq!(stats.max_concurrent, 3);
        assert_eq!(stats.total_acquires, 3);
        assert_eq!(stats.total_releases, 1);
        assert_eq!(stats.global_current, 2);
    }

    #[test]
    fn test_concurrency_limiter_key_count() {
        let limiter = ConcurrencyLimiter::new(5);
        assert_eq!(limiter.key_count(), 0);
        limiter.acquire("a").unwrap();
        limiter.acquire("b").unwrap();
        assert_eq!(limiter.key_count(), 2);
    }

    #[test]
    fn test_concurrency_guard_auto_release() {
        let limiter = ConcurrencyLimiter::new(1);
        {
            let guard = ConcurrencyGuard::acquire(&limiter, "k").unwrap();
            assert!(guard.is_acquired());
            assert_eq!(limiter.current_concurrent("k"), 1);
        }
        assert_eq!(limiter.current_concurrent("k"), 0);
    }

    #[test]
    fn test_concurrency_guard_manual_release() {
        let limiter = ConcurrencyLimiter::new(1);
        let guard = ConcurrencyGuard::acquire(&limiter, "k").unwrap();
        assert_eq!(limiter.current_concurrent("k"), 1);
        guard.release();
        assert_eq!(limiter.current_concurrent("k"), 0);
    }

    #[test]
    fn test_concurrency_guard_rejected() {
        let limiter = ConcurrencyLimiter::new(1);
        let g1 = ConcurrencyGuard::acquire(&limiter, "k").unwrap();
        let g2 = ConcurrencyGuard::acquire(&limiter, "k").unwrap();
        assert!(g1.is_acquired());
        assert!(!g2.is_acquired());
    }

    #[test]
    fn test_timed_concurrency_limiter_immediate() {
        let limiter = TimedConcurrencyLimiter::new(2, Duration::from_millis(100));
        let r = limiter.acquire("k").unwrap();
        assert!(r.allowed);
        limiter.release("k").unwrap();
    }

    #[test]
    fn test_timed_concurrency_limiter_timeout() {
        let limiter = TimedConcurrencyLimiter::new(1, Duration::from_millis(50))
            .with_retry_interval(Duration::from_millis(5));
        let r1 = limiter.acquire("k").unwrap();
        assert!(r1.allowed);
        let r2 = limiter.acquire("k").unwrap();
        assert!(!r2.allowed);
    }

    #[test]
    fn test_concurrency_limiter_release_below_zero_guard() {
        let limiter = ConcurrencyLimiter::new(5);
        limiter.release("k").unwrap();
        assert_eq!(limiter.current_concurrent("k"), 0);
    }

    #[test]
    fn test_concurrency_limiter_double_release() {
        let limiter = ConcurrencyLimiter::new(5);
        limiter.acquire("k").unwrap();
        limiter.release("k").unwrap();
        limiter.release("k").unwrap();
        assert_eq!(limiter.current_concurrent("k"), 0);
        assert_eq!(limiter.global_current(), 0);
    }
}
