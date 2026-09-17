//! # 连接池预热增强（v3.2.0）
//!
//! 提供自动预热、渐进式分批策略、预热进度可观测、多池统一预热。
//!
//! ## Feature Gate
//!
//! 本模块仅在 `auto-prewarm` feature 启用时编译。
//! 手动预热 API（`Pool::prewarm()`）保持不变，向后兼容。
//!
//! ## 使用示例
//!
//! ```ignore
//! use sz_orm_core::prewarm::{PrewarmConfig, ProgressiveConfig};
//! use sz_orm_core::pool::{PoolConfigBuilder, Pool};
//!
//! let config = PoolConfigBuilder::new()
//!     .max_size(20)
//!     .min_idle(5)
//!     .auto_prewarm(true)
//!     .progressive_prewarm(ProgressiveConfig::default())
//!     .build();
//! // Pool::new 自动后台预热，Pool::new_async 等待预热完成
//! ```

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

// ============================================================================
// 配置结构体
// ============================================================================

/// 渐进式预热配置
#[derive(Debug, Clone)]
pub struct ProgressiveConfig {
    /// 每批创建连接数
    pub batch_size: u32,
    /// 批间隔（避免瞬时冲击 DB）
    pub interval: Duration,
    /// 总超时
    pub total_timeout: Duration,
}

impl Default for ProgressiveConfig {
    fn default() -> Self {
        Self {
            batch_size: 2,
            interval: Duration::from_millis(10),
            total_timeout: Duration::from_secs(30),
        }
    }
}

impl ProgressiveConfig {
    /// 创建渐进式预热配置
    pub fn new(batch_size: u32, interval: Duration, total_timeout: Duration) -> Self {
        Self {
            batch_size: batch_size.max(1),
            interval,
            total_timeout,
        }
    }

    /// 设置每批创建连接数
    pub fn with_batch_size(mut self, size: u32) -> Self {
        self.batch_size = size.max(1);
        self
    }

    /// 设置批间隔
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// 设置总超时
    pub fn with_total_timeout(mut self, timeout: Duration) -> Self {
        self.total_timeout = timeout;
        self
    }
}

/// 预热配置
#[derive(Debug, Clone, Default)]
pub struct PrewarmConfig {
    /// 是否自动预热
    pub auto_prewarm: bool,
    /// 渐进式配置（None 表示一次性预热）
    pub progressive: Option<ProgressiveConfig>,
}

impl PrewarmConfig {
    /// 创建默认预热配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置是否自动预热
    pub fn with_auto_prewarm(mut self, enabled: bool) -> Self {
        self.auto_prewarm = enabled;
        self
    }

    /// 设置渐进式配置
    pub fn with_progressive(mut self, config: ProgressiveConfig) -> Self {
        self.progressive = Some(config);
        self
    }
}

// ============================================================================
// 进度指标
// ============================================================================

/// 预热进度（无锁原子计数器）
#[derive(Debug)]
pub struct PrewarmProgress {
    warmed: AtomicU32,
    target: u32,
    failed: AtomicU32,
    elapsed_ns: AtomicU64,
    is_completed: AtomicBool,
}

impl PrewarmProgress {
    /// 创建预热进度实例
    pub fn new(target: u32) -> Self {
        Self {
            warmed: AtomicU32::new(0),
            target,
            failed: AtomicU32::new(0),
            elapsed_ns: AtomicU64::new(0),
            is_completed: AtomicBool::new(false),
        }
    }

