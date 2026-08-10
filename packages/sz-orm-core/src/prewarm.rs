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
use std::time::Duration;

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
}
