//! v8.0.0 安全合规深化 — TDE 密钥管理与高可用
//!
//! 子模块：
//! - [`dek_rotation_manager`] — DEK 轮换管理器（90 天周期、重叠期 ≥ 24h、不中断服务）
//! - [`kms_ha_manager`] — KMS 高可用管理器（主备切换 + DEK 缓存兜底）
//! - [`column_policy_hot_updater`] — 列加密策略热更新器（≤ 10s 生效）

pub mod column_policy_hot_updater;
pub mod dek_rotation_manager;
pub mod kms_ha_manager;

#[cfg(feature = "key-auto-rotate")]
pub mod key_auto_rotate_scheduler;

pub use column_policy_hot_updater::{ColumnPolicyConfig, ColumnPolicyHotUpdater, HotUpdateResult};
pub use dek_rotation_manager::{
    DekRotationConfig, DekRotationManager, DekRotationRecord, DekRotationStatus,
};
pub use kms_ha_manager::{FailoverRecord, KmsHaConfig, KmsHaManager, KmsNode};

/// 安全合规深化统一错误类型
///
/// 涵盖 DEK 轮换、KMS 高可用、列策略热更新、证据链导出等场景的异常。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecError {
    /// DEK 轮换中断（回退旧 DEK，告警 `TDE_KEY_ROTATION_INTERRUPTED`）
    KeyRotationInterrupted(String),
    /// 主备 KMS 全部故障（使用 DEK 缓存，告警 `KMS_ALL_UNAVAILABLE`）
    KmsAllUnavailable(String),
    /// 证据链被篡改（告警 `EVIDENCE_CHAIN_TAMPERED`）
    EvidenceChainTampered(String),
    /// 策略冲突（按优先级仲裁 + 记录 `MASKING_POLICY_CONFLICT`）
    PolicyConflict(String),
    /// ABAC 评估超时（默认拒绝 + `ABAC_EVAL_TIMEOUT`）
    AbacEvalTimeout(String),
    /// 列策略热更新失败
    HotUpdateFailed(String),
    /// 通用参数错误
    InvalidArgument(String),
}

impl std::fmt::Display for SecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecError::KeyRotationInterrupted(msg) => {
                write!(f, "TDE_KEY_ROTATION_INTERRUPTED: {}", msg)
            }
            SecError::KmsAllUnavailable(msg) => write!(f, "KMS_ALL_UNAVAILABLE: {}", msg),
            SecError::EvidenceChainTampered(msg) => write!(f, "EVIDENCE_CHAIN_TAMPERED: {}", msg),
            SecError::PolicyConflict(msg) => write!(f, "MASKING_POLICY_CONFLICT: {}", msg),
            SecError::AbacEvalTimeout(msg) => write!(f, "ABAC_EVAL_TIMEOUT: {}", msg),
            SecError::HotUpdateFailed(msg) => write!(f, "Hot update failed: {}", msg),
            SecError::InvalidArgument(msg) => write!(f, "Invalid argument: {}", msg),
        }
    }
}

impl std::error::Error for SecError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sec_error_display_rotation_interrupted() {
        let err = SecError::KeyRotationInterrupted("overlap window closed".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("TDE_KEY_ROTATION_INTERRUPTED"));
        assert!(msg.contains("overlap window closed"));
    }

    #[test]
    fn sec_error_display_kms_all_unavailable() {
        let err = SecError::KmsAllUnavailable("primary+backup down".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("KMS_ALL_UNAVAILABLE"));
    }

    #[test]
    fn sec_error_display_evidence_tampered() {
        let err = SecError::EvidenceChainTampered("entry 3 hash mismatch".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("EVIDENCE_CHAIN_TAMPERED"));
    }

    #[test]
    fn sec_error_display_policy_conflict() {
        let err = SecError::PolicyConflict("field phone priority 1 vs 2".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("MASKING_POLICY_CONFLICT"));
    }

    #[test]
    fn sec_error_display_abac_timeout() {
        let err = SecError::AbacEvalTimeout("eval exceeded 5ms".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("ABAC_EVAL_TIMEOUT"));
    }

    #[test]
    fn sec_error_equality() {
        let a = SecError::InvalidArgument("x".to_string());
        let b = SecError::InvalidArgument("x".to_string());
        assert_eq!(a, b);
        let c = SecError::InvalidArgument("y".to_string());
        assert_ne!(a, c);
    }
}