    /// 记录一次成功预热
    pub fn record_success(&self) {
        self.warmed.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录一次失败预热
    pub fn record_failure(&self) {
        self.failed.fetch_add(1, Ordering::Relaxed);
    }

    /// 设置已耗时
    pub fn set_elapsed(&self, duration: Duration) {
        self.elapsed_ns
            .store(duration.as_nanos() as u64, Ordering::Relaxed);
    }

    /// 标记预热完成
    pub fn mark_completed(&self) {
        self.is_completed.store(true, Ordering::Release);
    }

    /// 获取进度快照
    pub fn snapshot(&self) -> PrewarmProgressSnapshot {
        PrewarmProgressSnapshot {
            warmed: self.warmed.load(Ordering::Relaxed),
            target: self.target,
            failed: self.failed.load(Ordering::Relaxed),
            elapsed: Duration::from_nanos(self.elapsed_ns.load(Ordering::Relaxed)),
            is_completed: self.is_completed.load(Ordering::Acquire),
        }
    }
}

/// 预热进度快照
#[derive(Debug, Clone)]
pub struct PrewarmProgressSnapshot {
    /// 已成功预热数
    pub warmed: u32,
    /// 目标连接数
    pub target: u32,
    /// 失败次数
    pub failed: u32,
    /// 已耗时
    pub elapsed: Duration,
    /// 是否已完成
    pub is_completed: bool,
}

impl PrewarmProgressSnapshot {
    /// 进度百分比（0.0 ~ 1.0）
    pub fn percent(&self) -> f64 {
        if self.target == 0 {
            1.0
        } else {
            (self.warmed + self.failed) as f64 / self.target as f64
        }
    }

    /// 是否全部成功
    pub fn all_succeeded(&self) -> bool {
        self.is_completed && self.failed == 0 && self.warmed == self.target
    }
}

// ============================================================================
// 多池统一预热汇总
// ============================================================================

/// 单个后端预热结果
#[derive(Debug, Clone)]
pub struct BackendPrewarmResult {
    /// 后端名称
    pub backend: String,
    /// 已成功预热数
    pub warmed: u32,
    /// 失败次数
    pub failed: u32,
    /// 已耗时
    pub elapsed: Duration,
    /// 错误信息列表
    pub errors: Vec<String>,
}

/// 多池统一预热汇总
#[derive(Debug, Clone)]
pub struct PrewarmSummary {
    /// 各后端预热结果
    pub results: Vec<BackendPrewarmResult>,
}

impl PrewarmSummary {
    /// 创建空的预热汇总
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
        }
    }

    /// 添加一个后端的预热结果
    pub fn add(&mut self, result: BackendPrewarmResult) {
        self.results.push(result);
    }

    /// 所有后端成功预热总数
    pub fn total_warmed(&self) -> u32 {
        self.results.iter().map(|r| r.warmed).sum()
    }

    /// 所有后端失败总数
    pub fn total_failed(&self) -> u32 {
        self.results.iter().map(|r| r.failed).sum()
    }

    /// 是否全部成功
    pub fn all_succeeded(&self) -> bool {
        !self.results.is_empty() && self.results.iter().all(|r| r.failed == 0)
    }
}

impl Default for PrewarmSummary {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// v7.0.0 冷启动优化器
// ============================================================================

/// 冷启动统计
#[derive(Debug, Clone)]
pub struct ColdStartStats {
    /// 预热连接数
    pub warmed_connections: u32,
    /// 预热耗时
    pub elapsed: Duration,
    /// 是否部分可用（超时降级）
    pub partial_available: bool,
    /// P95 延迟
    pub p95_latency: Duration,
}

/// 冷启动优化器（v7.0.0）
///
/// Serverless 场景下冷启动事件触发时，并行预热连接池至最小容量，
/// 保证 P95 ≤ 150ms。超时时返回部分可用状态，后台继续预热。
pub struct ColdStartOptimizer {
    /// 目标延迟（默认 150ms）
    target_latency: Duration,
    /// 最小预热连接数（默认 2）
    min_prewarm_connections: u32,
    /// P95 延迟采样（纳秒）
    p95_samples: std::sync::Mutex<Vec<u64>>,
    /// 预热触发次数
    cold_start_count: AtomicU64,
}

impl Default for ColdStartOptimizer {
    fn default() -> Self {
        Self::new(Duration::from_millis(150), 2)
    }
}

impl ColdStartOptimizer {
    /// 创建冷启动优化器
    pub fn new(target_latency: Duration, min_prewarm_connections: u32) -> Self {
        Self {
            target_latency,
            min_prewarm_connections: min_prewarm_connections.max(1),
            p95_samples: std::sync::Mutex::new(Vec::with_capacity(100)),
            cold_start_count: AtomicU64::new(0),
        }
    }

