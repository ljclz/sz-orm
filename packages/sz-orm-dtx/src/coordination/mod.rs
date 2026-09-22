//! 分布式协调模块（v7.1.0）
//!
//! 分布式锁 + Leader 选举 + 服务注册/发现 + Fencing Token。

pub mod backend;
pub mod fencing;
pub mod leader_election;
pub mod lock;
pub mod service_registry;
pub mod watchdog;

// v8.1.0 组4：Raft 共识优化（raft-optimize feature gate）
#[cfg(feature = "raft-optimize")]
pub mod raft_optimize;
// v8.1.0 组4：脑裂检测器（split-brain-detect feature gate）
#[cfg(feature = "split-brain-detect")]
pub mod split_brain_detector;

pub use backend::{
    CoordinationBackend, CoordinationError, InMemoryBackend, RedisBackend, SharedBackend,
};
pub use fencing::FencingTokenGenerator;
pub use leader_election::{LeaderElection, LeaderState};
pub use lock::{DistributedLock, LockGuard};
pub use service_registry::{ServiceInstance, ServiceRegistry};
pub use watchdog::LockWatchdog;

// v8.1.0 组4：导出共识优化组件
#[cfg(feature = "raft-optimize")]
pub use raft_optimize::{
    ClusterNode, ClusterTopology, DistError as RaftDistError, OptimizedParams, RaftOptimizeParams,
};
#[cfg(feature = "split-brain-detect")]
pub use split_brain_detector::{
    DistError as SplitBrainDistError, HeartbeatSample, SplitBrainDetector, SplitBrainStatus,
};
