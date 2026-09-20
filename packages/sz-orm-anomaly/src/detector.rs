//! 异常检测模块：Welford 在线基线 + 突增/耗尽/偏离检测 + 严重级别判定
//!
//! Welford 算法在线计算均值/标准差，数值稳定，单次更新 O(1)。
//! 突增判定：均值 + Nσ 或绝对阈值。基线样本不足时回退绝对阈值。

use std::sync::Arc;

use crate::alert::{Alert, AnomalyType, Baseline, Severity};
use crate::collector::{ErrorType, MetricCollector, PoolMetric};
use crate::config::{AnomalyConfig, ConfigStore};
use crate::window::{current_timestamp_ms, SlidingWindow};

/// Welford 在线算法基线计算器
///
/// 仅维护 count/mean/M2 三个状态量，单次更新 O(1)，数值稳定。
/// 适合滑动窗口实时计算基线（均值 + 标准差）。
#[derive(Debug, Clone, Default)]
pub struct BaselineCalculator {
    count: u64,
    mean: f64,
    m2: f64,
}

impl BaselineCalculator {
    /// 创建空基线计算器
    pub fn new() -> Self {
        Self::default()
    }

    /// 从样本序列创建基线计算器
    pub fn from_samples(samples: &[f64]) -> Self {
        let mut calc = Self::new();
        for &s in samples {
            calc.add(s);
        }
        calc
    }

    /// 添加一个样本（Welford 在线更新）
    pub fn add(&mut self, value: f64) {
        self.count += 1;
        let delta = value - self.mean;
        self.mean += delta / self.count as f64;
        let delta2 = value - self.mean;
        self.m2 += delta * delta2;
    }

    /// 样本数
    pub fn count(&self) -> u64 {
        self.count
    }

    /// 均值
    pub fn mean(&self) -> f64 {
        self.mean
    }

    /// 方差（总体方差）
    pub fn variance(&self) -> f64 {
        if self.count < 2 {
            0.0
        } else {
            self.m2 / self.count as f64
        }
    }

    /// 样本方差（Bessel 校正）
    pub fn sample_variance(&self) -> f64 {
        if self.count < 2 {
            0.0
        } else {
            self.m2 / (self.count as f64 - 1.0)
        }
    }

    /// 标准差
    pub fn stddev(&self) -> f64 {
        self.variance().sqrt()
    }

    /// 样本标准差
    pub fn sample_stddev(&self) -> f64 {
        self.sample_variance().sqrt()
    }

    /// 获取基线信息
    pub fn baseline(&self) -> Baseline {
        Baseline {
            mean: self.mean,
            stddev: self.stddev(),
            sample_count: self.count as usize,
        }
    }

    /// 重置基线
    pub fn reset(&mut self) {
        self.count = 0;
        self.mean = 0.0;
        self.m2 = 0.0;
    }

    /// 检查样本是否充足
    pub fn is_sufficient(&self, min_samples: usize) -> bool {
        self.count as usize >= min_samples
    }
}

/// 突增检测器
///
/// 检测慢查询突增、错误率突增、连接池耗尽、偏离基线。
pub struct SpikeDetector {
    collector: MetricCollector,
    config_store: ConfigStore,
}

impl SpikeDetector {
    /// 创建突增检测器
    pub fn new(collector: MetricCollector, config_store: ConfigStore) -> Self {
        Self {
            collector,
            config_store,
        }
    }

    /// 获取指标采集器引用
    pub fn collector(&self) -> &MetricCollector {
        &self.collector
    }

    /// 获取配置存储引用
    pub fn config_store(&self) -> &ConfigStore {
        &self.config_store
    }

    /// 慢查询突增检测（REQ-ANM-007）
    ///
    /// 核心逻辑：计算滑动窗口内慢查询计数 → 与基线均值 + 3σ 比较 → 超过则输出告警。
    /// 基线样本不足时仅用绝对阈值（`slow_query_spike_count`）。
    pub fn check_slow_query_spike(&self) -> Option<Alert> {
        let config = self.config_store.get();
        let metrics = self.collector.slow_query_metrics();
        let now = current_timestamp_ms();
        let current_count = metrics.len() as f64;

        // 计算每个时间段的慢查询数作为基线样本（按 1 分钟分桶）
        let bucketed_counts = bucket_counts(&metrics, 60_000);
        let baseline_calc = BaselineCalculator::from_samples(&bucketed_counts);

        let (threshold, baseline) = if baseline_calc.is_sufficient(config.min_baseline_samples) {
            // 基线 + Nσ
            let b = baseline_calc.baseline();
            let thr = b.mean + config.slow_query_sigma * b.stddev;
            (thr, Some(b))
        } else {
            // 基线样本不足，仅用绝对阈值
            (config.slow_query_spike_count as f64, None)
        };

        if current_count > threshold {
            let severity = judge_severity(current_count, threshold);
            let suggestion = format!(
                "慢查询突增：当前 {} 次，阈值 {:.1} 次。建议检查索引覆盖、SQL 执行计划、连接池状态",
                current_count as u64, threshold
            );
            let sql_summary = metrics.last().map(|m| m.sql_summary.clone());
            Some(Alert {
                anomaly_type: AnomalyType::SlowQuerySpike,
                severity,
                timestamp: now,
                metric_value: current_count,
                threshold,
                baseline,
                suggestion,
                sql_summary,
            })
        } else {
            None
        }
    }

