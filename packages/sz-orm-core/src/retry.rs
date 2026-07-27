//! 通用错误重试器
//!
//! 支持指数退避 + 抖动的可配置重试策略

use std::future::Future;
use std::time::Duration;

use crate::error::DbError;

/// 重试策略
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// 最大重试次数
    pub max_retries: u32,
    /// 初始延迟
    pub initial_delay: Duration,
    /// 最大延迟
    pub max_delay: Duration,
    /// 退避因子
    pub backoff_factor: f64,
    /// 是否添加抖动
    pub jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_secs(1),
            backoff_factor: 2.0,
            jitter: true,
        }
    }
}

impl RetryPolicy {
    /// 计算第 n 次重试的延迟
    pub fn delay(&self, retry: u32) -> Duration {
        let base = self.initial_delay.as_millis() as f64;
        let delay_ms = base * self.backoff_factor.powi(retry as i32);
        let delay = Duration::from_millis(delay_ms as u64).min(self.max_delay);
        if self.jitter {
            // 简单抖动：在 50%-150% 范围内随机
            let jitter_factor = 0.5 + rand_simple();
            Duration::from_millis((delay.as_millis() as f64 * jitter_factor) as u64)
        } else {
            delay
        }
    }
}

/// 简单伪随机（不依赖 rand crate）
fn rand_simple() -> f64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEED: AtomicU64 = AtomicU64::new(12345);
    let s = SEED.fetch_add(2654435761, Ordering::Relaxed);
    (s % 1000) as f64 / 1000.0
}

/// 带重试执行异步操作
///
/// 根据 `RetryPolicy` 重试可重试的 `DbError`，使用指数退避 + 抖动策略。
pub async fn retry_with_backoff<F, Fut, T>(
    policy: &RetryPolicy,
    mut operation: F,
) -> Result<T, DbError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, DbError>>,
{
    let mut last_err = None;
    for attempt in 0..=policy.max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if !e.is_retryable() || attempt == policy.max_retries {
                    return Err(e);
                }
                last_err = Some(e);
                tokio::time::sleep(policy.delay(attempt)).await;
            }
        }
    }
    Err(last_err.unwrap_or(DbError::Internal("retry exhausted".into())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_retry_policy_default() {
        let p = RetryPolicy::default();
        assert_eq!(p.max_retries, 3);
        assert_eq!(p.initial_delay, Duration::from_millis(10));
        assert_eq!(p.max_delay, Duration::from_secs(1));
        assert!((p.backoff_factor - 2.0).abs() < f64::EPSILON);
        assert!(p.jitter);
    }

    #[test]
    fn test_retry_policy_delay_within_bounds() {
        let p = RetryPolicy {
            jitter: false,
            ..Default::default()
        };
        let d0 = p.delay(0);
        let d1 = p.delay(1);
        // 无抖动时：delay(0) = 10ms, delay(1) = 20ms
        assert_eq!(d0, Duration::from_millis(10));
        assert_eq!(d1, Duration::from_millis(20));
    }

    #[test]
    fn test_retry_policy_delay_capped_at_max() {
        let p = RetryPolicy {
            jitter: false,
            max_delay: Duration::from_millis(50),
            ..Default::default()
        };
        // retry=10 → base * 2^10 = 10240ms，应被限制到 50ms
        let d = p.delay(10);
        assert_eq!(d, Duration::from_millis(50));
    }

    #[tokio::test]
    async fn test_retry_succeeds_first_attempt() {
        let policy = RetryPolicy::default();
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let result: Result<u32, DbError> = retry_with_backoff(&policy, || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(42u32)
            }
        })
        .await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_retries_on_retryable_error() {
        let policy = RetryPolicy {
            max_retries: 3,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(5),
            jitter: false,
            ..Default::default()
        };
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let result: Result<u32, DbError> = retry_with_backoff(&policy, || {
            let c = c.clone();
            async move {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err(DbError::ConnectionError("timeout".to_string()))
                } else {
                    Ok(42u32)
                }
            }
        })
        .await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_does_not_retry_non_retryable_error() {
        let policy = RetryPolicy::default();
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let result: Result<u32, DbError> = retry_with_backoff(&policy, || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err(DbError::QueryError("syntax error".to_string()))
            }
        })
        .await;
        assert!(result.is_err());
        // 非可重试错误应立即返回，不重试
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_exhausted_after_max_retries() {
        let policy = RetryPolicy {
            max_retries: 2,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(5),
            jitter: false,
            ..Default::default()
        };
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let result: Result<u32, DbError> = retry_with_backoff(&policy, || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err(DbError::ConnectionError("timeout".to_string()))
            }
        })
        .await;
        assert!(result.is_err());
        // max_retries=2 → 尝试 3 次（attempt 0,1,2）
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }
}
