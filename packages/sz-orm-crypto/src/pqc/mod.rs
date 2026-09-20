//! 量子安全准备（PQC）模块
//!
//! 提供 NIST 标准化后量子密码学算法白名单、trait 抽象、混合密钥协商、
//! 降级管理和迁移评估。

pub mod traits;
pub mod whitelist;

#[cfg(feature = "pqc-hybrid-kex")]
pub mod hybrid_kex;

#[cfg(feature = "pqc-hybrid-kex")]
pub mod degradation_manager;

pub mod migration_assessor;

pub use whitelist::{PqcAlgorithm, PqcAlgorithmWhitelist};

#[cfg(feature = "pqc-hybrid-kex")]
pub use hybrid_kex::{HybridKeyExchange, HybridSessionKey};

#[cfg(feature = "pqc-hybrid-kex")]
pub use degradation_manager::PqcDegradationManager;

pub use migration_assessor::{CryptoScenario, PqcMigrationAssessor, PqcMigrationReport};

/// PQC 错误类型
#[derive(Debug, thiserror::Error)]
pub enum PqcError {
    #[error("算法未批准: {0}")]
    AlgorithmNotApproved(String),
    #[error("算法不支持: {0}")]
    AlgorithmUnsupported(String),
    #[error("握手失败: {0}")]
    HandshakeFailed(String),
    #[error("签名无效: {0}")]
    SignatureInvalid(String),
    #[error("降级到经典密码学: {0}")]
    Degraded(String),
    #[error("扫描不完整: {0}")]
    ScanIncomplete(String),
}