    /// 错误率突增检测（REQ-ANM-008）
    ///
    /// 核心逻辑：计算滑动窗口内错误率 → 与基线均值 + 3σ 或绝对阈值比较 → 超过则输出告警。
    pub fn check_error_rate_spike(&self) -> Option<Alert> {
        let config = self.config_store.get();
        let error_metrics = self.collector.error_metrics();
        let slow_query_metrics = self.collector.slow_query_metrics();
        let now = current_timestamp_ms();

        // 错误率 = 错误数 / (错误数 + 查询数)
        // 这里用慢查询数作为查询数的代理（因为指标采集器只采集慢查询和错误）
        let total_queries = error_metrics.len() + slow_query_metrics.len();
        if total_queries == 0 {
            return None;
        }
        let error_count = error_metrics.len() as f64;
        let current_rate = error_count / total_queries as f64;

        // 计算分桶错误率作为基线样本
        let error_buckets = bucket_error_rates(&error_metrics, &slow_query_metrics, 60_000);
        let baseline_calc = BaselineCalculator::from_samples(&error_buckets);

        let (threshold, baseline) = if baseline_calc.is_sufficient(config.min_baseline_samples) {
            let b = baseline_calc.baseline();
            let thr = b.mean + config.error_rate_sigma * b.stddev;
            (thr.min(config.error_rate_threshold), Some(b))
        } else {
            (config.error_rate_threshold, None)
        };

        if current_rate > threshold {
            let severity = judge_severity(current_rate, threshold);
            let suggestion = format!(
                "错误率突增：当前 {:.2}%，阈值 {:.2}%。建议检查数据库连接、SQL 语法、超时配置",
                current_rate * 100.0,
                threshold * 100.0
            );
            Some(Alert {
                anomaly_type: AnomalyType::ErrorRateSpike,
                severity,
                timestamp: now,
                metric_value: current_rate,
                threshold,
                baseline,
                suggestion,
                sql_summary: None,
            })
        } else {
            None
        }
    }

    /// 连接池耗尽检测（REQ-ANM-009）
    ///
    /// 核心逻辑：活跃数=上限 且 等待数>阈值 或 等待耗时>阈值 → 输出告警。
    pub fn check_pool_exhaustion(&self) -> Option<Alert> {
        let config = self.config_store.get();
        let pool_metrics = self.collector.pool_metrics();
        let now = current_timestamp_ms();

        // 检查最近的连接池指标
        let recent: Vec<&PoolMetric> = pool_metrics.iter().rev().take(10).collect();
        if recent.is_empty() {
            return None;
        }

        // 检查是否有活跃=上限 且 等待>阈值 或 等待耗时>阈值
        let mut max_waiting: u32 = 0;
        let mut max_acquire_ms: u64 = 0;
        let mut pool_full_count = 0;
        for metric in &recent {
            if metric.active >= config.pool_max_connections {
                pool_full_count += 1;
            }
            if metric.waiting > max_waiting {
                max_waiting = metric.waiting;
            }
            if metric.acquire_ms > max_acquire_ms {
                max_acquire_ms = metric.acquire_ms;
            }
        }

        let wait_exceeded = max_waiting > config.pool_wait_count_threshold;
        let time_exceeded = max_acquire_ms > config.pool_wait_time_threshold_ms;
        let pool_full = pool_full_count > 0;

        if pool_full && (wait_exceeded || time_exceeded) {
            let metric_value = if wait_exceeded {
                max_waiting as f64
            } else {
                max_acquire_ms as f64
            };
            let threshold = if wait_exceeded {
                config.pool_wait_count_threshold as f64
            } else {
                config.pool_wait_time_threshold_ms as f64
            };
            let severity = judge_severity(metric_value, threshold);
            let suggestion = format!(
                "连接池耗尽：活跃={}（上限 {}），等待 {}（阈值 {}），获取耗时 {}ms（阈值 {}ms）。建议增大连接池或优化长事务",
                recent[0].active,
                config.pool_max_connections,
                max_waiting,
                config.pool_wait_count_threshold,
                max_acquire_ms,
                config.pool_wait_time_threshold_ms
            );
            Some(Alert {
                anomaly_type: AnomalyType::PoolExhaustion,
                severity,
                timestamp: now,
                metric_value,
                threshold,
                baseline: None,
                suggestion,
                sql_summary: None,
            })
        } else {
            None
        }
    }

