//! v7.9.0 跨存储生命周期编排
//!
//! 多级存储金字塔 + 成本模拟 + 合规证据链 + 联邦查询路由。
//! 复用 v7.8.0 的 `LifecycleRuleEngine`、`ColdHotClassifier`、`DataTemperature`。

pub mod cost_simulator;
pub mod evidence_chain;
pub mod pyramid;

#[cfg(feature = "federated-query")]
pub mod federated_query;

pub use cost_simulator::{Confidence, CostReport, CostSimulateConfig, CostSimulator};
pub use evidence_chain::{
    ComplianceEvidenceChain, ComplianceStandard, DestroyOperation, EvidenceChain, EvidenceConfig,
};
pub use pyramid::{
    MigrationReport, PyramidConfig, StoragePyramid, StorageTierClassifier, TierMigrationStep,
};

#[cfg(feature = "federated-query")]
pub use federated_query::{AuthContext, FederatedConfig, FederatedQueryRouter, FederatedResult};

/// 跨存储错误类型
#[derive(Debug, thiserror::Error)]
pub enum CrossStorageError {
    #[error("跨级跳转禁止: {from:?} → {to:?}")]
    SkipTierForbidden {
        from: super::types::DataTemperature,
        to: super::types::DataTemperature,
    },
    #[error("存储层级不可用: {0:?}")]
    StorageTierUnavailable(super::types::DataTemperature),
    #[error("合规证据链不可用: {0}")]
    ComplianceChainUnavailable(String),
    #[error("联邦查询部分失败: 超时层级 {0:?}")]
    FederatedQueryPartial(Vec<String>),
    #[error("越权访问: {0}")]
    Unauthorized(String),
    #[error("数据范围无效: {0}")]
    DataRangeInvalid(String),
}
