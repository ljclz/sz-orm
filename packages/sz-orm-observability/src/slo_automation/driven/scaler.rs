//! SLO 驱动的扩缩容决策器

use super::SloDrivenError;
use crate::slo_automation::types::SloAchievement;

/// 扩缩容动作
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ScaleAction {
    /// 扩容
    ScaleUp,
    /// 缩容
    ScaleDown,
    /// 无动作
    NoAction,
}

/// 扩缩容决策
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScaleDecision {
    /// 动作
    pub action: ScaleAction,
    /// 原因
    pub reason: String,
    /// 是否在冷却期内
    pub in_cooldown: bool,
}

/// 扩缩容配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScalerConfig {
    /// 冷却期（分钟），必须 >= 5 以防振荡
    pub cooldown_minutes: u32,
    /// 扩容阈值：SLO 达成率低于此值触发扩容
    pub scale_up_threshold: f64,
    /// 缩容阈值：SLO 达成率高于此值触发缩容
    pub scale_down_threshold: f64,
    /// 上次扩缩容时间戳（秒）
    pub last_scale_time: i64,
}

impl ScalerConfig {
    /// 创建扩缩容配置，校验冷却期 >= 5 分钟
    pub fn new(
        cooldown_minutes: u32,
        scale_up_threshold: f64,
        scale_down_threshold: f64,
        last_scale_time: i64,
    ) -> Result<Self, SloDrivenError> {
        if cooldown_minutes < 5 {
            return Err(SloDrivenError::InvalidCooldownMinutes(cooldown_minutes));
        }
        Ok(Self {
            cooldown_minutes,
            scale_up_threshold,
            scale_down_threshold,
            last_scale_time,
        })
    }
}

impl Default for ScalerConfig {
    fn default() -> Self {
        Self {
            cooldown_minutes: 10,
            scale_up_threshold: 0.95,
            scale_down_threshold: 0.99,
            last_scale_time: 0,
        }
    }
}

/// SLO 驱动的扩缩容决策器
pub struct SloDrivenScaler {
    config: ScalerConfig,
}

impl SloDrivenScaler {
    /// 创建扩缩容决策器，校验配置
    pub fn new(config: ScalerConfig) -> Result<Self, SloDrivenError> {
        if config.cooldown_minutes < 5 {
            return Err(SloDrivenError::InvalidCooldownMinutes(
                config.cooldown_minutes,
            ));
        }
        Ok(Self { config })
    }

    /// 根据 SLO 达成率决策扩缩容
    ///
    /// - SLO 达成率 < 扩容阈值 → 检查冷却期 → 触发扩容
    /// - SLO 达成率 > 缩容阈值 → 检查冷却期 → 触发缩容
    /// - 冷却期内拒绝扩缩容
    pub fn decide(
        &self,
        achievement: &SloAchievement,
        current_time: i64,
    ) -> Result<ScaleDecision, SloDrivenError> {
        if !achievement.sufficient_data {
            return Ok(ScaleDecision {
                action: ScaleAction::NoAction,
                reason: "数据不足，不执行扩缩容".to_string(),
                in_cooldown: false,
            });
        }

        let cooldown_seconds = i64::from(self.config.cooldown_minutes) * 60;
        let elapsed = current_time - self.config.last_scale_time;
        let in_cooldown = elapsed < cooldown_seconds;
        let rate = achievement.achievement_rate;

        if rate < self.config.scale_up_threshold {
            if in_cooldown {
                return Ok(ScaleDecision {
                    action: ScaleAction::NoAction,
                    reason: format!(
                        "SLO 达成率 {rate:.3} 低于扩容阈值 {:.3}，但冷却期内拒绝扩容",
                        self.config.scale_up_threshold
                    ),
                    in_cooldown: true,
                });
            }
            return Ok(ScaleDecision {
                action: ScaleAction::ScaleUp,
                reason: format!(
                    "SLO 达成率 {rate:.3} 低于扩容阈值 {:.3}，触发扩容",
                    self.config.scale_up_threshold
                ),
                in_cooldown: false,
            });
        }

        if rate > self.config.scale_down_threshold {
            if in_cooldown {
                return Ok(ScaleDecision {
                    action: ScaleAction::NoAction,
                    reason: format!(
                        "SLO 达成率 {rate:.3} 高于缩容阈值 {:.3}，但冷却期内拒绝缩容",
                        self.config.scale_down_threshold
                    ),
                    in_cooldown: true,
                });
            }
            return Ok(ScaleDecision {
                action: ScaleAction::ScaleDown,
                reason: format!(
                    "SLO 达成率 {rate:.3} 高于缩容阈值 {:.3}，触发缩容",
                    self.config.scale_down_threshold
                ),
                in_cooldown: false,
            });
        }

        Ok(ScaleDecision {
            action: ScaleAction::NoAction,
            reason: format!("SLO 达成率 {rate:.3} 处于正常区间，无需扩缩容"),
            in_cooldown: false,
        })
    }