    /// 偏离基线检测（REQ-ANM-010）
    ///
    /// 核心逻辑：检测平均耗时连续 N 窗口上升 → 输出告警。
    pub fn check_baseline_drift(&self) -> Option<Alert> {
        let config = self.config_store.get();
        let metrics = self.collector.slow_query_metrics();
        let now = current_timestamp_ms();

        if metrics.is_empty() {
            return None;
        }

        // 按 1 分钟分桶计算平均耗时
        let bucketed_avg = bucket_average_elapsed(&metrics, 60_000);
        if bucketed_avg.len() < config.baseline_drift_windows + 1 {
            return None;
        }

        // 检查最近 N 个窗口是否连续上升
        let recent_count = config.baseline_drift_windows;
        let start = bucketed_avg.len() - recent_count - 1;
        let recent = &bucketed_avg[start..];
        let mut continuous_rise = true;
        for i in 1..recent.len() {
            if recent[i] <= recent[i - 1] {
                continuous_rise = false;
                break;
            }
        }

        if continuous_rise {
            let current_avg = recent[recent.len() - 1];
            let baseline_avg = recent[0];
            let threshold = baseline_avg * 1.5; // 上升 50% 视为偏离
            let severity = judge_severity(current_avg, threshold);
            let suggestion = format!(
                "基线偏离：平均耗时连续 {} 窗口上升，从 {:.1}ms 升至 {:.1}ms。建议检查数据量增长、索引退化、统计信息过期",
                recent_count, baseline_avg, current_avg
            );
            Some(Alert {
                anomaly_type: AnomalyType::BaselineDrift,
                severity,
                timestamp: now,
                metric_value: current_avg,
                threshold,
                baseline: Some(Baseline {
                    mean: baseline_avg,
                    stddev: 0.0,
                    sample_count: recent_count,
                }),
                suggestion,
                sql_summary: None,
            })
        } else {
            None
        }
    }

    /// 统一检测入口：调用四类检测 + 严重级别判定
    pub fn detect_anomalies(&self) -> Vec<Alert> {
        let mut alerts = Vec::new();
        if let Some(alert) = self.check_slow_query_spike() {
            alerts.push(alert);
        }
        if let Some(alert) = self.check_error_rate_spike() {
            alerts.push(alert);
        }
        if let Some(alert) = self.check_pool_exhaustion() {
            alerts.push(alert);
        }
        if let Some(alert) = self.check_baseline_drift() {
            alerts.push(alert);
        }
        alerts
    }
}

/// 严重级别判定（REQ-ANM-011）
///
/// 规则：超阈值 1.5x → WARN，超 3x → CRITICAL，其他 → INFO
pub fn judge_severity(metric_value: f64, threshold: f64) -> Severity {
    if threshold <= 0.0 {
        return Severity::Info;
    }
    let ratio = metric_value / threshold;
    if ratio >= 3.0 {
        Severity::Critical
    } else if ratio >= 1.5 {
        Severity::Warn
    } else {
        Severity::Info
    }
}

/// 按时间桶分桶计数
fn bucket_counts(metrics: &[crate::collector::SlowQueryMetric], bucket_ms: u64) -> Vec<f64> {
    if metrics.is_empty() {
        return Vec::new();
    }
    let mut buckets: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
    for m in metrics {
        let bucket = m.timestamp / bucket_ms;
        *buckets.entry(bucket).or_insert(0) += 1;
    }
    let mut sorted: Vec<(u64, u64)> = buckets.into_iter().collect();
    sorted.sort_by_key(|(b, _)| *b);
    sorted.into_iter().map(|(_, c)| c as f64).collect()
}

/// 按时间桶计算错误率
fn bucket_error_rates(
    error_metrics: &[crate::collector::ErrorMetric],
    slow_query_metrics: &[crate::collector::SlowQueryMetric],
    bucket_ms: u64,
) -> Vec<f64> {
    let mut error_buckets: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
    for m in error_metrics {
        let bucket = m.timestamp / bucket_ms;
        *error_buckets.entry(bucket).or_insert(0) += 1;
    }
    let mut query_buckets: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
    for m in slow_query_metrics {
        let bucket = m.timestamp / bucket_ms;
        *query_buckets.entry(bucket).or_insert(0) += 1;
    }
    let mut all_buckets: std::collections::HashSet<u64> = std::collections::HashSet::new();
    all_buckets.extend(error_buckets.keys());
    all_buckets.extend(query_buckets.keys());
    let mut sorted: Vec<u64> = all_buckets.into_iter().collect();
    sorted.sort();
    sorted
        .into_iter()
        .map(|b| {
            let errors = error_buckets.get(&b).copied().unwrap_or(0) as f64;
            let queries = query_buckets.get(&b).copied().unwrap_or(0) as f64;
            let total = errors + queries;
            if total > 0.0 {
                errors / total
            } else {
                0.0
            }
        })
        .collect()
}

/// 按时间桶计算平均耗时
fn bucket_average_elapsed(
    metrics: &[crate::collector::SlowQueryMetric],
    bucket_ms: u64,
) -> Vec<f64> {
    if metrics.is_empty() {
        return Vec::new();
    }
    let mut buckets: std::collections::HashMap<u64, (u64, u64)> = std::collections::HashMap::new();
    for m in metrics {
        let bucket = m.timestamp / bucket_ms;
        let entry = buckets.entry(bucket).or_insert((0, 0));
        entry.0 += m.elapsed_ms;
        entry.1 += 1;
    }
    let mut sorted: Vec<(u64, (u64, u64))> = buckets.into_iter().collect();
    sorted.sort_by_key(|(b, _)| *b);
    sorted
        .into_iter()
        .map(|(_, (total, count))| {
            if count > 0 {
                total as f64 / count as f64
            } else {
                0.0
            }
        })
        .collect()
}

