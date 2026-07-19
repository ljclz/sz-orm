use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

pub trait RateLimiter: Send + Sync {
    fn acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError>;
    fn try_acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError>;
    fn reset(&self, key: &str) -> Result<(), RateLimitError>;
}

#[derive(Debug, Clone)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub remaining: u64,
    pub reset_at: i64,
}

impl RateLimitResult {
    pub fn allowed(remaining: u64, reset_at: i64) -> Self {
        Self {
            allowed: true,
            remaining,
            reset_at,
        }
    }

    pub fn rejected(remaining: u64, reset_at: i64) -> Self {
        Self {
            allowed: false,
            remaining,
            reset_at,
        }
    }
}

pub struct SlidingWindowRateLimiter {
    max_requests: u64,
    window_size: Duration,
    entries: Arc<RwLock<HashMap<String, SlidingWindowEntry>>>,
}

#[derive(Clone)]
struct SlidingWindowEntry {
    requests: Vec<Instant>,
}

impl SlidingWindowRateLimiter {
    pub fn new(max_requests: u64, window_size: Duration) -> Self {
        Self {
            max_requests,
            window_size,
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn cleanup_old_requests(&self, entry: &mut SlidingWindowEntry) {
        let now = Instant::now();
        entry
            .requests
            .retain(|&time| now.duration_since(time) < self.window_size);
    }
}

impl RateLimiter for SlidingWindowRateLimiter {
    fn acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;

        let entry = entries
            .entry(key.to_string())
            .or_insert_with(|| SlidingWindowEntry {
                requests: Vec::new(),
            });

        self.cleanup_old_requests(entry);

        if entry.requests.len() < self.max_requests as usize {
            entry.requests.push(Instant::now());
            let remaining = self.max_requests - entry.requests.len() as u64;
            let reset_at = now_timestamp() + self.window_size.as_millis() as i64;
            Ok(RateLimitResult::allowed(remaining, reset_at))
        } else {
            let oldest = entry
                .requests
                .first()
                .map(|t| {
                    let elapsed = t.elapsed().as_millis() as i64;
                    let window_ms = self.window_size.as_millis() as i64;
                    now_timestamp() + (window_ms - elapsed)
                })
                .unwrap_or(now_timestamp());

            let remaining = 0;
            Ok(RateLimitResult::rejected(remaining, oldest))
        }
    }

    fn try_acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        self.acquire(key)
    }

    fn reset(&self, key: &str) -> Result<(), RateLimitError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;
        entries.remove(key);
        Ok(())
    }
}

pub struct TokenBucketRateLimiter {
    capacity: f64,
    refill_rate: f64,
    entries: Arc<RwLock<HashMap<String, TokenBucketEntry>>>,
}

#[derive(Clone)]
struct TokenBucketEntry {
    tokens: f64,
    last_refill: Instant,
}

impl TokenBucketRateLimiter {
    pub fn new(capacity: u64, refill_per_second: f64) -> Self {
        Self {
            capacity: capacity as f64,
            refill_rate: refill_per_second,
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn refill(&self, entry: &mut TokenBucketEntry) {
        let now = Instant::now();
        let elapsed = now.duration_since(entry.last_refill).as_secs_f64();
        // 修复：refill_rate <= 0.0 时不补充令牌（也不消耗）
        // 避免负数 refill_rate 导致 tokens 递减
        let tokens_to_add = if self.refill_rate > 0.0 {
            elapsed * self.refill_rate
        } else {
            0.0
        };

        entry.tokens = (entry.tokens + tokens_to_add).min(self.capacity);
        entry.last_refill = now;
    }
}

impl RateLimiter for TokenBucketRateLimiter {
    fn acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;

        let entry = entries
            .entry(key.to_string())
            .or_insert_with(|| TokenBucketEntry {
                tokens: self.capacity,
                last_refill: Instant::now(),
            });

        self.refill(entry);

        if entry.tokens >= 1.0 {
            entry.tokens -= 1.0;
            let remaining = entry.tokens.floor() as u64;
            // 修复：refill_rate <= 0.0 时令牌不补充，reset_at 设为远期时间
            // 避免除零产生 inf，inf as i64 触发 panic
            let reset_at = if self.refill_rate > 0.0 {
                now_timestamp() + (1000.0 / self.refill_rate) as i64
            } else {
                // 令牌永不补充，reset_at 设为远期时间（约 292 年后的 i64::MAX）
                i64::MAX
            };
            Ok(RateLimitResult::allowed(remaining, reset_at))
        } else {
            // 修复：refill_rate <= 0.0 时令牌不补充，永远等待
            let reset_at = if self.refill_rate > 0.0 {
                let wait_time = ((1.0 - entry.tokens) / self.refill_rate * 1000.0) as i64;
                now_timestamp() + wait_time
            } else {
                // 令牌永不补充，永远等待
                i64::MAX
            };
            Ok(RateLimitResult::rejected(0, reset_at))
        }
    }

    fn try_acquire(&self, key: &str) -> Result<RateLimitResult, RateLimitError> {
        self.acquire(key)
    }

    fn reset(&self, key: &str) -> Result<(), RateLimitError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|e| RateLimitError::Internal(e.to_string()))?;
        entries.remove(key);
        Ok(())
    }
}

