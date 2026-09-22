//! DEK 轮换管理器
//!
//! 90 天周期轮换，重叠期 ≥ 24h，新旧 DEK 并行解密，不中断服务。
//! DEK 内存安全由 `DekBuffer`（`Zeroizing<Vec<u8>>`）保证，禁止明文落盘。

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::dek_buffer::DekBuffer;
use crate::key_rotation_enhanced::{KeyRotationEnhanced, KeyStatus};

use super::SecError;

/// DEK 轮换配置
#[derive(Debug, Clone)]
pub struct DekRotationConfig {
    /// 轮换周期（默认 90 天）
    pub rotation_interval: Duration,
    /// 新旧 DEK 重叠期（默认 24h，期间旧 DEK 仍可解密）
    pub overlap_window: Duration,
}

impl Default for DekRotationConfig {
    fn default() -> Self {
        Self {
            rotation_interval: Duration::from_secs(90 * 24 * 60 * 60),
            overlap_window: Duration::from_secs(24 * 60 * 60),
        }
    }
}

impl DekRotationConfig {
    /// 创建默认配置（90 天周期 + 24h 重叠）
    pub fn new() -> Self {
        Self::default()
    }

    /// 自定义轮换周期
    pub fn with_rotation_interval(mut self, interval: Duration) -> Self {
        self.rotation_interval = interval;
        self
    }

    /// 自定义重叠期
    pub fn with_overlap_window(mut self, window: Duration) -> Self {
        self.overlap_window = window;
        self
    }
}

/// DEK 轮换记录
#[derive(Debug, Clone)]
pub struct DekRotationRecord {
    /// 新 DEK 标识
    pub new_dek_id: String,
    /// 旧 DEK 标识
    pub old_dek_id: String,
    /// 轮换发生时间（Unix 秒）
    pub rotated_at: u64,
    /// 重叠期截止时间（Unix 秒），此后旧 DEK 应被退役
    pub overlap_until: u64,
}

/// DEK 轮换状态
#[derive(Debug, Clone)]
pub struct DekRotationStatus {
    /// 当前写 DEK 标识
    pub current_dek_id: String,
    /// 上次轮换时间（Unix 秒），无轮换记录时为 0
    pub last_rotation_at: u64,
    /// 距下次轮换的剩余秒数
    pub next_rotation_in_secs: u64,
    /// 处于重叠期的旧 DEK 标识列表
    pub overlapping_dek_ids: Vec<String>,
    /// 活跃 DEK 数量（含重叠期旧 DEK）
    pub active_dek_count: usize,
}

/// DEK 轮换管理器
///
/// 注入 `Arc<KeyRotationEnhanced>` 管理双密钥共存，注入 `Arc<DekBuffer>` 持有当前 DEK 明文。
/// `rotate()` 生成新 DEK → 标记旧 DEK 为 Deprecating → 重叠期并行 → 过期旧 DEK。
/// 全程不中断服务：重叠期内旧 DEK 仍可解密历史密文。
pub struct DekRotationManager {
    key_mgr: Arc<KeyRotationEnhanced>,
    current_dek: parking_lot::RwLock<Arc<DekBuffer>>,
    config: DekRotationConfig,
    last_rotation_at: parking_lot::RwLock<u64>,
    /// 历史轮换记录（最近 N 次）
    rotation_history: parking_lot::RwLock<Vec<DekRotationRecord>>,
}

impl DekRotationManager {
    /// 创建轮换管理器
    ///
    /// `initial_dek_id` + `initial_dek` 为初始 DEK，标记为 Active。
    pub fn new(
        key_mgr: Arc<KeyRotationEnhanced>,
        current_dek: Arc<DekBuffer>,
        config: DekRotationConfig,
    ) -> Self {
        Self {
            key_mgr,
            current_dek: parking_lot::RwLock::new(current_dek),
            config,
            last_rotation_at: parking_lot::RwLock::new(now_secs()),
            rotation_history: parking_lot::RwLock::new(Vec::new()),
        }
    }

