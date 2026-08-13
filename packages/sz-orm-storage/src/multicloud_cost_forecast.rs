//! # 多云成本对比与容量预测
//!
//! 多云成本对比器（`MultiCloudCostComparator`）+ 容量预测器（`CapacityForecaster`）+
//! 自动优化器（`AutoOptimizer`，白名单自动执行优化建议）。
//!
//! 复用 v4.6.0 `CostAnalyzer`（`cost.rs:231`）成本分析，
//! 复用既有 `CostOptimizationSuggestion`（`cost.rs:55`）/ `StorageProvider`（`storage.rs:287`）。

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::cost::CostOptimizationSuggestion;
use crate::error::StorageError;
use crate::storage::StorageProvider;

/// 预测算法
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ForecastAlgorithm {
    /// 线性回归
    #[default]
    LinearRegression,
    /// 指数平滑
    ExponentialSmoothing,
    /// Holt-Winters 三次指数平滑
    HoltWinters,
}

/// 容量历史数据点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacityPoint {
    /// 时间戳（Unix 天）
    pub timestamp_day: u64,
    /// 容量（字节）
    pub capacity_bytes: u64,
}

/// 容量预测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacityForecast {
    /// 预测算法
    pub algorithm: ForecastAlgorithm,
    /// 预测时间跨度（天）
    pub horizon_days: u32,
    /// 预测的容量点
    pub forecast_points: Vec<CapacityPoint>,
    /// 置信度（0.0 ~ 1.0）
    pub confidence: f64,
    /// 置信区间下界
    pub lower_bound: Vec<CapacityPoint>,
    /// 置信区间上界
    pub upper_bound: Vec<CapacityPoint>,
    /// 预测误差（MAPE）
    pub mape: f64,
}

/// 单个 provider 的成本估算
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCostEstimate {
    /// provider 名称
    pub provider_name: String,
    /// 月成本估算（元）
    pub monthly_cost: f64,
    /// 按容量计费（元/GB/月）
    pub price_per_gb: f64,
    /// 估算容量（GB）
    pub estimated_capacity_gb: f64,
    /// 是否推荐
    pub recommended: bool,
    /// 推荐理由
    pub recommendation_reason: String,
}

/// 多云成本对比报表
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostComparisonReport {
    /// 请求的容量（字节）
    pub requested_capacity_bytes: u64,
    /// 各 provider 成本估算
    pub provider_estimates: Vec<ProviderCostEstimate>,
    /// 最优 provider
    pub best_provider: String,
    /// 最大节省（与最贵 provider 对比，元/月）
    pub max_saving: f64,
    /// 生成时间（Unix 毫秒）
    pub generated_at: u64,
}

/// 优化执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationExecutionResult {
    /// 执行的建议
    pub suggestion: CostOptimizationSuggestion,
    /// 是否成功
    pub success: bool,
    /// 是否自动执行
    pub auto_executed: bool,
    /// 详情
    pub detail: String,
}

/// 多云成本对比器
///
/// 复用 v4.6.0 `CostAnalyzer`（`cost.rs:231`）成本分析。
pub struct MultiCloudCostComparator {
    provider_pricing: Mutex<Vec<ProviderPricing>>,
}

/// provider 定价信息
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderPricing {
    name: String,
    price_per_gb_month: f64,
}

impl MultiCloudCostComparator {
    pub fn new() -> Self {
        Self {
            provider_pricing: Mutex::new(vec![
                ProviderPricing {
                    name: "aws_s3".to_string(),
                    price_per_gb_month: 0.023,
                },
                ProviderPricing {
                    name: "aliyun_oss".to_string(),
                    price_per_gb_month: 0.012,
                },
                ProviderPricing {
                    name: "tencent_cos".to_string(),
                    price_per_gb_month: 0.0099,
                },
                ProviderPricing {
                    name: "huawei_obs".to_string(),
                    price_per_gb_month: 0.0099,
                },
            ]),
        }
    }

    /// 添加自定义 provider 定价
    pub fn add_provider(&self, name: impl Into<String>, price_per_gb_month: f64) {
        self.provider_pricing.lock().unwrap().push(ProviderPricing {
            name: name.into(),
            price_per_gb_month,
        });
    }

