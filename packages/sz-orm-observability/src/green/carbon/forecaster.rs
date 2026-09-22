//! 碳中和预测：基于历史数据的三情景预测 + 置信度评估

use super::super::types::CarbonFootprint;
use super::CarbonError;

/// 预测情景结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ForecastScenario {
    /// 预测排放量（kgCO2e）
    pub predicted_kgco2e: f64,
    /// 预测描述
    pub description: String,
}

/// 碳中和预测报告
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ForecastReport {
    /// 乐观情景（减排加速）
    pub optimistic: ForecastScenario,
    /// 中性情景（按当前趋势）
    pub neutral: ForecastScenario,
    /// 悲观情景（趋势恶化）
    pub pessimistic: ForecastScenario,
    /// 置信度（0.0 ~ 1.0）
    pub confidence: f64,
    /// 告警列表
    pub alerts: Vec<String>,
}

/// 碳中和预测器
///
/// 基于历史碳足迹数据，使用线性回归外推生成三情景预测。
/// 趋势波动大时降低置信度并发出 `FORECAST_LOW_CONFIDENCE` 告警。
pub struct CarbonNeutralityForecaster {
    /// 低置信度告警阈值（变异系数超过此值时告警）
    low_confidence_threshold: f64,
}

impl CarbonNeutralityForecaster {
    /// 创建新的预测器，默认低置信度阈值为 0.3
    pub fn new() -> Self {
        Self {
            low_confidence_threshold: 0.3,
        }
    }

    /// 设置低置信度阈值
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.low_confidence_threshold = threshold;
        self
    }

    /// 基于历史数据预测未来排放
    ///
    /// `history` 为按时间顺序排列的历史碳足迹记录
    pub fn forecast(&self, history: &[CarbonFootprint]) -> Result<ForecastReport, CarbonError> {
        if history.len() < 2 {
            return Err(CarbonError::ForecastDataInsufficient {
                actual: history.len(),
            });
        }

        let values: Vec<f64> = history.iter().map(|h| h.total_kgco2e).collect();
        let n = values.len() as f64;

        // 线性回归：y = slope * x + intercept，x = 1..=n
        let indices: Vec<f64> = (1..=n as usize).map(|x| x as f64).collect();
        let sum_x: f64 = indices.iter().sum();
        let sum_y: f64 = values.iter().sum();
        let sum_xy: f64 = indices.iter().zip(values.iter()).map(|(x, y)| x * y).sum();
        let sum_x2: f64 = indices.iter().map(|x| x * x).sum();
        let denominator = n * sum_x2 - sum_x * sum_x;
        let slope = if denominator.abs() > 1e-12 {
            (n * sum_xy - sum_x * sum_y) / denominator
        } else {
            0.0
        };
        let intercept = (sum_y - slope * sum_x) / n;

        // 外推到未来一个周期
        let future_x = n + 1.0;
        let neutral_predicted = (slope * future_x + intercept).max(0.0);

        // 乐观情景：减排加速 10%
        let optimistic_predicted = (neutral_predicted * 0.9).max(0.0);
        // 悲观情景：趋势恶化 10%
        let pessimistic_predicted = (neutral_predicted * 1.1).max(0.0);

        // 置信度：基于残差变异系数
        let mean = sum_y / n;
        let residuals: Vec<f64> = indices
            .iter()
            .map(|x| slope * x + intercept)
            .zip(values.iter())
            .map(|(predicted, actual)| predicted - actual)
            .collect();
        let residual_variance: f64 = residuals.iter().map(|r| r * r).sum::<f64>() / n;
        let residual_stddev = residual_variance.sqrt();
        let cv = if mean.abs() > 1e-12 {
            residual_stddev / mean.abs()
        } else {
            1.0
        };
        let confidence = (1.0_f64 - cv.clamp(0.0, 1.0)).max(0.0);

        let mut alerts = Vec::new();
        if cv > self.low_confidence_threshold {
            alerts.push("FORECAST_LOW_CONFIDENCE".to_string());
        }

        Ok(ForecastReport {
            optimistic: ForecastScenario {
                predicted_kgco2e: optimistic_predicted,
                description: "减排加速情景（-10%）".to_string(),
            },
            neutral: ForecastScenario {
                predicted_kgco2e: neutral_predicted,
                description: "按当前趋势外推".to_string(),
            },
            pessimistic: ForecastScenario {
                predicted_kgco2e: pessimistic_predicted,
                description: "趋势恶化情景（+10%）".to_string(),
            },
            confidence,
            alerts,
        })
    }
}

impl Default for CarbonNeutralityForecaster {
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

    #[test]
    fn test_forecast_three_scenarios() {
        let forecaster = CarbonNeutralityForecaster::new();
        let history: Vec<CarbonFootprint> = (1..=10)
            .map(|i| make_footprint(1000.0 - i as f64 * 50.0))
            .collect();
        let report = forecaster.forecast(&history).unwrap();
        assert!(report.optimistic.predicted_kgco2e < report.neutral.predicted_kgco2e);
        assert!(report.neutral.predicted_kgco2e < report.pessimistic.predicted_kgco2e);
        assert!(
            (report.pessimistic.predicted_kgco2e - report.optimistic.predicted_kgco2e).abs() > 0.0
        );
    }

    #[test]
    fn test_forecast_low_confidence_alert() {
        let forecaster = CarbonNeutralityForecaster::new();
        let history = vec![
            make_footprint(1000.0),
            make_footprint(200.0),
            make_footprint(900.0),
            make_footprint(300.0),
            make_footprint(800.0),
            make_footprint(400.0),
        ];
        let report = forecaster.forecast(&history).unwrap();
        assert!(
            report
                .alerts
                .contains(&"FORECAST_LOW_CONFIDENCE".to_string()),
            "波动大时应发出低置信度告警"
        );
        assert!(report.confidence < 1.0);
    }

    #[test]
    fn test_forecast_empty_history() {
        let forecaster = CarbonNeutralityForecaster::new();
        let result = forecaster.forecast(&[]);
        assert!(matches!(
            result,
            Err(CarbonError::ForecastDataInsufficient { actual: 0 })
        ));
    }

    #[test]
    fn test_forecast_single_record() {
        let forecaster = CarbonNeutralityForecaster::new();
        let history = vec![make_footprint(500.0)];
        let result = forecaster.forecast(&history);
        assert!(matches!(
            result,
            Err(CarbonError::ForecastDataInsufficient { actual: 1 })
        ));
    }

    #[test]
    fn test_forecast_stable_trend_high_confidence() {
        let forecaster = CarbonNeutralityForecaster::new();
        let history: Vec<CarbonFootprint> = (1..=10)
            .map(|i| make_footprint(1000.0 - i as f64 * 50.0))
            .collect();
        let report = forecaster.forecast(&history).unwrap();
        assert!(
            report.confidence > 0.9,
            "稳定趋势应高置信度，实际: {}",
            report.confidence
        );
        assert!(report.alerts.is_empty());
    }
}
