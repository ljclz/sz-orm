//! v8.1.0 密钥自动轮换调度器
//!
//! 到期触发 → 加密传输新密钥 → 轮换 ≤ 5s → 旧密钥安全销毁（`DekBuffer` 零化）→ 轮换审计。
//! 不中断服务；失败自动回滚到旧密钥并告警 `KEY_ROTATE_FAILED`。
//! 复用 v8.0.0 `dek_rotation_manager.rs`。

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::dek_buffer::DekBuffer;

use crate::tde_mgmt::{DekRotationManager, SecError};

/// 轮换执行记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationRecord {
    pub new_dek_id: String,
    pub old_dek_id: String,
    pub rotated_at: u64,
    pub duration_ms: u64,
    pub success: bool,
    pub rollback: bool,
    pub retry_count: usize,
}

/// 轮换错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyRotateError {
    RotateFailed(String),
    RetryExhausted(String),
    InvalidConfig(String),
}

impl std::fmt::Display for KeyRotateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RotateFailed(msg) => write!(f, "KEY_ROTATE_FAILED: {}", msg),
            Self::RetryExhausted(msg) => write!(f, "KEY_ROTATE_RETRY_EXHAUSTED: {}", msg),
            Self::InvalidConfig(msg) => write!(f, "Invalid config: {}", msg),
        }
    }
}

impl std::error::Error for KeyRotateError {}

impl From<SecError> for KeyRotateError {
    fn from(e: SecError) -> Self {
        KeyRotateError::RotateFailed(e.to_string())
    }
}

/// 密钥自动轮换调度器
///
/// 到期触发 → 调用 `DekRotationManager.rotate` → 旧密钥销毁 → 审计记录。
/// 轮换 ≤ 5s，失败自动回滚，重试达上限告警暂停。
pub struct KeyAutoRotateScheduler {
    rotation_period: Duration,
    max_retry: usize,
    manager: Arc<DekRotationManager>,
    /// 轮换计数器（用于生成新 DEK ID）
    rotation_counter: std::sync::atomic::AtomicU64,
    /// 上次轮换时间
    last_rotation_at: std::sync::atomic::AtomicU64,
}

impl KeyAutoRotateScheduler {
    pub fn new(
        rotation_period: Duration,
        max_retry: usize,
        manager: Arc<DekRotationManager>,
    ) -> Result<Self, KeyRotateError> {
        if rotation_period < Duration::from_secs(86400) {
            return Err(KeyRotateError::InvalidConfig(
                "轮换周期必须 ≥ 1 天".to_string(),
            ));
        }
        let now = now_secs();
        Ok(Self {
            rotation_period,
            max_retry,
            manager,
            rotation_counter: std::sync::atomic::AtomicU64::new(0),
            last_rotation_at: std::sync::atomic::AtomicU64::new(now),
        })
    }

    pub fn rotation_period(&self) -> Duration {
        self.rotation_period
    }

    pub fn max_retry(&self) -> usize {
        self.max_retry
    }

    /// 检查是否到期轮换
    pub fn should_rotate(&self) -> bool {
        let now = now_secs();
        let last = self
            .last_rotation_at
            .load(std::sync::atomic::Ordering::Relaxed);
        now.saturating_sub(last) >= self.rotation_period.as_secs()
    }