/// 异常检测器（统一入口）
///
/// 集成指标采集 + 滑动窗口 + 突增/耗尽检测 + 告警输出。
pub struct AnomalyDetector {
    detector: SpikeDetector,
    emitter: crate::alert::AlertEmitter,
}

impl AnomalyDetector {
    /// 创建异常检测器
    pub fn new(config: AnomalyConfig) -> Self {
        let config_store = ConfigStore::new(config.clone());
        let window = Arc::new(SlidingWindow::new(config.window_size));
        let collector = MetricCollector::new(window);
        let detector = SpikeDetector::new(collector, config_store);
        let emitter = crate::alert::AlertEmitter::from_config(&ConfigStore::new(config));
        Self { detector, emitter }
    }

    /// 从配置存储创建
    pub fn with_config_store(config_store: ConfigStore) -> Self {
        let config = config_store.get();
        let window = Arc::new(SlidingWindow::new(config.window_size));
        let collector = MetricCollector::new(window);
        let detector = SpikeDetector::new(collector, config_store);
        let emitter = crate::alert::AlertEmitter::from_config(&ConfigStore::new(config));
        Self { detector, emitter }
    }

    /// 记录慢查询指标
    pub fn record_slow_query(&self, elapsed_ms: u64, sql_summary: &str, timestamp: u64) {
        self.detector
            .collector()
            .record_slow_query(elapsed_ms, sql_summary, timestamp);
    }

    /// 记录查询错误指标
    pub fn record_error(&self, error_type: ErrorType, timestamp: u64) {
        self.detector
            .collector()
            .record_error(error_type, timestamp);
    }

    /// 记录连接池指标
    pub fn record_pool_usage(
        &self,
        active: u32,
        idle: u32,
        waiting: u32,
        acquire_ms: u64,
        timestamp: u64,
    ) {
        self.detector
            .collector()
            .record_pool_usage(active, idle, waiting, acquire_ms, timestamp);
    }

    /// 执行异常检测（含告警去重）
    ///
    /// 返回已发出的告警列表（被去重抑制的告警不包含在内）。
    pub fn detect_anomalies(&self) -> Vec<Alert> {
        let raw_alerts = self.detector.detect_anomalies();
        let mut emitted = Vec::new();
        for alert in raw_alerts {
            if let Some(a) = self.emitter.emit(alert) {
                emitted.push(a);
            }
        }
        emitted
    }

    /// 执行异常检测（不去重，用于测试/调试）
    pub fn detect_anomalies_raw(&self) -> Vec<Alert> {
        self.detector.detect_anomalies()
    }

    /// 订阅告警
    pub fn subscribe_alerts(
        &self,
        callback: crate::alert::AlertCallback,
    ) -> crate::alert::SubscriptionId {
        self.emitter.subscribe(callback)
    }

    /// 取消订阅
    pub fn unsubscribe_alerts(&self, id: crate::alert::SubscriptionId) -> bool {
        self.emitter.unsubscribe(id)
    }

    /// 热更新配置
    pub fn update_config(&self, new_config: AnomalyConfig) {
        self.detector.config_store().update(new_config);
    }

    /// 获取当前配置
    pub fn get_config(&self) -> AnomalyConfig {
        self.detector.config_store().get()
    }

    /// 获取指标采集器引用
    pub fn collector(&self) -> &MetricCollector {
        self.detector.collector()
    }

    /// 获取告警输出器引用
    pub fn emitter(&self) -> &crate::alert::AlertEmitter {
        &self.emitter
    }

    /// 获取突增检测器引用
    pub fn detector(&self) -> &SpikeDetector {
        &self.detector
    }

    /// 累计发出的告警数
    pub fn emitted_count(&self) -> u64 {
        self.emitter.emitted_count()
    }

    /// 被抑制的告警数
    pub fn suppressed_count(&self) -> u64 {
        self.emitter.suppressed_count()
    }

