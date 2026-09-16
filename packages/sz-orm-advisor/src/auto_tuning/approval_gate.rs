//! 人工审批门控（`query-auto-tuning` feature）
//!
//! 低置信度建议提交审批，等待人工 Approve/Reject；
//! 超时未审批自动过期丢弃，不阻塞闭环。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::suggestion::OptimizationSuggestion;

/// 审批状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalState {
    /// 等待审批
    Pending,
    /// 已批准
    Approved,
    /// 已拒绝（含原因）
    Rejected(String),
    /// 超时过期
    Expired,
}

/// 审批条目
#[derive(Debug, Clone)]
pub struct ApprovalEntry {
    /// 建议内容
    pub suggestion: OptimizationSuggestion,
    /// 当前状态
    pub state: ApprovalState,
    /// 提交时间
    pub submitted_at: Instant,
}

/// 审批门控配置
#[derive(Debug, Clone)]
pub struct ApprovalGateConfig {
    /// 是否要求审批（false = 自动通过所有建议）
    pub require_approval: bool,
    /// 审批超时（默认 24 小时）
    pub timeout: Duration,
}

impl Default for ApprovalGateConfig {
    fn default() -> Self {
        Self {
            require_approval: true,
            timeout: Duration::from_secs(86400),
        }
    }
}

/// 人工审批门控
#[derive(Debug)]
pub struct ApprovalGate {
    config: ApprovalGateConfig,
    entries: HashMap<String, ApprovalEntry>,
    next_id: u64,
}

impl ApprovalGate {
    pub fn new(config: ApprovalGateConfig) -> Self {
        Self {
            config,
            entries: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(ApprovalGateConfig::default())
    }

    pub fn config(&self) -> &ApprovalGateConfig {
        &self.config
    }

    /// 提交建议审批，返回审批 ID
    ///
    /// 若 `require_approval = false`，直接返回 ID 但状态标记为 Approved。
    pub fn submit(&mut self, suggestion: OptimizationSuggestion) -> String {
        let id = format!("apr-{}", self.next_id);
        self.next_id += 1;
        let state = if self.config.require_approval {
            ApprovalState::Pending
        } else {
            ApprovalState::Approved
        };
        self.entries.insert(
            id.clone(),
            ApprovalEntry {
                suggestion,
                state,
                submitted_at: Instant::now(),
            },
        );
        id
    }

    /// 批准建议
    pub fn approve(&mut self, id: &str) -> bool {
        if let Some(entry) = self.entries.get_mut(id) {
            if entry.state == ApprovalState::Pending {
                entry.state = ApprovalState::Approved;
                return true;
            }
        }
        false
    }

    /// 拒绝建议
    pub fn reject(&mut self, id: &str, reason: impl Into<String>) -> bool {
        if let Some(entry) = self.entries.get_mut(id) {
            if entry.state == ApprovalState::Pending {
                entry.state = ApprovalState::Rejected(reason.into());
                return true;
            }
        }
        false
    }

    /// 查询审批状态
    pub fn state(&self, id: &str) -> Option<&ApprovalState> {
        self.entries.get(id).map(|e| &e.state)
    }

    /// 是否已批准
    pub fn is_approved(&self, id: &str) -> bool {
        matches!(self.state(id), Some(ApprovalState::Approved))
    }

    /// 清理超时条目，返回过期数量
    pub fn purge_expired(&mut self) -> usize {
        let now = Instant::now();
        let timeout = self.config.timeout;
        let mut count = 0;
        for entry in self.entries.values_mut() {
            if entry.state == ApprovalState::Pending
                && now.duration_since(entry.submitted_at) > timeout
            {
                entry.state = ApprovalState::Expired;
                count += 1;
            }
        }
        count
    }

    /// 待审批条目数
    pub fn pending_count(&self) -> usize {
        self.entries
            .values()
            .filter(|e| e.state == ApprovalState::Pending)
            .count()
    }

    /// 获取建议内容
    pub fn suggestion(&self, id: &str) -> Option<&OptimizationSuggestion> {
        self.entries.get(id).map(|e| &e.suggestion)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suggestion::SuggestionType;

    fn test_suggestion() -> OptimizationSuggestion {
        OptimizationSuggestion::new(
            SuggestionType::AddIndex,
            "SELECT * FROM users WHERE email = ?",
            "full table scan",
            "CREATE INDEX idx ON users(email)",
            0.3,
        )
    }

    #[test]
    fn submit_and_approve() {
        let mut gate = ApprovalGate::with_defaults();
        let id = gate.submit(test_suggestion());
        assert_eq!(gate.state(&id), Some(&ApprovalState::Pending));
        assert!(gate.approve(&id));
        assert!(gate.is_approved(&id));
    }

    #[test]
    fn submit_and_reject() {
        let mut gate = ApprovalGate::with_defaults();
        let id = gate.submit(test_suggestion());
        assert!(gate.reject(&id, "index already exists"));
        assert_eq!(
            gate.state(&id),
            Some(&ApprovalState::Rejected("index already exists".into()))
        );
    }

    #[test]
    fn no_approval_required_auto_approves() {
        let mut gate = ApprovalGate::new(ApprovalGateConfig {
            require_approval: false,
            timeout: Duration::from_secs(3600),
        });
        let id = gate.submit(test_suggestion());
        assert!(gate.is_approved(&id));
    }

    #[test]
    fn purge_expired_entries() {
        let mut gate = ApprovalGate::new(ApprovalGateConfig {
            require_approval: true,
            timeout: Duration::from_millis(0),
        });
        let _id = gate.submit(test_suggestion());
        std::thread::sleep(Duration::from_millis(1));
        let expired = gate.purge_expired();
        assert_eq!(expired, 1);
    }

    #[test]
    fn pending_count_tracks_submissions() {
        let mut gate = ApprovalGate::with_defaults();
        let id1 = gate.submit(test_suggestion());
        let id2 = gate.submit(test_suggestion());
        assert_eq!(gate.pending_count(), 2);
        gate.approve(&id1);
        assert_eq!(gate.pending_count(), 1);
        gate.approve(&id2);
        assert_eq!(gate.pending_count(), 0);
    }
}
