//! 分布式锁 + 可重入
//!
//! 基于 CoordinationBackend 实现分布式锁，支持可重入语义。

use std::sync::atomic::{AtomicU64, Ordering};

use super::backend::{CoordinationError, SharedBackend};

static TOKEN_COUNTER: AtomicU64 = AtomicU64::new(0);

fn generate_token() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let seq = TOKEN_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{:x}", ts, seq)
}

/// 锁标识
type LockToken = String;

/// 分布式锁
pub struct DistributedLock {
    backend: SharedBackend,
    key: String,
    token: LockToken,
    ttl_secs: u64,
    reentrant_count: parking_lot::Mutex<u32>,
}

impl DistributedLock {
    /// 创建锁实例
    pub fn new(backend: SharedBackend, key: &str, ttl_secs: u64) -> Self {
        Self {
            backend,
            key: key.to_string(),
            token: generate_token(),
            ttl_secs,
            reentrant_count: parking_lot::Mutex::new(0),
        }
    }

    /// 获取锁
    pub async fn acquire(&self) -> Result<LockGuard, CoordinationError> {
        {
            let mut count = self.reentrant_count.lock();
            if *count > 0 {
                *count += 1;
                return Ok(LockGuard {
                    lock_key: self.key.clone(),
                    token: self.token.clone(),
                    reentrant: true,
                });
            }
        }
        let success = self
            .backend
            .set_nx(&self.key, &self.token, self.ttl_secs)
            .await?;
        if !success {
            return Err(CoordinationError::LockHeld(self.key.clone()));
        }
        *self.reentrant_count.lock() = 1;
        Ok(LockGuard {
            lock_key: self.key.clone(),
            token: self.token.clone(),
            reentrant: false,
        })
    }