    /// 获取历史告警
    pub fn alert_history(&self) -> Vec<Alert> {
        self.emitter.history()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn new_detector() -> AnomalyDetector {
        let config = AnomalyConfig::default()
            .with_window_size(Duration::from_secs(60))
            .with_alert_cooldown(Duration::from_millis(0))
            .with_min_baseline_samples(5)
            .with_slow_query_spike_count(5);
        AnomalyDetector::new(config)
    }

    #[test]
    fn test_welford_mean_stddev() {
        let mut calc = BaselineCalculator::new();
        let samples = [10.0, 20.0, 30.0, 40.0, 50.0];
        for &s in &samples {
            calc.add(s);
        }
        // 均值 = 30
        assert!((calc.mean() - 30.0).abs() < 1e-9);
        // 标准差 = sqrt(200) ≈ 14.14
        assert!((calc.stddev() - 200.0_f64.sqrt()).abs() < 1e-9);
        assert_eq!(calc.count(), 5);
    }

    #[test]
    fn test_welford_insufficient_samples() {
        let calc = BaselineCalculator::new();
        assert!(!calc.is_sufficient(100));
    }

    #[test]
    fn test_welford_single_sample() {
        let mut calc = BaselineCalculator::new();
        calc.add(42.0);
        assert!((calc.mean() - 42.0).abs() < 1e-9);
        assert_eq!(calc.stddev(), 0.0);
    }

    #[test]
    fn test_welford_reset() {
        let mut calc = BaselineCalculator::new();
        calc.add(10.0);
        calc.add(20.0);
        calc.reset();
        assert_eq!(calc.count(), 0);
        assert_eq!(calc.mean(), 0.0);
    }

    #[test]
    fn test_judge_severity_critical() {
        assert_eq!(judge_severity(300.0, 100.0), Severity::Critical);
        assert_eq!(judge_severity(301.0, 100.0), Severity::Critical);
    }

    #[test]
    fn test_judge_severity_warn() {
        assert_eq!(judge_severity(150.0, 100.0), Severity::Warn);
        assert_eq!(judge_severity(200.0, 100.0), Severity::Warn);
    }

    #[test]
    fn test_judge_severity_info() {
        assert_eq!(judge_severity(100.0, 100.0), Severity::Info);
        assert_eq!(judge_severity(110.0, 100.0), Severity::Info);
    }

    #[test]
    fn test_judge_severity_zero_threshold() {
        assert_eq!(judge_severity(100.0, 0.0), Severity::Info);
    }

    #[test]
    fn test_slow_query_spike_detection() {
        let detector = new_detector();
        let now = current_timestamp_ms();
        // 注入大量慢查询触发突增
        for i in 0..20 {
            detector.record_slow_query(150, "SELECT * FROM users WHERE id = ?", now + i);
        }
        let alert = detector.detector().check_slow_query_spike();
        assert!(alert.is_some());
        let alert = alert.unwrap();
        assert_eq!(alert.anomaly_type, AnomalyType::SlowQuerySpike);
        assert!(alert.metric_value > alert.threshold);
    }

    #[test]
    fn test_slow_query_no_spike() {
        let detector = new_detector();
        let now = current_timestamp_ms();
        // 仅少量慢查询，不触发突增
        for i in 0..3 {
            detector.record_slow_query(150, "SELECT * FROM users WHERE id = ?", now + i);
        }
        let alert = detector.detector().check_slow_query_spike();
        assert!(alert.is_none());
    }

    #[test]
    fn test_error_rate_spike_detection() {
        let detector = new_detector();
        let now = current_timestamp_ms();
        // 注入大量错误触发错误率突增
        for i in 0..10 {
            detector.record_error(ErrorType::SqlError, now + i);
        }
        for i in 0..2 {
            detector.record_slow_query(150, "SELECT 1", now + i);
        }
        let alert = detector.detector().check_error_rate_spike();
        assert!(alert.is_some());
        let alert = alert.unwrap();
        assert_eq!(alert.anomaly_type, AnomalyType::ErrorRateSpike);
    }

    #[test]
    fn test_pool_exhaustion_detection() {
        let config = AnomalyConfig::default()
            .with_window_size(Duration::from_secs(60))
            .with_alert_cooldown(Duration::from_millis(0))
            .with_pool_max_connections(50)
            .with_pool_wait_count_threshold(10);
        let detector = AnomalyDetector::new(config);
        let now = current_timestamp_ms();
        // 活跃=上限 且 等待>阈值
        detector.record_pool_usage(50, 0, 15, 1200, now);
        let alert = detector.detector().check_pool_exhaustion();
        assert!(alert.is_some());
        let alert = alert.unwrap();
        assert_eq!(alert.anomaly_type, AnomalyType::PoolExhaustion);
    }

    #[test]
    fn test_pool_no_exhaustion() {
        let detector = new_detector();
        let now = current_timestamp_ms();
        // 活跃未达上限
        detector.record_pool_usage(30, 20, 5, 100, now);
        let alert = detector.detector().check_pool_exhaustion();
        assert!(alert.is_none());
    }

    #[test]
    fn test_detect_anomalies() {
        let detector = new_detector();
        let now = current_timestamp_ms();
        for i in 0..20 {
            detector.record_slow_query(150, "SELECT * FROM users WHERE id = ?", now + i);
        }
        detector.record_pool_usage(50, 0, 15, 1200, now);
        let alerts = detector.detect_anomalies();
        assert!(!alerts.is_empty());
    }

    #[test]
    fn test_anomaly_detector_config_update() {
        let detector = new_detector();
        let initial = detector.get_config();
        assert_eq!(initial.slow_query_threshold_ms, 100);
        detector.update_config(AnomalyConfig::default().with_slow_query_threshold_ms(200));
        let updated = detector.get_config();
        assert_eq!(updated.slow_query_threshold_ms, 200);
    }

    #[test]
    fn test_baseline_drift_detection() {
        let config = AnomalyConfig::default()
            .with_window_size(Duration::from_secs(60))
            .with_alert_cooldown(Duration::from_millis(0))
            .with_min_baseline_samples(5)
            .with_slow_query_spike_count(100);
        let detector = AnomalyDetector::new(config);
        let now = current_timestamp_ms();
        // 模拟连续 4 个窗口平均耗时上升
        for bucket in 0..4 {
            let elapsed = 100 + bucket * 50;
            for i in 0..5 {
                detector.record_slow_query(elapsed, "SELECT * FROM t", now + bucket * 60_000 + i);
            }
        }
        let alert = detector.detector().check_baseline_drift();
        assert!(alert.is_some());
        let alert = alert.unwrap();
        assert_eq!(alert.anomaly_type, AnomalyType::BaselineDrift);
    }
}
#[derive(Debug, Clone, Default)]
pub struct AccuracyEvaluator {
    pub true_positives: u64,
    pub false_positives: u64,
    pub true_negatives: u64,
    pub false_negatives: u64,
}

#[derive(Debug, Clone)]
pub struct AccuracyReport {
    pub false_positive_rate: f64,
    pub false_negative_rate: f64,
    pub precision: f64,
    pub recall: f64,
    pub f1_score: f64,
    pub needs_recalibration: bool,
}

impl AccuracyEvaluator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, predicted_anomaly: bool, actual_anomaly: bool) {
        match (predicted_anomaly, actual_anomaly) {
            (true, true) => self.true_positives += 1,
            (true, false) => self.false_positives += 1,
            (false, true) => self.false_negatives += 1,
            (false, false) => self.true_negatives += 1,
        }
    }

    pub fn evaluate(&self) -> AccuracyReport {
        let fp_rate = if self.false_positives + self.true_negatives > 0 {
            self.false_positives as f64 / (self.false_positives + self.true_negatives) as f64
        } else {
            0.0
        };
        let fn_rate = if self.false_negatives + self.true_positives > 0 {
            self.false_negatives as f64 / (self.false_negatives + self.true_positives) as f64
        } else {
            0.0
        };
        let precision = if self.true_positives + self.false_positives > 0 {
            self.true_positives as f64 / (self.true_positives + self.false_positives) as f64
        } else {
            0.0
        };
        let recall = if self.true_positives + self.false_negatives > 0 {
            self.true_positives as f64 / (self.true_positives + self.false_negatives) as f64
        } else {
            0.0
        };
        let f1 = if precision + recall > 0.0 {
            2.0 * precision * recall / (precision + recall)
        } else {
            0.0
        };
        AccuracyReport {
            false_positive_rate: fp_rate,
            false_negative_rate: fn_rate,
            precision,
            recall,
            f1_score: f1,
            needs_recalibration: fp_rate > 0.05 || fn_rate > 0.05,
        }
    }
}
/// v7.6.0 多维度联合指标
#[derive(Debug, Clone)]
pub struct MultiDimMetrics {
    /// 当前 QPS
    pub qps: f64,
    /// P95 延迟（毫秒）
    pub latency_p95_ms: f64,
    /// 错误率（0.0~1.0）
    pub error_rate: f64,
    /// 连接池利用率（0.0~1.0）
    pub pool_utilization: f64,
}