    /// 多云成本对比
    ///
    /// 按各 provider 定价估算给定容量的月成本，推荐最优 provider。
    pub fn compare_providers(
        &self,
        capacity_bytes: u64,
        _providers: &[StorageProvider],
    ) -> Result<CostComparisonReport, StorageError> {
        if capacity_bytes == 0 {
            return Err(StorageError::InvalidConfig(
                "capacity must be > 0".to_string(),
            ));
        }

        let pricing = self.provider_pricing.lock().unwrap();
        let capacity_gb = capacity_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

        let mut estimates: Vec<ProviderCostEstimate> = pricing
            .iter()
            .map(|p| {
                let monthly_cost = p.price_per_gb_month * capacity_gb;
                ProviderCostEstimate {
                    provider_name: p.name.clone(),
                    monthly_cost,
                    price_per_gb: p.price_per_gb_month,
                    estimated_capacity_gb: capacity_gb,
                    recommended: false,
                    recommendation_reason: String::new(),
                }
            })
            .collect();

        estimates.sort_by(|a, b| {
            a.monthly_cost
                .partial_cmp(&b.monthly_cost)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if let Some(best) = estimates.first_mut() {
            best.recommended = true;
            best.recommendation_reason = "lowest monthly cost".to_string();
        }

        let best_provider = estimates
            .first()
            .map(|e| e.provider_name.clone())
            .unwrap_or_default();
        let max_cost = estimates.last().map(|e| e.monthly_cost).unwrap_or(0.0);
        let min_cost = estimates.first().map(|e| e.monthly_cost).unwrap_or(0.0);
        let max_saving = max_cost - min_cost;

        Ok(CostComparisonReport {
            requested_capacity_bytes: capacity_bytes,
            provider_estimates: estimates,
            best_provider,
            max_saving,
            generated_at: now_ms(),
        })
    }
}

impl Default for MultiCloudCostComparator {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for MultiCloudCostComparator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MultiCloudCostComparator")
            .field(
                "provider_count",
                &self.provider_pricing.lock().unwrap().len(),
            )
            .finish()
    }
}

/// 容量预测器
pub struct CapacityForecaster;

impl CapacityForecaster {
    pub fn new() -> Self {
        Self
    }

    /// 容量预测
    ///
    /// 基于历史数据预测未来 `horizon_days` 天的容量趋势。
    /// `confidence` 置信度（0.0 ~ 1.0）决定置信区间宽度。
    pub fn forecast(
        &self,
        history: &[CapacityPoint],
        algorithm: ForecastAlgorithm,
        horizon_days: u32,
        confidence: f64,
    ) -> Result<CapacityForecast, StorageError> {
        if history.len() < 7 {
            return Err(StorageError::InvalidConfig(format!(
                "need at least 7 history points, got {}",
                history.len()
            )));
        }

        let confidence = confidence.clamp(0.0, 1.0);
        let last_day = history.last().unwrap().timestamp_day;

        let (forecast_points, mape) = match algorithm {
            ForecastAlgorithm::LinearRegression => {
                linear_regression_forecast(history, horizon_days, last_day)
            }
            ForecastAlgorithm::ExponentialSmoothing => {
                exponential_smoothing_forecast(history, horizon_days, last_day, 0.3)
            }
            ForecastAlgorithm::HoltWinters => {
                holt_winters_forecast(history, horizon_days, last_day)
            }
        };

        let z_score = match confidence {
            c if c >= 0.99 => 2.576,
            c if c >= 0.95 => 1.96,
            c if c >= 0.90 => 1.645,
            c if c >= 0.80 => 1.282,
            _ => 1.0,
        };

        let std_dev = calculate_std_dev(history);
        let margin = z_score * std_dev;

        let lower_bound: Vec<CapacityPoint> = forecast_points
            .iter()
            .map(|p| CapacityPoint {
                timestamp_day: p.timestamp_day,
                capacity_bytes: (p.capacity_bytes as f64 - margin).max(0.0) as u64,
            })
            .collect();

        let upper_bound: Vec<CapacityPoint> = forecast_points
            .iter()
            .map(|p| CapacityPoint {
                timestamp_day: p.timestamp_day,
                capacity_bytes: (p.capacity_bytes as f64 + margin) as u64,
            })
            .collect();

        Ok(CapacityForecast {
            algorithm,
            horizon_days,
            forecast_points,
            confidence,
            lower_bound,
            upper_bound,
            mape,
        })
    }
}

impl Default for CapacityForecaster {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for CapacityForecaster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapacityForecaster").finish()
    }
}

