//! 密钥轮换管理器
//!
//! 生成新密钥 → 重叠期（≥24h）→ 切换 → 过期旧密钥。
//! 支持重叠期并行使用新旧密钥，中断时回退到旧密钥。

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use super::PqcExecError;

/// 重叠期下限（24 小时，单位秒）
const MIN_OVERLAP_SECONDS: u64 = 24 * 60 * 60;

/// 密钥轮换记录
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RotationRecord {
    pub new_key_id: String,
    pub old_key_id: String,
    pub rotated_at: i64,
    pub overlap_until: i64,
    pub completed: bool,
}

/// 轮换状态
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RotationStatus {
    pub active_key_id: String,
    pub previous_key_id: Option<String>,
    pub overlap_active: bool,
    pub overlap_remaining_seconds: i64,
    pub rotation_count: u64,
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 密钥轮换管理器
pub struct KeyRotationManager {
    active_key_id: Mutex<String>,
    previous_key_id: Mutex<Option<String>>,
    overlap_until: Mutex<Option<i64>>,
    rotation_count: Mutex<u64>,
    records: Mutex<Vec<RotationRecord>>,
    interrupted: Mutex<bool>,
}

impl KeyRotationManager {
    pub fn new(initial_key_id: &str) -> Self {
        Self {
            active_key_id: Mutex::new(initial_key_id.to_string()),
            previous_key_id: Mutex::new(None),
            overlap_until: Mutex::new(None),
            rotation_count: Mutex::new(0),
            records: Mutex::new(Vec::new()),
            interrupted: Mutex::new(false),
        }
    }

    /// 执行密钥轮换
    ///
    /// 生成新密钥 ID → 进入重叠期（≥24h）→ 切换 active → 标记旧密钥过期时间。
    /// 若 interrupted 标志为 true，返回 KeyRotationInterrupted 并回退。
    pub fn rotate(&self, new_key_id: &str) -> Result<RotationRecord, PqcExecError> {
        if *self.interrupted.lock().unwrap() {
            let old_id = self.active_key_id.lock().unwrap().clone();
            return Err(PqcExecError::KeyRotationInterrupted(format!(
                "轮换中断，回退到旧密钥 {}",
                old_id
            )));
        }

        let old_key_id = self.active_key_id.lock().unwrap().clone();
        if new_key_id == old_key_id {
            return Err(PqcExecError::KeyRotationInterrupted(format!(
                "新密钥 ID 与旧密钥 ID 相同: {}",
                new_key_id
            )));
        }

        let rotated_at = now_seconds();
        let overlap_until = rotated_at + MIN_OVERLAP_SECONDS as i64;

        let record = RotationRecord {
            new_key_id: new_key_id.to_string(),
            old_key_id: old_key_id.clone(),
            rotated_at,
            overlap_until,
            completed: false,
        };

        {
            let mut prev = self.previous_key_id.lock().unwrap();
            *prev = Some(old_key_id);
        }
        {
            let mut active = self.active_key_id.lock().unwrap();
            *active = new_key_id.to_string();
        }
        {
            let mut overlap = self.overlap_until.lock().unwrap();
            *overlap = Some(overlap_until);
        }
        {
            let mut count = self.rotation_count.lock().unwrap();
            *count += 1;
        }
        self.records.lock().unwrap().push(record.clone());

        Ok(record)
    }

    /// 标记重叠期结束，旧密钥正式过期
    pub fn complete_overlap(&self) -> Result<(), PqcExecError> {
        let mut overlap = self.overlap_until.lock().unwrap();
        if overlap.is_none() {
            return Err(PqcExecError::KeyRotationInterrupted(
                "无活跃的重叠期".to_string(),
            ));
        }
        *overlap = None;
        let mut prev = self.previous_key_id.lock().unwrap();
        *prev = None;
        if let Some(last) = self.records.lock().unwrap().last_mut() {
            last.completed = true;
        }
        Ok(())
    }

    /// 标记中断（用于测试中断回退）
    pub fn mark_interrupted(&self) {
        *self.interrupted.lock().unwrap() = true;
    }

    /// 清除中断标志
    pub fn clear_interrupted(&self) {
        *self.interrupted.lock().unwrap() = false;
    }

    /// 查询轮换状态
    pub fn status(&self) -> RotationStatus {
        let active_key_id = self.active_key_id.lock().unwrap().clone();
        let previous_key_id = self.previous_key_id.lock().unwrap().clone();
        let overlap_until = *self.overlap_until.lock().unwrap();
        let rotation_count = *self.rotation_count.lock().unwrap();

        let now = now_seconds();
        let (overlap_active, overlap_remaining_seconds) = match overlap_until {
            Some(until) => {
                let remaining = until - now;
                (remaining > 0, remaining)
            }
            None => (false, 0),
        };

        RotationStatus {
            active_key_id,
            previous_key_id,
            overlap_active,
            overlap_remaining_seconds,
            rotation_count,
        }
    }

    /// 获取所有轮换记录
    pub fn records(&self) -> Vec<RotationRecord> {
        self.records.lock().unwrap().clone()
    }

    /// 重叠期下限（秒）
    pub fn min_overlap_seconds() -> u64 {
        MIN_OVERLAP_SECONDS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rotate_success() {
        let manager = KeyRotationManager::new("key-old-001");
        let record = manager.rotate("key-new-001").unwrap();
        assert_eq!(record.old_key_id, "key-old-001");
        assert_eq!(record.new_key_id, "key-new-001");
        assert!(record.overlap_until - record.rotated_at >= MIN_OVERLAP_SECONDS as i64);

        let status = manager.status();
        assert_eq!(status.active_key_id, "key-new-001");
        assert_eq!(status.previous_key_id, Some("key-old-001".to_string()));
        assert!(status.overlap_active);
        assert!(status.overlap_remaining_seconds > 0);
        assert_eq!(status.rotation_count, 1);
    }

    #[test]
    fn test_overlap_parallel_use() {
        let manager = KeyRotationManager::new("key-old-002");
        manager.rotate("key-new-002").unwrap();
        let status = manager.status();
        assert!(status.overlap_active);
        assert!(status.previous_key_id.is_some());
        assert!(status.overlap_remaining_seconds >= MIN_OVERLAP_SECONDS as i64 - 5);
        let records = manager.records();
        assert_eq!(records.len(), 1);
        assert!(!records[0].completed);
    }

    #[test]
    fn test_interrupted_rollback() {
        let manager = KeyRotationManager::new("key-old-003");
        manager.mark_interrupted();
        let err = manager.rotate("key-new-003").unwrap_err();
        assert!(matches!(err, PqcExecError::KeyRotationInterrupted(_)));
        let status = manager.status();
        assert_eq!(status.active_key_id, "key-old-003");
        assert_eq!(status.rotation_count, 0);
    }

    #[test]
    fn test_same_key_id_rejected() {
        let manager = KeyRotationManager::new("key-same");
        let err = manager.rotate("key-same").unwrap_err();
        assert!(matches!(err, PqcExecError::KeyRotationInterrupted(_)));
    }

    #[test]
    fn test_complete_overlap() {
        let manager = KeyRotationManager::new("key-old-004");
        manager.rotate("key-new-004").unwrap();
        manager.complete_overlap().unwrap();
        let status = manager.status();
        assert!(!status.overlap_active);
        assert!(status.previous_key_id.is_none());
        let records = manager.records();
        assert!(records[0].completed);
    }

    #[test]
    fn test_complete_overlap_without_rotation() {
        let manager = KeyRotationManager::new("key-lonely");
        let err = manager.complete_overlap().unwrap_err();
        assert!(matches!(err, PqcExecError::KeyRotationInterrupted(_)));
    }

    #[test]
    fn test_multiple_rotations() {
        let manager = KeyRotationManager::new("key-v0");
        manager.rotate("key-v1").unwrap();
        manager.complete_overlap().unwrap();
        manager.rotate("key-v2").unwrap();
        let status = manager.status();
        assert_eq!(status.active_key_id, "key-v2");
        assert_eq!(status.rotation_count, 2);
        let records = manager.records();
        assert_eq!(records.len(), 2);
        assert!(records[0].completed);
        assert!(!records[1].completed);
    }

    #[test]
    fn test_clear_interrupted_then_rotate() {
        let manager = KeyRotationManager::new("key-old-005");
        manager.mark_interrupted();
        let err = manager.rotate("key-new-005").unwrap_err();
        assert!(matches!(err, PqcExecError::KeyRotationInterrupted(_)));
        manager.clear_interrupted();
        let record = manager.rotate("key-new-005").unwrap();
        assert_eq!(record.new_key_id, "key-new-005");
    }

    #[test]
    fn test_min_overlap_constant() {
        assert_eq!(KeyRotationManager::min_overlap_seconds(), 86_400);
    }
}
