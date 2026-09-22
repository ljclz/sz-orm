//! SLO 驱动的路由调整器

use super::SloDrivenError;
use crate::slo_automation::types::SloAchievement;

/// 路由权重
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RouteWeight {
    /// SLO 标识
    pub slo_id: String,
    /// 路由权重（归一化后总和为 1.0）
    pub weight: f64,
}

/// 路由调整结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RouteAdjustment {
    /// 路由权重列表
    pub route_weights: Vec<RouteWeight>,
}

/// 路由配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RouterConfig {
    /// 路由调整阈值，必须 ∈ (0, 1)
    pub adjustment_threshold: f64,
    /// 高优先级 SLO 标识列表
    pub high_priority_slo_ids: Vec<String>,
}

impl RouterConfig {
    /// 创建路由配置，校验阈值 ∈ (0, 1)
    pub fn new(
        adjustment_threshold: f64,
        high_priority_slo_ids: Vec<String>,
    ) -> Result<Self, SloDrivenError> {
        if adjustment_threshold <= 0.0 || adjustment_threshold >= 1.0 {
            return Err(SloDrivenError::InvalidRouterThreshold(adjustment_threshold));
        }
        Ok(Self {
            adjustment_threshold,
            high_priority_slo_ids,
        })
    }
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            adjustment_threshold: 0.9,
            high_priority_slo_ids: vec![],
        }
    }
}

/// SLO 驱动的路由调整器
pub struct SloDrivenRouter {
    config: RouterConfig,
}

impl SloDrivenRouter {
    /// 创建路由调整器，校验配置
    pub fn new(config: RouterConfig) -> Result<Self, SloDrivenError> {
        if config.adjustment_threshold <= 0.0 || config.adjustment_threshold >= 1.0 {
            return Err(SloDrivenError::InvalidRouterThreshold(
                config.adjustment_threshold,
            ));
        }
        Ok(Self { config })
    }

    /// 根据多个 SLO 达成率调整路由权重
    ///
    /// - 高优先级 SLO 达成率下降 → 大幅增加其权重
    /// - 普通优先级 SLO 达成率下降 → 小幅增加其权重
    /// - 权重归一化后返回
    pub fn adjust(
        &self,
        achievements: &[(String, SloAchievement)],
    ) -> Result<RouteAdjustment, SloDrivenError> {
        if achievements.is_empty() {
            return Ok(RouteAdjustment {
                route_weights: vec![],
            });
        }

        let mut weights: Vec<RouteWeight> = achievements
            .iter()
            .map(|(slo_id, achievement)| {
                let is_high_priority = self.config.high_priority_slo_ids.contains(slo_id);
                let rate = achievement.achievement_rate;
                let weight = if rate < self.config.adjustment_threshold {
                    let deficit = self.config.adjustment_threshold - rate;
                    if is_high_priority {
                        1.0 + deficit * 2.0
                    } else {
                        1.0 + deficit
                    }
                } else {
                    1.0
                };
                RouteWeight {
                    slo_id: slo_id.clone(),
                    weight,
                }
            })
            .collect();

        // 归一化权重
        let total: f64 = weights.iter().map(|w| w.weight).sum();
        if total > 0.0 {
            for w in &mut weights {
                w.weight /= total;
            }
        }

        Ok(RouteAdjustment {
            route_weights: weights,
        })
    }

    /// 返回配置引用
    pub fn config(&self) -> &RouterConfig {
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
    fn test_route_adjustment_normal() {
        let config = RouterConfig::new(0.9, vec![]).unwrap();
        let router = SloDrivenRouter::new(config).unwrap();
        let achievements = vec![
            ("slo_a".to_string(), make_achievement(0.95)),
            ("slo_b".to_string(), make_achievement(0.95)),
        ];
        let adjustment = router.adjust(&achievements).unwrap();
        assert_eq!(adjustment.route_weights.len(), 2);
        // 达成率高于阈值，权重均为 1.0，归一化后 0.5
        assert!((adjustment.route_weights[0].weight - 0.5).abs() < 1e-9);
        assert!((adjustment.route_weights[1].weight - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_high_priority_gets_more_weight() {
        let config = RouterConfig::new(0.9, vec!["slo_high".to_string()]).unwrap();
        let router = SloDrivenRouter::new(config).unwrap();
        let achievements = vec![
            ("slo_high".to_string(), make_achievement(0.70)),
            ("slo_low".to_string(), make_achievement(0.70)),
        ];
        let adjustment = router.adjust(&achievements).unwrap();
        let high_weight = adjustment
            .route_weights
            .iter()
            .find(|w| w.slo_id == "slo_high")
            .unwrap()
            .weight;
        let low_weight = adjustment
            .route_weights
            .iter()
            .find(|w| w.slo_id == "slo_low")
            .unwrap()
            .weight;
        assert!(
            high_weight > low_weight,
            "高优先级 SLO 权重 {high_weight} 应大于普通优先级 {low_weight}"
        );
    }

    #[test]
    fn test_threshold_boundary_zero() {
        let result = RouterConfig::new(0.0, vec![]);
        assert!(matches!(
            result,
            Err(SloDrivenError::InvalidRouterThreshold(0.0))
        ));
    }

    #[test]
    fn test_threshold_boundary_one() {
        let result = RouterConfig::new(1.0, vec![]);
        assert!(matches!(
            result,
            Err(SloDrivenError::InvalidRouterThreshold(1.0))
        ));
    }

    #[test]
    fn test_threshold_boundary_negative() {
        let result = RouterConfig::new(-0.1, vec![]);
        assert!(matches!(
            result,
            Err(SloDrivenError::InvalidRouterThreshold(_))
        ));
    }

    #[test]
    fn test_empty_achievements() {
        let config = RouterConfig::new(0.9, vec![]).unwrap();
        let router = SloDrivenRouter::new(config).unwrap();
        let adjustment = router.adjust(&[]).unwrap();
        assert!(adjustment.route_weights.is_empty());
    }

    #[test]
    fn test_weights_sum_to_one() {
        let config = RouterConfig::new(0.9, vec!["slo_a".to_string()]).unwrap();
        let router = SloDrivenRouter::new(config).unwrap();
        let achievements = vec![
            ("slo_a".to_string(), make_achievement(0.70)),
            ("slo_b".to_string(), make_achievement(0.80)),
            ("slo_c".to_string(), make_achievement(0.95)),
        ];
        let adjustment = router.adjust(&achievements).unwrap();
        let total: f64 = adjustment.route_weights.iter().map(|w| w.weight).sum();
        assert!((total - 1.0).abs() < 1e-9, "权重总和应为 1.0，实际 {total}");
    }
}
