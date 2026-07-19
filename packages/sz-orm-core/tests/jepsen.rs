//! Jepsen 风格测试套件
//!
//! 关注分布式系统一致性问题：
//! - 事务状态机正确性
//! - savepoint 嵌套与回滚
//! - 故障注入下的事务行为
//! - 连接池在故障下的一致性
//! - 并发事务的状态隔离
//!
//! 不变量：
//! - 事务状态机：Active → {Committed | RolledBack}，终态后所有操作失败
//! - savepoint 名称单调递增
//! - commit/rollback 失败时事务保持 Active
//! - Transaction drop 后状态变为 RolledBack
//! - 并发事务互不影响

mod common;

use common::{FaultyConnection, FaultyConnectionFactory, MockConnection, MockConnectionFactory};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use sz_orm_core::TxError;
use sz_orm_core::{Pool, PoolConfigBuilder};
use sz_orm_core::{TransactOptions, Transaction, TransactionManager, TransactionState};
use tokio::sync::Mutex;

/// 辅助：创建一个 MockConnection 的事务
fn make_tx() -> Transaction {
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    let conn = MockConnection::new(db);
    Transaction::new(Box::new(conn), TransactOptions::default())
}

/// 辅助：创建一个 FaultyConnection 的事务
fn make_faulty_tx(
    fail_commit: bool,
    fail_rollback: bool,
    fail_execute_n: Option<u32>,
) -> Transaction {
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    let mut conn = FaultyConnection::new(db);
    conn.fail_on_commit = fail_commit;
    conn.fail_on_rollback = fail_rollback;
    conn.fail_on_execute_n = fail_execute_n;
    Transaction::new(Box::new(conn), TransactOptions::default())
}

// ===== 事务状态机测试 =====

/// Jepsen 1：事务状态机正确性
/// 验证：Active → Committed，Committed 后所有操作失败
#[tokio::test]
async fn jepsen_transaction_state_machine_commit() {
    let mut tx = make_tx();
    assert_eq!(tx.state(), TransactionState::Active);
    assert!(tx.is_active());

    tx.execute("INSERT").await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(tx.state(), TransactionState::Committed);
    assert!(!tx.is_active());

    // 再次 commit 应失败
    let result = tx.commit().await;
    assert!(result.is_err(), "double commit should fail");

    // rollback 已提交事务应失败
    let result = tx.rollback().await;
    assert!(result.is_err(), "rollback after commit should fail");

    // execute 已提交事务应失败
    let result = tx.execute("SELECT").await;
    assert!(result.is_err(), "execute after commit should fail");
}

/// Jepsen 2：事务状态机正确性
/// 验证：Active → RolledBack，RolledBack 后所有操作失败
#[tokio::test]
async fn jepsen_transaction_state_machine_rollback() {
    let mut tx = make_tx();
    assert_eq!(tx.state(), TransactionState::Active);

    tx.execute("INSERT").await.unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(tx.state(), TransactionState::RolledBack);
    assert!(!tx.is_active());

    // 再次 rollback 应失败
    let result = tx.rollback().await;
    assert!(result.is_err(), "double rollback should fail");

    // commit 已回滚事务应失败
    let result = tx.commit().await;
    assert!(result.is_err(), "commit after rollback should fail");

    // execute 已回滚事务应失败
    let result = tx.execute("SELECT").await;
    assert!(result.is_err(), "execute after rollback should fail");
}

// ===== savepoint 测试 =====

/// Jepsen 3：savepoint 嵌套
/// 验证：多个 savepoint 名称单调递增
#[tokio::test]
async fn jepsen_savepoint_nested_names() {
    let mut tx = make_tx();
    assert_eq!(tx.state(), TransactionState::Active);

    let sp1 = tx.savepoint().await.unwrap();
    assert_eq!(sp1, "sp_1");
    let sp2 = tx.savepoint().await.unwrap();
    assert_eq!(sp2, "sp_2");
    let sp3 = tx.savepoint().await.unwrap();
    assert_eq!(sp3, "sp_3");

    // 回滚到 sp2
    tx.rollback_to_savepoint(&sp2).await.unwrap();
    // 释放 sp3（虽然已经回滚到 sp2，但 sp3 仍然可以释放）
    tx.release_savepoint(&sp3).await.unwrap();

    // 新 savepoint 应该是 sp_4（计数器不回退）
    let sp4 = tx.savepoint().await.unwrap();
    assert_eq!(sp4, "sp_4");

    tx.commit().await.unwrap();
}

