//! 配置热更新统一接口（v10.1.0）
//!
//! 提供加密策略/缓存策略/告警阈值/慢查询阈值的运行时热更新。
//! validate → apply 两步：先校验配置合法性，再原子应用。

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 热更新目标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HotUpdateTarget {
    /// 加密策略
    EncryptionPolicy { algorithm: String },
    /// 缓存策略
    CacheStrategy { max_entries: usize, ttl: Duration },
    /// 告警阈值
    AlertThreshold { warn: u32, critical: u32 },
    /// 慢查询阈值
    SlowQueryThreshold { threshold_ms: u64 },
}

/// 统一热更新器
pub struct UnifiedHotUpdater;

impl UnifiedHotUpdater {
    /// 校验热更新目标配置合法性
    ///
    /// 返回 `Err(String)` 含具体配置错误原因。
    pub fn validate(target: &HotUpdateTarget) -> Result<(), String> {
        match target {
            HotUpdateTarget::EncryptionPolicy { algorithm } => {
                if algorithm.is_empty() {
                    return Err("encryption algorithm must not be empty".to_string());
                }
                Ok(())
            }
            HotUpdateTarget::CacheStrategy { max_entries, ttl } => {
                if *max_entries == 0 {
                    return Err("cache max_entries must be > 0".to_string());
                }
                if ttl.is_zero() {
                    return Err("cache ttl must be > 0".to_string());
                }
                Ok(())
            }
            HotUpdateTarget::AlertThreshold { warn, critical } => {
                if *warn >= *critical {
                    return Err(format!(
                        "alert warn threshold ({}) must be < critical ({})",
                        warn, critical
                    ));
                }
                Ok(())
            }
            HotUpdateTarget::SlowQueryThreshold { threshold_ms } => {
                if *threshold_ms == 0 {
                    return Err("slow query threshold must be > 0".to_string());
                }
                Ok(())
            }
        }
    }

    /// 校验并应用热更新（生产消费点：调用方传入可变引用更新配置）
    ///
    /// `apply_fn` 是实际应用配置变更的闭包，由调用方提供。
    pub fn validate_and_apply<F>(target: &HotUpdateTarget, apply_fn: F) -> Result<(), String>
    where
        F: FnOnce(&HotUpdateTarget) -> Result<(), String>,
    {
        Self::validate(target)?;
        apply_fn(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_encryption_policy_empty_algorithm_rejected() {
        let target = HotUpdateTarget::EncryptionPolicy {
            algorithm: "".to_string(),
        };
        let result = UnifiedHotUpdater::validate(&target);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("algorithm"));
    }

    #[test]
    fn test_validate_encryption_policy_valid() {
        let target = HotUpdateTarget::EncryptionPolicy {
            algorithm: "AES-256-GCM".to_string(),
        };
        assert!(UnifiedHotUpdater::validate(&target).is_ok());
    }

    #[test]
    fn test_validate_cache_strategy_zero_entries_rejected() {
        let target = HotUpdateTarget::CacheStrategy {
            max_entries: 0,
            ttl: Duration::from_secs(60),
        };
        let result = UnifiedHotUpdater::validate(&target);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("max_entries"));
    }

    #[test]
    fn test_validate_cache_strategy_zero_ttl_rejected() {
        let target = HotUpdateTarget::CacheStrategy {
            max_entries: 100,
            ttl: Duration::ZERO,
        };
        let result = UnifiedHotUpdater::validate(&target);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ttl"));
    }

    #[test]
    fn test_validate_cache_strategy_valid() {
        let target = HotUpdateTarget::CacheStrategy {
            max_entries: 100,
            ttl: Duration::from_secs(60),
        };
        assert!(UnifiedHotUpdater::validate(&target).is_ok());
    }

    #[test]
    fn test_validate_alert_threshold_warn_ge_critical_rejected() {
        let target = HotUpdateTarget::AlertThreshold {
            warn: 80,
            critical: 80,
        };
        let result = UnifiedHotUpdater::validate(&target);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("warn"));
    }

    #[test]
    fn test_validate_alert_threshold_warn_gt_critical_rejected() {
        let target = HotUpdateTarget::AlertThreshold {
            warn: 90,
            critical: 80,
        };
        let result = UnifiedHotUpdater::validate(&target);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_alert_threshold_valid() {
        let target = HotUpdateTarget::AlertThreshold {
            warn: 70,
            critical: 90,
        };
        assert!(UnifiedHotUpdater::validate(&target).is_ok());
    }

    #[test]
    fn test_validate_slow_query_threshold_zero_rejected() {
        let target = HotUpdateTarget::SlowQueryThreshold { threshold_ms: 0 };
        let result = UnifiedHotUpdater::validate(&target);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("threshold"));
    }

    #[test]
    fn test_validate_slow_query_threshold_valid() {
        let target = HotUpdateTarget::SlowQueryThreshold { threshold_ms: 500 };
        assert!(UnifiedHotUpdater::validate(&target).is_ok());
    }

    #[test]
    fn test_validate_and_apply_runs_apply_on_valid() {
        let target = HotUpdateTarget::AlertThreshold {
            warn: 50,
            critical: 80,
        };
        let mut applied = false;
        let result = UnifiedHotUpdater::validate_and_apply(&target, |_| {
            applied = true;
            Ok(())
        });
        assert!(result.is_ok());
        assert!(applied, "apply_fn should have been called");
    }

    #[test]
    fn test_validate_and_apply_skips_apply_on_invalid() {
        let target = HotUpdateTarget::AlertThreshold {
            warn: 90,
            critical: 80,
        };
        let mut applied = false;
        let result = UnifiedHotUpdater::validate_and_apply(&target, |_| {
            applied = true;
            Ok(())
        });
        assert!(result.is_err());
        assert!(!applied, "apply_fn should not have been called on invalid");
    }
}