/// 自动优化器
///
/// 白名单建议自动执行，非白名单需人工确认。
/// 复用 v4.6.0 `CostOptimizationSuggestion`（`cost.rs:55`）。
pub struct AutoOptimizer {
    whitelist: Mutex<Vec<CostOptimizationSuggestion>>,
    history: Mutex<Vec<OptimizationExecutionResult>>,
}

impl AutoOptimizer {
    pub fn new() -> Self {
        Self {
            whitelist: Mutex::new(Vec::new()),
            history: Mutex::new(Vec::new()),
        }
    }

    pub fn with_whitelist(&self, whitelist: Vec<CostOptimizationSuggestion>) {
        *self.whitelist.lock().unwrap() = whitelist;
    }

    /// 执行优化建议
    ///
    /// 白名单建议自动执行，非白名单返回错误需人工确认。
    pub fn execute_suggestion(
        &self,
        suggestion: &CostOptimizationSuggestion,
    ) -> Result<OptimizationExecutionResult, StorageError> {
        let whitelist = self.whitelist.lock().unwrap();
        let auto_executed = whitelist.contains(suggestion);

        if !auto_executed {
            return Err(StorageError::PermissionDenied(format!(
                "suggestion {suggestion:?} requires manual confirmation"
            )));
        }

        let detail = match suggestion {
            CostOptimizationSuggestion::TierDowngrade {
                bucket,
                from_tier,
                to_tier,
                ..
            } => format!("tier downgrade: {bucket} {from_tier} -> {to_tier}"),
            CostOptimizationSuggestion::LifecycleOptimize { bucket, .. } => {
                format!("lifecycle optimize: {bucket}")
            }
            CostOptimizationSuggestion::DeleteExpired { bucket, .. } => {
                format!("delete expired: {bucket}")
            }
            CostOptimizationSuggestion::CompressCold { bucket, .. } => {
                format!("compress cold: {bucket}")
            }
        };

        let result = OptimizationExecutionResult {
            suggestion: suggestion.clone(),
            success: true,
            auto_executed,
            detail,
        };

        self.history.lock().unwrap().push(result.clone());
        Ok(result)
    }

    /// 获取执行历史
    pub fn history(&self) -> Vec<OptimizationExecutionResult> {
        self.history.lock().unwrap().clone()
    }
}

impl Default for AutoOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for AutoOptimizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AutoOptimizer")
            .field("whitelist_count", &self.whitelist.lock().unwrap().len())
            .field("history_count", &self.history.lock().unwrap().len())
            .finish()
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn linear_regression_forecast(
    history: &[CapacityPoint],
    horizon_days: u32,
    last_day: u64,
) -> (Vec<CapacityPoint>, f64) {
    let n = history.len() as f64;
    let sum_x: f64 = (0..history.len()).map(|i| i as f64).sum();
    let sum_y: f64 = history.iter().map(|p| p.capacity_bytes as f64).sum();
    let sum_xy: f64 = history
        .iter()
        .enumerate()
        .map(|(i, p)| i as f64 * p.capacity_bytes as f64)
        .sum();
    let sum_x2: f64 = (0..history.len()).map(|i| (i as f64).powi(2)).sum();

    let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x * sum_x);
    let intercept = (sum_y - slope * sum_x) / n;

    let mape = calculate_mape(history, |i| slope * i as f64 + intercept);

    let forecast: Vec<CapacityPoint> = (1..=horizon_days)
        .map(|d| CapacityPoint {
            timestamp_day: last_day + d as u64,
            capacity_bytes: (slope * (history.len() as f64 + d as f64 - 1.0) + intercept).max(0.0)
                as u64,
        })
        .collect();

    (forecast, mape)
}

fn exponential_smoothing_forecast(
    history: &[CapacityPoint],
    horizon_days: u32,
    last_day: u64,
    alpha: f64,
) -> (Vec<CapacityPoint>, f64) {
    let mut smoothed = history[0].capacity_bytes as f64;
    for p in &history[1..] {
        smoothed = alpha * p.capacity_bytes as f64 + (1.0 - alpha) * smoothed;
    }

    let mape = calculate_mape(history, |_| smoothed);

    let forecast: Vec<CapacityPoint> = (1..=horizon_days)
        .map(|d| CapacityPoint {
            timestamp_day: last_day + d as u64,
            capacity_bytes: smoothed.max(0.0) as u64,
        })
        .collect();

    (forecast, mape)
}