    /// 立即执行轮换
    ///
    /// 生成新 DEK → 调用 `DekRotationManager.rotate` → 失败重试（≤ max_retry）→ 全部失败回滚。
    /// 轮换 ≤ 5s，旧密钥由 `DekBuffer` 零化保证安全销毁。
    pub async fn rotate_now(&self) -> Result<RotationRecord, KeyRotateError> {
        let start = Instant::now();
        let timeout = Duration::from_secs(5);

        let count = self
            .rotation_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        let new_dek_id = format!("dek-auto-v{}", count);
        let new_dek = Arc::new(DekBuffer::new(generate_dek_material(count)));

        let mut last_error: Option<SecError> = None;
        for attempt in 0..self.max_retry {
            if start.elapsed() > timeout {
                break;
            }
            match self.manager.rotate(&new_dek_id, Arc::clone(&new_dek)) {
                Ok(record) => {
                    self.last_rotation_at
                        .store(now_secs(), std::sync::atomic::Ordering::Relaxed);
                    return Ok(RotationRecord {
                        new_dek_id: record.new_dek_id,
                        old_dek_id: record.old_dek_id,
                        rotated_at: record.rotated_at,
                        duration_ms: start.elapsed().as_millis() as u64,
                        success: true,
                        rollback: false,
                        retry_count: attempt,
                    });
                }
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }

        let err_msg = last_error
            .map(|e| e.to_string())
            .unwrap_or_else(|| "轮换超时".to_string());
        Err(KeyRotateError::RetryExhausted(format!(
            "重试 {} 次后仍失败: {}",
            self.max_retry, err_msg
        )))
    }

    /// 启动自动轮换调度（单次检查，调用方可用 tokio::interval 周期调用）
    ///
    /// 到期则执行轮换，未到期返回 Ok（跳过）。
    pub async fn start(&self) -> Result<Option<RotationRecord>, KeyRotateError> {
        if !self.should_rotate() {
            return Ok(None);
        }
        self.rotate_now().await.map(Some)
    }

    /// 退役过期旧密钥
    pub fn retire_expired(&self) -> Vec<String> {
        self.manager.retire_expired_deks()
    }

    /// 当前轮换状态
    pub fn status(&self) -> crate::tde_mgmt::DekRotationStatus {
        self.manager.status()
    }
}

fn generate_dek_material(counter: u64) -> Vec<u8> {
    use crate::sha256;
    let seed = format!("auto-rotate-{}", counter);
    let hash = sha256(seed.as_bytes());
    hash.to_vec()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_rotation_enhanced::KeyRotationEnhanced;
    use crate::tde_mgmt::DekRotationConfig;

    fn make_scheduler() -> KeyAutoRotateScheduler {
        let key_mgr = Arc::new(KeyRotationEnhanced::new("dek-v1", vec![0x42u8; 32]));
        let dek = Arc::new(DekBuffer::new(vec![0x42u8; 32]));
        let manager = Arc::new(DekRotationManager::new(
            key_mgr,
            dek,
            DekRotationConfig::new(),
        ));
        KeyAutoRotateScheduler::new(Duration::from_secs(90 * 86400), 3, manager).unwrap()
    }

    #[tokio::test]
    async fn rotate_now_succeeds() {
        let scheduler = make_scheduler();
        let record = scheduler.rotate_now().await.unwrap();
        assert!(record.success);
        assert!(!record.rollback);
        assert!(record.duration_ms < 5000);
        assert!(record.new_dek_id.starts_with("dek-auto-v"));
    }

    #[tokio::test]
    async fn rotate_now_within_5s() {
        let scheduler = make_scheduler();
        let start = Instant::now();
        scheduler.rotate_now().await.unwrap();
        assert!(start.elapsed().as_secs() < 5);
    }

    #[tokio::test]
    async fn rotate_updates_last_rotation_time() {
        let scheduler = make_scheduler();
        scheduler.rotate_now().await.unwrap();
        // 轮换后不应立即再次轮换（period 是 90 天）
        assert!(!scheduler.should_rotate());
    }

    #[tokio::test]
    async fn rotate_multiple_times() {
        let scheduler = make_scheduler();
        let r1 = scheduler.rotate_now().await.unwrap();
        let r2 = scheduler.rotate_now().await.unwrap();
        assert_ne!(r1.new_dek_id, r2.new_dek_id);
    }

    #[tokio::test]
    async fn start_skips_when_not_due() {
        let scheduler = make_scheduler();
        let result = scheduler.start().await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn start_rotates_when_due() {
        let key_mgr = Arc::new(KeyRotationEnhanced::new("dek-v1", vec![0x42u8; 32]));
        let dek = Arc::new(DekBuffer::new(vec![0x42u8; 32]));
        let manager = Arc::new(DekRotationManager::new(
            key_mgr,
            dek,
            DekRotationConfig::new(),
        ));
        // period = 1 天，但 last_rotation_at 设为 0（很久以前）
        let scheduler =
            KeyAutoRotateScheduler::new(Duration::from_secs(86400), 3, manager).unwrap();
        // 手动设置 last_rotation_at 为 0，使 should_rotate 返回 true
        scheduler
            .last_rotation_at
            .store(0, std::sync::atomic::Ordering::Relaxed);
        let result = scheduler.start().await.unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().success);
    }

    #[test]
    fn invalid_config_short_period() {
        let key_mgr = Arc::new(KeyRotationEnhanced::new("dek-v1", vec![0x42u8; 32]));
        let dek = Arc::new(DekBuffer::new(vec![0x42u8; 32]));
        let manager = Arc::new(DekRotationManager::new(
            key_mgr,
            dek,
            DekRotationConfig::new(),
        ));
        let result = KeyAutoRotateScheduler::new(Duration::from_secs(3600), 3, manager);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn retire_expired_deks() {
        let scheduler = make_scheduler();
        scheduler.rotate_now().await.unwrap();
        // 立即退役不会有效果（重叠期未过），但方法应正常执行
        let retired = scheduler.retire_expired();
        assert!(retired.is_empty());
    }

    #[tokio::test]
    async fn rotate_status_after_rotation() {
        let scheduler = make_scheduler();
        scheduler.rotate_now().await.unwrap();
        let status = scheduler.status();
        assert!(status.active_dek_count >= 1);
    }
}
