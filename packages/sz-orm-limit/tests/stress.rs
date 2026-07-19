//! sz-orm-limit 压力测试套件
//!
//! 超大数据量验证：
//! - 1 万个 key 限流
//! - 8 线程并发 acquire（spawn_blocking 模拟同步锁竞争）
//! - 滑动窗口与令牌桶在大并发下的一致性

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use sz_orm_limit::{RateLimiter, SlidingWindowRateLimiter, TokenBucketRateLimiter};

/// 验证：滑动窗口 1 万个 key 限流一致性
#[test]
fn stress_limit_sliding_window_10k_keys() {
    let limiter = SlidingWindowRateLimiter::new(10, Duration::from_secs(60));
    let n: usize = 10_000;

    for i in 0..n {
        let key = format!("user-{}", i);
        // 前 10 次成功
        for _ in 0..10 {
            let result = limiter.acquire(&key).unwrap();
            assert!(result.allowed, "key {} should be allowed", key);
        }
        // 第 11 次拒绝
        let result = limiter.acquire(&key).unwrap();
        assert!(!result.allowed, "key {} should be rejected", key);
    }
}

/// 验证：令牌桶 1 万个 key 限流一致性
#[test]
fn stress_limit_token_bucket_10k_keys() {
    let limiter = TokenBucketRateLimiter::new(5, 1.0);
    let n: usize = 10_000;

    for i in 0..n {
        let key = format!("user-{}", i);
        // 前 5 次成功（容量 5）
        for _ in 0..5 {
            let result = limiter.acquire(&key).unwrap();
            assert!(result.allowed, "key {} should be allowed", key);
        }
        // 第 6 次拒绝
        let result = limiter.acquire(&key).unwrap();
        assert!(!result.allowed, "key {} should be rejected", key);
    }
}

/// 验证：滑动窗口 8 线程并发 acquire 同一 key
#[test]
fn stress_limit_sliding_window_concurrent_same_key() {
    let limiter = Arc::new(SlidingWindowRateLimiter::new(100, Duration::from_secs(60)));
    let allowed = Arc::new(AtomicU64::new(0));
    let rejected = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::new();
    for _ in 0..8 {
        let l = limiter.clone();
        let a = allowed.clone();
        let r = rejected.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..1000 {
                let result = l.acquire("shared-key").unwrap();
                if result.allowed {
                    a.fetch_add(1, Ordering::SeqCst);
                } else {
                    r.fetch_add(1, Ordering::SeqCst);
                }
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }

    let total_allowed = allowed.load(Ordering::SeqCst);
    let total_rejected = rejected.load(Ordering::SeqCst);
    assert_eq!(total_allowed + total_rejected, 8000);
    // 最多允许 100 个
    assert!(
        total_allowed <= 100,
        "allowed {} must be <= 100",
        total_allowed
    );
    assert_eq!(total_allowed, 100, "exactly 100 should be allowed");
}

/// 验证：令牌桶 8 线程并发 acquire 同一 key
/// 注意：refill_rate=0.0 会导致源码 panic（1000.0/0.0=inf, inf as i64 未定义），
/// 这里用极小值 0.0001 模拟"几乎不补充"
#[test]
fn stress_limit_token_bucket_concurrent_same_key() {
    let limiter = Arc::new(TokenBucketRateLimiter::new(50, 0.0001));
    let allowed = Arc::new(AtomicU64::new(0));
    let rejected = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::new();
    for _ in 0..8 {
        let l = limiter.clone();
        let a = allowed.clone();
        let r = rejected.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..1000 {
                let result = l.acquire("shared-key").unwrap();
                if result.allowed {
                    a.fetch_add(1, Ordering::SeqCst);
                } else {
                    r.fetch_add(1, Ordering::SeqCst);
                }
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }

    let total_allowed = allowed.load(Ordering::SeqCst);
    let total_rejected = rejected.load(Ordering::SeqCst);
    assert_eq!(total_allowed + total_rejected, 8000);
    // 容量 50，几乎不补充，允许数应接近 50
    assert!(
        total_allowed <= 100,
        "allowed {} should be small",
        total_allowed
    );
}

/// 验证：reset 后可以重新获取
#[test]
fn stress_limit_reset_allows_acquire() {
    let limiter = SlidingWindowRateLimiter::new(5, Duration::from_secs(60));

    for _ in 0..5 {
        let result = limiter.acquire("key").unwrap();
        assert!(result.allowed);
    }
    let result = limiter.acquire("key").unwrap();
    assert!(!result.allowed);

    limiter.reset("key").unwrap();
    for _ in 0..5 {
        let result = limiter.acquire("key").unwrap();
        assert!(result.allowed, "should be allowed after reset");
    }
}

/// 验证：try_acquire 与 acquire 行为一致
#[test]
fn stress_limit_try_acquire_matches_acquire() {
    let limiter = SlidingWindowRateLimiter::new(10, Duration::from_secs(60));

    for _ in 0..10 {
        let r1 = limiter.acquire("key").unwrap();
        assert!(r1.allowed);
    }
    // 满了
    let r2 = limiter.try_acquire("key").unwrap();
    assert!(!r2.allowed);
}

/// 验证：滑动窗口在窗口过期后允许新请求
#[test]
fn stress_limit_sliding_window_expiry() {
    let limiter = SlidingWindowRateLimiter::new(3, Duration::from_millis(100));

    for _ in 0..3 {
        let result = limiter.acquire("key").unwrap();
        assert!(result.allowed);
    }
    let result = limiter.acquire("key").unwrap();
    assert!(!result.allowed);

    // 等待窗口过期
    std::thread::sleep(Duration::from_millis(150));

    let result = limiter.acquire("key").unwrap();
    assert!(result.allowed, "should be allowed after window expires");
}

/// 验证：令牌桶在补充后允许新请求
#[test]
fn stress_limit_token_bucket_refill() {
    let limiter = TokenBucketRateLimiter::new(2, 100.0); // 每秒 100 个令牌

    for _ in 0..2 {
        let result = limiter.acquire("key").unwrap();
        assert!(result.allowed);
    }
    let result = limiter.acquire("key").unwrap();
    assert!(!result.allowed, "should be rejected when bucket empty");

    // 等待令牌补充
    std::thread::sleep(Duration::from_millis(50));

    let result = limiter.acquire("key").unwrap();
    assert!(result.allowed, "should be allowed after refill");
}

/// 验证：不同 key 互不影响
#[test]
fn stress_limit_different_keys_independent() {
    let limiter = SlidingWindowRateLimiter::new(5, Duration::from_secs(60));

    for i in 0..1000 {
        let key = format!("key-{}", i);
        for _ in 0..5 {
            let result = limiter.acquire(&key).unwrap();
            assert!(result.allowed, "key {} should be allowed", key);
        }
        // 同一 key 第 6 次拒绝
        let result = limiter.acquire(&key).unwrap();
        assert!(!result.allowed);
    }
}