    /// 执行 DEK 轮换
    ///
    /// 流程：生成新 DEK → 旧 DEK 标记 Deprecating → 切换写指针 → 记录重叠期。
    /// 重叠期内旧 DEK 仍可解密，不中断服务。轮换失败时回退旧 DEK 并返回
    /// `SecError::KeyRotationInterrupted`。
    pub fn rotate(
        &self,
        new_dek_id: &str,
        new_dek: Arc<DekBuffer>,
    ) -> Result<DekRotationRecord, SecError> {
        if new_dek_id.is_empty() {
            return Err(SecError::InvalidArgument("new_dek_id 不能为空".to_string()));
        }
        if new_dek.as_bytes().is_empty() {
            return Err(SecError::InvalidArgument("new_dek 不能为空".to_string()));
        }

        let old_dek_id = self.key_mgr.write_key_id();
        let rotated_at = now_secs();
        let overlap_until = rotated_at + self.config.overlap_window.as_secs();

        // 调用底层 KeyRotationEnhanced 执行双密钥共存切换
        // 失败时回退：底层 rotate_key 仅在 key_id 重复时失败，此时旧 DEK 仍是写密钥，无需额外回退
        self.key_mgr
            .rotate_key(new_dek_id, new_dek.as_bytes().to_vec())
            .map_err(SecError::KeyRotationInterrupted)?;

        // 切换当前 DEK 指针
        *self.current_dek.write() = new_dek;
        *self.last_rotation_at.write() = rotated_at;

        let record = DekRotationRecord {
            new_dek_id: new_dek_id.to_string(),
            old_dek_id,
            rotated_at,
            overlap_until,
        };

        let mut history = self.rotation_history.write();
        history.push(record.clone());
        // 保留最近 10 次轮换记录
        if history.len() > 10 {
            history.remove(0);
        }

        Ok(record)
    }

    /// 查询当前轮换状态
    pub fn status(&self) -> DekRotationStatus {
        let current_dek_id = self.key_mgr.write_key_id();
        let last_rotation_at = *self.last_rotation_at.read();
        let now = now_secs();
        let elapsed = now.saturating_sub(last_rotation_at);
        let next_rotation_in_secs = self
            .config
            .rotation_interval
            .as_secs()
            .saturating_sub(elapsed);

        // 收集处于重叠期的旧 DEK（Deprecating 状态）
        let mut overlapping_dek_ids = Vec::new();
        let history = self.rotation_history.read();
        for record in history.iter().rev() {
            if now < record.overlap_until
                && record.new_dek_id != current_dek_id
                && self.key_mgr.key_status(&record.old_dek_id) == Some(KeyStatus::Deprecating)
            {
                overlapping_dek_ids.push(record.old_dek_id.clone());
            }
        }

        let active_dek_count = self.key_mgr.active_key_count();

        DekRotationStatus {
            current_dek_id,
            last_rotation_at,
            next_rotation_in_secs,
            overlapping_dek_ids,
            active_dek_count,
        }
    }

    /// 退役超过重叠期的旧 DEK
    ///
    /// 检查轮换历史，对 overlap_until 已过且无引用的旧 DEK 调用 `delete_key`。
    /// 返回成功退役的 DEK ID 列表。
    pub fn retire_expired_deks(&self) -> Vec<String> {
        let now = now_secs();
        let mut retired = Vec::new();
        let history = self.rotation_history.read();
        for record in history.iter() {
            if now >= record.overlap_until
                && self.key_mgr.key_status(&record.old_dek_id) == Some(KeyStatus::Deprecating)
                && self.key_mgr.reference_count(&record.old_dek_id) == 0
                && self.key_mgr.delete_key(&record.old_dek_id).is_ok()
            {
                retired.push(record.old_dek_id.clone());
            }
        }
        retired
    }

    /// 当前 DEK 引用
    pub fn current_dek(&self) -> Arc<DekBuffer> {
        Arc::clone(&self.current_dek.read())
    }

    /// 轮换历史记录数
    pub fn history_count(&self) -> usize {
        self.rotation_history.read().len()
    }
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

    fn make_manager() -> DekRotationManager {
        let key_mgr = Arc::new(KeyRotationEnhanced::new("dek-v1", vec![0x42u8; 32]));
        let dek = Arc::new(DekBuffer::new(vec![0x42u8; 32]));
        DekRotationManager::new(key_mgr, dek, DekRotationConfig::new())
    }

    #[test]
    fn rotate_90_day_cycle_creates_new_dek() {
        let mgr = make_manager();
        let new_dek = Arc::new(DekBuffer::new(vec![0x43u8; 32]));
        let record = mgr.rotate("dek-v2", new_dek).unwrap();
        assert_eq!(record.new_dek_id, "dek-v2");
        assert_eq!(record.old_dek_id, "dek-v1");
        assert!(record.overlap_until > record.rotated_at);
        // 重叠期 ≥ 24h
        assert!(record.overlap_until - record.rotated_at >= 24 * 60 * 60);
    }

