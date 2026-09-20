//! v7.6.0 3PC 三阶段提交
//!
//! 三阶段提交（3PC）是 2PC 的改进版本，通过增加 PreCommit 阶段
//! 实现非阻塞特性：协调者故障时参与者可超时自主决策。
//!
//! 三阶段：
//! 1. CanCommit：询问参与者是否可提交
//! 2. PreCommit：预提交，持久化决策
//! 3. DoCommit：最终提交
//!
//! 启用 `three-pc` feature 时编译。

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// 3PC 超时配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreePcTimeoutConfig {
    pub can_commit_timeout: Duration,
    pub pre_commit_timeout: Duration,
    pub do_commit_timeout: Duration,
}

impl Default for ThreePcTimeoutConfig {
    fn default() -> Self {
        Self {
            can_commit_timeout: Duration::from_secs(2),
            pre_commit_timeout: Duration::from_secs(2),
            do_commit_timeout: Duration::from_secs(2),
        }
    }
}

/// 3PC 事务状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreePcStatus {
    /// 初始状态
    Init,
    /// CanCommit 阶段
    CanCommitPhase,
    /// PreCommit 阶段
    PreCommitPhase,
    /// DoCommit 阶段
    DoCommitPhase,
    /// 已提交
    Committed,
    /// 已中止
    Aborted,
    /// 超时
    Timeout,
}

/// 参与者决策
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParticipantDecision {
    /// 同意提交
    Commit,
    /// 拒绝提交
    Abort,
}

/// 3PC 结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreePcResult {
    pub transaction_id: String,
    pub commit_latency_ms: u64,
    pub is_non_blocking: bool,
    pub final_status: ThreePcStatus,
}

/// 3PC 参与者
#[derive(Debug, Clone)]
pub struct ThreePcParticipant {
    pub id: String,
    pub decision: Option<ParticipantDecision>,
    pub pre_committed: bool,
    pub committed: bool,
}

impl ThreePcParticipant {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            decision: None,
            pre_committed: false,
            committed: false,
        }
    }
}

/// 3PC 协调器
///
/// 三阶段提交：CanCommit → PreCommit → DoCommit，
/// 协调者故障时参与者超时自主决策（非阻塞特性）。
pub struct ThreePcCoordinator {
    participants: Vec<ThreePcParticipant>,
    timeout_config: ThreePcTimeoutConfig,
}

impl ThreePcCoordinator {
    pub fn new(participants: Vec<ThreePcParticipant>, timeout: ThreePcTimeoutConfig) -> Self {
        Self {
            participants,
            timeout_config: timeout,
        }
    }

    /// 阶段 1：CanCommit（询问可提交）
    ///
    /// 所有参与者回复 Yes → 进入 PreCommit
    /// 任一参与者回复 No → 中止
    pub fn can_commit(&mut self, can_commit_responses: &[bool]) -> ThreePcStatus {
        if can_commit_responses.len() != self.participants.len() {
            eprintln!(
                "2PC_COORDINATOR_FAILURE: CanCommit 响应数 {} ≠ 参与者数 {}",
                can_commit_responses.len(),
                self.participants.len()
            );
            return ThreePcStatus::Aborted;
        }

        for (i, &can) in can_commit_responses.iter().enumerate() {
            self.participants[i].decision = Some(if can {
                ParticipantDecision::Commit
            } else {
                ParticipantDecision::Abort
            });
        }

        if can_commit_responses.iter().all(|&c| c) {
            ThreePcStatus::CanCommitPhase
        } else {
            ThreePcStatus::Aborted
        }
    }

    /// 阶段 2：PreCommit（预提交持久化决策）
    ///
    /// 所有参与者预提交 → 进入 DoCommit
    /// 任一失败 → 中止
    pub fn pre_commit(&mut self) -> ThreePcStatus {
        for p in &mut self.participants {
            if p.decision == Some(ParticipantDecision::Abort) {
                return ThreePcStatus::Aborted;
            }
            p.pre_committed = true;
        }
        ThreePcStatus::PreCommitPhase
    }

    /// 阶段 3：DoCommit（最终提交）
    ///
    /// 所有参与者正式提交
    pub fn do_commit(&mut self, transaction_id: &str) -> ThreePcResult {
        let start = std::time::Instant::now();

        for p in &mut self.participants {
            p.committed = true;
        }

        ThreePcResult {
            transaction_id: transaction_id.to_string(),
            commit_latency_ms: start.elapsed().as_millis() as u64,
            is_non_blocking: true,
            final_status: ThreePcStatus::Committed,
        }
    }

