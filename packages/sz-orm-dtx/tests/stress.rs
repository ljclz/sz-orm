//! sz-orm-dtx 压力测试套件
//!
//! 超大数据量验证：
//! - 1 万个分布式事务
//! - 100 个参与者 per 事务
//! - 部分参与者故障下的状态一致性
//! - DtxManager 在 std::sync::RwLock 下的并发安全性（用 spawn_blocking）

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use sz_orm_dtx::{
    DistributedTransaction, DtxManager, ParticipantState, TransactionParticipant, TransactionState,
};

/// 验证：1 万个事务 begin + commit
#[test]
fn stress_dtx_10k_transactions() {
    let manager = DtxManager::new();
    let n: u64 = 10_000;

    for i in 0..n {
        manager.begin(&format!("tx-{}", i)).unwrap();
    }
    assert_eq!(manager.list().len(), n as usize);

    for i in 0..n {
        manager.commit(&format!("tx-{}", i)).unwrap();
    }
    for i in 0..n {
        assert_eq!(
            manager.get(&format!("tx-{}", i)),
            Some(TransactionState::Committed)
        );
    }
}

/// 验证：单个事务 100 个参与者全部 prepare + commit
#[test]
fn stress_dtx_100_participants_per_tx() {
    let mut tx = DistributedTransaction::new("big-tx");
    let n: usize = 100;
    let counter = Arc::new(AtomicU64::new(0));

    for _ in 0..n {
        let c = counter.clone();
        let participant = TransactionParticipant::new("p")
            .with_prepare(move || Ok(()))
            .with_commit(move || {
                c.fetch_add(1, Ordering::Relaxed);
                Ok(())
            })
            .with_rollback(move || Ok(()));
        tx.add_participant(participant);
    }

    tx.prepare().unwrap();
    assert_eq!(tx.state(), TransactionState::Prepared);

    tx.commit().unwrap();
    assert_eq!(tx.state(), TransactionState::Committed);
    assert_eq!(counter.load(Ordering::Relaxed), n as u64);

    // 所有参与者状态
    let states = tx.participants();
    for p in states {
        assert_eq!(p.state, ParticipantState::Committed);
    }
}

/// 验证：prepare 失败时已 prepare 的参与者回滚
#[test]
fn stress_dtx_prepare_failure_rolls_back() {
    let mut tx = DistributedTransaction::new("fail-tx");
    let rollback_counter = Arc::new(AtomicU64::new(0));

    // 前 5 个参与者 prepare 成功
    for i in 0..5 {
        let r = rollback_counter.clone();
        let participant = TransactionParticipant::new(&format!("p-{}", i))
            .with_prepare(move || Ok(()))
            .with_rollback(move || {
                r.fetch_add(1, Ordering::Relaxed);
                Ok(())
            });
        tx.add_participant(participant);
    }

    // 第 6 个参与者 prepare 失败
    let failing = TransactionParticipant::new("p-fail")
        .with_prepare(|| Err("intentional failure".to_string()))
        .with_rollback(|| Ok(()));
    tx.add_participant(failing);

    let result = tx.prepare();
    assert!(result.is_err());
    assert_eq!(tx.state(), TransactionState::Failed);
    // 前 5 个已 prepare 的参与者应被回滚
    assert_eq!(rollback_counter.load(Ordering::Relaxed), 5);

    // 失败的参与者状态为 Failed
    let fail_p = tx
        .participants()
        .iter()
        .find(|p| p.resource_id == "p-fail")
        .unwrap();
    assert_eq!(fail_p.state, ParticipantState::Failed);
}

/// 验证：commit 失败时事务标记 Failed
#[test]
fn stress_dtx_commit_failure() {
    let mut tx = DistributedTransaction::new("commit-fail-tx");
    tx.add_participant(TransactionParticipant::new("p-1").with_prepare(|| Ok(())));
    tx.add_participant(
        TransactionParticipant::new("p-2")
            .with_prepare(|| Ok(()))
            .with_commit(|| Err("commit failure".to_string())),
    );

    tx.prepare().unwrap();
    let result = tx.commit();
    assert!(result.is_err());
    assert_eq!(tx.state(), TransactionState::Failed);
}

