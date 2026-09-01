//! 性能趋势预测器模块
//!
//! 基于历史性能时序数据预测未来性能趋势 + 建议干预时间点。
//! 例如"按当前增长速率，30 天后慢查询比例将达 20%"。

use serde::{Deserialize, Serialize};

/// 性能时序数据点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendDataPoint {
    /// 时间戳（Unix 秒）
    pub timestamp: u64,
    /// 指标值
    pub value: f64,
}

impl TrendDataPoint {
    /// 创建一个数据点
    pub fn new(timestamp: u64, value: f64) -> Self {
        Self { timestamp, value }
    }
}

/// 趋势预测方法
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrendMethod {
    /// 线性回归
    LinearRegression,
    /// 指数平滑
    ExponentialSmoothing,
    /// 移动平均
    MovingAverage,
}

/// 趋势预测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendPrediction {
    /// 预测方法
    pub method: TrendMethod,
    /// 斜率（每天变化量）
    pub slope_per_day: f64,
    /// 截距
    pub intercept: f64,
    /// 当前值
    pub current_value: f64,
    /// 预测 N 天后的值
    pub predicted_value: f64,
    /// 预测天数
    pub forecast_days: u32,
    /// 达到阈值的预测天数（None = 不会达到）
    pub days_to_threshold: Option<u32>,
    /// 预测达到阈值的时间点（Unix 秒，None = 不会达到）
    pub intervention_timestamp: Option<u64>,
    /// R² 拟合优度
    pub r_squared: f64,
    /// 预测说明
    pub summary: String,
}

/// 趋势预测器配置
#[derive(Debug, Clone)]
pub struct TrendPredictorConfig {
    /// 预测天数（默认 30）
    pub forecast_days: u32,
    /// 干预阈值（默认 0.2 = 20%）
    pub threshold: f64,
    /// 指数平滑系数（默认 0.3）
    pub smoothing_alpha: f64,
    /// 移动平均窗口大小（默认 7）
    pub moving_average_window: usize,
}

impl Default for TrendPredictorConfig {
    fn default() -> Self {
        Self {
            forecast_days: 30,
            threshold: 0.2,
            smoothing_alpha: 0.3,
            moving_average_window: 7,
        }
    }
}

/// 性能趋势预测器
///
/// 基于历史性能时序数据预测未来趋势 + 建议干预时间点。
pub struct TrendPredictor {
    config: TrendPredictorConfig,
}

impl Default for TrendPredictor {
    fn default() -> Self {
        Self::new(TrendPredictorConfig::default())
    }
}

impl TrendPredictor {
    /// 创建趋势预测器
    pub fn new(config: TrendPredictorConfig) -> Self {
        Self { config }
    }

    /// 预测趋势（线性回归）
    ///
    /// # 参数
    /// - `data`: 按时间排序的时序数据
    ///
    /// # 返回
    ///
    /// [`TrendPrediction`] 包含斜率、预测值、达到阈值天数等。
    pub fn predict_trend(&self, data: &[TrendDataPoint]) -> TrendPrediction {
        self.predict_linear(data)
    }

    fn predict_linear(&self, data: &[TrendDataPoint]) -> TrendPrediction {
        if data.len() < 2 {
            return TrendPrediction {
                method: TrendMethod::LinearRegression,
                slope_per_day: 0.0,
                intercept: 0.0,
                current_value: data.first().map(|d| d.value).unwrap_or(0.0),
                predicted_value: data.first().map(|d| d.value).unwrap_or(0.0),
                forecast_days: self.config.forecast_days,
                days_to_threshold: None,
                intervention_timestamp: None,
                r_squared: 0.0,
                summary: "数据不足，无法预测".to_string(),
            };
        }

        let first_ts = data.first().unwrap().timestamp as f64;
        let secs_per_day = 86400.0_f64;

        let x: Vec<f64> = data
            .iter()
            .map(|d| (d.timestamp as f64 - first_ts) / secs_per_day)
            .collect();
        let y: Vec<f64> = data.iter().map(|d| d.value).collect();

        let n = x.len() as f64;
        let sum_x: f64 = x.iter().sum();
        let sum_y: f64 = y.iter().sum();
        let sum_xy: f64 = x.iter().zip(&y).map(|(xi, yi)| xi * yi).sum();
        let sum_x2: f64 = x.iter().map(|xi| xi * xi).sum();

        let denominator = n * sum_x2 - sum_x * sum_x;
        let (slope, intercept) = if denominator.abs() < 1e-12 {
            (0.0, y[0])
        } else {
            let slope = (n * sum_xy - sum_x * sum_y) / denominator;
            let intercept = (sum_y - slope * sum_x) / n;
            (slope, intercept)
        };

        let current_value = *y.last().unwrap();
        let last_x = *x.last().unwrap();
        let predicted_x = last_x + self.config.forecast_days as f64;
        let predicted_value = slope * predicted_x + intercept;

        let r_squared = self.compute_r_squared(&x, &y, slope, intercept);

        let (days_to_threshold, intervention_timestamp) =
            self.compute_threshold_crossing(slope, current_value, data);

        let summary = self.build_summary(slope, current_value, predicted_value, days_to_threshold);

        TrendPrediction {
            method: TrendMethod::LinearRegression,
            slope_per_day: slope,
            intercept,
            current_value,
            predicted_value,
            forecast_days: self.config.forecast_days,
            days_to_threshold,
            intervention_timestamp,
            r_squared,
            summary,
        }
    }

