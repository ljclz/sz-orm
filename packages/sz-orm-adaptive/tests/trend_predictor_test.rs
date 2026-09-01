//! TASK-018: TrendPredictor 单元测试
//!
//! 验证基于历史性能时序数据的趋势预测 + 干预时间点。

use sz_orm_adaptive::{TrendDataPoint, TrendMethod, TrendPredictor, TrendPredictorConfig};

fn make_daily_data(start_value: f64, daily_increment: f64, days: usize) -> Vec<TrendDataPoint> {
    (0..days)
        .map(|i| TrendDataPoint::new(i as u64 * 86400, start_value + i as f64 * daily_increment))
        .collect()
}

#[test]
fn test_predict_trend_rising() {
    let predictor = TrendPredictor::default();
    let data = make_daily_data(0.05, 0.005, 30);

    let result = predictor.predict_trend(&data);

    assert_eq!(result.method, TrendMethod::LinearRegression);
    assert!(result.slope_per_day > 0.0);
    assert!(result.predicted_value > result.current_value);
}

#[test]
fn test_predict_trend_threshold_crossing() {
    let config = TrendPredictorConfig {
        forecast_days: 30,
        threshold: 0.2,
        smoothing_alpha: 0.3,
        moving_average_window: 7,
    };
    let predictor = TrendPredictor::new(config);
    let data = make_daily_data(0.05, 0.005, 30);

    let result = predictor.predict_trend(&data);

    assert!(result.days_to_threshold.is_some());
    let days = result.days_to_threshold.unwrap();
    assert!(days > 0);
    assert!(result.intervention_timestamp.is_some());
}

#[test]
fn test_predict_trend_no_threshold_crossing() {
    let predictor = TrendPredictor::default();
    let data = make_daily_data(0.05, -0.001, 30);

    let result = predictor.predict_trend(&data);

    assert!(result.slope_per_day < 0.0);
    assert!(result.days_to_threshold.is_none());
}

#[test]
fn test_predict_trend_stable() {
    let predictor = TrendPredictor::default();
    let data: Vec<TrendDataPoint> = (0..30)
        .map(|i| TrendDataPoint::new(i * 86400, 0.1))
        .collect();

    let result = predictor.predict_trend(&data);

    assert!(result.slope_per_day.abs() < 1e-6);
    assert!(result.predicted_value >= 0.0);
}

#[test]
fn test_predict_trend_empty_data() {
    let predictor = TrendPredictor::default();
    let result = predictor.predict_trend(&[]);

    assert_eq!(result.current_value, 0.0);
    assert!(result.summary.contains("数据不足"));
}

#[test]
fn test_predict_trend_single_point() {
    let predictor = TrendPredictor::default();
    let data = vec![TrendDataPoint::new(0, 0.1)];

    let result = predictor.predict_trend(&data);

    assert_eq!(result.current_value, 0.1);
    assert!(result.summary.contains("数据不足"));
}

#[test]
fn test_predict_trend_30_day_forecast() {
    let predictor = TrendPredictor::default();
    let data = make_daily_data(0.05, 0.003, 60);

    let result = predictor.predict_trend(&data);

    assert_eq!(result.forecast_days, 30);
    let expected_increase = 0.003 * 30.0;
    let actual_increase = result.predicted_value - result.current_value;
    assert!((actual_increase - expected_increase).abs() < 0.01);
}

#[test]
fn test_predict_trend_r_squared() {
    let predictor = TrendPredictor::default();
    let data = make_daily_data(0.05, 0.005, 30);

    let result = predictor.predict_trend(&data);

    assert!(result.r_squared > 0.99);
}

#[test]
fn test_predict_exponential_smoothing() {
    let predictor = TrendPredictor::default();
    let data = make_daily_data(0.05, 0.005, 30);

    let result = predictor.predict_exponential(&data);

    assert_eq!(result.method, TrendMethod::ExponentialSmoothing);
    assert!(result.predicted_value > 0.0);
}

#[test]
fn test_predict_moving_average() {
    let predictor = TrendPredictor::default();
    let data = make_daily_data(0.05, 0.005, 30);

    let result = predictor.predict_moving_average(&data);

    assert_eq!(result.method, TrendMethod::MovingAverage);
    assert!(result.predicted_value > 0.0);
}

#[test]
fn test_custom_config_threshold() {
    let config = TrendPredictorConfig {
        forecast_days: 60,
        threshold: 0.5,
        smoothing_alpha: 0.2,
        moving_average_window: 14,
    };
    let predictor = TrendPredictor::new(config);
    let data = make_daily_data(0.1, 0.01, 30);

    let result = predictor.predict_trend(&data);

    assert_eq!(result.forecast_days, 60);
    if let Some(days) = result.days_to_threshold {
        assert!(days > 0);
    }
}

#[test]
fn test_predict_trend_summary_contains_info() {
    let predictor = TrendPredictor::default();
    let data = make_daily_data(0.05, 0.005, 30);

    let result = predictor.predict_trend(&data);

    assert!(!result.summary.is_empty());
    assert!(result.summary.contains("当前"));
    assert!(result.summary.contains("趋势"));
}

#[test]
fn test_predict_trend_already_above_threshold() {
    let config = TrendPredictorConfig {
        forecast_days: 30,
        threshold: 0.2,
        smoothing_alpha: 0.3,
        moving_average_window: 7,
    };
    let predictor = TrendPredictor::new(config);
    let data = make_daily_data(0.25, 0.001, 30);

    let result = predictor.predict_trend(&data);

    assert!(result.current_value > 0.2);
    assert!(result.days_to_threshold.is_none());
}

#[test]
fn test_predict_trend_declining() {
    let predictor = TrendPredictor::default();
    let data = make_daily_data(0.15, -0.003, 30);

    let result = predictor.predict_trend(&data);

    assert!(result.slope_per_day < 0.0);
    assert!(result.predicted_value < result.current_value);
    assert!(result.days_to_threshold.is_none());
}
