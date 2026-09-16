//! 分布式协调模块（v7.1.0）
//!
//! 分布式锁 + Leader 选举 + 服务注册/发现 + Fencing Token。

pub mod backend;
pub mod fencing;
pub mod leader_election;
pub mod lock;
pub mod service_registry;
pub mod watchdog;

pub use backend::{
    CoordinationBackend, CoordinationError, InMemoryBackend, RedisBackend, SharedBackend,
};
pub use fencing::FencingTokenGenerator;
pub use leader_election::{LeaderElection, LeaderState};
pub use lock::{DistributedLock, LockGuard};
pub use service_registry::{ServiceInstance, ServiceRegistry};
pub use watchdog::LockWatchdog;