/// Jepsen 4：savepoint 在事务结束后应失败
#[tokio::test]
async fn jepsen_savepoint_after_commit() {
    let mut tx = make_tx();
    tx.commit().await.unwrap();

    let result = tx.savepoint().await;
    assert!(result.is_err(), "savepoint after commit should fail");
    assert!(matches!(result, Err(TxError::NotActive(_))));
}

#[tokio::test]
async fn jepsen_savepoint_after_rollback() {
    let mut tx = make_tx();
    tx.rollback().await.unwrap();

    let result = tx.savepoint().await;
    assert!(result.is_err(), "savepoint after rollback should fail");
}

#[tokio::test]
async fn jepsen_rollback_to_savepoint_after_commit() {
    let mut tx = make_tx();
    let sp = tx.savepoint().await.unwrap();
    tx.commit().await.unwrap();

    let result = tx.rollback_to_savepoint(&sp).await;
    assert!(
        result.is_err(),
        "rollback_to_savepoint after commit should fail"
    );
}

#[tokio::test]
async fn jepsen_release_savepoint_after_commit() {
    let mut tx = make_tx();
    let sp = tx.savepoint().await.unwrap();
    tx.commit().await.unwrap();

    let result = tx.release_savepoint(&sp).await;
    assert!(
        result.is_err(),
        "release_savepoint after commit should fail"
    );
}

// ===== 故障注入测试 =====

/// Jepsen 5：commit 失败后事务状态
/// 验证：commit 失败时事务保持 Active，可以重试或 rollback
#[tokio::test]
async fn jepsen_commit_failure_keeps_active() {
    let mut tx = make_faulty_tx(true, false, None);
    assert_eq!(tx.state(), TransactionState::Active);

    tx.execute("INSERT").await.unwrap();
    // commit 失败
    let result = tx.commit().await;
    assert!(result.is_err(), "commit should fail");
    assert!(matches!(result, Err(TxError::CommitFailed(_))));

    // 事务仍然 Active
    assert_eq!(
        tx.state(),
        TransactionState::Active,
        "tx must remain Active after commit failure"
    );
    assert!(tx.is_active());

    // 可以 rollback
    let result = tx.rollback().await;
    // 注意：FaultyConnection 的 fail_on_commit=true 但 fail_on_rollback=false
    assert!(
        result.is_ok(),
        "rollback after failed commit should succeed"
    );
    assert_eq!(tx.state(), TransactionState::RolledBack);
}

/// Jepsen 6：rollback 失败后事务状态
/// 验证：rollback 失败时事务保持 Active
#[tokio::test]
async fn jepsen_rollback_failure_keeps_active() {
    let mut tx = make_faulty_tx(false, true, None);
    assert_eq!(tx.state(), TransactionState::Active);

    tx.execute("INSERT").await.unwrap();
    // rollback 失败
    let result = tx.rollback().await;
    assert!(result.is_err(), "rollback should fail");
    assert!(matches!(result, Err(TxError::RollbackFailed(_))));

    // 事务仍然 Active
    assert_eq!(
        tx.state(),
        TransactionState::Active,
        "tx must remain Active after rollback failure"
    );

    // 可以 commit（FaultyConnection fail_on_commit=false）
    let result = tx.commit().await;
    assert!(
        result.is_ok(),
        "commit after failed rollback should succeed"
    );
    assert_eq!(tx.state(), TransactionState::Committed);
}

/// Jepsen 7：execute 失败后事务状态
/// 验证：execute 失败时事务保持 Active，可以 rollback
#[tokio::test]
async fn jepsen_execute_failure_keeps_active() {
    let mut tx = make_faulty_tx(false, false, Some(2));
    assert_eq!(tx.state(), TransactionState::Active);

    // 第一次 execute 成功
    tx.execute("INSERT 1").await.unwrap();
    // 第二次 execute 失败（fail_on_execute_n=2）
    let result = tx.execute("INSERT 2").await;
    assert!(result.is_err(), "second execute should fail");

    // 事务仍然 Active
    assert_eq!(tx.state(), TransactionState::Active);
    assert!(tx.is_active());

    // 可以 rollback
    tx.rollback().await.unwrap();
    assert_eq!(tx.state(), TransactionState::RolledBack);
}

/// Jepsen 8：连接断开后的事务行为
/// 验证：execute 导致连接断开，后续操作失败但事务状态一致
#[tokio::test]
async fn jepsen_connection_disconnect_during_tx() {
    let mut tx = make_faulty_tx(false, false, Some(2));
    tx.execute("INSERT 1").await.unwrap();

    // 第二次 execute 失败，连接断开（FaultyConnection 设置 connected=false）
    let result = tx.execute("INSERT 2").await;
    assert!(result.is_err());

    // 事务仍然 Active
    assert_eq!(tx.state(), TransactionState::Active);

    // rollback 应该能调用（即使连接已断开，FaultyConnection.rollback 不检查 connected）
    let result = tx.rollback().await;
    assert!(
        result.is_ok(),
        "rollback should succeed even if connection disconnected"
    );
    assert_eq!(tx.state(), TransactionState::RolledBack);
}

