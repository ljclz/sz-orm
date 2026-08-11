//! 缓存一致性协议模块（v4.1.0，`cache-coherence` feature gate）
//!
//! 实现 MESI 风格缓存一致性状态机，支持 WriteThrough/WriteBehind 策略，
//! 通过 trait-based 广播器实现跨实例失效广播。

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// MESI 缓存行状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MesiState {
    /// 已修改（本地修改，其他实例无副本）
    Modified,
    /// 独占（本地独有，未修改，其他实例无副本）
    Exclusive,
    /// 共享（多实例共享只读副本）
    Shared,
    /// 无效（缓存行不存在或已失效）
    Invalid,
}

/// 写策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsistencyStrategy {
    /// 写穿透（同步写缓存+DB）
    WriteThrough,
    /// 写后行（异步写 DB，先写缓存）
    WriteBehind,
}

/// 失效操作类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidationOp {
    /// 修改操作
    Modify,
    /// 删除操作
    Delete,
}

/// 失效广播事件
#[derive(Debug, Clone)]
pub struct InvalidationEvent {
    /// 缓存键
    pub key: String,
    /// 发送实例 ID
    pub instance_id: String,
    /// 时间戳（Unix 毫秒）
    pub timestamp: u64,
    /// 操作类型
    pub op: InvalidationOp,
}

/// 失效广播器 trait（抽象消息队列，避免硬依赖具体 MQ 实现）
pub trait InvalidationBroadcaster: Send + Sync {
    /// 广播失效事件
    fn broadcast(&self, event: &InvalidationEvent) -> Result<(), CoherenceError>;
}

/// 一致性指标
#[derive(Debug, Clone, Default)]
pub struct CoherenceMetrics {
    /// Modified 状态计数
    pub modified_count: u64,
    /// Exclusive 状态计数
    pub exclusive_count: u64,
    /// Shared 状态计数
    pub shared_count: u64,
    /// Invalid 状态计数
    pub invalid_count: u64,
    /// 失效广播次数
    pub invalidation_broadcasts: u64,
    /// 一致性违规次数
    pub coherence_violations: u64,
    /// Write-behind 回滚次数
    pub write_behind_rollbacks: u64,
}

/// 一致性错误
#[derive(Debug, Clone, thiserror::Error)]
pub enum CoherenceError {
    /// 广播失败
    #[error("broadcast failed: {0}")]
    BroadcastFailed(String),
    /// Write-behind 失败
    #[error("write-behind failed for key: {key}")]
    WriteBehindFailed {
        /// 失败的缓存键
        key: String,
    },
    /// 脑裂检测
    #[error("split-brain detected for key: {key}")]
    SplitBrain {
        /// 发生脑裂的缓存键
        key: String,
    },
    /// 缓存未命中
    #[error("cache miss for key: {0}")]
    CacheMiss(String),
}

/// 缓存一致性协议
pub struct CacheCoherenceProtocol {
    /// 缓存行状态表（key → MESI 状态）
    states: RwLock<HashMap<String, MesiState>>,
    /// 广播器
    broadcaster: Arc<dyn InvalidationBroadcaster>,
    /// 实例 ID
    instance_id: String,
    /// 写策略
    strategy: ConsistencyStrategy,
    /// 指标
    metrics: Arc<RwLock<CoherenceMetrics>>,
}

impl CacheCoherenceProtocol {
    /// 创建新的一致性协议实例
    pub fn new(
        instance_id: String,
        strategy: ConsistencyStrategy,
        broadcaster: Arc<dyn InvalidationBroadcaster>,
    ) -> Self {
        Self {
            states: RwLock::new(HashMap::new()),
            broadcaster,
            instance_id,
            strategy,
            metrics: Arc::new(RwLock::new(CoherenceMetrics::default())),
        }
    }

    /// 获取缓存行状态
    pub fn state(&self, key: &str) -> MesiState {
        self.states
            .read()
            .unwrap()
            .get(key)
            .copied()
            .unwrap_or(MesiState::Invalid)
    }

