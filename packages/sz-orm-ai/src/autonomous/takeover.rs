//! 自治人工接管：设置 AtomicBool 暂停自治，切换人工模式
//!
//! ADR-004：`AtomicBool` 无锁、零开销。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

/// 接管错误
#[derive(Debug, thiserror::Error)]
pub enum TakeoverError {
    #[error("已接管，不可重复接管")]
    AlreadyTakenOver,
    #[error("接管失败: {0}")]
    TakeoverFailed(String),
    #[error("未接管，无法释放")]
    NotTakenOver,
}

/// 接管记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TakeoverRecord {
    pub taken_over_at: SystemTime,
    pub operator: String,
    pub reason: String,
    pub previous_state: bool,
}

/// 自治人工接管
pub struct AutonomousTakeover {
    flag: Arc<AtomicBool>,
    record: parking_lot::Mutex<Option<TakeoverRecord>>,
}

impl AutonomousTakeover {
    pub fn new(flag: Arc<AtomicBool>) -> Self {
        Self {
            flag,
            record: parking_lot::Mutex::new(None),
        }
    }

    pub fn is_taken_over(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }

    /// 执行接管：设置 AtomicBool 为 true，记录接管事件
    pub fn takeover(&self, operator: &str, reason: &str) -> Result<TakeoverRecord, TakeoverError> {
        if self.flag.load(Ordering::Acquire) {
            return Err(TakeoverError::AlreadyTakenOver);
        }
        let previous_state = false;
        self.flag.store(true, Ordering::Release);
        let record = TakeoverRecord {
            taken_over_at: SystemTime::now(),
            operator: operator.to_string(),
            reason: reason.to_string(),
            previous_state,
        };
        *self.record.lock() = Some(record.clone());
        Ok(record)
    }

    /// 释放接管：恢复自治
    pub fn release(&self) -> Result<(), TakeoverError> {
        if !self.flag.load(Ordering::Acquire) {
            return Err(TakeoverError::NotTakenOver);
        }
        self.flag.store(false, Ordering::Release);
        *self.record.lock() = None;
        Ok(())
    }

    /// 查询接管记录
    pub fn record(&self) -> Option<TakeoverRecord> {
        self.record.lock().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_takeover_success() {
        let flag = Arc::new(AtomicBool::new(false));
        let takeover = AutonomousTakeover::new(flag.clone());
        assert!(!takeover.is_taken_over());
        let record = takeover.takeover("admin", "manual intervention").unwrap();
        assert!(takeover.is_taken_over());
        assert!(flag.load(Ordering::Acquire));
        assert_eq!(record.operator, "admin");
        assert_eq!(record.reason, "manual intervention");
    }

    #[test]
    fn test_takeover_already_taken_over() {
        let flag = Arc::new(AtomicBool::new(true));
        let takeover = AutonomousTakeover::new(flag);
        let result = takeover.takeover("admin", "test");
        assert!(matches!(result, Err(TakeoverError::AlreadyTakenOver)));
    }

    #[test]
    fn test_release_success() {
        let flag = Arc::new(AtomicBool::new(true));
        let takeover = AutonomousTakeover::new(flag.clone());
        takeover.release().unwrap();
        assert!(!takeover.is_taken_over());
        assert!(!flag.load(Ordering::Acquire));
    }

    #[test]
    fn test_release_not_taken_over() {
        let flag = Arc::new(AtomicBool::new(false));
        let takeover = AutonomousTakeover::new(flag);
        let result = takeover.release();
        assert!(matches!(result, Err(TakeoverError::NotTakenOver)));
    }

    #[test]
    fn test_takeover_record_query() {
        let flag = Arc::new(AtomicBool::new(false));
        let takeover = AutonomousTakeover::new(flag);
        assert!(takeover.record().is_none());
        takeover.takeover("ops", "emergency").unwrap();
        let record = takeover.record().unwrap();
        assert_eq!(record.operator, "ops");
        assert_eq!(record.reason, "emergency");
    }

    #[test]
    fn test_takeover_release_cycle() {
        let flag = Arc::new(AtomicBool::new(false));
        let takeover = AutonomousTakeover::new(flag);
        takeover.takeover("admin", "first").unwrap();
        assert!(takeover.is_taken_over());
        takeover.release().unwrap();
        assert!(!takeover.is_taken_over());
        assert!(takeover.record().is_none());
        takeover.takeover("admin", "second").unwrap();
        assert!(takeover.is_taken_over());
        assert_eq!(takeover.record().unwrap().reason, "second");
    }
}
