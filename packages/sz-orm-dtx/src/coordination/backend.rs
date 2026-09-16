//! 协调后端 trait + Redis/内存实现
//!
//! 抽象 KV 存储后端，支持 Redis 和内存两种实现。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;
use thiserror::Error;

/// 协调后端错误
#[derive(Debug, Error)]
pub enum CoordinationError {
    /// 锁已被持有
    #[error("锁 '{0}' 已被持有")]
    LockHeld(String),
    /// 锁不存在
    #[error("锁 '{0}' 不存在")]
    LockNotFound(String),
    /// 锁已过期
    #[error("锁 '{0}' 已过期")]
    LockExpired(String),
    /// 后端连接错误
    #[error("后端连接错误: {0}")]
    Connection(String),
    /// 序列化错误
    #[error("序列化错误: {0}")]
    Serialize(String),
    /// 操作冲突
    #[error("操作冲突: {0}")]
    Conflict(String),
}

/// 协调后端 trait
#[async_trait]
pub trait CoordinationBackend: Send + Sync {
    /// 设置键值（仅当键不存在时），返回是否成功
    async fn set_nx(
        &self,
        key: &str,
        value: &str,
        ttl_secs: u64,
    ) -> Result<bool, CoordinationError>;

    /// 获取键值
    async fn get(&self, key: &str) -> Result<Option<String>, CoordinationError>;

    /// 删除键值（仅当值匹配时），返回是否成功
    async fn del_if_match(&self, key: &str, expected: &str) -> Result<bool, CoordinationError>;

    /// 删除键值（无条件）
    async fn del(&self, key: &str) -> Result<bool, CoordinationError>;

    /// 原子递增并返回新值
    async fn incr(&self, key: &str) -> Result<i64, CoordinationError>;

    /// 设置键值（无条件）
    async fn set(&self, key: &str, value: &str, ttl_secs: u64) -> Result<(), CoordinationError>;
}

/// 内存后端（测试用）
pub struct InMemoryBackend {
    store: Mutex<HashMap<String, (String, Option<std::time::Instant>)>>,
}

impl InMemoryBackend {
    /// 创建内存后端
    pub fn new() -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CoordinationBackend for InMemoryBackend {
    async fn set_nx(
        &self,
        key: &str,
        value: &str,
        ttl_secs: u64,
    ) -> Result<bool, CoordinationError> {
        let mut store = self.store.lock();
        if let Some((_, Some(expire))) = store.get(key) {
            if *expire > std::time::Instant::now() {
                return Ok(false);
            }
        }
        let expire = if ttl_secs > 0 {
            Some(std::time::Instant::now() + std::time::Duration::from_secs(ttl_secs))
        } else {
            None
        };
        store.insert(key.to_string(), (value.to_string(), expire));
        Ok(true)
    }

    async fn get(&self, key: &str) -> Result<Option<String>, CoordinationError> {
        let store = self.store.lock();
        match store.get(key) {
            Some((value, Some(expire))) if *expire > std::time::Instant::now() => {
                Ok(Some(value.clone()))
            }
            Some((value, None)) => Ok(Some(value.clone())),
            _ => Ok(None),
        }
    }

    async fn del_if_match(&self, key: &str, expected: &str) -> Result<bool, CoordinationError> {
        let mut store = self.store.lock();
        match store.get(key) {
            Some((value, _)) if value == expected => {
                store.remove(key);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn del(&self, key: &str) -> Result<bool, CoordinationError> {
        let mut store = self.store.lock();
        Ok(store.remove(key).is_some())
    }

    async fn incr(&self, key: &str) -> Result<i64, CoordinationError> {
        let mut store = self.store.lock();
        let entry = store
            .entry(key.to_string())
            .or_insert(("0".to_string(), None));
        let n: i64 = entry.0.parse().unwrap_or(0) + 1;
        entry.0 = n.to_string();
        Ok(n)
    }

    async fn set(&self, key: &str, value: &str, ttl_secs: u64) -> Result<(), CoordinationError> {
        let mut store = self.store.lock();
        let expire = if ttl_secs > 0 {
            Some(std::time::Instant::now() + std::time::Duration::from_secs(ttl_secs))
        } else {
            None
        };
        store.insert(key.to_string(), (value.to_string(), expire));
        Ok(())
    }
}

/// Redis 后端
pub struct RedisBackend {
    client: redis::Client,
}

impl RedisBackend {
    /// 创建 Redis 后端
    pub fn new(url: &str) -> Result<Self, CoordinationError> {
        let client =
            redis::Client::open(url).map_err(|e| CoordinationError::Connection(e.to_string()))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl CoordinationBackend for RedisBackend {
    async fn set_nx(
        &self,
        key: &str,
        value: &str,
        ttl_secs: u64,
    ) -> Result<bool, CoordinationError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| CoordinationError::Connection(e.to_string()))?;
        let result: bool = redis::cmd("SET")
            .arg(key)
            .arg(value)
            .arg("NX")
            .arg("EX")
            .arg(ttl_secs)
            .query_async(&mut conn)
            .await
            .map_err(|e| CoordinationError::Connection(e.to_string()))?;
        Ok(result)
    }

    async fn get(&self, key: &str) -> Result<Option<String>, CoordinationError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| CoordinationError::Connection(e.to_string()))?;
        let result: Option<String> = redis::cmd("GET")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(|e| CoordinationError::Connection(e.to_string()))?;
        Ok(result)
    }

    async fn del_if_match(&self, key: &str, expected: &str) -> Result<bool, CoordinationError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| CoordinationError::Connection(e.to_string()))?;
        let script = r#"
            if redis.call("GET", KEYS[1]) == ARGV[1] then
                return redis.call("DEL", KEYS[1])
            else
                return 0
            end
        "#;
        let result: i64 = redis::cmd("EVAL")
            .arg(script)
            .arg(1)
            .arg(key)
            .arg(expected)
            .query_async(&mut conn)
            .await
            .map_err(|e| CoordinationError::Connection(e.to_string()))?;
        Ok(result == 1)
    }