    /// 读操作（触发状态转换：Invalid→Exclusive/Shared）
    pub fn read(&self, key: &str, other_instances_have: bool) -> MesiState {
        let mut states = self.states.write().unwrap();
        let mut metrics = self.metrics.write().unwrap();
        let current = states.get(key).copied().unwrap_or(MesiState::Invalid);
        let new_state = match current {
            MesiState::Invalid => {
                if other_instances_have {
                    MesiState::Shared
                } else {
                    MesiState::Exclusive
                }
            }
            other => other,
        };
        states.insert(key.to_string(), new_state);
        Self::update_metrics(&mut metrics, &new_state);
        new_state
    }

    /// 写操作（触发状态转换：→Modified，广播失效）
    pub fn write(&self, key: &str) -> Result<MesiState, CoherenceError> {
        let event = InvalidationEvent {
            key: key.to_string(),
            instance_id: self.instance_id.clone(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            op: InvalidationOp::Modify,
        };
        self.broadcaster.broadcast(&event)?;

        let mut states = self.states.write().unwrap();
        let mut metrics = self.metrics.write().unwrap();
        states.insert(key.to_string(), MesiState::Modified);
        metrics.invalidation_broadcasts += 1;
        Self::update_metrics(&mut metrics, &MesiState::Modified);
        Ok(MesiState::Modified)
    }

    /// 处理收到的失效广播（→Invalid）
    pub fn handle_invalidation(&self, event: &InvalidationEvent) {
        if event.instance_id == self.instance_id {
            return;
        }
        let mut states = self.states.write().unwrap();
        let mut metrics = self.metrics.write().unwrap();
        states.insert(event.key.clone(), MesiState::Invalid);
        metrics.invalid_count += 1;
    }

    /// 获取指标快照
    pub fn metrics(&self) -> CoherenceMetrics {
        self.metrics.read().unwrap().clone()
    }

    /// 获取写策略
    pub fn strategy(&self) -> ConsistencyStrategy {
        self.strategy
    }

    fn update_metrics(metrics: &mut CoherenceMetrics, state: &MesiState) {
        match state {
            MesiState::Modified => metrics.modified_count += 1,
            MesiState::Exclusive => metrics.exclusive_count += 1,
            MesiState::Shared => metrics.shared_count += 1,
            MesiState::Invalid => metrics.invalid_count += 1,
        }
    }
}

/// 空广播器（单实例模式，不广播）
pub struct NoopBroadcaster;

impl InvalidationBroadcaster for NoopBroadcaster {
    fn broadcast(&self, _event: &InvalidationEvent) -> Result<(), CoherenceError> {
        Ok(())
    }
}

/// 本地广播器（收集事件用于测试）
pub struct LocalBroadcaster {
    events: RwLock<Vec<InvalidationEvent>>,
}

impl LocalBroadcaster {
    /// 创建本地广播器
    pub fn new() -> Self {
        Self {
            events: RwLock::new(Vec::new()),
        }
    }

    /// 获取已收集的事件
    pub fn events(&self) -> Vec<InvalidationEvent> {
        self.events.read().unwrap().clone()
    }
}

impl Default for LocalBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

impl InvalidationBroadcaster for LocalBroadcaster {
    fn broadcast(&self, event: &InvalidationEvent) -> Result<(), CoherenceError> {
        self.events.write().unwrap().push(event.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mesi_state_transitions() {
        let broadcaster = Arc::new(LocalBroadcaster::new());
        let protocol = CacheCoherenceProtocol::new(
            "instance-A".to_string(),
            ConsistencyStrategy::WriteThrough,
            broadcaster,
        );

        assert_eq!(protocol.state("key1"), MesiState::Invalid);

        let s = protocol.read("key1", false);
        assert_eq!(s, MesiState::Exclusive);

        let s = protocol.read("key1", true);
        assert_eq!(s, MesiState::Exclusive);

        let s = protocol.write("key1").unwrap();
        assert_eq!(s, MesiState::Modified);
        assert_eq!(protocol.state("key1"), MesiState::Modified);
    }

    #[test]
    fn test_invalid_to_shared() {
        let broadcaster = Arc::new(LocalBroadcaster::new());
        let protocol = CacheCoherenceProtocol::new(
            "instance-A".to_string(),
            ConsistencyStrategy::WriteThrough,
            broadcaster,
        );

        let s = protocol.read("key1", true);
        assert_eq!(s, MesiState::Shared);
    }

    #[test]
    fn test_invalid_to_exclusive() {
        let broadcaster = Arc::new(LocalBroadcaster::new());
        let protocol = CacheCoherenceProtocol::new(
            "instance-A".to_string(),
            ConsistencyStrategy::WriteThrough,
            broadcaster,
        );

        let s = protocol.read("key1", false);
        assert_eq!(s, MesiState::Exclusive);
    }

    #[test]
    fn test_write_broadcasts_invalidation() {
        let broadcaster = Arc::new(LocalBroadcaster::new());
        let protocol = CacheCoherenceProtocol::new(
            "instance-A".to_string(),
            ConsistencyStrategy::WriteThrough,
            broadcaster.clone(),
        );

        protocol.write("key1").unwrap();
        let events = broadcaster.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].key, "key1");
        assert_eq!(events[0].op, InvalidationOp::Modify);
    }

