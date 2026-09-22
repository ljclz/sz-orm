//! v8.0.0 双向同步模块（`dist-sync-bi` feature gate）
//!
//! HLC 混合逻辑时钟 + 双向 CDC 同步冲突解决。
//! 复用 `CdcSyncCoordinator`（`packages/sz-orm-fusion/src/cdc_sync.rs:38`）
//! 与 `ConflictResolver`（`packages/sz-orm-fusion/src/conflict.rs:181`）。

pub mod bi_sync_coordinator;
pub mod hlc_clock;

pub use bi_sync_coordinator::{
    BiDirectionalSyncCoordinator, BiSyncConfig, BiSyncResult, ConflictStrategy,
};
pub use hlc_clock::{HlcClock, HlcConfig, HlcTimestamp};

/// 分布式增强错误类型
#[derive(Debug, Clone)]
pub enum DistEnhanceError {
    /// 冲突无法解决（保留双方待人工，告警 `DIST_SYNC_CONFLICT_UNRESOLVED`）
    ConflictUnresolved(String),
    /// HLC 时钟偏移
    ClockSkew(String),
    /// 同步失败
    SyncFailed(String),
}

impl std::fmt::Display for DistEnhanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConflictUnresolved(k) => write!(f, "conflict unresolved for key: {k}"),
            Self::ClockSkew(msg) => write!(f, "hlc clock skew: {msg}"),
            Self::SyncFailed(msg) => write!(f, "sync failed: {msg}"),
        }
    }
}

impl std::error::Error for DistEnhanceError {}

/// 方法签名统一错误别名
pub type DistError = DistEnhanceError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let e = DistEnhanceError::ConflictUnresolved("k1".into());
        assert_eq!(e.to_string(), "conflict unresolved for key: k1");
        let e2 = DistEnhanceError::SyncFailed("x".into());
        assert_eq!(e2.to_string(), "sync failed: x");
    }
}