/// v7.6.0 多维度联合异常检测结果
#[derive(Debug, Clone)]
pub struct AnomalyDetectionResult {
    pub anomaly_type: AnomalyType,
    pub is_anomaly: bool,
    pub severity: Severity,
    pub detail: String,
    pub detected_dimensions: Vec<String>,
}

/// v7.6.0 多维度联合异常检测器
///
/// 在同一滑动窗口内对 QPS + 延迟 + 错误率 + 连接池利用率
/// 执行多维度联合判定，减少单维度误报。
pub struct MultiDimDetector {
    dimensions: Vec<AnomalyType>,
    qps_baseline: BaselineCalculator,
    latency_baseline: BaselineCalculator,
    error_baseline: BaselineCalculator,
    pool_baseline: BaselineCalculator,
}

impl MultiDimDetector {
    pub fn new(dimensions: Vec<AnomalyType>) -> Self {
        Self {
            dimensions,
            qps_baseline: BaselineCalculator::new(),
            latency_baseline: BaselineCalculator::new(),
            error_baseline: BaselineCalculator::new(),
            pool_baseline: BaselineCalculator::new(),
        }
    }

    /// 更新基线样本
    pub fn update_baselines(&mut self, metrics: &MultiDimMetrics) {
        self.qps_baseline.add(metrics.qps);
        self.latency_baseline.add(metrics.latency_p95_ms);
        self.error_baseline.add(metrics.error_rate);
        self.pool_baseline.add(metrics.pool_utilization);
    }

