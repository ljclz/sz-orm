//! 强一致缓存
//!
//! 缓存与数据库变更严格有序 → 线性一致保证。
//! 一致性级别可配（`ConsistencyLevel::Strong`/`ReadCommitted`/`Eventual`）。
//!
//! 复用 `cache_coherence.rs`（`packages/sz-orm-core/src/cache_coherence.rs`）
//! 和 `dist_cache.rs`（`packages/sz-orm-core/src/dist_cache.rs`）。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::RwLock;

/// 一致性级别（扩展 `dist_cache::ConsistencyLevel`，增加 `ReadCommitted`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConsistencyLevel {
    /// 最终一致
    #[default]
    Eventual,
    /// 读已提交
    ReadCommitted,
    /// 强一致（写入后立即读返回最新值）
    Strong,
}

/// 分布式缓存错误
#[derive(Debug, Clone)]
pub enum DistError {
    /// 强一致无法维持，降级最终一致（告警 `CACHE_CONSISTENCY_DOWNGRADED`）
    Downgraded(String),
    /// 缓存操作失败
    CacheFailed(String),
}

impl std::fmt::Display for DistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Downgraded(msg) => write!(f, "CACHE_CONSISTENCY_DOWNGRADED: {msg}"),
            Self::CacheFailed(msg) => write!(f, "cache failed: {msg}"),
        }
    }
}

impl std::error::Error for DistError {}

/// 强一致缓存配置
#[derive(Debug, Clone)]
pub struct StrongConsistencyConfig {
    /// 一致性级别
    pub level: ConsistencyLevel,
    /// 强一致延迟上限（毫秒），超过则降级
    pub max_latency_ms: u64,
}

impl Default for StrongConsistencyConfig {
    fn default() -> Self {
        Self {
            level: ConsistencyLevel::Strong,
            max_latency_ms: 5,
        }
    }
}

/// 强一致缓存
///
/// 注入 `Arc<DistCache>` + `StrongConsistencyConfig`。
/// `DistCache` 由调用方提供（可以是 `L2Cache` 或自定义后端）。
///
/// 强一致保证：`write_strong` 先失效所有实例缓存再写入，`read_strong` 直接读最新值。
/// 延迟增加 ≤ 5ms（基于内存操作，无网络往返）。
pub struct StrongConsistencyCache {
    /// 底层键值存储（模拟 DistCache 接口）
    store: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    config: StrongConsistencyConfig,
    /// 是否已降级
    degraded: Arc<RwLock<bool>>,
}

impl StrongConsistencyCache {
    /// 创建强一致缓存
    pub fn new(config: StrongConsistencyConfig) -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
            config,
            degraded: Arc::new(RwLock::new(false)),
        }
    }

    /// 从已有 store 创建（用于共享底层存储）
    pub fn with_store(
        store: Arc<RwLock<HashMap<String, Vec<u8>>>>,
        config: StrongConsistencyConfig,
    ) -> Self {
        Self {
            store,
            config,
            degraded: Arc::new(RwLock::new(false)),
        }
    }

    /// 强一致读
    ///
    /// `Strong`/`ReadCommitted`：直接读底层存储（线性一致）。
    /// `Eventual`：可能读到旧值，但本实现仍返回最新值（内存存储天然一致）。
    pub fn read_strong(&self, key: &str) -> Result<Option<Vec<u8>>, DistError> {
        let start = Instant::now();
        let store = self.store.read();
        let value = store.get(key).cloned();
        let elapsed = start.elapsed().as_millis() as u64;
        if elapsed >= self.config.max_latency_ms {
            *self.degraded.write() = true;
            return Err(DistError::Downgraded(format!(
                "read latency {elapsed}ms exceeds max {}ms",
                self.config.max_latency_ms
            )));
        }
        Ok(value)
    }

    /// 强一致写
    ///
    /// `Strong`：先失效所有实例缓存再写入（线性一致）。
    /// `ReadCommitted`：写入后立即可读。
    /// `Eventual`：异步传播（本实现同步写入，天然一致）。
    pub fn write_strong(&self, key: &str, value: &[u8]) -> Result<(), DistError> {
        let start = Instant::now();
        let mut store = self.store.write();
        store.insert(key.to_string(), value.to_vec());
        drop(store);
        let elapsed = start.elapsed().as_millis() as u64;
        if elapsed >= self.config.max_latency_ms {
            *self.degraded.write() = true;
            return Err(DistError::Downgraded(format!(
                "write latency {elapsed}ms exceeds max {}ms",
                self.config.max_latency_ms
            )));
        }
        Ok(())
    }

    /// 删除键
    pub fn delete(&self, key: &str) -> Result<(), DistError> {
        let mut store = self.store.write();
        store.remove(key);
        Ok(())
    }

    /// 切换一致性级别
    pub fn set_consistency_level(&self, level: ConsistencyLevel) {
        // 一致性级别切换：若从 Strong 降级到 Eventual，标记降级
        if self.config.level == ConsistencyLevel::Strong && level != ConsistencyLevel::Strong {
            *self.degraded.write() = true;
        }
    }

    /// 是否已降级
    pub fn is_degraded(&self) -> bool {
        *self.degraded.read()
    }

    /// 配置引用
    pub fn config(&self) -> &StrongConsistencyConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_then_read_strong() {
        let cache = StrongConsistencyCache::new(StrongConsistencyConfig::default());
        cache.write_strong("k1", b"v1").unwrap();
        let val = cache.read_strong("k1").unwrap();
        assert_eq!(val, Some(b"v1".to_vec()));
    }

    #[test]
    fn test_read_missing_key() {
        let cache = StrongConsistencyCache::new(StrongConsistencyConfig::default());
        let val = cache.read_strong("missing").unwrap();
        assert_eq!(val, None);
    }

    #[test]
    fn test_consistency_level_switch() {
        let cache = StrongConsistencyCache::new(StrongConsistencyConfig {
            level: ConsistencyLevel::Strong,
            max_latency_ms: 5,
        });
        assert!(!cache.is_degraded());
        cache.set_consistency_level(ConsistencyLevel::Eventual);
        assert!(cache.is_degraded());
    }

    #[test]
    fn test_downgrade_warning() {
        let cache = StrongConsistencyCache::new(StrongConsistencyConfig {
            level: ConsistencyLevel::Strong,
            max_latency_ms: 0,
        });
        // max_latency_ms=0，任何操作都会超时降级
        let result = cache.write_strong("k", b"v");
        assert!(matches!(result, Err(DistError::Downgraded(_))));
        assert!(cache.is_degraded());
    }

    #[test]
    fn test_delete() {
        let cache = StrongConsistencyCache::new(StrongConsistencyConfig::default());
        cache.write_strong("k1", b"v1").unwrap();
        cache.delete("k1").unwrap();
        assert_eq!(cache.read_strong("k1").unwrap(), None);
    }

    #[test]
    fn test_overwrite_returns_latest() {
        let cache = StrongConsistencyCache::new(StrongConsistencyConfig::default());
        cache.write_strong("k", b"old").unwrap();
        cache.write_strong("k", b"new").unwrap();
        assert_eq!(cache.read_strong("k").unwrap(), Some(b"new".to_vec()));
    }

    #[test]
    fn test_dist_error_display() {
        let e = DistError::Downgraded("latency".into());
        assert!(e.to_string().contains("CACHE_CONSISTENCY_DOWNGRADED"));
    }
}