#[derive(Debug)]
pub enum RateLimitError {
    KeyNotFound(String),
    Internal(String),
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RateLimitError::KeyNotFound(key) => write!(f, "Key not found: {}", key),
            RateLimitError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for RateLimitError {}

impl serde::Serialize for RateLimitError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

fn now_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_result_allowed() {
        let result = RateLimitResult::allowed(5, 1000);
        assert!(result.allowed);
        assert_eq!(result.remaining, 5);
        assert_eq!(result.reset_at, 1000);
    }

    #[test]
    fn test_rate_limit_result_rejected() {
        let result = RateLimitResult::rejected(0, 2000);
        assert!(!result.allowed);
        assert_eq!(result.remaining, 0);
    }

    #[test]
    fn test_sliding_window_limiter_new() {
        let limiter = SlidingWindowRateLimiter::new(10, Duration::from_secs(60));
        let result = limiter.acquire("test-key");
        assert!(result.is_ok());
        assert!(result.unwrap().allowed);
    }

    #[test]
    fn test_sliding_window_limiter_full() {
        let limiter = SlidingWindowRateLimiter::new(2, Duration::from_secs(60));

        let r1 = limiter.acquire("key1").unwrap();
        assert!(r1.allowed);

        let r2 = limiter.acquire("key1").unwrap();
        assert!(r2.allowed);

        let r3 = limiter.acquire("key1").unwrap();
        assert!(!r3.allowed);
    }

    #[test]
    fn test_sliding_window_different_keys() {
        let limiter = SlidingWindowRateLimiter::new(1, Duration::from_secs(60));

        let r1 = limiter.acquire("key-a").unwrap();
        assert!(r1.allowed);

        let r2 = limiter.acquire("key-b").unwrap();
        assert!(r2.allowed);
    }

    #[test]
    fn test_sliding_window_reset() {
        let limiter = SlidingWindowRateLimiter::new(1, Duration::from_secs(60));

        limiter.acquire("reset-key").unwrap();
        limiter.acquire("reset-key").unwrap();

        limiter.reset("reset-key").unwrap();

        let result = limiter.acquire("reset-key").unwrap();
        assert!(result.allowed);
    }

    #[test]
    fn test_token_bucket_limiter_new() {
        let limiter = TokenBucketRateLimiter::new(10, 1.0);
        let result = limiter.acquire("test-key");
        assert!(result.is_ok());
        assert!(result.unwrap().allowed);
    }

    #[test]
    fn test_token_bucket_limiter_depletes() {
        let limiter = TokenBucketRateLimiter::new(2, 1.0);

        let r1 = limiter.acquire("key1").unwrap();
        assert!(r1.allowed);
        assert_eq!(r1.remaining, 1);

        let r2 = limiter.acquire("key1").unwrap();
        assert!(r2.allowed);
        assert_eq!(r2.remaining, 0);

        let r3 = limiter.acquire("key1").unwrap();
        assert!(!r3.allowed);
    }

    #[test]
    fn test_token_bucket_different_keys() {
        let limiter = TokenBucketRateLimiter::new(1, 1.0);

        let r1 = limiter.acquire("key-a").unwrap();
        assert!(r1.allowed);

        let r2 = limiter.acquire("key-b").unwrap();
        assert!(r2.allowed);
    }

    #[test]
    fn test_token_bucket_reset() {
        let limiter = TokenBucketRateLimiter::new(1, 1.0);

        limiter.acquire("reset-key").unwrap();
        limiter.acquire("reset-key").unwrap();

        limiter.reset("reset-key").unwrap();

        let result = limiter.acquire("reset-key").unwrap();
        assert!(result.allowed);
    }

    #[test]
    fn test_limiter_try_acquire() {
        let limiter = SlidingWindowRateLimiter::new(1, Duration::from_secs(60));

        let r1 = limiter.try_acquire("key").unwrap();
        assert!(r1.allowed);

        let r2 = limiter.try_acquire("key").unwrap();
        assert!(!r2.allowed);
    }

    // ===== TDD RED：refill_rate=0 panic 修复测试 =====

    #[test]
    fn test_token_bucket_zero_refill_rate_does_not_panic() {
        // refill_rate=0.0 表示令牌永不补充
        // capacity=1，第一次 acquire 应允许，第二次应拒绝且不 panic
        let limiter = TokenBucketRateLimiter::new(1, 0.0);

        let r1 = limiter.acquire("zero-refill").unwrap();
        assert!(r1.allowed, "first acquire should be allowed");

        // 第二次 acquire：令牌耗尽且不补充，应拒绝而非 panic
        let r2 = limiter.acquire("zero-refill").unwrap();
        assert!(!r2.allowed, "second acquire should be rejected");
        // reset_at 应为一个合理的远期时间（令牌不补充，永远等待）
        assert!(
            r2.reset_at > 0,
            "reset_at should be a valid timestamp, got: {}",
            r2.reset_at
        );
    }

    #[test]
    fn test_token_bucket_negative_refill_rate_does_not_panic() {
        // refill_rate=-1.0 是错误配置，应被当作 0.0 处理而非 panic
        let limiter = TokenBucketRateLimiter::new(1, -1.0);

        let r1 = limiter.acquire("neg-refill").unwrap();
        assert!(r1.allowed, "first acquire should be allowed");

        let r2 = limiter.acquire("neg-refill").unwrap();
        assert!(!r2.allowed, "second acquire should be rejected");
        assert!(
            r2.reset_at > 0,
            "reset_at should be a valid timestamp, got: {}",
            r2.reset_at
        );
    }
}