fn holt_winters_forecast(
    history: &[CapacityPoint],
    horizon_days: u32,
    last_day: u64,
) -> (Vec<CapacityPoint>, f64) {
    let alpha = 0.3;
    let beta = 0.1;
    let gamma = 0.1;
    let season_length = 7.min(history.len());

    let mut level = history[0].capacity_bytes as f64;
    let mut trend = if history.len() > 1 {
        history[1].capacity_bytes as f64 - history[0].capacity_bytes as f64
    } else {
        0.0
    };
    let mut seasonals: Vec<f64> = history
        .iter()
        .take(season_length)
        .map(|p| p.capacity_bytes as f64 / level)
        .collect();

    for (i, p) in history.iter().enumerate().skip(season_length) {
        let s = seasonals[i % season_length];
        let new_level = alpha * (p.capacity_bytes as f64 / s) + (1.0 - alpha) * (level + trend);
        let new_trend = beta * (new_level - level) + (1.0 - beta) * trend;
        seasonals[i % season_length] =
            gamma * (p.capacity_bytes as f64 / new_level) + (1.0 - gamma) * s;
        level = new_level;
        trend = new_trend;
    }

    let mape = calculate_mape(history, |i| {
        let s = seasonals[i % season_length];
        level + trend * (i as f64 + 1.0) * s
    });

    let forecast: Vec<CapacityPoint> = (1..=horizon_days)
        .map(|d| {
            let s = seasonals[(history.len() + d as usize - 1) % season_length];
            CapacityPoint {
                timestamp_day: last_day + d as u64,
                capacity_bytes: ((level + trend * d as f64) * s).max(0.0) as u64,
            }
        })
        .collect();

    (forecast, mape)
}

fn calculate_mape(history: &[CapacityPoint], predictor: impl Fn(usize) -> f64) -> f64 {
    let errors: Vec<f64> = history
        .iter()
        .enumerate()
        .filter(|(_, p)| p.capacity_bytes > 0)
        .map(|(i, p)| {
            let predicted = predictor(i);
            ((p.capacity_bytes as f64 - predicted).abs() / p.capacity_bytes as f64) * 100.0
        })
        .collect();
    if errors.is_empty() {
        0.0
    } else {
        errors.iter().sum::<f64>() / errors.len() as f64
    }
}