    /// 尝试获取锁（非阻塞）
    pub async fn try_acquire(&self) -> Result<Option<LockGuard>, CoordinationError> {
        match self.acquire().await {
            Ok(guard) => Ok(Some(guard)),
            Err(CoordinationError::LockHeld(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// 释放锁
    pub async fn release(&self) -> Result<bool, CoordinationError> {
        let should_del = {
            let mut count = self.reentrant_count.lock();
            if *count == 0 {
                return Ok(false);
            }
            *count -= 1;
            *count > 0
        };
        if should_del {
            return Ok(true);
        }
        self.backend.del_if_match(&self.key, &self.token).await
    }

    /// 续约锁
    pub async fn renew(&self) -> Result<bool, CoordinationError> {
        let is_held = *self.reentrant_count.lock() > 0;
        if !is_held {
            return Ok(false);
        }
        let current = self.backend.get(&self.key).await?;
        match current {
            Some(val) if val == self.token => {
                self.backend
                    .set(&self.key, &self.token, self.ttl_secs)
                    .await?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// 锁键名
    pub fn key(&self) -> &str {
        &self.key
    }

    /// 是否持有锁
    pub fn is_held(&self) -> bool {
        *self.reentrant_count.lock() > 0
    }
}

/// 锁守卫
#[derive(Debug, Clone)]
pub struct LockGuard {
    /// 锁键
    pub lock_key: String,
    /// 锁令牌
    pub token: LockToken,
    /// 是否可重入
    pub reentrant: bool,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::super::backend::InMemoryBackend;
    use super::*;

    fn make_backend() -> SharedBackend {
        Arc::new(InMemoryBackend::new())
    }

    #[tokio::test]
    async fn test_acquire_and_release() {
        let backend = make_backend();
        let lock = DistributedLock::new(backend, "test_lock", 10);
        let _guard = lock.acquire().await.unwrap();
        assert!(lock.is_held());
        assert!(lock.release().await.unwrap());
        assert!(!lock.is_held());
    }

    #[tokio::test]
    async fn test_lock_contention() {
        let backend = make_backend();
        let lock1 = DistributedLock::new(backend.clone(), "contended", 10);
        let lock2 = DistributedLock::new(backend, "contended", 10);
        lock1.acquire().await.unwrap();
        let result = lock2.acquire().await;
        assert!(matches!(result, Err(CoordinationError::LockHeld(_))));
    }

    #[tokio::test]
    async fn test_try_acquire() {
        let backend = make_backend();
        let lock1 = DistributedLock::new(backend.clone(), "try_lock", 10);
        let lock2 = DistributedLock::new(backend, "try_lock", 10);
        lock1.acquire().await.unwrap();
        let result = lock2.try_acquire().await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_reentrant_same_instance() {
        let backend = make_backend();
        let lock = DistributedLock::new(backend, "reentrant", 10);
        let _g1 = lock.acquire().await.unwrap();
        let g2 = lock.acquire().await.unwrap();
        assert!(g2.reentrant);
        assert!(lock.release().await.unwrap());
        assert!(lock.is_held());
        assert!(lock.release().await.unwrap());
        assert!(!lock.is_held());
    }

    #[tokio::test]
    async fn test_release_unheld_lock() {
        let backend = make_backend();
        let lock = DistributedLock::new(backend, "unheld", 10);
        assert!(!lock.release().await.unwrap());
    }

    #[tokio::test]
    async fn test_renew_active_lock() {
        let backend = make_backend();
        let lock = DistributedLock::new(backend, "renew_test", 10);
        lock.acquire().await.unwrap();
        assert!(lock.renew().await.unwrap());
        lock.release().await.unwrap();
    }

    #[tokio::test]
    async fn test_renew_expired_lock() {
        let backend = make_backend();
        let lock = DistributedLock::new(backend, "expired", 1);
        lock.acquire().await.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(!lock.renew().await.unwrap());
    }

    #[tokio::test]
    async fn test_lock_key() {
        let backend = make_backend();
        let lock = DistributedLock::new(backend, "my_key", 10);
        assert_eq!(lock.key(), "my_key");
    }

    #[tokio::test]
    async fn test_multiple_locks_different_keys() {
        let backend = make_backend();
        let lock1 = DistributedLock::new(backend.clone(), "key1", 10);
        let lock2 = DistributedLock::new(backend, "key2", 10);
        lock1.acquire().await.unwrap();
        lock2.acquire().await.unwrap();
        assert!(lock1.is_held());
        assert!(lock2.is_held());
        lock1.release().await.unwrap();
        lock2.release().await.unwrap();
    }

    #[tokio::test]
    async fn test_release_after_contention() {
        let backend = make_backend();
        let lock1 = DistributedLock::new(backend.clone(), "serial", 10);
        let lock2 = DistributedLock::new(backend, "serial", 10);
        lock1.acquire().await.unwrap();
        lock1.release().await.unwrap();
        lock2.acquire().await.unwrap();
        lock2.release().await.unwrap();
    }

    #[tokio::test]
    async fn test_guard_contains_token() {
        let backend = make_backend();
        let lock = DistributedLock::new(backend, "guard_test", 10);
        let guard = lock.acquire().await.unwrap();
        assert!(!guard.token.is_empty());
        assert_eq!(guard.lock_key, "guard_test");
    }

    #[tokio::test]
    async fn test_reentrant_count_tracking() {
        let backend = make_backend();
        let lock = DistributedLock::new(backend, "count_test", 10);
        lock.acquire().await.unwrap();
        lock.acquire().await.unwrap();
        lock.acquire().await.unwrap();
        assert!(lock.is_held());
        lock.release().await.unwrap();
        lock.release().await.unwrap();
        assert!(lock.is_held());
        lock.release().await.unwrap();
        assert!(!lock.is_held());
    }

    #[tokio::test]
    async fn test_try_acquire_available() {
        let backend = make_backend();
        let lock = DistributedLock::new(backend, "available", 10);
        let result = lock.try_acquire().await.unwrap();
        assert!(result.is_some());
    }
}
