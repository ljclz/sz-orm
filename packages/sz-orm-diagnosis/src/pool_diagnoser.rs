//! 连接池诊断器
//!
//! 监控连接池健康状态：利用率、等待队列、连接泄漏检测。
//! 本模块不依赖 `slow-query-diagnosis` feature，可独立使用。

use std::collections::HashMap;
use std::time::Instant;

/// 连接池健康状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolHealthStatus {
    /// 健康（利用率 < 70%）
    Healthy,
    /// 警告（利用率 70%~90%）
    Warning,
    /// 严重（利用率 ≥ 90% 或等待队列过长）
    Critical,
}

impl PoolHealthStatus {
    /// 人类可读名称
    pub fn as_str(&self) -> &'static str {
        match self {
            PoolHealthStatus::Healthy => "healthy",
            PoolHealthStatus::Warning => "warning",
            PoolHealthStatus::Critical => "critical",
        }
    }
}

/// 连接池指标快照
#[derive(Debug, Clone)]
pub struct ConnectionPoolMetrics {
    /// 最大连接数
    pub max_pool_size: u32,
    /// 活跃连接数
    pub active_connections: u32,
    /// 空闲连接数
    pub idle_connections: u32,
    /// 等待队列长度
    pub wait_queue_length: u32,
    /// 累计获取连接次数
    pub total_acquires: u64,
    /// 累计获取失败次数
    pub total_acquire_failures: u64,
    /// 累计等待时间（毫秒）
    pub total_wait_ms: u64,
    /// 累计连接持有时间（毫秒）
    pub total_hold_ms: u64,
}

impl ConnectionPoolMetrics {
    /// 创建指标快照
    pub fn new(max_pool_size: u32) -> Self {
        Self {
            max_pool_size,
            active_connections: 0,
            idle_connections: max_pool_size,
            wait_queue_length: 0,
            total_acquires: 0,
            total_acquire_failures: 0,
            total_wait_ms: 0,
            total_hold_ms: 0,
        }
    }

    /// 连接池利用率（0.0~1.0，max_pool_size 为 0 返回 0.0）
    pub fn utilization_rate(&self) -> f64 {
        if self.max_pool_size == 0 {
            return 0.0;
        }
        self.active_connections as f64 / self.max_pool_size as f64
    }

    /// 获取失败率（0.0~1.0）
    pub fn failure_rate(&self) -> f64 {
        if self.total_acquires == 0 {
            0.0
        } else {
            self.total_acquire_failures as f64 / self.total_acquires as f64
        }
    }

    /// 平均等待时间（毫秒）
    pub fn avg_wait_ms(&self) -> f64 {
        if self.total_acquires == 0 {
            0.0
        } else {
            self.total_wait_ms as f64 / self.total_acquires as f64
        }
    }

    /// 平均持有时间（毫秒）
    pub fn avg_hold_ms(&self) -> f64 {
        if self.total_acquires == 0 {
            0.0
        } else {
            self.total_hold_ms as f64 / self.total_acquires as f64
        }
    }

    /// 空闲率（0.0~1.0）
    pub fn idle_rate(&self) -> f64 {
        if self.max_pool_size == 0 {
            return 0.0;
        }
        self.idle_connections as f64 / self.max_pool_size as f64
    }
}

/// 连接池诊断结果
#[derive(Debug, Clone)]
pub struct PoolDiagnosisResult {
    /// 健康状态
    pub health_status: PoolHealthStatus,
    /// 利用率
    pub utilization_rate: f64,
    /// 失败率
    pub failure_rate: f64,
    /// 平均等待时间（毫秒）
    pub avg_wait_ms: f64,
    /// 平均持有时间（毫秒）
    pub avg_hold_ms: f64,
    /// 诊断建议列表
    pub suggestions: Vec<PoolSuggestion>,
}

/// 连接池建议类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PoolSuggestion {
    /// 增大连接池大小（建议值）
    IncreasePoolSize(u32),
    /// 减小连接池大小（建议值）
    DecreasePoolSize(u32),
    /// 启用连接预热
    EnablePrewarm,
    /// 检查连接泄漏
    CheckLeak,
    /// 优化查询持有时间
    OptimizeHoldTime,
    /// 增加等待超时
    IncreaseWaitTimeout,
}

