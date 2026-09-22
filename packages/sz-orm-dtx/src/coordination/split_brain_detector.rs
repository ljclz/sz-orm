//! 脑裂检测器（三重判定，ADR-005，`split-brain-detect` feature gate）
//!
//! 三重判定：(1) 心跳超时超过阈值 (2) 可用节点数 < 仲裁节点数 (3) 超时持续超过忽略窗口。
//! 三者同时满足才确认脑裂 → ≤ 5s 检测告警 `SPLIT_BRAIN_DETECTED` → 降级仲裁模式 → 最多一个主。

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

/// 脑裂检测错误
#[derive(Debug, Clone)]
pub enum DistError {
    /// 仲裁节点数 ≤ 节点数/2（无效仲裁配置）
    InvalidQuorumConfig { quorum: usize, total: usize },
    /// 仲裁模式切换失败
    QuorumModeFailed(String),
    /// 鉴权失败
    Unauthorized(String),
}

impl std::fmt::Display for DistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidQuorumConfig { quorum, total } => {
                write!(f, "invalid quorum: {quorum} <= {total}/2")
            }
            Self::QuorumModeFailed(msg) => write!(f, "quorum mode failed: {msg}"),
            Self::Unauthorized(msg) => write!(f, "unauthorized: {msg}"),
        }
    }
}

impl std::error::Error for DistError {}

/// 脑裂状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitBrainStatus {
    /// 正常（无脑裂）
    Healthy,
    /// 窗口内短抖动（`SPLIT_BRAIN_FALSE_POSITIVE`，不告警）
    FalsePositive { reason: String },
    /// 脑裂确认（`SPLIT_BRAIN_DETECTED`）
    Confirmed {
        heartbeat_elapsed: Duration,
        available_nodes: usize,
        quorum_nodes: usize,
    },
}

/// 心跳采样
#[derive(Debug, Clone)]
pub struct HeartbeatSample {
    pub timestamp: Instant,
    pub available_nodes: usize,
    pub heartbeat_timeout: bool,
}

/// 脑裂检测器
///
/// ADR-005 三重判定：(1) 心跳超时超过阈值 (2) 可用节点数 < 仲裁节点数 (3) 超时持续超过忽略窗口。
#[derive(Debug)]
pub struct SplitBrainDetector {
    /// 心跳超时阈值（默认 3000ms）
    pub heartbeat_timeout: Duration,
    /// 仲裁节点数
    pub quorum_nodes: usize,
    /// 忽略窗口（默认 5000ms，窗口内短抖动不告警）
    pub ignore_window: Duration,
    total_nodes: usize,
    quorum_mode: Arc<Mutex<bool>>,
    heartbeat_timeout_start: Arc<Mutex<Option<Instant>>>,
    /// 检测次数（指标 `sz_orm_split_brain_detect_count`）
    detect_count: Arc<std::sync::atomic::AtomicU64>,
    auth_token: Arc<Mutex<Option<String>>>,
}

impl SplitBrainDetector {
    /// 创建脑裂检测器
    pub fn new(total_nodes: usize, quorum_nodes: usize) -> Result<Self, DistError> {
        if quorum_nodes <= total_nodes / 2 {
            return Err(DistError::InvalidQuorumConfig {
                quorum: quorum_nodes,
                total: total_nodes,
            });
        }
        Ok(Self {
            heartbeat_timeout: Duration::from_millis(3000),
            quorum_nodes,
            ignore_window: Duration::from_millis(5000),
            total_nodes,
            quorum_mode: Arc::new(Mutex::new(false)),
            heartbeat_timeout_start: Arc::new(Mutex::new(None)),
            detect_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            auth_token: Arc::new(Mutex::new(None)),
        })
    }

    /// 设置鉴权令牌
    pub fn authorize(&self, token: String) {
        *self.auth_token.lock() = Some(token);
    }

    /// 检测脑裂（三重判定）
    ///
    /// ADR-005 三重判定：
    /// 1. 心跳超时超过阈值
    /// 2. 可用节点数 < 仲裁节点数
    /// 3. 超时持续超过忽略窗口
    ///
    /// 三者同时满足才确认脑裂 → ≤ 5s 检测告警 `SPLIT_BRAIN_DETECTED`。
    pub fn detect(&self, sample: &HeartbeatSample) -> SplitBrainStatus {
        self.detect_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let now = sample.timestamp;
        let mut start = self.heartbeat_timeout_start.lock();

        // 条件 1: 心跳超时
        if sample.heartbeat_timeout {
            if start.is_none() {
                *start = Some(now);
            }
        } else {
            *start = None;
            return SplitBrainStatus::Healthy;
        }

        // 条件 2: 可用节点数 < 仲裁节点数
        if sample.available_nodes >= self.quorum_nodes {
            *start = None;
            return SplitBrainStatus::Healthy;
        }

        let start_time = start.unwrap();
        let elapsed = now.duration_since(start_time);

        // 条件 3: 超时持续超过忽略窗口
        if elapsed < self.ignore_window {
            return SplitBrainStatus::FalsePositive {
                reason: format!(
                    "heartbeat timeout {elapsed:?} < ignore_window {:?}",
                    self.ignore_window
                ),
            };
        }

        SplitBrainStatus::Confirmed {
            heartbeat_elapsed: elapsed,
            available_nodes: sample.available_nodes,
            quorum_nodes: self.quorum_nodes,
        }
    }

    /// 进入仲裁模式（降级，最多一个主）
    pub fn enter_quorum_mode(&self) -> Result<(), DistError> {
        if self.auth_token.lock().is_none() {
            return Err(DistError::Unauthorized(
                "quorum mode requires authorization".into(),
            ));
        }
        *self.quorum_mode.lock() = true;
        Ok(())
    }

