//! v7.6.0 组3.9：3PC 三阶段提交端到端测试
//!
//! 验证：3PC 三阶段提交 + 非阻塞特性 + 协调者故障参与者自主决策 + 提交延迟 ≤ 3s

use sz_orm_dtx::three_pc::{
    ParticipantDecision, ThreePcCoordinator, ThreePcParticipant, ThreePcStatus,
    ThreePcTimeoutConfig,
};

#[test]
fn test_e2e_three_pc_commit() {
    let participants = vec![
        ThreePcParticipant::new("p1"),
        ThreePcParticipant::new("p2"),
        ThreePcParticipant::new("p3"),
    ];
    let mut coord = ThreePcCoordinator::new(participants, ThreePcTimeoutConfig::default());
    let result = coord.execute("tx-e2e-1", &[true, true, true]);
    assert_eq!(result.final_status, ThreePcStatus::Committed);
    assert!(result.is_non_blocking);
    assert!(result.commit_latency_ms <= 3000);
}

#[test]
fn test_e2e_three_pc_abort() {
    let participants = vec![ThreePcParticipant::new("p1"), ThreePcParticipant::new("p2")];
    let mut coord = ThreePcCoordinator::new(participants, ThreePcTimeoutConfig::default());
    let result = coord.execute("tx-e2e-2", &[true, false]);
    assert_eq!(result.final_status, ThreePcStatus::Aborted);
}

#[test]
fn test_e2e_three_pc_non_blocking() {
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
fn test_e2e_three_pc_phases() {
    let participants = vec![ThreePcParticipant::new("p1"), ThreePcParticipant::new("p2")];
    let mut coord = ThreePcCoordinator::new(participants, ThreePcTimeoutConfig::default());

    let s1 = coord.can_commit(&[true, true]);
    assert_eq!(s1, ThreePcStatus::CanCommitPhase);

    let s2 = coord.pre_commit();
    assert_eq!(s2, ThreePcStatus::PreCommitPhase);

    let result = coord.do_commit("tx-e2e-3");
    assert_eq!(result.final_status, ThreePcStatus::Committed);
}
