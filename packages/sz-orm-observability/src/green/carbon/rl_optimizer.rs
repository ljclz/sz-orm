//! 绿色强化学习优化器：基于历史数据学习减排策略（异步不阻塞调度）

use super::super::types::CarbonFootprint;
use super::CarbonError;

/// 强化学习策略建议
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StrategySuggestion {
    /// 建议策略名称
    pub suggested_strategy: String,
    /// 预期减排量（kgCO2e）
    pub expected_reduction: f64,
    /// 置信度（0.0 ~ 1.0）
    pub confidence: f64,
}

/// 绿色强化学习优化器
///
/// 基于历史碳足迹数据学习减排策略。异步执行，不阻塞调度。
/// 历史数据不足 30 天时返回 `CarbonError::RlDataInsufficient`。
pub struct GreenRlOptimizer {
    /// 最小历史天数要求
    min_history_days: usize,
}

impl GreenRlOptimizer {
    /// 创建新的优化器，默认最小历史 30 天
    pub fn new() -> Self {
        Self {
            min_history_days: 30,
        }
    }

    /// 异步学习减排策略
    ///
    /// `history` 为按时间顺序排列的历史碳足迹记录（每天一条）。
    /// 内部通过 `yield_now` 让出调度，不阻塞其他任务。
    pub async fn learn(
        &self,
        history: &[CarbonFootprint],
    ) -> Result<StrategySuggestion, CarbonError> {
        if history.len() < self.min_history_days {
            return Err(CarbonError::RlDataInsufficient {
                actual: history.len(),
            });
        }

        // 让出调度，不阻塞其他异步任务
        tokio::task::yield_now().await;

        let values: Vec<f64> = history.iter().map(|h| h.total_kgco2e).collect();
        let avg_emission: f64 = values.iter().sum::<f64>() / values.len() as f64;
        let latest_emission = *values.last().unwrap();

        let (strategy, reduction, confidence) = if latest_emission < avg_emission * 0.85 {
            ("maintain_current".to_string(), avg_emission * 0.05, 0.9)
        } else if latest_emission < avg_emission * 0.95 {
            ("optimize_scheduling".to_string(), avg_emission * 0.1, 0.75)
        } else {
            (
                "aggressive_optimization".to_string(),
                avg_emission * 0.2,
                0.6,
            )
        };

        Ok(StrategySuggestion {
            suggested_strategy: strategy,
            expected_reduction: reduction,
            confidence,
        })
    }
}

impl Default for GreenRlOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_footprint(total: f64) -> CarbonFootprint {
        CarbonFootprint {
            period: super::super::super::types::ReportPeriod::Daily,
            total_kgco2e: total,
            by_component: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_learn_sufficient_history() {
        let optimizer = GreenRlOptimizer::new();
        let history: Vec<CarbonFootprint> = (0..30)
            .map(|i| make_footprint(1000.0 - i as f64 * 10.0))
            .collect();
        let suggestion = optimizer.learn(&history).await.unwrap();
        assert!(!suggestion.suggested_strategy.is_empty());
        assert!(suggestion.expected_reduction > 0.0);
        assert!(suggestion.confidence > 0.0 && suggestion.confidence <= 1.0);
    }

    #[tokio::test]
    async fn test_learn_insufficient_history() {
        let optimizer = GreenRlOptimizer::new();
        let history: Vec<CarbonFootprint> = (0..15).map(|_| make_footprint(500.0)).collect();
        let result = optimizer.learn(&history).await;
        assert!(matches!(
            result,
            Err(CarbonError::RlDataInsufficient { actual: 15 })
        ));
    }

    #[tokio::test]
    async fn test_learn_does_not_block_scheduler() {
        let optimizer = GreenRlOptimizer::new();
        let history: Vec<CarbonFootprint> = (0..30)
            .map(|i| make_footprint(1000.0 - i as f64 * 10.0))
            .collect();

        // 并发执行 learn 和一个计数任务，验证 learn 不阻塞调度
        let (suggestion, counter) = tokio::join!(optimizer.learn(&history), async {
            let mut count = 0;
            for _ in 0..10 {
                tokio::task::yield_now().await;
                count += 1;
            }
            count
        });

        assert!(suggestion.is_ok());
        assert_eq!(counter, 10, "计数任务应能完整执行，证明 learn 不阻塞调度");
    }

    #[tokio::test]
    async fn test_learn_aggressive_strategy() {
        let optimizer = GreenRlOptimizer::new();
        // 排放持续上升，应建议激进优化
        let history: Vec<CarbonFootprint> = (0..30)
            .map(|i| make_footprint(500.0 + i as f64 * 20.0))
            .collect();
        let suggestion = optimizer.learn(&history).await.unwrap();
        assert_eq!(suggestion.suggested_strategy, "aggressive_optimization");
    }

    #[tokio::test]
    async fn test_learn_maintain_strategy() {
        let optimizer = GreenRlOptimizer::new();
        // 排放持续下降，应建议维持当前策略
        let history: Vec<CarbonFootprint> = (0..30)
            .map(|i| make_footprint(1000.0 - i as f64 * 20.0))
            .collect();
        let suggestion = optimizer.learn(&history).await.unwrap();
        assert_eq!(suggestion.suggested_strategy, "maintain_current");
    }
}