    /// 多维度联合检测
    pub fn detect_multi_dim(&self, metrics: &MultiDimMetrics) -> Vec<AnomalyDetectionResult> {
        let mut results = Vec::new();
        let mut anomaly_count = 0u32;

        if self.dimensions.contains(&AnomalyType::SlowQuerySpike) {
            let is_anomaly = self.is_spike(&self.latency_baseline, metrics.latency_p95_ms);
            if is_anomaly {
                anomaly_count += 1;
            }
            results.push(AnomalyDetectionResult {
                anomaly_type: AnomalyType::SlowQuerySpike,
                is_anomaly,
                severity: if is_anomaly {
                    Severity::Warn
                } else {
                    Severity::Info
                },
                detail: format!(
                    "latency_p95={:.1}ms baseline_mean={:.1}ms stddev={:.1}",
                    metrics.latency_p95_ms,
                    self.latency_baseline.mean(),
                    self.latency_baseline.stddev()
                ),
                detected_dimensions: vec!["latency_p95".to_string()],
            });
        }

        if self.dimensions.contains(&AnomalyType::ErrorRateSpike) {
            let is_anomaly = self.is_spike(&self.error_baseline, metrics.error_rate);
            if is_anomaly {
                anomaly_count += 1;
            }
            results.push(AnomalyDetectionResult {
                anomaly_type: AnomalyType::ErrorRateSpike,
                is_anomaly,
                severity: if is_anomaly {
                    Severity::Warn
                } else {
                    Severity::Info
                },
                detail: format!(
                    "error_rate={:.4} baseline_mean={:.4} stddev={:.4}",
                    metrics.error_rate,
                    self.error_baseline.mean(),
                    self.error_baseline.stddev()
                ),
                detected_dimensions: vec!["error_rate".to_string()],
            });
        }

        if self.dimensions.contains(&AnomalyType::PoolExhaustion) {
            let is_anomaly = metrics.pool_utilization > 0.9
                || self.is_spike(&self.pool_baseline, metrics.pool_utilization);
            if is_anomaly {
                anomaly_count += 1;
            }
            results.push(AnomalyDetectionResult {
                anomaly_type: AnomalyType::PoolExhaustion,
                is_anomaly,
                severity: if is_anomaly {
                    Severity::Critical
                } else {
                    Severity::Info
                },
                detail: format!(
                    "pool_utilization={:.2} baseline_mean={:.2}",
                    metrics.pool_utilization,
                    self.pool_baseline.mean()
                ),
                detected_dimensions: vec!["pool_utilization".to_string()],
            });
        }

        if anomaly_count >= 2 {
            for r in &mut results {
                if r.is_anomaly {
                    r.severity = Severity::Critical;
                    r.detail = format!(
                        "{} [multi-dim correlated: {} dims]",
                        r.detail, anomaly_count
                    );
                }
            }
        }

        results
    }

    fn is_spike(&self, baseline: &BaselineCalculator, value: f64) -> bool {
        if baseline.count() < 10 {
            return false;
        }
        let mean = baseline.mean();
        let stddev = baseline.stddev();
        if stddev > 0.0 {
            (value - mean) > 3.0 * stddev
        } else {
            false
        }
    }
}

/// v7.6.0 调优后的阈值
#[derive(Debug, Clone)]
pub struct TunedThreshold {
    pub threshold: f64,
    pub false_positive_rate: f64,
    pub false_negative_rate: f64,
    pub tuning_basis: String,
}

/// v7.6.0 阈值自动调优器
///
/// 基于 Welford 基线学习自动调优阈值，
/// 误报率 > 2% 或漏报率 > 2% 时触发基线重新学习。
pub struct ThresholdAutoTuner {
    baseline: BaselineCalculator,
    current_threshold: f64,
    samples: Vec<f64>,
}

impl ThresholdAutoTuner {
    pub fn new() -> Self {
        Self {
            baseline: BaselineCalculator::new(),
            current_threshold: 0.0,
            samples: Vec::new(),
        }
    }

    /// 基于基线调优阈值
    pub fn tune_threshold(&self, baseline: &BaselineCalculator) -> TunedThreshold {
        if baseline.count() < 10 {
            return TunedThreshold {
                threshold: 0.0,
                false_positive_rate: 0.0,
                false_negative_rate: 0.0,
                tuning_basis: "insufficient samples".to_string(),
            };
        }
        let mean = baseline.mean();
        let stddev = baseline.stddev();
        let threshold = mean + 3.0 * stddev;

        let fp_rate = if stddev > 0.0 {
            let normal_cdf =
                |x: f64| 0.5 * (1.0 + Self::erf((x - mean) / (stddev * 2.0_f64.sqrt())));
            1.0 - normal_cdf(threshold)
        } else {
            0.0
        };

        TunedThreshold {
            threshold,
            false_positive_rate: fp_rate,
            false_negative_rate: 0.0,
            tuning_basis: format!(
                "mean={:.4} stddev={:.4} threshold=mean+3σ={:.4}",
                mean, stddev, threshold
            ),
        }
    }

    /// 重新学习基线
    pub fn relearn_baseline(&mut self, recent_samples: &[f64]) {
        self.baseline = BaselineCalculator::from_samples(recent_samples);
        self.samples = recent_samples.to_vec();
        let tuned = self.tune_threshold(&self.baseline);
        self.current_threshold = tuned.threshold;
    }

    /// 获取当前阈值
    pub fn current_threshold(&self) -> f64 {
        self.current_threshold
    }

    /// 获取基线
    pub fn baseline(&self) -> &BaselineCalculator {
        &self.baseline
    }