    /// 退出仲裁模式
    pub fn exit_quorum_mode(&self) {
        *self.quorum_mode.lock() = false;
    }

    /// 是否在仲裁模式
    pub fn is_in_quorum_mode(&self) -> bool {
        *self.quorum_mode.lock()
    }

    /// 检测次数（指标 `sz_orm_split_brain_detect_count`）
    pub fn detect_count(&self) -> u64 {
        self.detect_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 拒绝危险操作（脑裂期间拒绝双写，最多一个主）
    pub fn reject_dangerous_op(&self, op: &str) -> Result<(), DistError> {
        if self.is_in_quorum_mode() && op == "dual_write" {
            return Err(DistError::QuorumModeFailed(
                "dual_write rejected in quorum mode".into(),
            ));
        }
        Ok(())
    }

    /// 总节点数
    pub fn total_nodes(&self) -> usize {
        self.total_nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_quorum_config() {
        let err = SplitBrainDetector::new(5, 2).unwrap_err();
        assert!(matches!(err, DistError::InvalidQuorumConfig { .. }));
    }

    #[test]
    fn test_valid_quorum_config() {
        let detector = SplitBrainDetector::new(5, 3).unwrap();
        assert_eq!(detector.total_nodes(), 5);
    }

    #[test]
    fn test_healthy_status() {
        let detector = SplitBrainDetector::new(5, 3).unwrap();
        let sample = HeartbeatSample {
            timestamp: Instant::now(),
            available_nodes: 4,
            heartbeat_timeout: false,
        };
        assert_eq!(detector.detect(&sample), SplitBrainStatus::Healthy);
    }

    #[test]
    fn test_false_positive_within_window() {
        let detector = SplitBrainDetector::new(5, 3).unwrap();
        let start = Instant::now();
        let s1 = HeartbeatSample {
            timestamp: start,
            available_nodes: 2,
            heartbeat_timeout: true,
        };
        let status = detector.detect(&s1);
        assert!(matches!(status, SplitBrainStatus::FalsePositive { .. }));
        let s2 = HeartbeatSample {
            timestamp: start + Duration::from_millis(2000),
            available_nodes: 2,
            heartbeat_timeout: true,
        };
        let status = detector.detect(&s2);
        assert!(matches!(status, SplitBrainStatus::FalsePositive { .. }));
    }

    #[test]
    fn test_confirmed_after_window() {
        let detector = SplitBrainDetector::new(5, 3).unwrap();
        let start = Instant::now();
        let s1 = HeartbeatSample {
            timestamp: start,
            available_nodes: 2,
            heartbeat_timeout: true,
        };
        detector.detect(&s1);
        let s2 = HeartbeatSample {
            timestamp: start + Duration::from_millis(6000),
            available_nodes: 2,
            heartbeat_timeout: true,
        };
        let status = detector.detect(&s2);
        assert!(matches!(status, SplitBrainStatus::Confirmed { .. }));
    }

    #[test]
    fn test_quorum_mode_requires_auth() {
        let detector = SplitBrainDetector::new(5, 3).unwrap();
        let err = detector.enter_quorum_mode().unwrap_err();
        assert!(matches!(err, DistError::Unauthorized { .. }));
    }

    #[test]
    fn test_quorum_mode_with_auth() {
        let detector = SplitBrainDetector::new(5, 3).unwrap();
        detector.authorize("admin_token".into());
        detector.enter_quorum_mode().unwrap();
        assert!(detector.is_in_quorum_mode());
    }

    #[test]
    fn test_reject_dual_write_in_quorum_mode() {
        let detector = SplitBrainDetector::new(5, 3).unwrap();
        detector.authorize("admin_token".into());
        detector.enter_quorum_mode().unwrap();
        let err = detector.reject_dangerous_op("dual_write").unwrap_err();
        assert!(matches!(err, DistError::QuorumModeFailed { .. }));
    }

    #[test]
    fn test_allow_read_in_quorum_mode() {
        let detector = SplitBrainDetector::new(5, 3).unwrap();
        detector.authorize("admin_token".into());
        detector.enter_quorum_mode().unwrap();
        detector.reject_dangerous_op("read").unwrap();
    }

    #[test]
    fn test_detect_count_metric() {
        let detector = SplitBrainDetector::new(5, 3).unwrap();
        let sample = HeartbeatSample {
            timestamp: Instant::now(),
            available_nodes: 4,
            heartbeat_timeout: false,
        };
        detector.detect(&sample);
        detector.detect(&sample);
        assert_eq!(detector.detect_count(), 2);
    }

    #[test]
    fn test_heartbeat_recovery_resets() {
        let detector = SplitBrainDetector::new(5, 3).unwrap();
        let start = Instant::now();
        let s1 = HeartbeatSample {
            timestamp: start,
            available_nodes: 2,
            heartbeat_timeout: true,
        };
        detector.detect(&s1);
        let s2 = HeartbeatSample {
            timestamp: start + Duration::from_millis(1000),
            available_nodes: 4,
            heartbeat_timeout: false,
        };
        assert_eq!(detector.detect(&s2), SplitBrainStatus::Healthy);
        let s3 = HeartbeatSample {
            timestamp: start + Duration::from_millis(2000),
            available_nodes: 2,
            heartbeat_timeout: true,
        };
        let status = detector.detect(&s3);
        assert!(matches!(status, SplitBrainStatus::FalsePositive { .. }));
    }
}
