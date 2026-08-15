//! TTL 融合缓存（`db-fusion-v2` feature）
//!
//! [`TtlFusionCache`] 实现 [`crate::executor::FusionCache`] trait，
//! 每个缓存条目关联一个 TTL（Time-To-Live），过期自动失效。
//!
//! 复用既有 `FusionCache` trait `packages/sz-orm-fusion/src/executor.rs:8`，
//! 模式参考既有 `MemoryFusionCache` `packages/sz-orm-fusion/src/executor.rs:16`。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::executor::FusionCache;

/// TTL 融合缓存（转正 API）
///
/// 每个缓存条目存储 `(value, deadline)`，`get_with_ttl` 时检查 `Instant::now` vs `deadline`，
/// 过期返回 `None` 并清理条目。`set_with_ttl` 存储 `(value, Instant::now + ttl)`。
///
/// ```rust,ignore
/// use std::time::Duration;
/// use sz_orm_fusion::TtlFusionCache;
/// let cache = TtlFusionCache::new(Duration::from_secs(60));
/// cache.set_with_ttl("key", "value".into(), Duration::from_secs(30));
/// assert_eq!(cache.get_with_ttl("key"), Some("value".into()));
/// ```
pub struct TtlFusionCache {
    inner: Mutex<HashMap<String, (String, Instant)>>,
    default_ttl: Duration,
}

impl TtlFusionCache {
    /// 创建 TTL 缓存，指定默认 TTL
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            default_ttl,
        }
    }

    /// 默认 TTL
    pub fn default_ttl(&self) -> Duration {
        self.default_ttl
    }
}

impl Default for TtlFusionCache {
    fn default() -> Self {
        Self::new(Duration::from_secs(60))
    }
}

impl FusionCache for TtlFusionCache {
    fn get(&self, key: &str) -> Option<String> {
        self.get_with_ttl(key)
    }

    fn set(&self, key: &str, value: String) {
        self.set_with_ttl(key, value, self.default_ttl);
    }

    fn get_with_ttl(&self, key: &str) -> Option<String> {
        let mut inner = self.inner.lock().ok()?;
        let (value, deadline) = inner.get(key)?;
        if Instant::now() > *deadline {
            inner.remove(key);
            return None;
        }
        Some(value.clone())
    }

    fn set_with_ttl(&self, key: &str, value: String, ttl: Duration) {
        if let Ok(mut inner) = self.inner.lock() {
            let deadline = Instant::now() + ttl;
            inner.insert(key.to_string(), (value, deadline));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn set_and_get_within_ttl() {
        let cache = TtlFusionCache::new(Duration::from_secs(60));
        cache.set_with_ttl("key1", "value1".into(), Duration::from_secs(60));
        assert_eq!(cache.get_with_ttl("key1"), Some("value1".into()));
    }

    #[test]
    fn expired_entry_returns_none() {
        let cache = TtlFusionCache::new(Duration::from_secs(60));
        cache.set_with_ttl("key1", "value1".into(), Duration::from_millis(0));
        std::thread::sleep(Duration::from_millis(1));
        assert_eq!(cache.get_with_ttl("key1"), None);
    }

    #[test]
    fn implements_fusion_cache_trait() {
        let cache = TtlFusionCache::new(Duration::from_secs(60));
        cache.set("key1", "value1".into());
        assert_eq!(cache.get("key1"), Some("value1".into()));
    }

    #[test]
    fn default_ttl_used_by_set() {
        let cache = TtlFusionCache::new(Duration::from_secs(60));
        cache.set("key1", "value1".into());
        assert_eq!(cache.get_with_ttl("key1"), Some("value1".into()));
    }

    #[test]
    fn ttl_zero_immediately_expires() {
        let cache = TtlFusionCache::new(Duration::from_secs(60));
        cache.set_with_ttl("key1", "value1".into(), Duration::from_secs(0));
        std::thread::sleep(Duration::from_millis(1));
        assert_eq!(cache.get_with_ttl("key1"), None);
    }

    #[test]
    fn missing_key_returns_none() {
        let cache = TtlFusionCache::new(Duration::from_secs(60));
        assert_eq!(cache.get_with_ttl("nonexistent"), None);
    }

    #[test]
    fn overwrite_updates_value_and_ttl() {
        let cache = TtlFusionCache::new(Duration::from_secs(60));
        cache.set_with_ttl("key1", "old".into(), Duration::from_secs(60));
        cache.set_with_ttl("key1", "new".into(), Duration::from_secs(60));
        assert_eq!(cache.get_with_ttl("key1"), Some("new".into()));
    }

    #[test]
    fn expired_entry_is_cleaned_up() {
        let cache = TtlFusionCache::new(Duration::from_secs(60));
        cache.set_with_ttl("key1", "value1".into(), Duration::from_millis(0));
        std::thread::sleep(Duration::from_millis(1));
        let _ = cache.get_with_ttl("key1");
        let inner = cache.inner.lock().unwrap_or_else(|e| e.into_inner());
        assert!(!inner.contains_key("key1"));
    }

    #[test]
    fn default_impl_uses_60s_ttl() {
        let cache = TtlFusionCache::default();
        assert_eq!(cache.default_ttl(), Duration::from_secs(60));
    }
}