impl PoolSuggestion {
    /// 建议描述
    pub fn description(&self) -> String {
        match self {
            PoolSuggestion::IncreasePoolSize(n) => {
                format!("increase pool size to {n} (utilization too high)")
            }
            PoolSuggestion::DecreasePoolSize(n) => {
                format!("decrease pool size to {n} (utilization too low, wasting resources)")
            }
            PoolSuggestion::EnablePrewarm => {
                "enable connection prewarm to reduce cold-start latency".to_string()
            }
            PoolSuggestion::CheckLeak => {
                "check for connection leaks (active connections stay high)".to_string()
            }
            PoolSuggestion::OptimizeHoldTime => {
                "optimize query to reduce connection hold time".to_string()
            }
            PoolSuggestion::IncreaseWaitTimeout => {
                "increase wait timeout to reduce acquire failures".to_string()
            }
        }
    }
}

/// 连接池诊断器配置
#[derive(Debug, Clone)]
pub struct PoolDiagnoserConfig {
    /// 警告利用率阈值（默认 0.7）
    pub warning_utilization: f64,
    /// 严重利用率阈值（默认 0.9）
    pub critical_utilization: f64,
    /// 严重等待队列长度阈值（默认 10）
    pub critical_wait_queue: u32,
    /// 警告平均持有时间阈值（毫秒，默认 5000）
    pub warning_hold_ms: f64,
    /// 警告失败率阈值（默认 0.01）
    pub warning_failure_rate: f64,
}

impl Default for PoolDiagnoserConfig {
    fn default() -> Self {
        Self {
            warning_utilization: 0.7,
            critical_utilization: 0.9,
            critical_wait_queue: 10,
            warning_hold_ms: 5000.0,
            warning_failure_rate: 0.01,
        }
    }
}

/// 连接池诊断器
///
/// 根据 [`ConnectionPoolMetrics`] 快照诊断连接池健康状态并生成建议。
pub struct ConnectionPoolDiagnoser {
    config: PoolDiagnoserConfig,
}

impl ConnectionPoolDiagnoser {
    /// 创建诊断器
    pub fn new(config: PoolDiagnoserConfig) -> Self {
        Self { config }
    }

    /// 使用默认配置创建诊断器
    pub fn with_defaults() -> Self {
        Self::new(PoolDiagnoserConfig::default())
    }

    /// 诊断连接池
    pub fn diagnose(&self, metrics: &ConnectionPoolMetrics) -> PoolDiagnosisResult {
        let utilization = metrics.utilization_rate();
        let failure_rate = metrics.failure_rate();
        let avg_wait = metrics.avg_wait_ms();
        let avg_hold = metrics.avg_hold_ms();

        let health_status = self.determine_health(metrics, utilization);
        let suggestions = self.generate_suggestions(metrics, utilization, failure_rate, avg_hold);

        PoolDiagnosisResult {
            health_status,
            utilization_rate: utilization,
            failure_rate,
            avg_wait_ms: avg_wait,
            avg_hold_ms: avg_hold,
            suggestions,
        }
    }

    /// 判定健康状态
    fn determine_health(
        &self,
        metrics: &ConnectionPoolMetrics,
        utilization: f64,
    ) -> PoolHealthStatus {
        if utilization >= self.config.critical_utilization
            || metrics.wait_queue_length >= self.config.critical_wait_queue
        {
            return PoolHealthStatus::Critical;
        }
        if utilization >= self.config.warning_utilization {
            return PoolHealthStatus::Warning;
        }
        PoolHealthStatus::Healthy
    }