    /// 评估误报/漏报率
    pub fn evaluate(&self, predictions: &[(bool, bool)]) -> (f64, f64) {
        let mut fp = 0u64;
        let mut fn_ = 0u64;
        let mut total_neg = 0u64;
        let mut total_pos = 0u64;
        for &(predicted, actual) in predictions {
            if predicted && !actual {
                fp += 1;
            }
            if !predicted && actual {
                fn_ += 1;
            }
            if !actual {
                total_neg += 1;
            }
            if actual {
                total_pos += 1;
            }
        }
        let fp_rate = if total_neg > 0 {
            fp as f64 / total_neg as f64
        } else {
            0.0
        };
        let fn_rate = if total_pos > 0 {
            fn_ as f64 / total_pos as f64
        } else {
            0.0
        };
        (fp_rate, fn_rate)
    }

    /// 误差函数近似（Abramowitz & Stegun 7.1.26）
    fn erf(x: f64) -> f64 {
        let a1 = 0.254829592;
        let a2 = -0.284496736;
        let a3 = 1.421413741;
        let a4 = -1.453152027;
        let a5 = 1.061405429;
        let p = 0.3275911;
        let sign = if x < 0.0 { -1.0 } else { 1.0 };
        let abs_x = x.abs();
        let t = 1.0 / (1.0 + p * abs_x);
        let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-abs_x * abs_x).exp();
        sign * y
    }
}

impl Default for ThresholdAutoTuner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod v760_multi_dim_tests {
    use super::*;

    #[test]
    fn test_multi_dim_detector_normal() {
        let mut detector = MultiDimDetector::new(vec![
            AnomalyType::SlowQuerySpike,
            AnomalyType::ErrorRateSpike,
            AnomalyType::PoolExhaustion,
        ]);
        for i in 0..100 {
            detector.update_baselines(&MultiDimMetrics {
                qps: 100.0,
                latency_p95_ms: 10.0 + (i as f64 % 5.0),
                error_rate: 0.01,
                pool_utilization: 0.5,
            });
        }
        let results = detector.detect_multi_dim(&MultiDimMetrics {
            qps: 100.0,
            latency_p95_ms: 12.0,
            error_rate: 0.01,
            pool_utilization: 0.5,
        });
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| !r.is_anomaly));
    }

    #[test]
    fn test_multi_dim_detector_anomaly() {
        let mut detector = MultiDimDetector::new(vec![
            AnomalyType::SlowQuerySpike,
            AnomalyType::ErrorRateSpike,
            AnomalyType::PoolExhaustion,
        ]);
        for i in 0..100 {
            detector.update_baselines(&MultiDimMetrics {
                qps: 100.0,
                latency_p95_ms: 10.0 + (i as f64 % 5.0),
                error_rate: 0.01 + (i as f64 % 3.0) * 0.001,
                pool_utilization: 0.5,
            });
        }
        let results = detector.detect_multi_dim(&MultiDimMetrics {
            qps: 100.0,
            latency_p95_ms: 100.0,
            error_rate: 0.5,
            pool_utilization: 0.5,
        });
        let anomalies: Vec<_> = results.iter().filter(|r| r.is_anomaly).collect();
        assert!(anomalies.len() >= 2);
        assert!(anomalies.iter().all(|r| r.severity == Severity::Critical));
    }

    #[test]
    fn test_multi_dim_pool_exhaustion() {
        let mut detector = MultiDimDetector::new(vec![AnomalyType::PoolExhaustion]);
        for _ in 0..20 {
            detector.update_baselines(&MultiDimMetrics {
                qps: 100.0,
                latency_p95_ms: 10.0,
                error_rate: 0.01,
                pool_utilization: 0.5,
            });
        }
        let results = detector.detect_multi_dim(&MultiDimMetrics {
            qps: 100.0,
            latency_p95_ms: 10.0,
            error_rate: 0.01,
            pool_utilization: 0.95,
        });
        assert!(results[0].is_anomaly);
        assert_eq!(results[0].severity, Severity::Critical);
    }

    #[test]
    fn test_threshold_auto_tuner() {
        let mut tuner = ThresholdAutoTuner::new();
        let samples: Vec<f64> = (0..100).map(|i| 10.0 + (i as f64 % 10.0)).collect();
        tuner.relearn_baseline(&samples);
        let tuned = tuner.tune_threshold(tuner.baseline());
        assert!(tuned.threshold > 10.0);
        assert!(tuned.false_positive_rate <= 0.02);
    }

    #[test]
    fn test_threshold_auto_tuner_evaluate() {
        let tuner = ThresholdAutoTuner::new();
        let predictions = vec![(true, true), (true, false), (false, true), (false, false)];
        let (fp, fn_) = tuner.evaluate(&predictions);
        assert!((fp - 0.5).abs() < 0.01);
        assert!((fn_ - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_threshold_auto_tuner_insufficient() {
        let tuner = ThresholdAutoTuner::new();
        let baseline = BaselineCalculator::from_samples(&[1.0, 2.0]);
        let tuned = tuner.tune_threshold(&baseline);
        assert_eq!(tuned.threshold, 0.0);
        assert!(tuned.tuning_basis.contains("insufficient"));
    }
}