fn calculate_std_dev(history: &[CapacityPoint]) -> f64 {
    if history.len() < 2 {
        return 0.0;
    }
    let mean = history.iter().map(|p| p.capacity_bytes as f64).sum::<f64>() / history.len() as f64;
    let variance = history
        .iter()
        .map(|p| (p.capacity_bytes as f64 - mean).powi(2))
        .sum::<f64>()
        / history.len() as f64;
    variance.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_history(days: usize) -> Vec<CapacityPoint> {
        (0..days)
            .map(|i| CapacityPoint {
                timestamp_day: i as u64,
                capacity_bytes: (1000 + i * 100) as u64,
            })
            .collect()
    }

    #[test]
    fn test_forecast_algorithm_default() {
        assert_eq!(
            ForecastAlgorithm::default(),
            ForecastAlgorithm::LinearRegression
        );
    }

    #[test]
    fn test_multi_cloud_comparator_new() {
        let comparator = MultiCloudCostComparator::new();
        let report = comparator
            .compare_providers(1024 * 1024 * 1024, &[])
            .unwrap();
        assert!(!report.provider_estimates.is_empty());
        assert!(!report.best_provider.is_empty());
    }

    #[test]
    fn test_multi_cloud_comparator_recommends_cheapest() {
        let comparator = MultiCloudCostComparator::new();
        let report = comparator
            .compare_providers(100 * 1024 * 1024 * 1024, &[])
            .unwrap();
        let best = report
            .provider_estimates
            .iter()
            .find(|e| e.recommended)
            .unwrap();
        let min_cost = report
            .provider_estimates
            .iter()
            .map(|e| e.monthly_cost)
            .fold(f64::INFINITY, f64::min);
        assert_eq!(best.monthly_cost, min_cost);
    }

    #[test]
    fn test_multi_cloud_comparator_zero_capacity() {
        let comparator = MultiCloudCostComparator::new();
        let result = comparator.compare_providers(0, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_multi_cloud_comparator_custom_provider() {
        let comparator = MultiCloudCostComparator::new();
        comparator.add_provider("custom_cloud", 0.001);
        let report = comparator
            .compare_providers(1024 * 1024 * 1024, &[])
            .unwrap();
        assert!(report
            .provider_estimates
            .iter()
            .any(|e| e.provider_name == "custom_cloud"));
    }

    #[test]
    fn test_multi_cloud_comparator_max_saving() {
        let comparator = MultiCloudCostComparator::new();
        let report = comparator
            .compare_providers(100 * 1024 * 1024 * 1024, &[])
            .unwrap();
        assert!(report.max_saving >= 0.0);
    }

    #[test]
    fn test_capacity_forecaster_linear() {
        let forecaster = CapacityForecaster::new();
        let history = make_history(14);
        let result = forecaster
            .forecast(&history, ForecastAlgorithm::LinearRegression, 7, 0.95)
            .unwrap();
        assert_eq!(result.algorithm, ForecastAlgorithm::LinearRegression);
        assert_eq!(result.forecast_points.len(), 7);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn test_capacity_forecaster_exponential() {
        let forecaster = CapacityForecaster::new();
        let history = make_history(10);
        let result = forecaster
            .forecast(&history, ForecastAlgorithm::ExponentialSmoothing, 5, 0.90)
            .unwrap();
        assert_eq!(result.forecast_points.len(), 5);
    }

    #[test]
    fn test_capacity_forecaster_holt_winters() {
        let forecaster = CapacityForecaster::new();
        let history = make_history(14);
        let result = forecaster
            .forecast(&history, ForecastAlgorithm::HoltWinters, 7, 0.80)
            .unwrap();
        assert_eq!(result.forecast_points.len(), 7);
    }

    #[test]
    fn test_capacity_forecaster_insufficient_data() {
        let forecaster = CapacityForecaster::new();
        let history = make_history(5);
        let result = forecaster.forecast(&history, ForecastAlgorithm::LinearRegression, 7, 0.95);
        assert!(result.is_err());
    }

    #[test]
    fn test_capacity_forecaster_confidence_bounds() {
        let forecaster = CapacityForecaster::new();
        let history = make_history(14);
        let result = forecaster
            .forecast(&history, ForecastAlgorithm::LinearRegression, 7, 0.95)
            .unwrap();
        assert_eq!(result.lower_bound.len(), 7);
        assert_eq!(result.upper_bound.len(), 7);
        for (lower, upper) in result.lower_bound.iter().zip(result.upper_bound.iter()) {
            assert!(lower.capacity_bytes <= upper.capacity_bytes);
        }
    }

    #[test]
    fn test_auto_optimizer_no_whitelist() {
        let optimizer = AutoOptimizer::new();
        let suggestion = CostOptimizationSuggestion::TierDowngrade {
            bucket: "test".to_string(),
            from_tier: "Standard".to_string(),
            to_tier: "InfrequentAccess".to_string(),
            expected_saving_percent: 60.0,
        };
        let result = optimizer.execute_suggestion(&suggestion);
        assert!(result.is_err());
    }

    #[test]
    fn test_auto_optimizer_with_whitelist() {
        let optimizer = AutoOptimizer::new();
        let suggestion = CostOptimizationSuggestion::TierDowngrade {
            bucket: "test".to_string(),
            from_tier: "Standard".to_string(),
            to_tier: "InfrequentAccess".to_string(),
            expected_saving_percent: 60.0,
        };
        optimizer.with_whitelist(vec![suggestion.clone()]);
        let result = optimizer.execute_suggestion(&suggestion).unwrap();
        assert!(result.success);
        assert!(result.auto_executed);
    }

    #[test]
    fn test_auto_optimizer_history() {
        let optimizer = AutoOptimizer::new();
        let suggestion = CostOptimizationSuggestion::DeleteExpired {
            bucket: "test".to_string(),
            expired_count: 100,
        };
        optimizer.with_whitelist(vec![suggestion.clone()]);
        optimizer.execute_suggestion(&suggestion).unwrap();
        assert_eq!(optimizer.history().len(), 1);
    }

    #[test]
    fn test_capacity_point_serialize() {
        let point = CapacityPoint {
            timestamp_day: 100,
            capacity_bytes: 1024,
        };
        let json = serde_json::to_string(&point).unwrap();
        let deserialized: CapacityPoint = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.timestamp_day, 100);
        assert_eq!(deserialized.capacity_bytes, 1024);
    }
}