    /// 生成建议
    fn generate_suggestions(
        &self,
        metrics: &ConnectionPoolMetrics,
        utilization: f64,
        failure_rate: f64,
        avg_hold: f64,
    ) -> Vec<PoolSuggestion> {
        let mut suggestions = Vec::new();

        // 利用率过高 → 增大连接池
        if utilization >= self.config.critical_utilization {
            let suggested = metrics
                .max_pool_size
                .saturating_mul(2)
                .max(metrics.max_pool_size + 1);
            suggestions.push(PoolSuggestion::IncreasePoolSize(suggested));
        }

        // 利用率过低 → 减小连接池
        if utilization < 0.2 && metrics.max_pool_size > 1 {
            let suggested = (metrics.max_pool_size / 2).max(1);
            suggestions.push(PoolSuggestion::DecreasePoolSize(suggested));
        }

        // 等待队列过长 → 增大连接池或预热
        if metrics.wait_queue_length >= self.config.critical_wait_queue {
            suggestions.push(PoolSuggestion::EnablePrewarm);
        }

        // 失败率高 → 增加等待超时
        if failure_rate >= self.config.warning_failure_rate {
            suggestions.push(PoolSuggestion::IncreaseWaitTimeout);
        }

        // 持有时间过长 → 优化查询
        if avg_hold >= self.config.warning_hold_ms {
            suggestions.push(PoolSuggestion::OptimizeHoldTime);
        }

        // 活跃连接持续高 → 检查泄漏
        if utilization >= self.config.warning_utilization && metrics.idle_connections == 0 {
            suggestions.push(PoolSuggestion::CheckLeak);
        }

        suggestions
    }
}

/// 连接泄漏检测器
///
/// 跟踪每个获取的连接及其持有时间，检测可能泄漏的连接
/// （持有时间超过阈值且未释放）。
#[derive(Debug)]
pub struct PoolLeakDetector {
    /// 泄漏阈值（毫秒，默认 30_000）
    leak_threshold_ms: u64,
    /// 活跃连接记录：connection_id -> (acquire_time)
    active: HashMap<u64, Instant>,
}

impl PoolLeakDetector {
    /// 创建泄漏检测器
    pub fn new(leak_threshold_ms: u64) -> Self {
        Self {
            leak_threshold_ms,
            active: HashMap::new(),
        }
    }

    /// 使用默认阈值（30 秒）创建
    pub fn with_defaults() -> Self {
        Self::new(30_000)
    }

    /// 记录连接获取
    pub fn on_acquire(&mut self, connection_id: u64) {
        self.active.insert(connection_id, Instant::now());
    }

    /// 记录连接释放
    pub fn on_release(&mut self, connection_id: u64) -> Option<u64> {
        self.active
            .remove(&connection_id)
            .map(|t| Instant::now().duration_since(t).as_millis() as u64)
    }

    /// 检测当前疑似泄漏的连接
    ///
    /// 返回 `(connection_id, 持有毫秒)` 列表，按持有时间降序。
    pub fn detect_leaks(&self) -> Vec<(u64, u64)> {
        let now = Instant::now();
        let mut leaks: Vec<(u64, u64)> = self
            .active
            .iter()
            .filter_map(|(&id, &acquire_time)| {
                let hold_ms = now.duration_since(acquire_time).as_millis() as u64;
                if hold_ms >= self.leak_threshold_ms {
                    Some((id, hold_ms))
                } else {
                    None
                }
            })
            .collect();
        leaks.sort_by_key(|(_, ms)| std::cmp::Reverse(*ms));
        leaks
    }

    /// 当前活跃连接数
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// 泄漏阈值（毫秒）
    pub fn leak_threshold_ms(&self) -> u64 {
        self.leak_threshold_ms
    }

    /// 清空所有记录
    pub fn clear(&mut self) {
        self.active.clear();
    }
}

/// 连接池历史采样器
///
/// 周期性采样连接池指标，提供趋势分析。
#[derive(Debug, Clone)]
pub struct PoolMetricsSampler {
    samples: Vec<ConnectionPoolMetrics>,
    max_samples: usize,
}

impl PoolMetricsSampler {
    /// 创建采样器
    pub fn new(max_samples: usize) -> Self {
        Self {
            samples: Vec::with_capacity(max_samples),
            max_samples,
        }
    }