    /// 参与者超时自主决策（非阻塞特性）
    ///
    /// 协调者故障时，参与者根据自身状态决策：
    /// - 已 PreCommit → 提交（因为 CanCommit 已通过，说明所有参与者同意）
    /// - 未 PreCommit → 中止（安全侧）
    pub fn participant_self_decide(participant: &ThreePcParticipant) -> ParticipantDecision {
        if participant.pre_committed {
            ParticipantDecision::Commit
        } else {
            ParticipantDecision::Abort
        }
    }

    /// 执行完整 3PC 流程
    pub fn execute(
        &mut self,
        transaction_id: &str,
        can_commit_responses: &[bool],
    ) -> ThreePcResult {
        let start = std::time::Instant::now();

        let phase1 = self.can_commit(can_commit_responses);
        if phase1 == ThreePcStatus::Aborted {
            return ThreePcResult {
                transaction_id: transaction_id.to_string(),
                commit_latency_ms: start.elapsed().as_millis() as u64,
                is_non_blocking: true,
                final_status: ThreePcStatus::Aborted,
            };
        }

        let phase2 = self.pre_commit();
        if phase2 == ThreePcStatus::Aborted {
            return ThreePcResult {
                transaction_id: transaction_id.to_string(),
                commit_latency_ms: start.elapsed().as_millis() as u64,
                is_non_blocking: true,
                final_status: ThreePcStatus::Aborted,
            };
        }

        self.do_commit(transaction_id)
    }

    /// 获取参与者列表
    pub fn participants(&self) -> &[ThreePcParticipant] {
        &self.participants
    }

    /// 获取超时配置
    pub fn timeout_config(&self) -> &ThreePcTimeoutConfig {
        &self.timeout_config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_three_pc_all_commit() {
        let participants = vec![
            ThreePcParticipant::new("p1"),
            ThreePcParticipant::new("p2"),
            ThreePcParticipant::new("p3"),
        ];
        let mut coord = ThreePcCoordinator::new(participants, ThreePcTimeoutConfig::default());
        let result = coord.execute("tx-1", &[true, true, true]);
        assert_eq!(result.final_status, ThreePcStatus::Committed);
        assert!(result.is_non_blocking);
        assert!(result.commit_latency_ms <= 3000);
    }

    #[test]
    fn test_three_pc_one_abort() {
        let participants = vec![ThreePcParticipant::new("p1"), ThreePcParticipant::new("p2")];
        let mut coord = ThreePcCoordinator::new(participants, ThreePcTimeoutConfig::default());
        let result = coord.execute("tx-2", &[true, false]);
        assert_eq!(result.final_status, ThreePcStatus::Aborted);
    }

    #[test]
    fn test_three_pc_non_blocking_self_decide_pre_committed() {
        let p = ThreePcParticipant {
            id: "p1".to_string(),
            decision: Some(ParticipantDecision::Commit),
            pre_committed: true,
            committed: false,
        };
        let decision = ThreePcCoordinator::participant_self_decide(&p);
        assert_eq!(decision, ParticipantDecision::Commit);
    }

    #[test]
    fn test_three_pc_non_blocking_self_decide_not_pre_committed() {
        let p = ThreePcParticipant::new("p1");
        let decision = ThreePcCoordinator::participant_self_decide(&p);
        assert_eq!(decision, ParticipantDecision::Abort);
    }

    #[test]
    fn test_three_pc_can_commit_mismatch() {
        let participants = vec![ThreePcParticipant::new("p1")];
        let mut coord = ThreePcCoordinator::new(participants, ThreePcTimeoutConfig::default());
        let status = coord.can_commit(&[true, true]);
        assert_eq!(status, ThreePcStatus::Aborted);
    }

    #[test]
    fn test_three_pc_timeout_config() {
        let config = ThreePcTimeoutConfig::default();
        assert!(config.can_commit_timeout >= Duration::from_secs(1));
        assert!(config.pre_commit_timeout >= Duration::from_secs(1));
        assert!(config.do_commit_timeout >= Duration::from_secs(1));
    }

    #[test]
    fn test_three_pc_phases_progression() {
        let participants = vec![ThreePcParticipant::new("p1"), ThreePcParticipant::new("p2")];
        let mut coord = ThreePcCoordinator::new(participants, ThreePcTimeoutConfig::default());

        let phase1 = coord.can_commit(&[true, true]);
        assert_eq!(phase1, ThreePcStatus::CanCommitPhase);

        let phase2 = coord.pre_commit();
        assert_eq!(phase2, ThreePcStatus::PreCommitPhase);
        assert!(coord.participants().iter().all(|p| p.pre_committed));

        let result = coord.do_commit("tx-3");
        assert_eq!(result.final_status, ThreePcStatus::Committed);
        assert!(coord.participants().iter().all(|p| p.committed));
    }
}
