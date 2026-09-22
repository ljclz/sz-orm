//! PQC 迁移执行与密钥轮换模块
//!
//! 提供渐进迁移执行器、密钥轮换管理、性能基准追踪和算法敏捷性切换。

mod agility_switcher;
mod baseline_tracker;
mod executor;

#[cfg(feature = "pqc-key-rotation")]
mod key_rotation;

pub use agility_switcher::{AgilityAuditLog, AlgorithmAgilitySwitcher};
pub use baseline_tracker::{PerformanceBaselineTracker, PerformanceReport, SceneMetrics};
pub use executor::{MigrationCheckpoint, MigrationProgress, PqcMigrationExecutor, SceneResult};

#[cfg(feature = "pqc-key-rotation")]
pub use key_rotation::{KeyRotationManager, RotationRecord, RotationStatus};

/// PQC 迁移执行错误类型
#[derive(Debug, thiserror::Error)]
pub enum PqcExecError {
    #[error("缺少评估报告，无法执行迁移: {0}")]
    AssessmentRequired(String),
    #[error("场景执行失败: {0}")]
    SceneFailed(String),
    #[error("密钥轮换被中断: {0}")]
    KeyRotationInterrupted(String),
    #[error("算法不在白名单中: {0}")]
    AlgorithmNotInWhitelist(String),
    #[error("算法不兼容: {0}")]
    AlgorithmIncompatible(String),
    #[error("迁移已暂停: {0}")]
    MigrationPaused(String),
    #[error("性能基准缺失: {0}")]
    BaselineMissing(String),
}