    /// 采样一次
    pub fn sample(&mut self, metrics: ConnectionPoolMetrics) {
        if self.samples.len() >= self.max_samples {
            self.samples.remove(0);
        }
        self.samples.push(metrics);
    }

    /// 采样数
    pub fn count(&self) -> usize {
        self.samples.len()
    }

    /// 平均利用率
    pub fn avg_utilization(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.samples.iter().map(|m| m.utilization_rate()).sum();
        sum / self.samples.len() as f64
    }

    /// 最大利用率
    pub fn max_utilization(&self) -> f64 {
        self.samples
            .iter()
            .map(|m| m.utilization_rate())
            .fold(0.0, f64::max)
    }

    /// 利用率趋势（最后 N 个采样的变化率，正数=上升）
    pub fn utilization_trend(&self) -> f64 {
        let n = self.samples.len();
        if n < 2 {
            return 0.0;
        }
        let recent = self.samples[n - 1].utilization_rate();
        let prev = self.samples[n - 2].utilization_rate();
        recent - prev
    }

    /// 所有采样引用
    pub fn samples(&self) -> &[ConnectionPoolMetrics] {
        &self.samples
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy_metrics() -> ConnectionPoolMetrics {
        ConnectionPoolMetrics {
            max_pool_size: 100,
            active_connections: 20,
            idle_connections: 80,
            wait_queue_length: 0,
            total_acquires: 1000,
            total_acquire_failures: 0,
            total_wait_ms: 100,
            total_hold_ms: 50_000,
        }
    }

    fn warning_metrics() -> ConnectionPoolMetrics {
        ConnectionPoolMetrics {
            max_pool_size: 100,
            active_connections: 75,
            idle_connections: 25,
            wait_queue_length: 3,
            total_acquires: 1000,
            total_acquire_failures: 0,
            total_wait_ms: 500,
            total_hold_ms: 100_000,
        }
    }

    fn critical_metrics() -> ConnectionPoolMetrics {
        ConnectionPoolMetrics {
            max_pool_size: 100,
            active_connections: 95,
            idle_connections: 5,
            wait_queue_length: 15,
            total_acquires: 1000,
            total_acquire_failures: 5,
            total_wait_ms: 5000,
            total_hold_ms: 500_000,
        }
    }

    // --- PoolHealthStatus tests ---

    #[test]
    fn health_status_as_str() {
        assert_eq!(PoolHealthStatus::Healthy.as_str(), "healthy");
        assert_eq!(PoolHealthStatus::Warning.as_str(), "warning");
        assert_eq!(PoolHealthStatus::Critical.as_str(), "critical");
    }

    #[test]
    fn health_status_distinct() {
        assert_ne!(PoolHealthStatus::Healthy, PoolHealthStatus::Warning);
        assert_ne!(PoolHealthStatus::Warning, PoolHealthStatus::Critical);
    }

    // --- ConnectionPoolMetrics tests ---

    #[test]
    fn metrics_new() {
        let m = ConnectionPoolMetrics::new(50);
        assert_eq!(m.max_pool_size, 50);
        assert_eq!(m.active_connections, 0);
        assert_eq!(m.idle_connections, 50);
    }

    #[test]
    fn utilization_rate_basic() {
        let m = healthy_metrics();
        assert!((m.utilization_rate() - 0.2).abs() < 1e-9);
    }

    #[test]
    fn utilization_rate_zero_pool() {
        let m = ConnectionPoolMetrics::new(0);
        assert_eq!(m.utilization_rate(), 0.0);
    }

    #[test]
    fn utilization_rate_full() {
        let m = ConnectionPoolMetrics {
            max_pool_size: 10,
            active_connections: 10,
            idle_connections: 0,
            wait_queue_length: 0,
            total_acquires: 0,
            total_acquire_failures: 0,
            total_wait_ms: 0,
            total_hold_ms: 0,
        };
        assert!((m.utilization_rate() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn failure_rate_zero_acquires() {
        let m = ConnectionPoolMetrics::new(10);
        assert_eq!(m.failure_rate(), 0.0);
    }

    #[test]
    fn failure_rate_with_failures() {
        let m = critical_metrics();
        assert!((m.failure_rate() - 0.005).abs() < 1e-9);
    }

    #[test]
    fn avg_wait_ms_basic() {
        let m = healthy_metrics();
        assert!((m.avg_wait_ms() - 0.1).abs() < 1e-9);
    }

    #[test]
    fn avg_hold_ms_basic() {
        let m = healthy_metrics();
        assert!((m.avg_hold_ms() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn idle_rate_basic() {
        let m = healthy_metrics();
        assert!((m.idle_rate() - 0.8).abs() < 1e-9);
    }

    #[test]
    fn idle_rate_zero_pool() {
        let m = ConnectionPoolMetrics::new(0);
        assert_eq!(m.idle_rate(), 0.0);
    }

    // --- PoolSuggestion tests ---

    #[test]
    fn suggestion_descriptions_nonempty() {
        assert!(!PoolSuggestion::IncreasePoolSize(20)
            .description()
            .is_empty());
        assert!(!PoolSuggestion::DecreasePoolSize(5).description().is_empty());
        assert!(!PoolSuggestion::EnablePrewarm.description().is_empty());
        assert!(!PoolSuggestion::CheckLeak.description().is_empty());
        assert!(!PoolSuggestion::OptimizeHoldTime.description().is_empty());
        assert!(!PoolSuggestion::IncreaseWaitTimeout.description().is_empty());
    }

    #[test]
    fn suggestion_increase_contains_size() {
        let s = PoolSuggestion::IncreasePoolSize(200);
        assert!(s.description().contains("200"));
    }

    #[test]
    fn suggestion_distinct() {
        let a = PoolSuggestion::IncreasePoolSize(10);
        let b = PoolSuggestion::DecreasePoolSize(10);
        assert_ne!(a, b);
    }

    // --- ConnectionPoolDiagnoser tests ---

    #[test]
    fn diagnose_healthy() {
        let d = ConnectionPoolDiagnoser::with_defaults();
        let result = d.diagnose(&healthy_metrics());
        assert_eq!(result.health_status, PoolHealthStatus::Healthy);
        assert!(result.suggestions.is_empty());
    }

    #[test]
    fn diagnose_warning() {
        let d = ConnectionPoolDiagnoser::with_defaults();
        let result = d.diagnose(&warning_metrics());
        assert_eq!(result.health_status, PoolHealthStatus::Warning);
    }

    #[test]
    fn diagnose_critical_by_utilization() {
        let d = ConnectionPoolDiagnoser::with_defaults();
        let result = d.diagnose(&critical_metrics());
        assert_eq!(result.health_status, PoolHealthStatus::Critical);
    }

    #[test]
    fn diagnose_critical_by_wait_queue() {
        let d = ConnectionPoolDiagnoser::with_defaults();
        let m = ConnectionPoolMetrics {
            max_pool_size: 100,
            active_connections: 50,
            idle_connections: 50,
            wait_queue_length: 20,
            total_acquires: 100,
            total_acquire_failures: 0,
            total_wait_ms: 0,
            total_hold_ms: 0,
        };
        let result = d.diagnose(&m);
        assert_eq!(result.health_status, PoolHealthStatus::Critical);
    }

    #[test]
    fn diagnose_suggests_increase_pool() {
        let d = ConnectionPoolDiagnoser::with_defaults();
        let result = d.diagnose(&critical_metrics());
        assert!(result
            .suggestions
            .iter()
            .any(|s| matches!(s, PoolSuggestion::IncreasePoolSize(_))));
    }

    #[test]
    fn diagnose_suggests_decrease_pool() {
        let d = ConnectionPoolDiagnoser::with_defaults();
        let m = ConnectionPoolMetrics {
            max_pool_size: 100,
            active_connections: 5,
            idle_connections: 95,
            wait_queue_length: 0,
            total_acquires: 100,
            total_acquire_failures: 0,
            total_wait_ms: 0,
            total_hold_ms: 0,
        };
        let result = d.diagnose(&m);
        assert!(result
            .suggestions
            .iter()
            .any(|s| matches!(s, PoolSuggestion::DecreasePoolSize(_))));
    }

    #[test]
    fn diagnose_suggests_prewarm_for_wait_queue() {
        let d = ConnectionPoolDiagnoser::with_defaults();
        let m = ConnectionPoolMetrics {
            max_pool_size: 100,
            active_connections: 50,
            idle_connections: 50,
            wait_queue_length: 15,
            total_acquires: 100,
            total_acquire_failures: 0,
            total_wait_ms: 0,
            total_hold_ms: 0,
        };
        let result = d.diagnose(&m);
        assert!(result
            .suggestions
            .iter()
            .any(|s| matches!(s, PoolSuggestion::EnablePrewarm)));
    }

    #[test]
    fn diagnose_suggests_increase_timeout_for_failures() {
        let d = ConnectionPoolDiagnoser::with_defaults();
        let m = ConnectionPoolMetrics {
            max_pool_size: 100,
            active_connections: 10,
            idle_connections: 90,
            wait_queue_length: 0,
            total_acquires: 100,
            total_acquire_failures: 10,
            total_wait_ms: 0,
            total_hold_ms: 0,
        };
        let result = d.diagnose(&m);
        assert!(result
            .suggestions
            .iter()
            .any(|s| matches!(s, PoolSuggestion::IncreaseWaitTimeout)));
    }

    #[test]
    fn diagnose_suggests_optimize_hold_time() {
        let d = ConnectionPoolDiagnoser::with_defaults();
        let m = ConnectionPoolMetrics {
            max_pool_size: 100,
            active_connections: 10,
            idle_connections: 90,
            wait_queue_length: 0,
            total_acquires: 100,
            total_acquire_failures: 0,
            total_wait_ms: 0,
            total_hold_ms: 1_000_000,
        };
        let result = d.diagnose(&m);
        assert!(result
            .suggestions
            .iter()
            .any(|s| matches!(s, PoolSuggestion::OptimizeHoldTime)));
    }

    #[test]
    fn diagnose_suggests_check_leak() {
        let d = ConnectionPoolDiagnoser::with_defaults();
        let m = ConnectionPoolMetrics {
            max_pool_size: 100,
            active_connections: 80,
            idle_connections: 0,
            wait_queue_length: 0,
            total_acquires: 100,
            total_acquire_failures: 0,
            total_wait_ms: 0,
            total_hold_ms: 0,
        };
        let result = d.diagnose(&m);
        assert!(result
            .suggestions
            .iter()
            .any(|s| matches!(s, PoolSuggestion::CheckLeak)));
    }

    #[test]
    fn diagnose_result_fields_populated() {
        let d = ConnectionPoolDiagnoser::with_defaults();
        let result = d.diagnose(&healthy_metrics());
        assert!((result.utilization_rate - 0.2).abs() < 1e-9);
        assert!(result.failure_rate < 1e-9);
        assert!(result.avg_wait_ms > 0.0);
        assert!(result.avg_hold_ms > 0.0);
    }

    // --- PoolLeakDetector tests ---

    #[test]
    fn leak_detector_no_leaks_initially() {
        let detector = PoolLeakDetector::with_defaults();
        assert!(detector.detect_leaks().is_empty());
        assert_eq!(detector.active_count(), 0);
    }

    #[test]
    fn leak_detector_acquire_increment_count() {
        let mut detector = PoolLeakDetector::with_defaults();
        detector.on_acquire(1);
        detector.on_acquire(2);
        assert_eq!(detector.active_count(), 2);
    }

    #[test]
    fn leak_detector_release_decrement_count() {
        let mut detector = PoolLeakDetector::with_defaults();
        detector.on_acquire(1);
        detector.on_release(1);
        assert_eq!(detector.active_count(), 0);
    }

    #[test]
    fn leak_detector_release_returns_hold_time() {
        let mut detector = PoolLeakDetector::with_defaults();
        detector.on_acquire(1);
        std::thread::sleep(std::time::Duration::from_millis(10));
        let hold = detector.on_release(1);
        assert!(hold.is_some());
        assert!(hold.unwrap() >= 10);
    }

    #[test]
    fn leak_detector_release_unknown_returns_none() {
        let mut detector = PoolLeakDetector::with_defaults();
        assert!(detector.on_release(999).is_none());
    }

    #[test]
    fn leak_detector_clear() {
        let mut detector = PoolLeakDetector::with_defaults();
        detector.on_acquire(1);
        detector.on_acquire(2);
        detector.clear();
        assert_eq!(detector.active_count(), 0);
    }

    #[test]
    fn leak_detector_threshold_getter() {
        let detector = PoolLeakDetector::new(5000);
        assert_eq!(detector.leak_threshold_ms(), 5000);
    }

    // --- PoolMetricsSampler tests ---

    #[test]
    fn sampler_empty_initially() {
        let s = PoolMetricsSampler::new(10);
        assert_eq!(s.count(), 0);
        assert_eq!(s.avg_utilization(), 0.0);
    }

    #[test]
    fn sampler_count_increments() {
        let mut s = PoolMetricsSampler::new(10);
        s.sample(healthy_metrics());
        s.sample(warning_metrics());
        assert_eq!(s.count(), 2);
    }

    #[test]
    fn sampler_evicts_oldest() {
        let mut s = PoolMetricsSampler::new(2);
        s.sample(healthy_metrics());
        s.sample(warning_metrics());
        s.sample(critical_metrics());
        assert_eq!(s.count(), 2);
    }

    #[test]
    fn sampler_avg_utilization() {
        let mut s = PoolMetricsSampler::new(10);
        s.sample(healthy_metrics());
        s.sample(warning_metrics());
        let avg = s.avg_utilization();
        let expected = (0.2 + 0.75) / 2.0;
        assert!((avg - expected).abs() < 1e-9);
    }

    #[test]
    fn sampler_max_utilization() {
        let mut s = PoolMetricsSampler::new(10);
        s.sample(healthy_metrics());
        s.sample(critical_metrics());
        let max = s.max_utilization();
        assert!((max - 0.95).abs() < 1e-9);
    }

    #[test]
    fn sampler_trend_empty() {
        let s = PoolMetricsSampler::new(10);
        assert_eq!(s.utilization_trend(), 0.0);
    }

    #[test]
    fn sampler_trend_single_sample() {
        let mut s = PoolMetricsSampler::new(10);
        s.sample(healthy_metrics());
        assert_eq!(s.utilization_trend(), 0.0);
    }

    #[test]
    fn sampler_trend_increasing() {
        let mut s = PoolMetricsSampler::new(10);
        s.sample(healthy_metrics());
        s.sample(critical_metrics());
        let trend = s.utilization_trend();
        assert!(trend > 0.0);
    }

    #[test]
    fn sampler_samples_ref() {
        let mut s = PoolMetricsSampler::new(10);
        s.sample(healthy_metrics());
        assert_eq!(s.samples().len(), 1);
    }

    // --- PoolDiagnoserConfig tests ---

    #[test]
    fn config_default_values() {
        let c = PoolDiagnoserConfig::default();
        assert!((c.warning_utilization - 0.7).abs() < 1e-9);
        assert!((c.critical_utilization - 0.9).abs() < 1e-9);
        assert_eq!(c.critical_wait_queue, 10);
    }

    #[test]
    fn config_custom_values() {
        let c = PoolDiagnoserConfig {
            warning_utilization: 0.6,
            critical_utilization: 0.85,
            critical_wait_queue: 5,
            warning_hold_ms: 3000.0,
            warning_failure_rate: 0.005,
        };
        let d = ConnectionPoolDiagnoser::new(c);
        let m = ConnectionPoolMetrics {
            max_pool_size: 100,
            active_connections: 65,
            idle_connections: 35,
            wait_queue_length: 0,
            total_acquires: 100,
            total_acquire_failures: 0,
            total_wait_ms: 0,
            total_hold_ms: 0,
        };
        let result = d.diagnose(&m);
        assert_eq!(result.health_status, PoolHealthStatus::Warning);
    }
}