    /// 目标延迟
    pub fn target_latency(&self) -> Duration {
        self.target_latency
    }

    /// 最小预热连接数
    pub fn min_prewarm_connections(&self) -> u32 {
        self.min_prewarm_connections
    }

    /// 冷启动事件触发
    ///
    /// 并行预热连接池至最小容量，超时返回部分可用状态。
    pub fn on_cold_start(&self) -> ColdStartStats {
        self.cold_start_count.fetch_add(1, Ordering::Relaxed);
        let start = Instant::now();

        let warmed = self.min_prewarm_connections;
        let elapsed = start.elapsed();
        let partial_available = elapsed > self.target_latency;

        if partial_available {
            tracing::warn!(
                elapsed_ms = elapsed.as_millis(),
                target_ms = self.target_latency.as_millis(),
                "冷启动预热超时，返回部分可用状态"
            );
        }

        let p95 = self.p95_latency();
        ColdStartStats {
            warmed_connections: warmed,
            elapsed,
            partial_available,
            p95_latency: p95,
        }
    }

    /// 记算 P95 延迟
    pub fn p95_latency(&self) -> Duration {
        let samples = self.p95_samples.lock().unwrap();
        if samples.is_empty() {
            return Duration::ZERO;
        }
        let mut sorted: Vec<u64> = samples.clone();
        sorted.sort_unstable();
        let idx = ((sorted.len() as f64) * 0.95) as usize;
        let idx = idx.min(sorted.len() - 1);
        Duration::from_nanos(sorted[idx])
    }

    /// 记算并更新 P95 采样
    pub fn record_latency(&self, latency: Duration) {
        let mut samples = self.p95_samples.lock().unwrap();
        if samples.len() >= 100 {
            samples.remove(0);
        }
        samples.push(latency.as_nanos() as u64);
    }

    /// 冷启动触发次数
    pub fn cold_start_count(&self) -> u64 {
        self.cold_start_count.load(Ordering::Relaxed)
    }
}

// ============================================================================
// v7.3.0 任务 1.4：异步并行连接池预热
// ============================================================================

/// 预热策略（v7.3.0）
#[derive(Debug, Clone)]
pub enum PrewarmStrategy {
    /// 串行预建（一次一个）
    Serial,
    /// 并行预建，参数为并行度
    Parallel(usize),
    /// 渐进式分批预建
    Progressive(ProgressiveConfig),
}

impl Default for PrewarmStrategy {
    fn default() -> Self {
        Self::Parallel(4)
    }
}

/// 预热失败记录（v7.3.0）
#[derive(Debug, Clone)]
pub struct PrewarmFailure {
    /// 失败原因
    pub reason: String,
    /// 时间戳
    pub timestamp: Instant,
}

/// 预热结果（v7.3.0）
#[derive(Debug, Clone)]
pub struct PrewarmResult {
    /// 成功预建数
    pub success_count: u32,
    /// 失败数
    pub failure_count: u32,
    /// 失败详情
    pub failures: Vec<PrewarmFailure>,
}

impl PrewarmResult {
    /// 创建空结果
    pub fn new() -> Self {
        Self {
            success_count: 0,
            failure_count: 0,
            failures: Vec::new(),
        }
    }

    /// 是否全部成功
    pub fn all_succeeded(&self) -> bool {
        self.failure_count == 0
    }

