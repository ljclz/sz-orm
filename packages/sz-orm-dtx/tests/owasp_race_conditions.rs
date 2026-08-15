#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A14: 业务逻辑并发竞态条件渗透测试（dtx 包）
//!
//! 对应 REQ-V49-014（OWASP 竞态条件）
//!
//! 渗透测试向量：
//! - DTX 状态机一致：并发 commit/rollback 同一事务，仅一个成功

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::scope;
use sz_orm_dtx::DtxManager;

/// A14-7：DTX 状态机一致——并发 commit/rollback 同一事务，仅一个成功
///
/// 攻击模型：攻击者并发提交和回滚同一分布式事务，
/// 试图导致部分提交（一些参与者提交，一些回滚）。
/// 防护：DtxManager 使用 RwLock 保护状态机，状态转换原子化。
#[test]
fn race_dtx_state_machine_consistent() {
    let manager = Arc::new(DtxManager::new());
    manager.begin("tx-race-001").expect("begin 应成功");
    manager.prepare("tx-race-001").expect("prepare 应成功");

    let commit_success = Arc::new(AtomicU32::new(0));
    let rollback_success = Arc::new(AtomicU32::new(0));

    scope(|s| {
        let manager_clone1 = Arc::clone(&manager);
        let commit_success_clone = Arc::clone(&commit_success);
        s.spawn(move || {
            let result = manager_clone1.commit("tx-race-001");
            if result.is_ok() {
                commit_success_clone.fetch_add(1, Ordering::SeqCst);
            }
        });

        let manager_clone2 = Arc::clone(&manager);
        let rollback_success_clone = Arc::clone(&rollback_success);
        s.spawn(move || {
            let result = manager_clone2.rollback("tx-race-001");
            if result.is_ok() {
                rollback_success_clone.fetch_add(1, Ordering::SeqCst);
            }
        });
    });

    let commits = commit_success.load(Ordering::SeqCst);
    let rollbacks = rollback_success.load(Ordering::SeqCst);
    let total = commits + rollbacks;

    assert_eq!(
        total, 1,
        "commit 和 rollback 仅一个应成功，实际 commit={} rollback={}",
        commits, rollbacks
    );
}
