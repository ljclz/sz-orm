//! Fencing Token 生成器
//!
//! 单调递增的 fencing token，用于防止过期锁持有者的操作干扰。

use std::sync::atomic::{AtomicI64, Ordering};

use super::backend::{CoordinationError, SharedBackend};

/// Fencing Token 生成器
pub struct FencingTokenGenerator {
    backend: SharedBackend,
    counter_key: String,
    local_cache: AtomicI64,
}

impl FencingTokenGenerator {
    /// 创建生成器
    pub fn new(backend: SharedBackend, name: &str) -> Self {
        Self {
            backend,
            counter_key: format!("fencing:{}", name),
            local_cache: AtomicI64::new(0),
        }
    }

    /// 获取下一个 fencing token（单调递增）
    pub async fn next_token(&self) -> Result<i64, CoordinationError> {
        let token = self.backend.incr(&self.counter_key).await?;
        self.local_cache.fetch_max(token, Ordering::Relaxed);
        Ok(token)
    }

    /// 获取当前 token（不递增）
    pub async fn current_token(&self) -> Result<i64, CoordinationError> {
        match self.backend.get(&self.counter_key).await? {
            Some(val) => Ok(val.parse().unwrap_or(0)),
            None => Ok(0),
        }
    }

    /// 本地缓存的 token
    pub fn cached_token(&self) -> i64 {
        self.local_cache.load(Ordering::Relaxed)
    }

    /// 验证 token 是否单调递增
    pub fn is_monotonic(&self, new_token: i64) -> bool {
        new_token > self.local_cache.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::backend::InMemoryBackend;
    use super::*;

    fn make_generator() -> FencingTokenGenerator {
        FencingTokenGenerator::new(Arc::new(InMemoryBackend::new()), "test")
    }

    #[tokio::test]
    async fn test_next_token_monotonic() {
        let gen = make_generator();
        let t1 = gen.next_token().await.unwrap();
        let t2 = gen.next_token().await.unwrap();
        let t3 = gen.next_token().await.unwrap();
        assert!(t1 < t2);
        assert!(t2 < t3);
    }

    #[tokio::test]
    async fn test_current_token() {
        let gen = make_generator();
        assert_eq!(gen.current_token().await.unwrap(), 0);
        gen.next_token().await.unwrap();
        gen.next_token().await.unwrap();
        assert_eq!(gen.current_token().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_cached_token() {
        let gen = make_generator();
        gen.next_token().await.unwrap();
        gen.next_token().await.unwrap();
        assert_eq!(gen.cached_token(), 2);
    }

    #[tokio::test]
    async fn test_is_monotonic() {
        let gen = make_generator();
        let t1 = gen.next_token().await.unwrap();
        assert!(!gen.is_monotonic(t1));
        assert!(gen.is_monotonic(t1 + 1));
    }

    #[tokio::test]
    async fn test_multiple_generators_share_counter() {
        let backend: SharedBackend = Arc::new(InMemoryBackend::new());
        let gen1 = FencingTokenGenerator::new(backend.clone(), "shared");
        let gen2 = FencingTokenGenerator::new(backend, "shared");
        let t1 = gen1.next_token().await.unwrap();
        let t2 = gen2.next_token().await.unwrap();
        assert!(t1 < t2);
    }
}