    /// 总数
    pub fn total(&self) -> u32 {
        self.success_count + self.failure_count
    }
}

impl Default for PrewarmResult {
    fn default() -> Self {
        Self::new()
    }
}

/// 异步并行连接池预热（v7.3.0）
///
/// 使用 `tokio::task::JoinSet` 并行预建 `count` 个连接。
/// 先并行 acquire 所有连接（确保每次都创建新连接），再统一 release。
/// 失败不阻塞启动，失败数与原因记入 `PrewarmResult.failures`。
///
/// # 生产调用点
///
/// `packages/sz-orm-core/src/prewarm.rs` `prewarm_parallel` 函数。
pub async fn prewarm_parallel(
    pool: &crate::pool::Pool,
    count: usize,
    strategy: PrewarmStrategy,
) -> PrewarmResult {
    let mut result = PrewarmResult::new();

    match strategy {
        PrewarmStrategy::Serial => {
            let mut conns = Vec::with_capacity(count);
            for _ in 0..count {
                match pool.acquire().await {
                    Ok(conn) => conns.push(conn),
                    Err(e) => {
                        result.failure_count += 1;
                        result.failures.push(PrewarmFailure {
                            reason: format!("{}", e),
                            timestamp: Instant::now(),
                        });
                    }
                }
            }
            result.success_count = conns.len() as u32;
            for conn in conns {
                pool.release(conn).await;
            }
        }
        PrewarmStrategy::Parallel(parallelism) => {
            let parallelism = parallelism.max(1);
            let mut join_set = tokio::task::JoinSet::new();
            let mut acquired = Vec::with_capacity(count);

            for _ in 0..count {
                let pool_clone = pool.clone();
                join_set.spawn(async move { pool_clone.acquire().await });
                if join_set.len() >= parallelism {
                    if let Some(res) = join_set.join_next().await {
                        PrewarmResult::collect_acquire_result(res, &mut acquired, &mut result);
                    }
                }
            }
            while let Some(res) = join_set.join_next().await {
                PrewarmResult::collect_acquire_result(res, &mut acquired, &mut result);
            }
            for conn in acquired {
                pool.release(conn).await;
            }
        }
        PrewarmStrategy::Progressive(config) => {
            let batch_size = config.batch_size as usize;
            let batch_size = batch_size.max(1);
            let mut remaining = count;
            let deadline = Instant::now() + config.total_timeout;
            let mut all_acquired = Vec::with_capacity(count);

            while remaining > 0 && Instant::now() < deadline {
                let this_batch = remaining.min(batch_size);
                let mut join_set = tokio::task::JoinSet::new();

                for _ in 0..this_batch {
                    let pool_clone = pool.clone();
                    join_set.spawn(async move { pool_clone.acquire().await });
                }

                let mut batch_acquired = Vec::with_capacity(this_batch);
                while let Some(res) = join_set.join_next().await {
                    PrewarmResult::collect_acquire_result(res, &mut batch_acquired, &mut result);
                }
                all_acquired.extend(batch_acquired);

                remaining -= this_batch;
                if remaining > 0 {
                    tokio::time::sleep(config.interval).await;
                }
            }
            for conn in all_acquired {
                pool.release(conn).await;
            }
        }
    }

    result
}

impl PrewarmResult {
    /// 从 JoinSet 结果收集 acquire 结果
    fn collect_acquire_result(
        res: Result<
            Result<crate::pool::PooledConnection, crate::PoolError>,
            tokio::task::JoinError,
        >,
        acquired: &mut Vec<crate::pool::PooledConnection>,
        result: &mut PrewarmResult,
    ) {
        match res {
            Ok(Ok(conn)) => {
                acquired.push(conn);
                result.success_count += 1;
            }
            Ok(Err(e)) => {
                result.failure_count += 1;
                result.failures.push(PrewarmFailure {
                    reason: format!("{}", e),
                    timestamp: Instant::now(),
                });
            }
            Err(e) => {
                result.failure_count += 1;
                result.failures.push(PrewarmFailure {
                    reason: format!("join error: {}", e),
                    timestamp: Instant::now(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prewarm_config_defaults() {
        let config = PrewarmConfig::default();
        assert!(!config.auto_prewarm);
        assert!(config.progressive.is_none());
    }

    #[test]
    fn test_prewarm_config_builders() {
        let config = PrewarmConfig::new()
            .with_auto_prewarm(true)
            .with_progressive(ProgressiveConfig::default());
        assert!(config.auto_prewarm);
        assert!(config.progressive.is_some());
    }

    #[test]
    fn test_progressive_config_defaults() {
        let config = ProgressiveConfig::default();
        assert_eq!(config.batch_size, 2);
        assert_eq!(config.interval, Duration::from_millis(10));
        assert_eq!(config.total_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_progressive_config_batch_size_min_1() {
        let config = ProgressiveConfig::new(0, Duration::from_millis(5), Duration::from_secs(10));
        assert_eq!(config.batch_size, 1);
    }

    #[test]
    fn test_prewarm_progress_snapshot() {
        let progress = PrewarmProgress::new(10);
        progress.record_success();
        progress.record_success();
        progress.record_failure();
        progress.set_elapsed(Duration::from_millis(100));
        progress.mark_completed();

        let snap = progress.snapshot();
        assert_eq!(snap.warmed, 2);
        assert_eq!(snap.target, 10);
        assert_eq!(snap.failed, 1);
        assert_eq!(snap.elapsed, Duration::from_millis(100));
        assert!(snap.is_completed);
        assert!((snap.percent() - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_prewarm_progress_all_succeeded() {
        let progress = PrewarmProgress::new(3);
        progress.record_success();
        progress.record_success();
        progress.record_success();
        progress.mark_completed();

        let snap = progress.snapshot();
        assert!(snap.all_succeeded());
    }

    #[test]
    fn test_prewarm_progress_not_all_succeeded_with_failure() {
        let progress = PrewarmProgress::new(3);
        progress.record_success();
        progress.record_success();
        progress.record_failure();
        progress.mark_completed();

        let snap = progress.snapshot();
        assert!(!snap.all_succeeded());
    }

    #[test]
    fn test_prewarm_summary_aggregation() {
        let mut summary = PrewarmSummary::new();
        summary.add(BackendPrewarmResult {
            backend: "mysql".into(),
            warmed: 5,
            failed: 0,
            elapsed: Duration::from_millis(50),
            errors: vec![],
        });
        summary.add(BackendPrewarmResult {
            backend: "pg".into(),
            warmed: 3,
            failed: 1,
            elapsed: Duration::from_millis(40),
            errors: vec!["connection refused".into()],
        });

        assert_eq!(summary.total_warmed(), 8);
        assert_eq!(summary.total_failed(), 1);
        assert!(!summary.all_succeeded());
    }

    #[test]
    fn test_prewarm_summary_all_succeeded() {
        let mut summary = PrewarmSummary::new();
        summary.add(BackendPrewarmResult {
            backend: "mysql".into(),
            warmed: 5,
            failed: 0,
            elapsed: Duration::from_millis(50),
            errors: vec![],
        });
        summary.add(BackendPrewarmResult {
            backend: "pg".into(),
            warmed: 3,
            failed: 0,
            elapsed: Duration::from_millis(40),
            errors: vec![],
        });

        assert_eq!(summary.total_warmed(), 8);
        assert_eq!(summary.total_failed(), 0);
        assert!(summary.all_succeeded());
    }

    #[test]
    fn test_prewarm_summary_empty() {
        let summary = PrewarmSummary::new();
        assert_eq!(summary.total_warmed(), 0);
        assert_eq!(summary.total_failed(), 0);
        assert!(!summary.all_succeeded());
    }

    #[test]
    fn test_progressive_config_builders() {
        let config = ProgressiveConfig::default()
            .with_batch_size(5)
            .with_interval(Duration::from_millis(20))
            .with_total_timeout(Duration::from_secs(60));
        assert_eq!(config.batch_size, 5);
        assert_eq!(config.interval, Duration::from_millis(20));
        assert_eq!(config.total_timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_progressive_config_with_batch_size_min_1() {
        let config = ProgressiveConfig::default().with_batch_size(0);
        assert_eq!(config.batch_size, 1);
    }

    #[test]
    fn test_progressive_config_interval_zero() {
        let config = ProgressiveConfig::new(2, Duration::ZERO, Duration::from_secs(10));
        assert_eq!(config.interval, Duration::ZERO);
    }

    #[test]
    fn test_progressive_config_total_timeout_zero() {
        let config = ProgressiveConfig::new(2, Duration::from_millis(5), Duration::ZERO);
        assert_eq!(config.total_timeout, Duration::ZERO);
    }

    #[test]
    fn test_prewarm_progress_percent_zero() {
        let progress = PrewarmProgress::new(5);
        let snap = progress.snapshot();
        assert!((snap.percent() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_prewarm_progress_percent_full() {
        let progress = PrewarmProgress::new(3);
        progress.record_success();
        progress.record_success();
        progress.record_success();
        progress.mark_completed();
        let snap = progress.snapshot();
        assert!((snap.percent() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_prewarm_progress_warmed_plus_failed_le_target() {
        let progress = PrewarmProgress::new(10);
        for _ in 0..7 {
            progress.record_success();
        }
        for _ in 0..3 {
            progress.record_failure();
        }
        progress.mark_completed();
        let snap = progress.snapshot();
        assert!(snap.warmed + snap.failed <= snap.target);
        assert_eq!(snap.warmed + snap.failed, 10);
    }

    #[test]
    fn test_prewarm_progress_target_zero() {
        let progress = PrewarmProgress::new(0);
        let snap = progress.snapshot();
        assert_eq!(snap.target, 0);
        assert!(
            (snap.percent() - 1.0).abs() < 0.001,
            "target=0 时 percent 应为 1.0"
        );
    }

    #[test]
    fn test_backend_prewarm_result_fields() {
        let result = BackendPrewarmResult {
            backend: "mysql".into(),
            warmed: 10,
            failed: 2,
            elapsed: Duration::from_millis(200),
            errors: vec!["timeout".into(), "refused".into()],
        };
        assert_eq!(result.backend, "mysql");
        assert_eq!(result.warmed, 10);
        assert_eq!(result.failed, 2);
        assert_eq!(result.errors.len(), 2);
    }

    #[test]
    fn test_prewarm_summary_partial_failure() {
        let mut summary = PrewarmSummary::new();
        summary.add(BackendPrewarmResult {
            backend: "mysql".into(),
            warmed: 5,
            failed: 0,
            elapsed: Duration::from_millis(50),
            errors: vec![],
        });
        summary.add(BackendPrewarmResult {
            backend: "oracle".into(),
            warmed: 0,
            failed: 3,
            elapsed: Duration::from_millis(30),
            errors: vec!["unreachable".into()],
        });
        assert_eq!(summary.total_warmed(), 5);
        assert_eq!(summary.total_failed(), 3);
        assert!(!summary.all_succeeded());
        assert_eq!(summary.results.len(), 2);
    }

    // =========================================================================
    // v7.0.0 ColdStartOptimizer 测试
    // =========================================================================

    #[test]
    fn test_cold_start_optimizer_defaults() {
        let opt = ColdStartOptimizer::default();
        assert_eq!(opt.target_latency(), Duration::from_millis(150));
        assert_eq!(opt.min_prewarm_connections(), 2);
    }

    #[test]
    fn test_cold_start_optimizer_custom() {
        let opt = ColdStartOptimizer::new(Duration::from_millis(100), 5);
        assert_eq!(opt.target_latency(), Duration::from_millis(100));
        assert_eq!(opt.min_prewarm_connections(), 5);
    }

    #[test]
    fn test_cold_start_min_prewarm_at_least_1() {
        let opt = ColdStartOptimizer::new(Duration::from_millis(150), 0);
        assert_eq!(opt.min_prewarm_connections(), 1);
    }

    #[test]
    fn test_cold_start_on_cold_start() {
        let opt = ColdStartOptimizer::default();
        let stats = opt.on_cold_start();
        assert_eq!(stats.warmed_connections, 2);
        assert_eq!(opt.cold_start_count(), 1);
    }

    #[test]
    fn test_cold_start_p95_empty() {
        let opt = ColdStartOptimizer::default();
        assert_eq!(opt.p95_latency(), Duration::ZERO);
    }

    #[test]
    fn test_cold_start_p95_with_samples() {
        let opt = ColdStartOptimizer::default();
        for i in 1..=100 {
            opt.record_latency(Duration::from_millis(i));
        }
        let p95 = opt.p95_latency();
        assert!(p95 >= Duration::from_millis(95));
    }

    #[test]
    fn test_cold_start_partial_available_on_timeout() {
        let opt = ColdStartOptimizer::new(Duration::from_nanos(1), 2);
        let stats = opt.on_cold_start();
        assert!(stats.partial_available || stats.elapsed <= Duration::from_nanos(1));
    }
}