    /// 返回配置引用
    pub fn config(&self) -> &ScalerConfig {
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
    fn test_scale_up_when_below_threshold() {
        let config = ScalerConfig::new(10, 0.95, 0.99, 0).unwrap();
        let scaler = SloDrivenScaler::new(config).unwrap();
        let achievement = make_achievement(0.80);
        let decision = scaler.decide(&achievement, 600).unwrap();
        assert_eq!(decision.action, ScaleAction::ScaleUp);
        assert!(!decision.in_cooldown);
        assert!(decision.reason.contains("触发扩容"));
    }

    #[test]
    fn test_scale_down_when_above_threshold() {
        let config = ScalerConfig::new(10, 0.95, 0.99, 0).unwrap();
        let scaler = SloDrivenScaler::new(config).unwrap();
        let achievement = make_achievement(0.999);
        let decision = scaler.decide(&achievement, 600).unwrap();
        assert_eq!(decision.action, ScaleAction::ScaleDown);
        assert!(!decision.in_cooldown);
        assert!(decision.reason.contains("触发缩容"));
    }

    #[test]
    fn test_cooldown_rejects_scale_up() {
        let config = ScalerConfig::new(10, 0.95, 0.99, 0).unwrap();
        let scaler = SloDrivenScaler::new(config).unwrap();
        let achievement = make_achievement(0.80);
        // 冷却期 10 分钟 = 600 秒，当前时间 100 秒，仍在冷却期内
        let decision = scaler.decide(&achievement, 100).unwrap();
        assert_eq!(decision.action, ScaleAction::NoAction);
        assert!(decision.in_cooldown);
        assert!(decision.reason.contains("冷却期内拒绝扩容"));
    }

    #[test]
    fn test_cooldown_rejects_scale_down() {
        let config = ScalerConfig::new(10, 0.95, 0.99, 0).unwrap();
        let scaler = SloDrivenScaler::new(config).unwrap();
        let achievement = make_achievement(0.999);
        let decision = scaler.decide(&achievement, 100).unwrap();
        assert_eq!(decision.action, ScaleAction::NoAction);
        assert!(decision.in_cooldown);
        assert!(decision.reason.contains("冷却期内拒绝缩容"));
    }

    #[test]
    fn test_oscillation_protection_invalid_cooldown() {
        let result = ScalerConfig::new(4, 0.95, 0.99, 0);
        assert!(matches!(
            result,
            Err(SloDrivenError::InvalidCooldownMinutes(4))
        ));
    }

    #[test]
    fn test_oscillation_protection_in_scaler_new() {
        let bad_config = ScalerConfig {
            cooldown_minutes: 3,
            scale_up_threshold: 0.95,
            scale_down_threshold: 0.99,
            last_scale_time: 0,
        };
        let result = SloDrivenScaler::new(bad_config);
        assert!(matches!(
            result,
            Err(SloDrivenError::InvalidCooldownMinutes(3))
        ));
    }

    #[test]
    fn test_no_action_in_normal_range() {
        let config = ScalerConfig::new(10, 0.95, 0.99, 0).unwrap();
        let scaler = SloDrivenScaler::new(config).unwrap();
        let achievement = make_achievement(0.97);
        let decision = scaler.decide(&achievement, 600).unwrap();
        assert_eq!(decision.action, ScaleAction::NoAction);
        assert!(!decision.in_cooldown);
    }

    #[test]
    fn test_insufficient_data_no_action() {
        let config = ScalerConfig::new(10, 0.95, 0.99, 0).unwrap();
        let scaler = SloDrivenScaler::new(config).unwrap();
        let achievement = SloAchievement {
            window: CalcWindow::TwentyFourHours,
            achievement_rate: 0.50,
            sufficient_data: false,
        };
        let decision = scaler.decide(&achievement, 600).unwrap();
        assert_eq!(decision.action, ScaleAction::NoAction);
        assert!(!decision.in_cooldown);
    }

    #[test]
    fn test_cooldown_boundary_exact() {
        let config = ScalerConfig::new(10, 0.95, 0.99, 0).unwrap();
        let scaler = SloDrivenScaler::new(config).unwrap();
        let achievement = make_achievement(0.80);
        // 冷却期 10 分钟 = 600 秒，当前时间恰好 600 秒，刚出冷却期
        let decision = scaler.decide(&achievement, 600).unwrap();
        assert_eq!(decision.action, ScaleAction::ScaleUp);
        assert!(!decision.in_cooldown);
    }
}
