//! SLO 达成率计算器

use super::types::{CalcWindow, SliMetrics, SloAchievement};
use super::SloAutomationError;

/// SLO 达成率计算器
pub struct SloAchievementCalculator {
    slo_target: f64,
}

impl SloAchievementCalculator {
    pub fn new(slo_target: f64) -> Self {
        Self { slo_target }
    }

    pub fn calculate(
        &self,
        metrics: &[SliMetrics],
        window: CalcWindow,
    ) -> Result<SloAchievement, SloAutomationError> {
        let min_count = match window {
            CalcWindow::OneHour => 10,
            CalcWindow::TwentyFourHours => 100,
            CalcWindow::SevenDays => 500,
            CalcWindow::TwentyDays => 1000,
        };

        if metrics.len() < min_count {
            return Ok(SloAchievement {
                window,
                achievement_rate: 0.0,
                sufficient_data: false,
            });
        }

        let avg_availability: f64 =
            metrics.iter().map(|m| m.availability).sum::<f64>() / metrics.len() as f64;
        let achievement_rate = avg_availability / self.slo_target;

        Ok(SloAchievement {
            window,
            achievement_rate: achievement_rate.min(1.0),
            sufficient_data: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_metrics(availability: f64) -> SliMetrics {
        SliMetrics {
            timestamp: 0,
            availability,
            latency_ms: 10.0,
            throughput: 100.0,
            correctness: availability,
        }
    }

    #[test]
    fn test_calculate_sufficient() {
        let calc = SloAchievementCalculator::new(0.999);
        let metrics: Vec<SliMetrics> = (0..200).map(|_| make_metrics(0.999)).collect();
        let result = calc
            .calculate(&metrics, CalcWindow::TwentyFourHours)
            .unwrap();
        assert!(result.sufficient_data);
        assert!(result.achievement_rate > 0.99);
    }

    #[test]
    fn test_calculate_insufficient() {
        let calc = SloAchievementCalculator::new(0.999);
        let metrics = vec![make_metrics(0.999)];
        let result = calc
            .calculate(&metrics, CalcWindow::TwentyFourHours)
            .unwrap();
        assert!(!result.sufficient_data);
    }
}
