//! 多 SLO 优先级仲裁器

use super::SloDrivenError;

/// SLO 动作请求
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SloAction {
    /// SLO 标识
    pub slo_id: String,
    /// 优先级（数值越大优先级越高）
    pub priority: u32,
    /// 权重
    pub weight: f64,
    /// 历史成功率 ∈ [0, 1]
    pub historical_success_rate: f64,
}

/// 仲裁结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArbitrationOutcome {
    /// 获胜的 SLO 标识
    pub winner: String,
    /// 仲裁理由
    pub rationale: String,
    /// 被牺牲的 SLO 列表
    pub sacrificed_slos: Vec<String>,
}

/// 多 SLO 优先级仲裁器
///
/// 按 `优先级 × 权重 × 历史成功率` 综合得分仲裁，得分最高者赢得资源。
pub struct SloArbitrator {
    /// 仲裁无法解决的最小得分差（低于此值视为并列）
    resolution_threshold: f64,
}

impl SloArbitrator {
    /// 创建仲裁器
    pub fn new() -> Self {
        Self {
            resolution_threshold: 1e-9,
        }
    }

    /// 仲裁多个冲突的 SLO 动作
    ///
    /// - 单个 SLO → 直接返回
    /// - 多个 SLO → 按综合得分仲裁
    /// - 得分并列 → 返回 `SloArbitrationUnresolved` 错误
    pub fn arbitrate(
        &self,
        conflicting_actions: &[SloAction],
    ) -> Result<ArbitrationOutcome, SloDrivenError> {
        if conflicting_actions.is_empty() {
            return Err(SloDrivenError::SloArbitrationUnresolved(
                "无冲突动作可仲裁".to_string(),
            ));
        }

        if conflicting_actions.len() == 1 {
            return Ok(ArbitrationOutcome {
                winner: conflicting_actions[0].slo_id.clone(),
                rationale: "仅单个 SLO，无需仲裁".to_string(),
                sacrificed_slos: vec![],
            });
        }

        // 综合得分 = 优先级 × 权重 × 历史成功率
        let scores: Vec<(usize, f64)> = conflicting_actions
            .iter()
            .enumerate()
            .map(|(i, action)| {
                let score =
                    f64::from(action.priority) * action.weight * action.historical_success_rate;
                (i, score)
            })
            .collect();

        let (winner_idx, winner_score) = scores
            .iter()
            .copied()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .ok_or_else(|| {
                SloDrivenError::SloArbitrationUnresolved("无法计算最高分".to_string())
            })?;

        // 检查并列
        let tied_count = scores
            .iter()
            .filter(|(_, s)| (s - winner_score).abs() < self.resolution_threshold)
            .count();

        if tied_count > 1 {
            let tied_ids: Vec<String> = scores
                .iter()
                .filter(|(_, s)| (s - winner_score).abs() < self.resolution_threshold)
                .map(|(i, _)| conflicting_actions[*i].slo_id.clone())
                .collect();
            return Err(SloDrivenError::SloArbitrationUnresolved(format!(
                "多个 SLO 得分并列：{tied_ids:?}"
            )));
        }

        let winner = conflicting_actions[winner_idx].slo_id.clone();
        let sacrificed_slos: Vec<String> = conflicting_actions
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != winner_idx)
            .map(|(_, action)| action.slo_id.clone())
            .collect();

        let rationale = format!(
            "SLO {} 综合得分 {:.4}（优先级 {} × 权重 {:.3} × 历史成功率 {:.3}）最高，赢得仲裁",
            winner,
            winner_score,
            conflicting_actions[winner_idx].priority,
            conflicting_actions[winner_idx].weight,
            conflicting_actions[winner_idx].historical_success_rate
        );

        Ok(ArbitrationOutcome {
            winner,
            rationale,
            sacrificed_slos,
        })
    }
}

impl Default for SloArbitrator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_action(slo_id: &str, priority: u32, weight: f64, success_rate: f64) -> SloAction {
        SloAction {
            slo_id: slo_id.to_string(),
            priority,
            weight,
            historical_success_rate: success_rate,
        }
    }

    #[test]
    fn test_single_slo_no_arbitration() {
        let arbitrator = SloArbitrator::new();
        let actions = vec![make_action("slo_a", 1, 1.0, 0.9)];
        let outcome = arbitrator.arbitrate(&actions).unwrap();
        assert_eq!(outcome.winner, "slo_a");
        assert!(outcome.sacrificed_slos.is_empty());
        assert!(outcome.rationale.contains("仅单个 SLO"));
    }

    #[test]
    fn test_priority_based_arbitration() {
        let arbitrator = SloArbitrator::new();
        let actions = vec![
            make_action("slo_low", 1, 1.0, 1.0),
            make_action("slo_high", 5, 1.0, 1.0),
        ];
        let outcome = arbitrator.arbitrate(&actions).unwrap();
        assert_eq!(outcome.winner, "slo_high");
        assert_eq!(outcome.sacrificed_slos, vec!["slo_low".to_string()]);
    }

    #[test]
    fn test_weighted_arbitration() {
        let arbitrator = SloArbitrator::new();
        // 同优先级、同成功率，但权重不同
        let actions = vec![
            make_action("slo_a", 2, 0.5, 1.0),
            make_action("slo_b", 2, 1.0, 1.0),
        ];
        let outcome = arbitrator.arbitrate(&actions).unwrap();
        assert_eq!(outcome.winner, "slo_b");
    }

    #[test]
    fn test_historical_success_rate_arbitration() {
        let arbitrator = SloArbitrator::new();
        // 同优先级、同权重，但历史成功率不同
        let actions = vec![
            make_action("slo_a", 2, 1.0, 0.8),
            make_action("slo_b", 2, 1.0, 0.95),
        ];
        let outcome = arbitrator.arbitrate(&actions).unwrap();
        assert_eq!(outcome.winner, "slo_b");
    }

    #[test]
    fn test_unresolvable_tie() {
        let arbitrator = SloArbitrator::new();
        let actions = vec![
            make_action("slo_a", 2, 1.0, 0.9),
            make_action("slo_b", 2, 1.0, 0.9),
        ];
        let result = arbitrator.arbitrate(&actions);
        assert!(matches!(
            result,
            Err(SloDrivenError::SloArbitrationUnresolved(_))
        ));
    }

    #[test]
    fn test_empty_actions_error() {
        let arbitrator = SloArbitrator::new();
        let result = arbitrator.arbitrate(&[]);
        assert!(matches!(
            result,
            Err(SloDrivenError::SloArbitrationUnresolved(_))
        ));
    }

    #[test]
    fn test_combined_score_arbitration() {
        let arbitrator = SloArbitrator::new();
        // slo_a: 3 × 0.8 × 0.9 = 2.16
        // slo_b: 2 × 1.0 × 1.0 = 2.0
        // slo_c: 1 × 1.0 × 1.0 = 1.0
        let actions = vec![
            make_action("slo_a", 3, 0.8, 0.9),
            make_action("slo_b", 2, 1.0, 1.0),
            make_action("slo_c", 1, 1.0, 1.0),
        ];
        let outcome = arbitrator.arbitrate(&actions).unwrap();
        assert_eq!(outcome.winner, "slo_a");
        assert_eq!(outcome.sacrificed_slos.len(), 2);
        assert!(outcome.sacrificed_slos.contains(&"slo_b".to_string()));
        assert!(outcome.sacrificed_slos.contains(&"slo_c".to_string()));
    }
}