    async fn del(&self, key: &str) -> Result<bool, CoordinationError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| CoordinationError::Connection(e.to_string()))?;
        let result: i64 = redis::cmd("DEL")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(|e| CoordinationError::Connection(e.to_string()))?;
        Ok(result == 1)
    }

    async fn incr(&self, key: &str) -> Result<i64, CoordinationError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| CoordinationError::Connection(e.to_string()))?;
        let n: i64 = redis::cmd("INCR")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(|e| CoordinationError::Connection(e.to_string()))?;
        Ok(n)
    }

    async fn set(&self, key: &str, value: &str, ttl_secs: u64) -> Result<(), CoordinationError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| CoordinationError::Connection(e.to_string()))?;
        redis::cmd("SET")
            .arg(key)
            .arg(value)
            .arg("EX")
            .arg(ttl_secs)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| CoordinationError::Connection(e.to_string()))?;
        Ok(())
    }
}

/// 共享后端
pub type SharedBackend = Arc<dyn CoordinationBackend>;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_set_nx_success() {
        let backend = InMemoryBackend::new();
        assert!(backend.set_nx("key1", "val1", 10).await.unwrap());
        assert!(!backend.set_nx("key1", "val2", 10).await.unwrap());
    }

    #[tokio::test]
    async fn test_get_set() {
        let backend = InMemoryBackend::new();
        backend.set("key", "value", 0).await.unwrap();
        assert_eq!(backend.get("key").await.unwrap(), Some("value".to_string()));
    }

    #[tokio::test]
    async fn test_del_if_match() {
        let backend = InMemoryBackend::new();
        backend.set("key", "value", 0).await.unwrap();
        assert!(!backend.del_if_match("key", "wrong").await.unwrap());
        assert!(backend.del_if_match("key", "value").await.unwrap());
        assert_eq!(backend.get("key").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_del_unconditional() {
        let backend = InMemoryBackend::new();
        backend.set("key", "value", 0).await.unwrap();
        assert!(backend.del("key").await.unwrap());
        assert!(!backend.del("key").await.unwrap());
    }

    #[tokio::test]
    async fn test_incr() {
        let backend = InMemoryBackend::new();
        assert_eq!(backend.incr("counter").await.unwrap(), 1);
        assert_eq!(backend.incr("counter").await.unwrap(), 2);
        assert_eq!(backend.incr("counter").await.unwrap(), 3);
    }

    #[tokio::test]
    async fn test_ttl_expiration() {
        let backend = InMemoryBackend::new();
        backend.set_nx("key", "value", 1).await.unwrap();
        assert_eq!(backend.get("key").await.unwrap(), Some("value".to_string()));
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        assert_eq!(backend.get("key").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_set_nx_after_expiration() {
        let backend = InMemoryBackend::new();
        backend.set_nx("key", "val1", 1).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        assert!(backend.set_nx("key", "val2", 10).await.unwrap());
    }

    #[tokio::test]
    async fn test_overwrite_with_set() {
        let backend = InMemoryBackend::new();
        backend.set_nx("key", "val1", 10).await.unwrap();
        backend.set("key", "val2", 10).await.unwrap();
        assert_eq!(backend.get("key").await.unwrap(), Some("val2".to_string()));
    }
}