/// 验证：rollback 在 Active/Prepared/Failed 状态下都成功
#[test]
fn stress_dtx_rollback_from_various_states() {
    // Active → RolledBack
    let mut tx1 = DistributedTransaction::new("tx1");
    tx1.rollback().unwrap();
    assert_eq!(tx1.state(), TransactionState::RolledBack);

    // Prepared → RolledBack
    let mut tx2 = DistributedTransaction::new("tx2");
    tx2.add_participant(TransactionParticipant::new("p").with_prepare(|| Ok(())));
    tx2.prepare().unwrap();
    tx2.rollback().unwrap();
    assert_eq!(tx2.state(), TransactionState::RolledBack);

    // Failed → RolledBack
    let mut tx3 = DistributedTransaction::new("tx3");
    tx3.add_participant(TransactionParticipant::new("p").with_prepare(|| Err("fail".to_string())));
    let _ = tx3.prepare();
    assert_eq!(tx3.state(), TransactionState::Failed);
    tx3.rollback().unwrap();
    assert_eq!(tx3.state(), TransactionState::RolledBack);
}

/// 验证：rollback 终态（Committed/RolledBack）失败
#[test]
fn stress_dtx_rollback_terminal_state_fails() {
    let mut tx = DistributedTransaction::new("tx");
    tx.commit().unwrap();
    let result = tx.rollback();
    assert!(result.is_err());

    let mut tx2 = DistributedTransaction::new("tx2");
    tx2.rollback().unwrap();
    let result = tx2.rollback();
    assert!(result.is_err());
}

/// 验证：DtxManager 并发安全（用 std::thread 模拟同步锁竞争）
#[test]
fn stress_dtx_manager_concurrent_isolation() {
    let manager = Arc::new(DtxManager::new());
    let task_count: u64 = 8;
    let per_task: u64 = 1000;
    let total = task_count * per_task;

    let mut handles = Vec::new();
    for task_id in 0..task_count {
        let m = manager.clone();
        handles.push(std::thread::spawn(move || {
            for i in 0..per_task {
                let tx_id = format!("t{}-{}", task_id, i);
                m.begin(&tx_id).unwrap();
                m.commit(&tx_id).unwrap();
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(manager.list().len(), total as usize);
    for task_id in 0..task_count {
        for i in 0..per_task {
            let tx_id = format!("t{}-{}", task_id, i);
            assert_eq!(manager.get(&tx_id), Some(TransactionState::Committed));
        }
    }
}

/// 验证：DtxManager 重复 begin 失败
#[test]
fn stress_dtx_manager_duplicate_begin() {
    let manager = DtxManager::new();
    manager.begin("dup").unwrap();
    for _ in 0..100 {
        let result = manager.begin("dup");
        assert!(result.is_err(), "duplicate begin must fail");
    }
    assert_eq!(manager.list().len(), 1);
}

/// 验证：DtxManager 操作不存在的事务失败
#[test]
fn stress_dtx_manager_not_found() {
    let manager = DtxManager::new();
    for i in 0..100 {
        let id = format!("nonexistent-{}", i);
        assert!(manager.prepare(&id).is_err());
        assert!(manager.commit(&id).is_err());
        assert!(manager.rollback(&id).is_err());
        assert!(manager
            .add_participant(&id, TransactionParticipant::new("p"))
            .is_err());
        assert_eq!(manager.get(&id), None);
        assert_eq!(manager.participant_states(&id), None);
    }
}

/// 验证：participant_states 在大量参与者下准确
#[test]
fn stress_dtx_manager_participant_states() {
    let manager = DtxManager::new();
    manager.begin("tx").unwrap();
    let n: usize = 1000;
    for i in 0..n {
        manager
            .add_participant("tx", TransactionParticipant::new(&format!("p-{}", i)))
            .unwrap();
    }
    let states = manager.participant_states("tx").unwrap();
    assert_eq!(states.len(), n);
    for s in &states {
        assert_eq!(*s, ParticipantState::Active);
    }

    manager.prepare("tx").unwrap();
    let states = manager.participant_states("tx").unwrap();
    for s in &states {
        assert_eq!(*s, ParticipantState::Prepared);
    }
}

/// 验证：add_participant 在非 Active 状态失败
#[test]
fn stress_dtx_add_participant_after_prepare() {
    let manager = DtxManager::new();
    manager.begin("tx").unwrap();
    manager
        .add_participant("tx", TransactionParticipant::new("p-1"))
        .unwrap();
    manager.prepare("tx").unwrap();

    // prepare 后不能 add
    let result = manager.add_participant("tx", TransactionParticipant::new("p-2"));
    assert!(result.is_err());
}