    /// 使用指数平滑预测
    pub fn predict_exponential(&self, data: &[TrendDataPoint]) -> TrendPrediction {
        if data.is_empty() {
            return self.predict_trend(data);
        }

        let alpha = self.config.smoothing_alpha;
        let mut smoothed = data[0].value;
        for point in data.iter().skip(1) {
            smoothed = alpha * point.value + (1.0 - alpha) * smoothed;
        }

        let current_value = data.last().unwrap().value;
        let trend = current_value - smoothed;
        let predicted_value = smoothed + trend * self.config.forecast_days as f64;

        let (days_to_threshold, intervention_timestamp) =
            self.compute_threshold_crossing(trend, current_value, data);

        let summary = format!(
            "指数平滑预测（α={:.2}）：当前 {:.4}，{} 天后预测 {:.4}，趋势 {:+.6}/天",
            alpha, current_value, self.config.forecast_days, predicted_value, trend
        );

        TrendPrediction {
            method: TrendMethod::ExponentialSmoothing,
            slope_per_day: trend,
            intercept: smoothed,
            current_value,
            predicted_value,
            forecast_days: self.config.forecast_days,
            days_to_threshold,
            intervention_timestamp,
            r_squared: 0.0,
            summary,
        }
    }

    /// 使用移动平均预测
    pub fn predict_moving_average(&self, data: &[TrendDataPoint]) -> TrendPrediction {
        if data.is_empty() {
            return self.predict_trend(data);
        }

        let window = self.config.moving_average_window.min(data.len());
        let recent: Vec<f64> = data.iter().rev().take(window).map(|d| d.value).collect();
        let ma = recent.iter().sum::<f64>() / recent.len() as f64;

        let current_value = data.last().unwrap().value;
        let slope = if data.len() >= 2 {
            let prev = data[data.len() - 2].value;
            (current_value - prev) / 1.0
        } else {
            0.0
        };
        let predicted_value = ma + slope * self.config.forecast_days as f64;

        let summary = format!(
            "移动平均预测（窗口 {}）：当前 {:.4}，MA {:.4}，{} 天后预测 {:.4}",
            window, current_value, ma, self.config.forecast_days, predicted_value
        );

        TrendPrediction {
            method: TrendMethod::MovingAverage,
            slope_per_day: slope,
            intercept: ma,
            current_value,
            predicted_value,
            forecast_days: self.config.forecast_days,
            days_to_threshold: None,
            intervention_timestamp: None,
            r_squared: 0.0,
            summary,
        }
    }

    fn compute_r_squared(&self, x: &[f64], y: &[f64], slope: f64, intercept: f64) -> f64 {
        let mean_y: f64 = y.iter().sum::<f64>() / y.len() as f64;
        let ss_total: f64 = y.iter().map(|yi| (yi - mean_y).powi(2)).sum();
        if ss_total.abs() < 1e-12 {
            return 1.0;
        }
        let ss_residual: f64 = x
            .iter()
            .zip(y)
            .map(|(xi, yi)| (yi - (slope * xi + intercept)).powi(2))
            .sum();
        1.0 - ss_residual / ss_total
    }

    fn compute_threshold_crossing(
        &self,
        slope: f64,
        current_value: f64,
        data: &[TrendDataPoint],
    ) -> (Option<u32>, Option<u64>) {
        if slope <= 1e-9 || current_value >= self.config.threshold {
            return (None, None);
        }
        let days = ((self.config.threshold - current_value) / slope).ceil() as u32;
        if days == 0 || days > 365 * 10 {
            return (None, None);
        }
        let last_ts = data.last().unwrap().timestamp;
        let intervention_ts = last_ts + days as u64 * 86400;
        (Some(days), Some(intervention_ts))
    }

    fn build_summary(
        &self,
        slope: f64,
        current_value: f64,
        predicted_value: f64,
        days_to_threshold: Option<u32>,
    ) -> String {
        let trend_desc = if slope.abs() < 1e-9 {
            "平稳".to_string()
        } else if slope > 0.0 {
            format!("上升（{:+.6}/天）", slope)
        } else {
            format!("下降（{:+.6}/天）", slope)
        };

        let threshold_desc = match days_to_threshold {
            Some(days) => format!(
                "预计 {} 天后达到阈值 {:.0}%",
                days,
                self.config.threshold * 100.0
            ),
            None => "不会达到阈值".to_string(),
        };

        format!(
            "当前 {:.4}，趋势 {}，{} 天后预测 {:.4}，{}",
            current_value, trend_desc, self.config.forecast_days, predicted_value, threshold_desc
        )
    }
}
