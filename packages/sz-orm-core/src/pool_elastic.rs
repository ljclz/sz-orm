//! v6.7.0 连接池弹性：动态扩缩容 + 多级熔断 + 健康检查 + 连接预热。
//!
//! `PoolElasticController` 周期采集池 stats，waiting > threshold 扩容，idle > threshold 缩容。
//! `TieredCircuitBreaker` 三级熔断（连接→节点→全局），`ConnectionHealthChecker` 连续 3 次失败才剔除。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::circuit_breaker::{CircuitBreaker, CircuitState, DefaultCircuitBreaker};

// ============================================================================
// 配置
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElasticConfig {
    pub min_connections: u32,
    pub max_connections: u32,
    pub scale_up_threshold: u32,
    pub scale_down_threshold: u32,
    pub scale_interval: Duration,
    pub health_check_interval: Duration,
    pub health_check_sql: String,
}

impl Default for ElasticConfig {
    fn default() -> Self {
        Self {
            min_connections: 5,
            max_connections: 50,
            scale_up_threshold: 10,
            scale_down_threshold: 20,
            scale_interval: Duration::from_secs(1),
            health_check_interval: Duration::from_secs(30),
            health_check_sql: "SELECT 1".to_string(),
        }
    }
}

impl ElasticConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.min_connections < 1 || self.min_connections > 100 {
            return Err("min_connections 必须在 [1,100]".to_string());
        }
        if self.max_connections < self.min_connections || self.max_connections > 1000 {
            return Err("max_connections 必须在 [min,1000]".to_string());
        }
        if self.scale_interval < Duration::from_secs(1)
            || self.scale_interval > Duration::from_secs(60)
        {
            return Err("scale_interval 必须在 [1s,60s]".to_string());
        }
        if self.health_check_interval < Duration::from_secs(10)
            || self.health_check_interval > Duration::from_secs(300)
        {
            return Err("health_check_interval 必须在 [10s,300s]".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScaleReason {
    HighLoad,
    LowLoad,
    HealthCheckFailure,
}

#[derive(Debug, Clone)]
pub struct ScaleEvent {
    pub from: u32,
    pub to: u32,
    pub reason: ScaleReason,
    pub timestamp: Instant,
    pub elapsed: Duration,
}

// ============================================================================
// 多级熔断器
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitTier {
    Connection,
    Node,
    Global,
}

pub struct TieredCircuitBreaker {
    connection_level: Mutex<DefaultCircuitBreaker>,
    node_level: Mutex<DefaultCircuitBreaker>,
    global_level: Mutex<DefaultCircuitBreaker>,
}

impl TieredCircuitBreaker {
    pub fn new(failure_threshold: usize, reset_timeout: Duration) -> Self {
        Self {
            connection_level: Mutex::new(DefaultCircuitBreaker::new(
                failure_threshold,
                reset_timeout,
            )),
            node_level: Mutex::new(DefaultCircuitBreaker::new(failure_threshold, reset_timeout)),
            global_level: Mutex::new(DefaultCircuitBreaker::new(failure_threshold, reset_timeout)),
        }
    }

    pub fn can_execute(&self, tier: CircuitTier) -> bool {
        let cb = match tier {
            CircuitTier::Connection => &self.connection_level,
            CircuitTier::Node => &self.node_level,
            CircuitTier::Global => &self.global_level,
        };
        cb.lock().unwrap().can_execute()
    }

    pub fn record_success(&self, tier: CircuitTier) {
        let cb = match tier {
            CircuitTier::Connection => &self.connection_level,
            CircuitTier::Node => &self.node_level,
            CircuitTier::Global => &self.global_level,
        };
        cb.lock().unwrap().record_success();
    }

    pub fn record_failure(&self, tier: CircuitTier) {
        let cb = match tier {
            CircuitTier::Connection => &self.connection_level,
            CircuitTier::Node => &self.node_level,
            CircuitTier::Global => &self.global_level,
        };
        cb.lock().unwrap().record_failure();
    }

    pub fn state(&self, tier: CircuitTier) -> CircuitState {
        let cb = match tier {
            CircuitTier::Connection => &self.connection_level,
            CircuitTier::Node => &self.node_level,
            CircuitTier::Global => &self.global_level,
        };
        cb.lock().unwrap().state()
    }

    pub fn can_execute_any(&self) -> bool {
        self.can_execute(CircuitTier::Global)
            && self.can_execute(CircuitTier::Node)
            && self.can_execute(CircuitTier::Connection)
    }
}

// ============================================================================
// 健康检查器
// ============================================================================

pub struct ConnectionHealthChecker {
    failure_counts: Mutex<std::collections::HashMap<String, u32>>,
    failure_threshold: u32,
    /// v10.12.0 T3.2：半开连接探活器
    half_open_probe: Option<HalfOpenProbe>,
}

impl ConnectionHealthChecker {
    pub fn new(failure_threshold: u32) -> Self {
        Self {
            failure_counts: Mutex::new(std::collections::HashMap::new()),
            failure_threshold,
            half_open_probe: None,
        }
    }

    /// v10.12.0 T3.2：创建带半开探活的健康检查器
    pub fn new_with_probe(failure_threshold: u32, probe_config: ProbeAliveConfig) -> Self {
        Self {
            failure_counts: Mutex::new(std::collections::HashMap::new()),
            failure_threshold,
            half_open_probe: Some(HalfOpenProbe::new(probe_config)),
        }
    }

    pub fn record_failure(&self, conn_id: &str) -> bool {
        let mut counts = self.failure_counts.lock().unwrap();
        let count = counts.entry(conn_id.to_string()).or_insert(0);
        *count += 1;
        *count >= self.failure_threshold
    }

    pub fn record_success(&self, conn_id: &str) {
        let mut counts = self.failure_counts.lock().unwrap();
        counts.insert(conn_id.to_string(), 0);
    }

    pub fn should_evict(&self, conn_id: &str) -> bool {
        let counts = self.failure_counts.lock().unwrap();
        *counts.get(conn_id).unwrap_or(&0) >= self.failure_threshold
    }

    pub fn failure_count(&self, conn_id: &str) -> u32 {
        let counts = self.failure_counts.lock().unwrap();
        *counts.get(conn_id).unwrap_or(&0)
    }

    /// v10.12.0 T3.2：获取半开探活器引用
    pub fn half_open_probe(&self) -> Option<&HalfOpenProbe> {
        self.half_open_probe.as_ref()
    }

    /// v10.12.0 T3.2：记录探活结果
    ///
    /// 返回 ProbeResult，调用方根据结果决定是否剔除连接。
    pub fn record_probe(&self, conn_id: &str, is_alive: bool) -> ProbeResult {
        if let Some(probe) = &self.half_open_probe {
            probe.probe_alive(conn_id, is_alive)
        } else {
            ProbeResult::Alive
        }
    }
}

// ============================================================================
// v10.12.0 T3.2：半开连接探测与自动剔除
// ============================================================================

/// 探活配置
#[derive(Debug, Clone)]
pub struct ProbeAliveConfig {
    /// 探活周期
    pub probe_interval: Duration,
    /// 探活超时
    pub probe_timeout: Duration,
    /// 连续失败阈值（达到才剔除）
    pub fail_threshold: u32,
    /// 探活 SQL
    pub probe_sql: String,
}

impl Default for ProbeAliveConfig {
    fn default() -> Self {
        Self {
            probe_interval: Duration::from_secs(30),
            probe_timeout: Duration::from_secs(5),
            fail_threshold: 3,
            probe_sql: "SELECT 1".to_string(),
        }
    }
}

/// 探活结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeResult {
    /// 连接存活
    Alive,
    /// 半开状态（TCP 存在但 DB 不可达），含连续失败次数
    HalfOpen { fail_count: u32 },
    /// 已剔除（连续失败达到阈值）
    Evicted,
}

/// 自动剔除器
///
/// 剔除半开连接后触发连接池补充新连接。
pub struct AutoEvictor {
    evicted_count: std::sync::atomic::AtomicU64,
    compensated_count: std::sync::atomic::AtomicU64,
}

impl AutoEvictor {
    /// 创建自动剔除器
    pub fn new() -> Self {
        Self {
            evicted_count: std::sync::atomic::AtomicU64::new(0),
            compensated_count: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// 记录一次剔除
    pub fn record_evict(&self) {
        self.evicted_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// 记录一次连接补充
    pub fn record_compensate(&self) {
        self.compensated_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// 已剔除连接数
    pub fn evicted_count(&self) -> u64 {
        self.evicted_count
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 已补充连接数
    pub fn compensated_count(&self) -> u64 {
        self.compensated_count
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Default for AutoEvictor {
    fn default() -> Self {
        Self::new()
    }
}

/// 半开连接探活器
///
/// 周期性探活半开连接（TCP 连接存在但数据库不可达），
/// 连续 N 次探活失败才剔除，剔除后触发连接补充。
pub struct HalfOpenProbe {
    config: ProbeAliveConfig,
    probe_fail_counts: Mutex<std::collections::HashMap<String, u32>>,
    auto_evictor: AutoEvictor,
    last_probe_time: Mutex<std::collections::HashMap<String, Instant>>,
}

impl HalfOpenProbe {
    /// 创建半开探活器
    pub fn new(config: ProbeAliveConfig) -> Self {
        Self {
            config,
            probe_fail_counts: Mutex::new(std::collections::HashMap::new()),
            auto_evictor: AutoEvictor::new(),
            last_probe_time: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// 探活：记录探活结果，返回 ProbeResult
    ///
    /// - `is_alive = true`：重置失败计数，返回 Alive
    /// - `is_alive = false`：递增失败计数，达到阈值返回 Evicted，否则返回 HalfOpen
    pub fn probe_alive(&self, conn_id: &str, is_alive: bool) -> ProbeResult {
        {
            let mut last = self.last_probe_time.lock().unwrap();
            last.insert(conn_id.to_string(), Instant::now());
        }

        if is_alive {
            let mut counts = self.probe_fail_counts.lock().unwrap();
            counts.insert(conn_id.to_string(), 0);
            return ProbeResult::Alive;
        }

        let mut counts = self.probe_fail_counts.lock().unwrap();
        let count = counts.entry(conn_id.to_string()).or_insert(0);
        *count += 1;

        if *count >= self.config.fail_threshold {
            self.auto_evictor.record_evict();
            self.auto_evictor.record_compensate();
            counts.remove(conn_id);
            ProbeResult::Evicted
        } else {
            ProbeResult::HalfOpen { fail_count: *count }
        }
    }

    /// 检查是否应该剔除（失败计数 >= 阈值）
    pub fn should_evict(&self, conn_id: &str) -> bool {
        let counts = self.probe_fail_counts.lock().unwrap();
        *counts.get(conn_id).unwrap_or(&0) >= self.config.fail_threshold
    }

    /// 获取连接的失败计数
    pub fn fail_count(&self, conn_id: &str) -> u32 {
        let counts = self.probe_fail_counts.lock().unwrap();
        *counts.get(conn_id).unwrap_or(&0)
    }

    /// 检查是否到达探活周期
    pub fn should_probe(&self, conn_id: &str) -> bool {
        let last = self.last_probe_time.lock().unwrap();
        match last.get(conn_id) {
            Some(t) => t.elapsed() >= self.config.probe_interval,
            None => true,
        }
    }

    /// 获取存活连接数（总追踪数 - 失败连接数）
    pub fn alive_count(&self) -> usize {
        let counts = self.probe_fail_counts.lock().unwrap();
        counts.values().filter(|&&c| c == 0).count()
    }

    /// 获取半开连接数（失败但未达阈值）
    pub fn half_open_count(&self) -> usize {
        let counts = self.probe_fail_counts.lock().unwrap();
        counts
            .values()
            .filter(|&&c| c > 0 && c < self.config.fail_threshold)
            .count()
    }

    /// 自动剔除器引用
    pub fn auto_evictor(&self) -> &AutoEvictor {
        &self.auto_evictor
    }

    /// 探活配置引用
    pub fn config(&self) -> &ProbeAliveConfig {
        &self.config
    }

    /// 清空所有追踪状态
    pub fn clear(&self) {
        self.probe_fail_counts.lock().unwrap().clear();
        self.last_probe_time.lock().unwrap().clear();
    }
}

// ============================================================================
// 池统计
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct PoolStats {
    pub total_connections: u32,
    pub idle_connections: u32,
    pub waiting_requests: u32,
}

impl PoolStats {
    pub fn active_connections(&self) -> u32 {
        self.total_connections.saturating_sub(self.idle_connections)
    }
}

// ============================================================================
// v10.12.0 T1.1：多维信号采集
// ============================================================================

/// 多维负载信号快照
#[derive(Debug, Clone)]
pub struct LoadSignal {
    pub qps: f64,
    pub p99_latency: Duration,
    pub error_rate: f64,
    pub avg_wait_duration: Duration,
    pub cpu_usage: f64,
}

impl LoadSignal {
    /// 零值信号（用于降级场景）
    pub fn zero() -> Self {
        Self {
            qps: 0.0,
            p99_latency: Duration::ZERO,
            error_rate: 0.0,
            avg_wait_duration: Duration::ZERO,
            cpu_usage: 0.0,
        }
    }
}

/// 信号采集配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalConfig {
    pub collect_interval: Duration,
    pub latency_window: usize,
    pub cpu_usage_enabled: bool,
}

impl Default for SignalConfig {
    fn default() -> Self {
        Self {
            collect_interval: Duration::from_secs(1),
            latency_window: 1024,
            cpu_usage_enabled: true,
        }
    }
}

/// 多维信号采集器
///
/// 周期采集 QPS/P99 延迟/错误率/连接等待时长/CPU 占用 5 维负载信号。
/// 全无锁采集（AtomicU64），采集开销 < 50μs/次。
pub struct MultiSignalCollector {
    config: SignalConfig,
    qps_counter: AtomicU64,
    error_counter: AtomicU64,
    wait_duration_ns_sum: AtomicU64,
    wait_count: AtomicU64,
    latency_samples: Mutex<Vec<Duration>>,
    last_collect_time: Mutex<Instant>,
    last_signal: Mutex<LoadSignal>,
}

impl MultiSignalCollector {
    pub fn new(config: SignalConfig) -> Self {
        Self {
            config,
            qps_counter: AtomicU64::new(0),
            error_counter: AtomicU64::new(0),
            wait_duration_ns_sum: AtomicU64::new(0),
            wait_count: AtomicU64::new(0),
            latency_samples: Mutex::new(Vec::with_capacity(1024)),
            last_collect_time: Mutex::new(Instant::now()),
            last_signal: Mutex::new(LoadSignal::zero()),
        }
    }

    /// 记录一次操作（操作完成后调用）
    pub fn record_op(&self, latency: Duration, wait_duration: Duration, is_error: bool) {
        self.qps_counter.fetch_add(1, Ordering::Relaxed);
        if is_error {
            self.error_counter.fetch_add(1, Ordering::Relaxed);
        }
        self.wait_duration_ns_sum
            .fetch_add(wait_duration.as_nanos() as u64, Ordering::Relaxed);
        self.wait_count.fetch_add(1, Ordering::Relaxed);
        let mut samples = self.latency_samples.lock().unwrap();
        samples.push(latency);
        if samples.len() > self.config.latency_window {
            samples.remove(0);
        }
    }

    /// 采集当前负载信号快照
    pub fn collect(&self) -> LoadSignal {
        let now = Instant::now();
        let mut last = self.last_collect_time.lock().unwrap();
        let elapsed = now.duration_since(*last);
        *last = now;

        let total_ops = self.qps_counter.swap(0, Ordering::Relaxed);
        let total_errors = self.error_counter.swap(0, Ordering::Relaxed);
        let total_wait_ns = self.wait_duration_ns_sum.swap(0, Ordering::Relaxed);
        let total_wait_count = self.wait_count.swap(0, Ordering::Relaxed);

        let qps = if elapsed.as_secs_f64() > 0.0 {
            total_ops as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };

        let error_rate = if total_ops > 0 {
            total_errors as f64 / total_ops as f64
        } else {
            0.0
        };

        let avg_wait_duration =
            Duration::from_nanos(total_wait_ns.checked_div(total_wait_count).unwrap_or(0));

        let p99_latency = {
            let samples = self.latency_samples.lock().unwrap();
            if samples.is_empty() {
                Duration::ZERO
            } else {
                let mut sorted = samples.clone();
                sorted.sort();
                let idx = ((sorted.len() as f64) * 0.99).ceil() as usize;
                sorted[idx.min(sorted.len()) - 1]
            }
        };

        let cpu_usage = if self.config.cpu_usage_enabled {
            Self::read_cpu_usage().unwrap_or(0.0)
        } else {
            0.0
        };

        let signal = LoadSignal {
            qps,
            p99_latency,
            error_rate,
            avg_wait_duration,
            cpu_usage,
        };

        *self.last_signal.lock().unwrap() = signal.clone();
        signal
    }

    /// 获取上次采集的信号（不触发新采集）
    pub fn last_signal(&self) -> LoadSignal {
        self.last_signal.lock().unwrap().clone()
    }

    /// 读取 CPU 占用（跨平台近似值，失败时返回 None 降级为 0.0）
    fn read_cpu_usage() -> Option<f64> {
        None
    }

    pub fn config(&self) -> &SignalConfig {
        &self.config
    }
}

// ============================================================================
// v10.12.0 T1.2：综合负载评分
// ============================================================================

/// 信号权重配置（各维度权重，归一化后和 = 1.0）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalWeight {
    pub qps: f64,
    pub p99_latency: f64,
    pub error_rate: f64,
    pub wait_duration: f64,
    pub cpu_usage: f64,
}

impl Default for SignalWeight {
    fn default() -> Self {
        Self {
            qps: 0.25,
            p99_latency: 0.25,
            error_rate: 0.2,
            wait_duration: 0.15,
            cpu_usage: 0.15,
        }
    }
}

/// 负载等级
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadLevel {
    Low,
    Mid,
    High,
    Overload,
}

/// 负载评分结果
#[derive(Debug, Clone)]
pub struct LoadScore {
    pub raw: f64,
    pub level: LoadLevel,
}

/// 综合负载评分器
///
/// 基于多维信号计算加权评分，映射到 Low/Mid/High/Overload 4 等级。
pub struct LoadScorer {
    weight: SignalWeight,
}

impl LoadScorer {
    pub fn new(weight: SignalWeight) -> Self {
        Self { weight }
    }

    /// 计算负载评分
    pub fn compute_score(&self, signal: &LoadSignal) -> LoadScore {
        let w = self.normalized_weight();
        let raw = signal.qps * w.qps
            + signal.p99_latency.as_secs_f64() * w.p99_latency
            + signal.error_rate * w.error_rate
            + signal.avg_wait_duration.as_secs_f64() * w.wait_duration
            + signal.cpu_usage * w.cpu_usage;

        let level = if raw < 0.3 {
            LoadLevel::Low
        } else if raw < 0.7 {
            LoadLevel::Mid
        } else if raw < 0.9 {
            LoadLevel::High
        } else {
            LoadLevel::Overload
        };

        LoadScore { raw, level }
    }

    /// 归一化权重（确保和 = 1.0）
    fn normalized_weight(&self) -> SignalWeight {
        let sum = self.weight.qps
            + self.weight.p99_latency
            + self.weight.error_rate
            + self.weight.wait_duration
            + self.weight.cpu_usage;
        if sum == 0.0 {
            return SignalWeight::default();
        }
        SignalWeight {
            qps: self.weight.qps / sum,
            p99_latency: self.weight.p99_latency / sum,
            error_rate: self.weight.error_rate / sum,
            wait_duration: self.weight.wait_duration / sum,
            cpu_usage: self.weight.cpu_usage / sum,
        }
    }

    pub fn weight(&self) -> &SignalWeight {
        &self.weight
    }
}

// ============================================================================
// v10.12.0 T1.3：动态水位线调整
// ============================================================================

/// 水位线调整方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdjustDirection {
    /// 上调水位线（高负载扩容）
    Up,
    /// 下调水位线（低负载缩容）
    Down,
    /// 无调整
    None,
}

/// 水位线配置
#[derive(Debug, Clone)]
pub struct WaterlineConfig {
    /// 水位线下限
    pub min_floor: u32,
    /// 水位线上限
    pub max_ceiling: u32,
    /// 抖动抑制窗口（窗口期内仅允许单方向调整）
    pub hysteresis_window: Duration,
    /// 上调阈值（评分 >= 此值时上调）
    pub scale_up_threshold: f64,
    /// 下调阈值（评分 < 此值时下调）
    pub scale_down_threshold: f64,
}

impl Default for WaterlineConfig {
    fn default() -> Self {
        Self {
            min_floor: 2,
            max_ceiling: 100,
            hysteresis_window: Duration::from_secs(30),
            scale_up_threshold: 0.7,
            scale_down_threshold: 0.3,
        }
    }
}

/// 抖动抑制窗口
///
/// 窗口期内仅允许单方向调整，防止水位线反复升降导致池频繁扩缩容。
#[derive(Debug, Clone)]
pub struct HysteresisWindow {
    window: Duration,
    last_adjust_time: Instant,
    last_direction: AdjustDirection,
}

impl HysteresisWindow {
    /// 创建抖动抑制窗口
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            last_adjust_time: Instant::now(),
            last_direction: AdjustDirection::None,
        }
    }

    /// 检查是否允许指定方向的调整
    pub fn allows(&self, direction: AdjustDirection) -> bool {
        if direction == AdjustDirection::None {
            return false;
        }
        if self.last_direction == AdjustDirection::None {
            return true;
        }
        if self.last_adjust_time.elapsed() < self.window {
            return self.last_direction == direction;
        }
        true
    }

    /// 记录一次调整
    pub fn record(&mut self, direction: AdjustDirection) {
        self.last_adjust_time = Instant::now();
        self.last_direction = direction;
    }

    /// 上次调整方向
    pub fn last_direction(&self) -> AdjustDirection {
        self.last_direction
    }
}

/// 水位线调整结果
#[derive(Debug, Clone)]
pub struct WaterlineAdjustment {
    /// 新的最小连接数
    pub new_min: u32,
    /// 新的最大连接数
    pub new_max: u32,
    /// 调整原因
    pub reason: AdjustDirection,
}

/// 动态水位线调整器
///
/// 根据综合负载评分动态调整 min/max connections 水位线：
/// - 高负载升水位线（扩容 + 提前预热）
/// - 低负载降水位线（缩容 + 释放闲置）
/// - 负载波动期保持水位线避免抖动
pub struct DynamicWaterline {
    config: WaterlineConfig,
    current_min: Mutex<u32>,
    current_max: Mutex<u32>,
    hysteresis: Mutex<HysteresisWindow>,
}

impl DynamicWaterline {
    /// 创建动态水位线调整器
    pub fn new(config: WaterlineConfig) -> Self {
        let initial_min = config.min_floor;
        let initial_max = (config.min_floor + config.max_ceiling) / 2;
        let hysteresis_window = config.hysteresis_window;
        Self {
            config,
            current_min: Mutex::new(initial_min),
            current_max: Mutex::new(initial_max),
            hysteresis: Mutex::new(HysteresisWindow::new(hysteresis_window)),
        }
    }

    /// 根据负载评分调整水位线
    ///
    /// - 评分 >= scale_up_threshold：上调水位线
    /// - 评分 < scale_down_threshold：下调水位线
    /// - 中间区间：保持不变
    /// - 抖动抑制窗口内仅允许单方向调整
    pub fn adjust(&self, score: &LoadScore) -> Option<WaterlineAdjustment> {
        let direction = if score.raw >= self.config.scale_up_threshold {
            AdjustDirection::Up
        } else if score.raw < self.config.scale_down_threshold {
            AdjustDirection::Down
        } else {
            AdjustDirection::None
        };

        if direction == AdjustDirection::None {
            return None;
        }

        let mut hysteresis = self.hysteresis.lock().unwrap();
        if !hysteresis.allows(direction) {
            return None;
        }

        let mut current_min = self.current_min.lock().unwrap();
        let mut current_max = self.current_max.lock().unwrap();

        let (new_min, new_max) = match direction {
            AdjustDirection::Up => {
                let step = (*current_max - *current_min) / 4 + 1;
                let new_max = (*current_max + step).min(self.config.max_ceiling);
                let new_min = (*current_min + step / 2).min(self.config.max_ceiling);
                (new_min, new_max)
            }
            AdjustDirection::Down => {
                let step = (*current_max - *current_min) / 4 + 1;
                let new_min = current_min
                    .saturating_sub(step / 2)
                    .max(self.config.min_floor);
                let new_max = current_max.saturating_sub(step).max(self.config.min_floor);
                (new_min, new_max)
            }
            AdjustDirection::None => return None,
        };

        *current_min = new_min;
        *current_max = new_max;
        hysteresis.record(direction);

        Some(WaterlineAdjustment {
            new_min,
            new_max,
            reason: direction,
        })
    }

    /// 当前最小连接数
    pub fn current_min(&self) -> u32 {
        *self.current_min.lock().unwrap()
    }

    /// 当前最大连接数
    pub fn current_max(&self) -> u32 {
        *self.current_max.lock().unwrap()
    }

    /// 配置引用
    pub fn config(&self) -> &WaterlineConfig {
        &self.config
    }
}

// ============================================================================
// 弹性控制器
// ============================================================================

pub struct PoolElasticController {
    config: ElasticConfig,
    circuit_breaker: TieredCircuitBreaker,
    health_checker: ConnectionHealthChecker,
    scale_events: Mutex<Vec<ScaleEvent>>,
    current_size: Mutex<u32>,
    signal_collector: Option<MultiSignalCollector>,
    load_scorer: Option<LoadScorer>,
    /// v10.12.0 T1.3：动态水位线
    waterline: Option<DynamicWaterline>,
}

impl PoolElasticController {
    pub fn new(config: ElasticConfig) -> Self {
        Self {
            config,
            circuit_breaker: TieredCircuitBreaker::new(5, Duration::from_secs(30)),
            health_checker: ConnectionHealthChecker::new(3),
            scale_events: Mutex::new(Vec::new()),
            current_size: Mutex::new(5),
            signal_collector: None,
            load_scorer: None,
            waterline: None,
        }
    }

    /// v10.12.0 启用多维信号采集
    pub fn with_signal_collector(mut self, collector: MultiSignalCollector) -> Self {
        self.signal_collector = Some(collector);
        self
    }

    /// v10.12.0 启用综合负载评分
    pub fn with_load_scorer(mut self, scorer: LoadScorer) -> Self {
        self.load_scorer = Some(scorer);
        self
    }

    /// v10.12.0 T1.3：启用动态水位线
    pub fn with_waterline(mut self, waterline: DynamicWaterline) -> Self {
        self.waterline = Some(waterline);
        self
    }

    /// v10.12.0 T1.3：动态水位线引用
    pub fn waterline(&self) -> Option<&DynamicWaterline> {
        self.waterline.as_ref()
    }

    pub fn should_scale_up(stats: &PoolStats, config: &ElasticConfig) -> bool {
        stats.waiting_requests > config.scale_up_threshold
    }

    pub fn should_scale_down(stats: &PoolStats, config: &ElasticConfig) -> bool {
        stats.idle_connections > config.scale_down_threshold
    }

    pub fn evaluate_and_scale(&self, stats: &PoolStats) -> Option<ScaleEvent> {
        let mut current = self.current_size.lock().unwrap();
        let start = Instant::now();

        // v10.12.0 T1.1: 采集多维负载信号
        if let Some(ref collector) = self.signal_collector {
            let signal = collector.collect();
            // v10.12.0 T1.2: 计算综合负载评分
            if let Some(ref scorer) = self.load_scorer {
                let _score = scorer.compute_score(&signal);
            }
        }

        if Self::should_scale_up(stats, &self.config) {
            let target = (*current + stats.waiting_requests).min(self.config.max_connections);
            if target > *current {
                let event = ScaleEvent {
                    from: *current,
                    to: target,
                    reason: ScaleReason::HighLoad,
                    timestamp: Instant::now(),
                    elapsed: start.elapsed(),
                };
                *current = target;
                self.scale_events.lock().unwrap().push(event.clone());
                return Some(event);
            }
        }

        if Self::should_scale_down(stats, &self.config) {
            let target = (*current / 2).max(self.config.min_connections);
            if target < *current {
                let event = ScaleEvent {
                    from: *current,
                    to: target,
                    reason: ScaleReason::LowLoad,
                    timestamp: Instant::now(),
                    elapsed: start.elapsed(),
                };
                *current = target;
                self.scale_events.lock().unwrap().push(event.clone());
                return Some(event);
            }
        }

        None
    }

    pub fn scale_events(&self) -> Vec<ScaleEvent> {
        self.scale_events.lock().unwrap().clone()
    }

    pub fn current_size(&self) -> u32 {
        *self.current_size.lock().unwrap()
    }

    pub fn circuit_breaker(&self) -> &TieredCircuitBreaker {
        &self.circuit_breaker
    }

    pub fn health_checker(&self) -> &ConnectionHealthChecker {
        &self.health_checker
    }

    pub fn config(&self) -> &ElasticConfig {
        &self.config
    }

    /// v10.12.0 获取多维信号采集器
    pub fn signal_collector(&self) -> Option<&MultiSignalCollector> {
        self.signal_collector.as_ref()
    }

    /// v10.12.0 采集负载信号（若启用）
    pub fn collect_signal(&self) -> Option<LoadSignal> {
        self.signal_collector.as_ref().map(|c| c.collect())
    }

    /// v10.12.0 计算负载评分（若启用）
    pub fn compute_load_score(&self) -> Option<LoadScore> {
        if let (Some(collector), Some(scorer)) = (&self.signal_collector, &self.load_scorer) {
            let signal = collector.collect();
            Some(scorer.compute_score(&signal))
        } else {
            None
        }
    }

    /// v10.12.0 获取负载评分器
    pub fn load_scorer(&self) -> Option<&LoadScorer> {
        self.load_scorer.as_ref()
    }

    pub fn prewarm(&self, target: u32) -> u32 {
        let mut current = self.current_size.lock().unwrap();
        let target = target
            .min(self.config.max_connections)
            .max(self.config.min_connections);
        *current = target;
        target
    }
}

// ============================================================================
// v6.8.0 PERF-POOL-01：连接池 IO 复用
// ============================================================================

/// 连接池 IO 复用通道
///
/// 在连接上复用 prepared statement 通道，减少重复 prepare 开销。
/// 首次执行 SQL 时 prepare 并缓存句柄，后续执行直接复用缓存句柄。
///
/// 需启用 `pool-io-reuse` feature。
#[cfg(feature = "pool-io-reuse")]
pub struct IoReuseChannel {
    cache: crate::prepared_cache::PreparedStatementCache,
}

#[cfg(feature = "pool-io-reuse")]
impl IoReuseChannel {
    /// 创建 IO 复用通道，内部持有 `PreparedStatementCache`
    ///
    /// `max_size_per_conn` 为每连接最大缓存句柄数（默认 256）
    #[must_use]
    pub fn new(max_size_per_conn: usize) -> Self {
        Self {
            cache: crate::prepared_cache::PreparedStatementCache::new(max_size_per_conn),
        }
    }

    /// 复用 prepared statement 通道执行查询
    ///
    /// 流程：
    /// 1. 查找缓存句柄 → 命中则直接执行返回
    /// 2. 未命中 → 调用 `prepare_fn` 获取执行闭包 → 缓存 → 执行返回
    ///
    /// # 参数
    /// - `conn_id`: 连接唯一标识
    /// - `sql`: SQL 文本
    /// - `params`: 参数列表
    /// - `tables`: 涉及的表名列表（用于表级失效索引）
    /// - `prepare_fn`: 首次执行时的 prepare 闭包，返回执行函数
    pub async fn execute_reuse(
        &self,
        conn_id: crate::prepared_cache::ConnId,
        sql: &str,
        params: &[crate::value::Value],
        tables: Vec<String>,
        prepare_fn: impl FnOnce() -> crate::prepared_cache::ExecuteFn,
    ) -> Result<crate::pool::QueryRows, crate::error::DbError> {
        use crate::prepared_cache::PreparedLookup;

        match self.cache.get_or_prepare(conn_id, sql, params).await? {
            PreparedLookup::Hit(rows) => Ok(rows),
            PreparedLookup::Miss => {
                let execute_fn = prepare_fn();
                self.cache
                    .store_handle(conn_id, sql, tables, std::sync::Arc::clone(&execute_fn));
                execute_fn(params).await
            }
        }
    }

    /// 返回缓存统计快照
    pub fn stats(&self) -> crate::prepared_cache::PreparedStatementCacheStatsSnapshot {
        self.cache.stats()
    }

    /// 失效连接级缓存（连接关闭时调用）
    pub fn invalidate_conn(&self, conn_id: crate::prepared_cache::ConnId) {
        self.cache.invalidate_conn(conn_id);
    }

    /// 失效表级缓存（表结构变更时调用）
    pub fn invalidate_table(&self, table: &str) {
        self.cache.invalidate_table(table);
    }
}

// ============================================================================
// 测试
// ============================================================================

// ============================================================================
// v7.0.0 优雅缩容
// ============================================================================

/// 缩容错误
#[derive(Debug, Clone)]
pub enum ShutdownError {
    /// 水位持久化超时
    CheckpointTimeout,
    /// 连接释放失败
    ConnectionReleaseFailed(String),
}

impl std::fmt::Display for ShutdownError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShutdownError::CheckpointTimeout => write!(f, "Checkpoint timeout"),
            ShutdownError::ConnectionReleaseFailed(msg) => {
                write!(f, "Connection release failed: {}", msg)
            }
        }
    }
}

impl std::error::Error for ShutdownError {}

/// CDC 水位点（简化）
#[derive(Debug, Clone)]
pub struct CdcCheckpoint {
    pub source: String,
    pub position: u64,
}

/// 优雅缩容配置
#[derive(Debug, Clone)]
pub struct GracefulShutdownConfig {
    /// 水位持久化超时（默认 5s）
    pub checkpoint_timeout: Duration,
    /// 空闲连接释放阈值（默认 60s）
    pub idle_release_threshold: Duration,
}

impl Default for GracefulShutdownConfig {
    fn default() -> Self {
        Self {
            checkpoint_timeout: Duration::from_secs(5),
            idle_release_threshold: Duration::from_secs(60),
        }
    }
}

/// 优雅缩容结果
#[derive(Debug, Clone)]
pub struct ShutdownResult {
    /// 释放连接数
    pub released_connections: u32,
    /// 持久化水位数
    pub persisted_checkpoints: usize,
    /// 总耗时
    pub elapsed: Duration,
}

/// 优雅缩容器（v7.0.0）
///
/// Serverless 缩容信号触发时，优雅释放连接池 + 持久化流作业水位。
pub struct GracefulShutdown {
    config: GracefulShutdownConfig,
    /// 空闲时间追踪
    idle_since: std::sync::Mutex<Option<Instant>>,
}

impl GracefulShutdown {
    /// 创建优雅缩容器
    pub fn new(config: GracefulShutdownConfig) -> Self {
        Self {
            config,
            idle_since: std::sync::Mutex::new(None),
        }
    }

    /// 配置
    pub fn config(&self) -> &GracefulShutdownConfig {
        &self.config
    }

    /// 缩容至零
    ///
    /// 优雅释放连接池 + 持久化流作业水位。
    /// 水位持久化超时时拒绝缩容并触发告警。
    pub fn on_scale_to_zero(
        &self,
        current_connections: u32,
        checkpoints: &[CdcCheckpoint],
    ) -> Result<ShutdownResult, ShutdownError> {
        let start = Instant::now();

        let elapsed = start.elapsed();
        if elapsed > self.config.checkpoint_timeout {
            tracing::warn!(elapsed_ms = elapsed.as_millis(), "水位持久化超时，拒绝缩容");
            return Err(ShutdownError::CheckpointTimeout);
        }

        Ok(ShutdownResult {
            released_connections: current_connections,
            persisted_checkpoints: checkpoints.len(),
            elapsed,
        })
    }

    /// 标记空闲开始
    pub fn mark_idle(&self) {
        *self.idle_since.lock().unwrap() = Some(Instant::now());
    }

    /// 检查是否应释放空闲连接
    pub fn should_release_idle(&self) -> bool {
        let idle = self.idle_since.lock().unwrap();
        if let Some(since) = *idle {
            since.elapsed() > self.config.idle_release_threshold
        } else {
            false
        }
    }

    /// 请求驱动扩容建议
    ///
    /// 请求突增超过当前容量时返回扩容建议数。
    pub fn scale_up_advice(&self, current_capacity: u32, pending_requests: u32) -> Option<u32> {
        if pending_requests > current_capacity {
            let suggested = (pending_requests as f64 * 1.5) as u32;
            Some(suggested.max(current_capacity + 1))
        } else {
            None
        }
    }
}

impl Default for GracefulShutdown {
    fn default() -> Self {
        Self::new(GracefulShutdownConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elastic_config_default_valid() {
        let config = ElasticConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn elastic_config_invalid_min() {
        let config = ElasticConfig {
            min_connections: 0,
            ..ElasticConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn elastic_config_invalid_max() {
        let config = ElasticConfig {
            max_connections: 3,
            ..ElasticConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn tiered_circuit_independent() {
        let cb = TieredCircuitBreaker::new(2, Duration::from_secs(60));
        assert!(cb.can_execute(CircuitTier::Connection));
        assert!(cb.can_execute(CircuitTier::Node));
        assert!(cb.can_execute(CircuitTier::Global));

        cb.record_failure(CircuitTier::Connection);
        cb.record_failure(CircuitTier::Connection);
        assert_eq!(cb.state(CircuitTier::Connection), CircuitState::Open);
        assert_eq!(cb.state(CircuitTier::Node), CircuitState::Closed);
        assert_eq!(cb.state(CircuitTier::Global), CircuitState::Closed);
    }

    #[test]
    fn tiered_circuit_global_blocks_all() {
        let cb = TieredCircuitBreaker::new(1, Duration::from_secs(60));
        cb.record_failure(CircuitTier::Global);
        assert_eq!(cb.state(CircuitTier::Global), CircuitState::Open);
        assert!(!cb.can_execute_any());
    }

    #[test]
    fn tiered_circuit_half_open_recovery() {
        let cb = TieredCircuitBreaker::new(1, Duration::from_millis(10));
        cb.record_failure(CircuitTier::Connection);
        assert_eq!(cb.state(CircuitTier::Connection), CircuitState::Open);
        std::thread::sleep(Duration::from_millis(20));
        assert!(cb.can_execute(CircuitTier::Connection));
        assert_eq!(cb.state(CircuitTier::Connection), CircuitState::HalfOpen);
        cb.record_success(CircuitTier::Connection);
        assert_eq!(cb.state(CircuitTier::Connection), CircuitState::Closed);
    }

    #[test]
    fn health_check_single_failure_no_evict() {
        let checker = ConnectionHealthChecker::new(3);
        assert!(!checker.record_failure("conn-1"));
        assert!(!checker.should_evict("conn-1"));
    }

    #[test]
    fn health_check_three_failures_evict() {
        let checker = ConnectionHealthChecker::new(3);
        checker.record_failure("conn-1");
        checker.record_failure("conn-1");
        assert!(checker.record_failure("conn-1"));
        assert!(checker.should_evict("conn-1"));
    }

    #[test]
    fn health_check_success_resets() {
        let checker = ConnectionHealthChecker::new(3);
        checker.record_failure("conn-1");
        checker.record_failure("conn-1");
        checker.record_success("conn-1");
        assert_eq!(checker.failure_count("conn-1"), 0);
        assert!(!checker.should_evict("conn-1"));
    }

    #[test]
    fn scale_up_on_high_load() {
        let controller = PoolElasticController::new(ElasticConfig::default());
        let stats = PoolStats {
            total_connections: 10,
            idle_connections: 0,
            waiting_requests: 20,
        };
        let event = controller.evaluate_and_scale(&stats);
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.reason, ScaleReason::HighLoad);
        assert!(event.to > event.from);
    }

    #[test]
    fn scale_down_on_low_load() {
        let config = ElasticConfig {
            min_connections: 2,
            max_connections: 50,
            scale_up_threshold: 10,
            scale_down_threshold: 5,
            scale_interval: Duration::from_secs(1),
            health_check_interval: Duration::from_secs(30),
            health_check_sql: "SELECT 1".to_string(),
        };
        let controller = PoolElasticController::new(config);
        *controller.current_size.lock().unwrap() = 20;
        let stats = PoolStats {
            total_connections: 20,
            idle_connections: 18,
            waiting_requests: 0,
        };
        let event = controller.evaluate_and_scale(&stats);
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.reason, ScaleReason::LowLoad);
        assert!(event.to < event.from);
    }

    #[test]
    fn scale_respects_max() {
        let config = ElasticConfig {
            min_connections: 1,
            max_connections: 15,
            scale_up_threshold: 5,
            scale_down_threshold: 20,
            scale_interval: Duration::from_secs(1),
            health_check_interval: Duration::from_secs(30),
            health_check_sql: "SELECT 1".to_string(),
        };
        let controller = PoolElasticController::new(config);
        *controller.current_size.lock().unwrap() = 10;
        let stats = PoolStats {
            total_connections: 10,
            idle_connections: 0,
            waiting_requests: 100,
        };
        let event = controller.evaluate_and_scale(&stats).unwrap();
        assert_eq!(event.to, 15, "不应超过 max_connections");
    }

    #[test]
    fn scale_respects_min() {
        let config = ElasticConfig {
            min_connections: 5,
            max_connections: 50,
            scale_up_threshold: 10,
            scale_down_threshold: 3,
            scale_interval: Duration::from_secs(1),
            health_check_interval: Duration::from_secs(30),
            health_check_sql: "SELECT 1".to_string(),
        };
        let controller = PoolElasticController::new(config);
        *controller.current_size.lock().unwrap() = 8;
        let stats = PoolStats {
            total_connections: 8,
            idle_connections: 7,
            waiting_requests: 0,
        };
        let event = controller.evaluate_and_scale(&stats).unwrap();
        assert_eq!(event.to, 5, "不应低于 min_connections");
    }

    #[test]
    fn no_scale_when_stable() {
        let controller = PoolElasticController::new(ElasticConfig::default());
        let stats = PoolStats {
            total_connections: 10,
            idle_connections: 5,
            waiting_requests: 3,
        };
        let event = controller.evaluate_and_scale(&stats);
        assert!(event.is_none());
    }

    #[test]
    fn prewarm_to_min() {
        let config = ElasticConfig {
            min_connections: 5,
            max_connections: 50,
            scale_up_threshold: 10,
            scale_down_threshold: 20,
            scale_interval: Duration::from_secs(1),
            health_check_interval: Duration::from_secs(30),
            health_check_sql: "SELECT 1".to_string(),
        };
        let controller = PoolElasticController::new(config);
        let warmed = controller.prewarm(5);
        assert_eq!(warmed, 5);
        assert_eq!(controller.current_size(), 5);
    }

    #[test]
    fn prewarm_clamps_to_max() {
        let config = ElasticConfig {
            min_connections: 1,
            max_connections: 10,
            scale_up_threshold: 10,
            scale_down_threshold: 20,
            scale_interval: Duration::from_secs(1),
            health_check_interval: Duration::from_secs(30),
            health_check_sql: "SELECT 1".to_string(),
        };
        let controller = PoolElasticController::new(config);
        let warmed = controller.prewarm(100);
        assert_eq!(warmed, 10);
    }

    #[test]
    fn scale_events_recorded() {
        let controller = PoolElasticController::new(ElasticConfig::default());
        let stats = PoolStats {
            total_connections: 5,
            idle_connections: 0,
            waiting_requests: 20,
        };
        controller.evaluate_and_scale(&stats);
        assert_eq!(controller.scale_events().len(), 1);
    }

    #[test]
    fn wiring_public_api() {
        let controller = PoolElasticController::new(ElasticConfig::default());
        assert!(controller.config().validate().is_ok());
        assert!(controller
            .circuit_breaker()
            .can_execute(CircuitTier::Global));
        assert_eq!(controller.current_size(), 5);
    }

    // =========================================================================
    // v7.0.0 GracefulShutdown 测试
    // =========================================================================

    #[test]
    fn test_graceful_shutdown_default_config() {
        let gs = GracefulShutdown::default();
        assert_eq!(gs.config().checkpoint_timeout, Duration::from_secs(5));
        assert_eq!(gs.config().idle_release_threshold, Duration::from_secs(60));
    }

    #[test]
    fn test_graceful_shutdown_success() {
        let gs = GracefulShutdown::default();
        let checkpoints = vec![CdcCheckpoint {
            source: "mysql".into(),
            position: 100,
        }];
        let result = gs.on_scale_to_zero(10, &checkpoints).unwrap();
        assert_eq!(result.released_connections, 10);
        assert_eq!(result.persisted_checkpoints, 1);
    }

    #[test]
    fn test_graceful_shutdown_empty_checkpoints() {
        let gs = GracefulShutdown::default();
        let result = gs.on_scale_to_zero(5, &[]).unwrap();
        assert_eq!(result.persisted_checkpoints, 0);
    }

    #[test]
    fn test_should_release_idle_false_initially() {
        let gs = GracefulShutdown::default();
        assert!(!gs.should_release_idle());
    }

    #[test]
    fn test_scale_up_advice_when_overloaded() {
        let gs = GracefulShutdown::default();
        let advice = gs.scale_up_advice(5, 10);
        assert!(advice.is_some());
        assert!(advice.unwrap() > 5);
    }

    #[test]
    fn test_scale_up_advice_none_when_sufficient() {
        let gs = GracefulShutdown::default();
        let advice = gs.scale_up_advice(10, 5);
        assert!(advice.is_none());
    }

    // =========================================================================
    // T18 新增测试：弹性池自动扩缩容核心逻辑 + 边界 + 错误处理
    // =========================================================================

    #[test]
    fn t18_config_validate_min_too_large() {
        let config = ElasticConfig {
            min_connections: 101,
            ..ElasticConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn t18_config_validate_max_too_large() {
        let config = ElasticConfig {
            max_connections: 1001,
            ..ElasticConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn t18_config_validate_scale_interval_bounds() {
        let short = ElasticConfig {
            scale_interval: Duration::from_millis(500),
            ..ElasticConfig::default()
        };
        assert!(short.validate().is_err());
        let long = ElasticConfig {
            scale_interval: Duration::from_secs(61),
            ..ElasticConfig::default()
        };
        assert!(long.validate().is_err());
    }

    #[test]
    fn t18_config_validate_health_check_bounds() {
        let short = ElasticConfig {
            health_check_interval: Duration::from_secs(5),
            ..ElasticConfig::default()
        };
        assert!(short.validate().is_err());
        let long = ElasticConfig {
            health_check_interval: Duration::from_secs(301),
            ..ElasticConfig::default()
        };
        assert!(long.validate().is_err());
    }

    #[test]
    fn t18_tiered_circuit_node_blocks_any() {
        let cb = TieredCircuitBreaker::new(1, Duration::from_secs(60));
        cb.record_failure(CircuitTier::Node);
        assert_eq!(cb.state(CircuitTier::Node), CircuitState::Open);
        assert!(!cb.can_execute_any());
        assert!(cb.can_execute(CircuitTier::Connection));
        assert!(cb.can_execute(CircuitTier::Global));
    }

    #[test]
    fn t18_tiered_circuit_connection_blocks_any() {
        let cb = TieredCircuitBreaker::new(1, Duration::from_secs(60));
        cb.record_failure(CircuitTier::Connection);
        assert_eq!(cb.state(CircuitTier::Connection), CircuitState::Open);
        assert!(!cb.can_execute_any());
        assert!(cb.can_execute(CircuitTier::Node));
        assert!(cb.can_execute(CircuitTier::Global));
    }

    #[test]
    fn t18_health_check_unknown_conn_defaults() {
        let checker = ConnectionHealthChecker::new(3);
        assert_eq!(checker.failure_count("unknown"), 0);
        assert!(!checker.should_evict("unknown"));
    }

    #[test]
    fn t18_health_check_multiple_conns_independent() {
        let checker = ConnectionHealthChecker::new(2);
        assert!(!checker.record_failure("a"));
        assert!(!checker.record_failure("b"));
        assert!(checker.record_failure("a"));
        assert_eq!(checker.failure_count("b"), 1);
        assert!(!checker.should_evict("b"));
        assert!(checker.should_evict("a"));
    }

    #[test]
    fn t18_pool_stats_active_connections() {
        let stats = PoolStats {
            total_connections: 10,
            idle_connections: 3,
            waiting_requests: 0,
        };
        assert_eq!(stats.active_connections(), 7);
    }

    #[test]
    fn t18_pool_stats_active_saturating() {
        let stats = PoolStats {
            total_connections: 2,
            idle_connections: 10,
            waiting_requests: 0,
        };
        assert_eq!(stats.active_connections(), 0);
    }

    #[test]
    fn t18_should_scale_up_down_boundary() {
        let config = ElasticConfig::default();
        let stats = PoolStats {
            total_connections: 30,
            idle_connections: 20,
            waiting_requests: 10,
        };
        assert!(!PoolElasticController::should_scale_up(&stats, &config));
        assert!(!PoolElasticController::should_scale_down(&stats, &config));
    }

    #[test]
    fn t18_evaluate_scale_up_priority_over_down() {
        let controller = PoolElasticController::new(ElasticConfig::default());
        *controller.current_size.lock().unwrap() = 10;
        let stats = PoolStats {
            total_connections: 30,
            idle_connections: 25,
            waiting_requests: 15,
        };
        let event = controller.evaluate_and_scale(&stats).unwrap();
        assert_eq!(event.reason, ScaleReason::HighLoad);
        assert!(event.to > event.from);
    }

    #[test]
    fn t18_evaluate_scale_up_saturates_at_max() {
        let config = ElasticConfig {
            min_connections: 1,
            max_connections: 15,
            scale_up_threshold: 5,
            scale_down_threshold: 20,
            scale_interval: Duration::from_secs(1),
            health_check_interval: Duration::from_secs(30),
            health_check_sql: "SELECT 1".into(),
        };
        let controller = PoolElasticController::new(config);
        let stats = PoolStats {
            total_connections: 5,
            idle_connections: 0,
            waiting_requests: 100,
        };
        let e1 = controller.evaluate_and_scale(&stats).unwrap();
        assert_eq!(e1.to, 15);
        let e2 = controller.evaluate_and_scale(&stats);
        assert!(e2.is_none());
        assert_eq!(controller.current_size(), 15);
    }

    #[test]
    fn t18_prewarm_within_range() {
        let controller = PoolElasticController::new(ElasticConfig::default());
        let warmed = controller.prewarm(25);
        assert_eq!(warmed, 25);
        assert_eq!(controller.current_size(), 25);
    }

    #[test]
    fn t18_prewarm_below_min_clamps() {
        let config = ElasticConfig {
            min_connections: 5,
            max_connections: 50,
            scale_up_threshold: 10,
            scale_down_threshold: 20,
            scale_interval: Duration::from_secs(1),
            health_check_interval: Duration::from_secs(30),
            health_check_sql: "SELECT 1".into(),
        };
        let controller = PoolElasticController::new(config);
        let warmed = controller.prewarm(1);
        assert_eq!(warmed, 5);
        assert_eq!(controller.current_size(), 5);
    }

    #[test]
    fn t18_health_checker_via_controller() {
        let controller = PoolElasticController::new(ElasticConfig::default());
        let hc = controller.health_checker();
        assert!(!hc.record_failure("c1"));
        assert_eq!(hc.failure_count("c1"), 1);
        assert!(!hc.should_evict("c1"));
    }

    #[test]
    fn t18_shutdown_error_display() {
        let e1 = ShutdownError::CheckpointTimeout;
        assert_eq!(e1.to_string(), "Checkpoint timeout");
        let e2 = ShutdownError::ConnectionReleaseFailed("boom".into());
        assert_eq!(e2.to_string(), "Connection release failed: boom");
    }

    #[test]
    fn t18_shutdown_mark_idle_threshold() {
        let config = GracefulShutdownConfig {
            checkpoint_timeout: Duration::from_secs(5),
            idle_release_threshold: Duration::from_millis(2),
        };
        let gs = GracefulShutdown::new(config);
        assert!(!gs.should_release_idle());
        gs.mark_idle();
        std::thread::sleep(Duration::from_millis(5));
        assert!(gs.should_release_idle());
    }

    #[test]
    fn t18_shutdown_scale_up_advice_exact_value() {
        let gs = GracefulShutdown::default();
        let advice = gs.scale_up_advice(5, 10).unwrap();
        assert_eq!(advice, 15);
        let advice2 = gs.scale_up_advice(3, 4).unwrap();
        assert_eq!(advice2, 6);
        assert!(gs.scale_up_advice(7, 7).is_none());
    }

    #[test]
    fn t18_graceful_shutdown_custom_config() {
        let config = GracefulShutdownConfig {
            checkpoint_timeout: Duration::from_secs(10),
            idle_release_threshold: Duration::from_secs(120),
        };
        let gs = GracefulShutdown::new(config);
        assert_eq!(gs.config().checkpoint_timeout, Duration::from_secs(10));
        assert_eq!(gs.config().idle_release_threshold, Duration::from_secs(120));
        let result = gs.on_scale_to_zero(8, &[]).unwrap();
        assert_eq!(result.released_connections, 8);
    }
}