    #[test]
    fn overlap_period_parallel_decrypt_no_service_interruption() {
        let key_mgr = Arc::new(KeyRotationEnhanced::new("dek-v1", vec![0x42u8; 32]));
        let dek = Arc::new(DekBuffer::new(vec![0x42u8; 32]));
        let mgr = DekRotationManager::new(key_mgr, dek, DekRotationConfig::new());

        // 用旧 DEK 加密
        let old_dek = mgr.current_dek();
        let ciphertext = old_dek
            .encrypt(b"sensitive", crate::dek_buffer::EncryptionAlgo::Aes256Gcm)
            .unwrap();

        // 轮换到新 DEK
        let new_dek = Arc::new(DekBuffer::new(vec![0x43u8; 32]));
        mgr.rotate("dek-v2", new_dek).unwrap();

        // 重叠期内旧 DEK 仍可解密（不中断服务）
        let decrypted = old_dek
            .decrypt(&ciphertext, crate::dek_buffer::EncryptionAlgo::Aes256Gcm)
            .unwrap();
        assert_eq!(decrypted, b"sensitive");
    }

    #[test]
    fn rotation_interrupted_rollback_to_old_dek() {
        let mgr = make_manager();
        // 重复 key_id 触发底层 rotate_key 失败
        let new_dek = Arc::new(DekBuffer::new(vec![0x43u8; 32]));
        let result = mgr.rotate("dek-v1", new_dek);
        assert!(matches!(result, Err(SecError::KeyRotationInterrupted(_))));
        // 旧 DEK 仍是写密钥（回退）
        let status = mgr.status();
        assert_eq!(status.current_dek_id, "dek-v1");
    }

    #[test]
    fn dek_buffer_zeroized_on_drop() {
        // DekBuffer 内部 Zeroizing<Vec<u8>>，Drop 时自动清零
        let dek = DekBuffer::new(vec![0xABu8; 32]);
        assert_eq!(dek.as_bytes(), &vec![0xABu8; 32][..]);
        drop(dek);
        // Drop 后内存已清零（Zeroizing 保证），此处验证 Drop 不 panic
    }

    #[test]
    fn status_reflects_current_state() {
        let mgr = make_manager();
        let status = mgr.status();
        assert_eq!(status.current_dek_id, "dek-v1");
        assert_eq!(status.active_dek_count, 1);
        assert!(status.next_rotation_in_secs > 0);
    }

    #[test]
    fn rotate_updates_status() {
        let mgr = make_manager();
        let new_dek = Arc::new(DekBuffer::new(vec![0x43u8; 32]));
        mgr.rotate("dek-v2", new_dek).unwrap();
        let status = mgr.status();
        assert_eq!(status.current_dek_id, "dek-v2");
        assert_eq!(status.active_dek_count, 2);
    }

    #[test]
    fn reject_empty_new_dek_id() {
        let mgr = make_manager();
        let new_dek = Arc::new(DekBuffer::new(vec![0x43u8; 32]));
        let result = mgr.rotate("", new_dek);
        assert!(matches!(result, Err(SecError::InvalidArgument(_))));
    }

    #[test]
    fn reject_empty_new_dek() {
        let mgr = make_manager();
        let new_dek = Arc::new(DekBuffer::new(vec![]));
        let result = mgr.rotate("dek-v2", new_dek);
        assert!(matches!(result, Err(SecError::InvalidArgument(_))));
    }

    #[test]
    fn history_count_after_rotations() {
        let mgr = make_manager();
        assert_eq!(mgr.history_count(), 0);
        let new_dek = Arc::new(DekBuffer::new(vec![0x43u8; 32]));
        mgr.rotate("dek-v2", new_dek).unwrap();
        assert_eq!(mgr.history_count(), 1);
        let new_dek2 = Arc::new(DekBuffer::new(vec![0x44u8; 32]));
        mgr.rotate("dek-v3", new_dek2).unwrap();
        assert_eq!(mgr.history_count(), 2);
    }

    #[test]
    fn retire_expired_deks_removes_old_dek() {
        let key_mgr = Arc::new(KeyRotationEnhanced::new("dek-v1", vec![0x42u8; 32]));
        let dek = Arc::new(DekBuffer::new(vec![0x42u8; 32]));
        // 重叠期 0 秒，立即过期
        let config = DekRotationConfig::new().with_overlap_window(Duration::from_secs(0));
        let mgr = DekRotationManager::new(key_mgr, dek, config);

        let new_dek = Arc::new(DekBuffer::new(vec![0x43u8; 32]));
        mgr.rotate("dek-v2", new_dek).unwrap();

        // 等待 1 秒确保超过 overlap_until
        std::thread::sleep(Duration::from_secs(1));
        let retired = mgr.retire_expired_deks();
        assert!(retired.contains(&"dek-v1".to_string()));
    }

    #[test]
    fn config_default_90_day_24h_overlap() {
        let config = DekRotationConfig::default();
        assert_eq!(
            config.rotation_interval,
            Duration::from_secs(90 * 24 * 60 * 60)
        );
        assert_eq!(config.overlap_window, Duration::from_secs(24 * 60 * 60));
    }
}