    #[test]
    fn test_handle_invalidation_sets_invalid() {
        let broadcaster = Arc::new(LocalBroadcaster::new());
        let protocol = CacheCoherenceProtocol::new(
            "instance-A".to_string(),
            ConsistencyStrategy::WriteThrough,
            broadcaster,
        );

        protocol.read("key1", false);
        assert_eq!(protocol.state("key1"), MesiState::Exclusive);

        let event = InvalidationEvent {
            key: "key1".to_string(),
            instance_id: "instance-B".to_string(),
            timestamp: 0,
            op: InvalidationOp::Modify,
        };
        protocol.handle_invalidation(&event);
        assert_eq!(protocol.state("key1"), MesiState::Invalid);
    }

    #[test]
    fn test_ignore_self_invalidation() {
        let broadcaster = Arc::new(LocalBroadcaster::new());
        let protocol = CacheCoherenceProtocol::new(
            "instance-A".to_string(),
            ConsistencyStrategy::WriteThrough,
            broadcaster,
        );

        protocol.read("key1", false);
        assert_eq!(protocol.state("key1"), MesiState::Exclusive);

        let event = InvalidationEvent {
            key: "key1".to_string(),
            instance_id: "instance-A".to_string(),
            timestamp: 0,
            op: InvalidationOp::Modify,
        };
        protocol.handle_invalidation(&event);
        assert_eq!(protocol.state("key1"), MesiState::Exclusive);
    }

    #[test]
    fn test_metrics_tracking() {
        let broadcaster = Arc::new(LocalBroadcaster::new());
        let protocol = CacheCoherenceProtocol::new(
            "instance-A".to_string(),
            ConsistencyStrategy::WriteThrough,
            broadcaster,
        );

        protocol.read("key1", false);
        protocol.read("key2", true);
        protocol.write("key1").unwrap();

        let metrics = protocol.metrics();
        assert!(metrics.exclusive_count > 0);
        assert!(metrics.shared_count > 0);
        assert!(metrics.modified_count > 0);
        assert!(metrics.invalidation_broadcasts > 0);
    }

    #[test]
    fn test_noop_broadcaster() {
        let broadcaster = Arc::new(NoopBroadcaster);
        let protocol = CacheCoherenceProtocol::new(
            "instance-A".to_string(),
            ConsistencyStrategy::WriteBehind,
            broadcaster,
        );

        let result = protocol.write("key1");
        assert!(result.is_ok());
    }

    #[test]
    fn test_shared_to_modified_on_write() {
        let broadcaster = Arc::new(LocalBroadcaster::new());
        let protocol = CacheCoherenceProtocol::new(
            "instance-A".to_string(),
            ConsistencyStrategy::WriteThrough,
            broadcaster,
        );

        let s = protocol.read("key1", true);
        assert_eq!(s, MesiState::Shared);

        let s = protocol.write("key1").unwrap();
        assert_eq!(s, MesiState::Modified);
    }

    #[test]
    fn test_strategy_access() {
        let broadcaster = Arc::new(NoopBroadcaster);
        let protocol = CacheCoherenceProtocol::new(
            "instance-A".to_string(),
            ConsistencyStrategy::WriteBehind,
            broadcaster,
        );
        assert_eq!(protocol.strategy(), ConsistencyStrategy::WriteBehind);
    }
}
