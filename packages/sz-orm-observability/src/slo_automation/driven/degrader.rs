//! SLO 驱动的降级决策器

use super::SloDrivenError;
use crate::slo_automation::types::SloAchievement;

/// 降级决策
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DegradeDecision {
    /// 已降级的非核心功能列表
    pub degraded_features: Vec<String>,
    /// 是否阻塞核心功能（始终为 false，核心功能不可降级）
    pub core_blocked: bool,
}

/// 降级配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DegraderConfig {
    /// 降级阈值：SLO 达成率低于此值触发降级
    pub degrade_threshold: f64,
    /// 非核心功能列表（可降级）
    pub non_core_features: Vec<String>,
    /// 核心功能列表（不可降级）
    pub core_features: Vec<String>,
}

impl Default for DegraderConfig {
    fn default() -> Self {
        Self {
            degrade_threshold: 0.8,
            non_core_features: vec!["analytics".to_string(), "reporting".to_string()],
            core_features: vec!["query".to_string(), "transaction".to_string()],
        }
    }
}

/// SLO 驱动的降级决策器
pub struct SloDrivenDegrader {
    config: DegraderConfig,
}

impl SloDrivenDegrader {
    /// 创建降级决策器
    pub fn new(config: DegraderConfig) -> Self {
        Self { config }
    }

    /// 根据 SLO 达成率决策降级
    ///
    /// - SLO 严重不达标 → 识别非核心功能 → 降级
    /// - 核心功能不可降级，若无非核心功能可降级则返回 `DegradeCoreBlocked` 错误
    pub fn decide(&self, achievement: &SloAchievement) -> Result<DegradeDecision, SloDrivenError> {
        if !achievement.sufficient_data {
            return Ok(DegradeDecision {
                degraded_features: vec![],
                core_blocked: false,
            });
        }

        let rate = achievement.achievement_rate;
        if rate >= self.config.degrade_threshold {
            return Ok(DegradeDecision {
                degraded_features: vec![],
                core_blocked: false,
            });
        }

        // SLO 严重不达标，降级非核心功能
        if self.config.non_core_features.is_empty() {
            return Err(SloDrivenError::DegradeCoreBlocked(format!(
                "SLO 达成率 {rate:.3} 严重不达标，但无非核心功能可降级，核心功能不可降级"
            )));
        }

        Ok(DegradeDecision {
            degraded_features: self.config.non_core_features.clone(),
            core_blocked: false,
        })
    }

    /// 返回配置引用
    pub fn config(&self) -> &DegraderConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slo_automation::types::CalcWindow;

    fn make_achievement(rate: f64) -> SloAchievement {
        SloAchievement {
            window: CalcWindow::TwentyFourHours,
            achievement_rate: rate,
            sufficient_data: true,
        }
    }

    #[test]
    fn test_degrade_non_core_features() {
        let config = DegraderConfig {
            degrade_threshold: 0.8,
            non_core_features: vec!["analytics".to_string(), "reporting".to_string()],
            core_features: vec!["query".to_string()],
        };
        let degrader = SloDrivenDegrader::new(config);
        let achievement = make_achievement(0.60);
        let decision = degrader.decide(&achievement).unwrap();
        assert_eq!(decision.degraded_features.len(), 2);
        assert!(decision
            .degraded_features
            .contains(&"analytics".to_string()));
        assert!(decision
            .degraded_features
            .contains(&"reporting".to_string()));
        assert!(!decision.core_blocked);
    }

    #[test]
    fn test_core_features_protected() {
        let config = DegraderConfig {
            degrade_threshold: 0.8,
            non_core_features: vec!["analytics".to_string()],
            core_features: vec!["query".to_string(), "transaction".to_string()],
        };
        let degrader = SloDrivenDegrader::new(config);
        let achievement = make_achievement(0.60);
        let decision = degrader.decide(&achievement).unwrap();
        // 核心功能不出现在降级列表中
        assert!(!decision.degraded_features.contains(&"query".to_string()));
        assert!(!decision
            .degraded_features
            .contains(&"transaction".to_string()));
        assert!(!decision.core_blocked);
    }

    #[test]
    fn test_config_error_no_non_core_features() {
        let config = DegraderConfig {
            degrade_threshold: 0.8,
            non_core_features: vec![],
            core_features: vec!["query".to_string()],
        };
        let degrader = SloDrivenDegrader::new(config);
        let achievement = make_achievement(0.60);
        let result = degrader.decide(&achievement);
        assert!(matches!(result, Err(SloDrivenError::DegradeCoreBlocked(_))));
    }

    #[test]
    fn test_no_degrade_when_above_threshold() {
        let config = DegraderConfig::default();
        let degrader = SloDrivenDegrader::new(config);
        let achievement = make_achievement(0.90);
        let decision = degrader.decide(&achievement).unwrap();
        assert!(decision.degraded_features.is_empty());
        assert!(!decision.core_blocked);
    }

    #[test]
    fn test_no_degrade_when_insufficient_data() {
        let config = DegraderConfig::default();
        let degrader = SloDrivenDegrader::new(config);
        let achievement = SloAchievement {
            window: CalcWindow::TwentyFourHours,
            achievement_rate: 0.50,
            sufficient_data: false,
        };
        let decision = degrader.decide(&achievement).unwrap();
        assert!(decision.degraded_features.is_empty());
    }

    #[test]
    fn test_degrade_at_exact_threshold() {
        let config = DegraderConfig::default();
        let degrader = SloDrivenDegrader::new(config);
        let achievement = make_achievement(0.80);
        let decision = degrader.decide(&achievement).unwrap();
        // 达成率等于阈值，不触发降级
        assert!(decision.degraded_features.is_empty());
    }
}