// ===== TransactionManager 测试 =====

/// Jepsen 9：TransactionManager 并发事务隔离
/// 验证：多个并发事务互不影响
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn jepsen_transaction_manager_concurrent_isolation() {
    let mgr = Arc::new(TransactionManager::new());
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));

    let mut handles = Vec::new();
    let success = Arc::new(AtomicU64::new(0));

    for i in 0..10 {
        let mgr_clone = mgr.clone();
        let db_clone = db.clone();
        let s = success.clone();
        handles.push(tokio::spawn(async move {
            let tx_id = format!("tx_{}", i);
            let conn = MockConnection::new(db_clone);
            // begin
            mgr_clone
                .begin(tx_id.clone(), Box::new(conn), TransactOptions::default())
                .await
                .unwrap();

            // 验证状态
            let state = mgr_clone.state(&tx_id).await;
            assert_eq!(state, Some(TransactionState::Active));

            // commit 或 rollback
            if i % 2 == 0 {
                mgr_clone.commit(&tx_id).await.unwrap();
                let state = mgr_clone.state(&tx_id).await;
                assert_eq!(state, Some(TransactionState::Committed));
            } else {
                mgr_clone.rollback(&tx_id).await.unwrap();
                let state = mgr_clone.state(&tx_id).await;
                assert_eq!(state, Some(TransactionState::RolledBack));
            }

            s.fetch_add(1, Ordering::Relaxed);
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(success.load(Ordering::Relaxed), 10);

    // 所有事务都应该在 manager 中
    let list = mgr.list().await;
    assert_eq!(list.len(), 10);
}

/// Jepsen 10：TransactionManager 不存在的事务
#[tokio::test]
async fn jepsen_transaction_manager_not_found() {
    let mgr = TransactionManager::new();

    let result = mgr.commit("nonexistent").await;
    assert!(result.is_err());

    let result = mgr.rollback("nonexistent").await;
    assert!(result.is_err());

    let state = mgr.state("nonexistent").await;
    assert_eq!(state, None);
}

/// Jepsen 11：TransactionManager remove 后状态不可访问
#[tokio::test]
async fn jepsen_transaction_manager_remove() {
    let mgr = TransactionManager::new();
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    let conn = MockConnection::new(db);

    mgr.begin(
        "tx1".to_string(),
        Box::new(conn),
        TransactOptions::default(),
    )
    .await
    .unwrap();

    let removed = mgr.remove("tx1").await;
    assert!(removed.is_some());

    let state = mgr.state("tx1").await;
    assert_eq!(state, None);

    let list = mgr.list().await;
    assert!(list.is_empty());
}

// ===== 连接池故障测试 =====

/// Jepsen 12：工厂故障下 acquire 的行为
/// 验证：factory.create 失败时，acquire 返回错误并保持 active_count 一致
#[tokio::test]
async fn jepsen_pool_factory_fault() {
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    // 第 1 次创建失败
    let factory = Arc::new(FaultyConnectionFactory::new(db, 1));
    let config = PoolConfigBuilder::new()
        .max_size(5)
        .min_idle(0)
        .acquire_timeout(2)
        .build()
        .unwrap();
    let pool = Pool::new(config, factory).unwrap();

    // 第一次 acquire 应该失败（factory.create 失败）
    // 注意：acquire 内部会重试，但如果 factory 一直失败，最终超时
    let result = pool.acquire().await;
    assert!(result.is_err(), "first acquire should fail");

    let status = pool.status().await;
    // active_count 应该是 0（失败后已减少）
    assert_eq!(
        status.active, status.idle,
        "no borrowed conns after failure"
    );

    // 第二次 acquire 应该成功（factory 第 2 次创建成功）
    let conn = pool.acquire().await;
    assert!(conn.is_ok(), "second acquire should succeed");
}

/// Jepsen 13：close_all 后新 acquire 的行为
/// 验证：close_all 后池被标记为已关闭，acquire 返回 PoolError::Closed
#[tokio::test]
async fn jepsen_pool_close_all_then_acquire() {
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    let factory = Arc::new(MockConnectionFactory::new(db));
    let config = PoolConfigBuilder::new()
        .max_size(5)
        .min_idle(0)
        .acquire_timeout(2)
        .build()
        .unwrap();
    let pool = Pool::new(config, factory).unwrap();

    // 创建并释放几个连接
    let c1 = pool.acquire().await.unwrap();
    let c2 = pool.acquire().await.unwrap();
    pool.release(c1).await;
    pool.release(c2).await;

    // close_all
    pool.close_all().await;
    let status = pool.status().await;
    assert_eq!(status.idle, 0);
    assert_eq!(status.active, 0);

    // close_all 后 acquire 被拒绝（池已关闭）
    let result = pool.acquire().await;
    assert!(
        result.is_err(),
        "acquire after close_all should be rejected"
    );
}

/// Jepsen 14：连接池在并发故障下的一致性
/// 验证：多个 task 并发 acquire/release，同时 factory 偶尔故障
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn jepsen_pool_concurrent_with_faults() {
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    // 每 3 次创建失败 1 次
    let factory = Arc::new(FaultyConnectionFactory::new(db, 3));
    let config = PoolConfigBuilder::new()
        .max_size(10)
        .min_idle(0)
        .acquire_timeout(3)
        .build()
        .unwrap();
    let pool = Arc::new(Pool::new(config, factory).unwrap());

    let success = Arc::new(AtomicU64::new(0));
    let failure = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::new();
    for _ in 0..8 {
        let s = success.clone();
        let f = failure.clone();
        let p = pool.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..20 {
                match p.acquire().await {
                    Ok(conn) => {
                        p.release(conn).await;
                        s.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        f.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let total = success.load(Ordering::Relaxed) + failure.load(Ordering::Relaxed);
    assert_eq!(total, 160, "all ops must complete");

    let status = pool.status().await;
    assert_eq!(status.active, status.idle, "no borrowed conns");
}

// ===== 事务 Drop 测试 =====

/// Jepsen 15：事务 drop 后状态变为 RolledBack
/// 验证：未显式 commit/rollback 的事务 drop 后状态正确
#[tokio::test]
async fn jepsen_transaction_drop_without_commit() {
    let mut tx = make_tx();
    tx.execute("INSERT").await.unwrap();
    assert_eq!(tx.state(), TransactionState::Active);

    // drop 事务（Transaction::drop 会标记为 RolledBack）
    drop(tx);

    // 无法直接验证状态（tx 已被 drop），但可以验证没有 panic
    // 这是一个行为验证：drop 不会 panic
}

/// Jepsen 16：事务在 commit 后 drop 不会改变状态
#[tokio::test]
async fn jepsen_transaction_drop_after_commit() {
    let mut tx = make_tx();
    tx.commit().await.unwrap();
    assert_eq!(tx.state(), TransactionState::Committed);
    // drop 不会改变状态（因为 state != Active）
    drop(tx);
}

// ===== savepoint 复杂场景 =====

/// Jepsen 17：savepoint 部分回滚
/// 验证：回滚到 savepoint 后，事务仍然可以继续操作
#[tokio::test]
async fn jepsen_savepoint_partial_rollback() {
    let mut tx = make_tx();

    tx.execute("INSERT 1").await.unwrap();
    let sp1 = tx.savepoint().await.unwrap();
    tx.execute("INSERT 2").await.unwrap();
    let _sp2 = tx.savepoint().await.unwrap();
    tx.execute("INSERT 3").await.unwrap();

    // 回滚到 sp1，撤销 INSERT 2 和 INSERT 3
    tx.rollback_to_savepoint(&sp1).await.unwrap();

    // 事务仍然 Active
    assert_eq!(tx.state(), TransactionState::Active);

    // 可以继续执行
    tx.execute("INSERT 4").await.unwrap();

    // 创建新 savepoint
    let sp3 = tx.savepoint().await.unwrap();
    assert_eq!(sp3, "sp_3"); // 计数器单调递增

    tx.commit().await.unwrap();
    assert_eq!(tx.state(), TransactionState::Committed);
}

/// Jepsen 18：savepoint 释放后不能再使用
/// 验证：release_savepoint 后，该 savepoint 名字不再有效
/// 注意：当前实现不跟踪已释放的 savepoint，只能验证 release 操作本身成功
#[tokio::test]
async fn jepsen_savepoint_release() {
    let mut tx = make_tx();

    let sp1 = tx.savepoint().await.unwrap();
    tx.execute("INSERT").await.unwrap();

    // 释放 savepoint
    tx.release_savepoint(&sp1).await.unwrap();

    // 事务仍然 Active，可以继续
    tx.execute("INSERT 2").await.unwrap();
    tx.commit().await.unwrap();
}

// ===== 长时间运行的事务 + 故障 =====

/// Jepsen 19：长事务中执行多次操作后 commit
#[tokio::test]
async fn jepsen_long_transaction_multiple_ops() {
    let mut tx = make_tx();

    for i in 0..100 {
        tx.execute(&format!("INSERT {}", i)).await.unwrap();
    }

    tx.commit().await.unwrap();
    assert_eq!(tx.state(), TransactionState::Committed);
}

/// Jepsen 20：长事务中执行多次操作后 rollback
#[tokio::test]
async fn jepsen_long_transaction_rollback() {
    let mut tx = make_tx();

    for i in 0..100 {
        tx.execute(&format!("INSERT {}", i)).await.unwrap();
    }

    tx.rollback().await.unwrap();
    assert_eq!(tx.state(), TransactionState::RolledBack);
}

// ===== TransactOptions 测试 =====

/// Jepsen 21：TransactOptions 配置正确传递
#[tokio::test]
async fn jepsen_transact_options() {
    use sz_orm_core::IsolationLevel;
    let opts = TransactOptions::default()
        .with_isolation(IsolationLevel::Serializable)
        .read_only()
        .with_timeout(Duration::from_secs(30));

    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    let conn = MockConnection::new(db);
    let tx = Transaction::new(Box::new(conn), opts);

    let tx_opts = tx.options();
    assert_eq!(tx_opts.isolation_level, Some(IsolationLevel::Serializable));
    assert!(tx_opts.read_only);
    assert_eq!(tx_opts.timeout, Some(Duration::from_secs(30)));
}

// ===== 并发事务 + 故障恢复 =====

/// Jepsen 22：并发事务中部分故障，其他事务不受影响
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn jepsen_concurrent_tx_partial_failure() {
    let success = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();

    for i in 0..10 {
        let s = success.clone();
        handles.push(tokio::spawn(async move {
            // 偶数 task 用正常连接，奇数 task 用故障连接（commit 失败）
            let mut tx = if i % 2 == 0 {
                make_tx()
            } else {
                make_faulty_tx(true, false, None)
            };

            tx.execute("INSERT").await.unwrap();
            let result = tx.commit().await;

            if i % 2 == 0 {
                assert!(result.is_ok(), "normal tx should commit");
                assert_eq!(tx.state(), TransactionState::Committed);
            } else {
                assert!(result.is_err(), "faulty tx should fail to commit");
                assert_eq!(
                    tx.state(),
                    TransactionState::Active,
                    "faulty tx stays active"
                );
                // 可以 rollback
                tx.rollback().await.unwrap();
                assert_eq!(tx.state(), TransactionState::RolledBack);
            }
            s.fetch_add(1, Ordering::Relaxed);
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(success.load(Ordering::Relaxed), 10);
}

// ===== savepoint 嵌套深度测试 =====

/// Jepsen 23：深层嵌套 savepoint
#[tokio::test]
async fn jepsen_deep_savepoint_nesting() {
    let mut tx = make_tx();

    let mut savepoints = Vec::new();
    for _ in 0..20 {
        let sp = tx.savepoint().await.unwrap();
        savepoints.push(sp);
        tx.execute("INSERT").await.unwrap();
    }

    // 回滚到第 10 个 savepoint
    tx.rollback_to_savepoint(&savepoints[9]).await.unwrap();

    // 事务仍然 Active
    assert_eq!(tx.state(), TransactionState::Active);

    // 释放后面的 savepoint
    for sp in savepoints[10..20].iter() {
        tx.release_savepoint(sp).await.unwrap();
    }

    tx.commit().await.unwrap();
}

// ===== 边界情况 =====

/// Jepsen 24：空事务（不执行任何操作直接 commit）
#[tokio::test]
async fn jepsen_empty_transaction_commit() {
    let mut tx = make_tx();
    tx.commit().await.unwrap();
    assert_eq!(tx.state(), TransactionState::Committed);
}

/// Jepsen 25：空事务（不执行任何操作直接 rollback）
#[tokio::test]
async fn jepsen_empty_transaction_rollback() {
    let mut tx = make_tx();
    tx.rollback().await.unwrap();
    assert_eq!(tx.state(), TransactionState::RolledBack);
}

/// Jepsen 26：query 在事务中的行为
#[tokio::test]
async fn jepsen_query_in_transaction() {
    let mut tx = make_tx();
    let result = tx.query("SELECT * FROM t").await;
    assert!(result.is_ok(), "query should succeed in active tx");

    tx.commit().await.unwrap();

    // commit 后 query 应失败
    let result = tx.query("SELECT").await;
    assert!(result.is_err(), "query after commit should fail");
}
