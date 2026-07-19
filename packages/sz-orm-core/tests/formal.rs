//! Formal 形式化验证测试套件
//!
//! 通过穷举状态空间和属性测试验证系统不变量：
//! - 事务状态机：穷举所有 (state, operation) 对，验证状态转换正确性
//! - 连接池不变量：属性测试验证 active <= max_size、active = idle + borrowed
//! - 终态不可逃逸：Committed/RolledBack 后所有操作必失败
//! - 状态单调性：事务只能从 Active 转向终态，不能回退
//!
//! 形式化不变量（Invariant）：
//! I1: ∀ s ∈ {Committed, RolledBack}: operations(s) → Err
//! I2: ∀ s ∈ {Committed, RolledBack}: state(s, op) = s（状态不变）
//! I3: active_count ≤ max_size（池安全）
//! I4: active_count = idle_count + borrowed_count（守恒）
//! I5: closed(pool) → release(conn) → conn.is_connected() = false（close 后释放必关闭）
//! I6: savepoint_counter 单调递增
//! I7: 事务状态单调：Active → terminal，不可回退

mod common;

use common::MockConnectionFactory;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use sz_orm_core::TransactOptions;
use sz_orm_core::Transaction;
use sz_orm_core::{Pool, PoolConfigBuilder};
use tokio::sync::Mutex;

// ===================== 形式化模型定义 =====================

/// 形式化事务状态（与 TransactionState 一一对应）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FormalTxState {
    Active,
    Committed,
    RolledBack,
}

impl FormalTxState {
    /// 是否为终态
    fn is_terminal(&self) -> bool {
        matches!(self, FormalTxState::Committed | FormalTxState::RolledBack)
    }
}

/// 形式化操作（R3 修复：补充 RollbackToSavepoint 和 ReleaseSavepoint）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FormalOp {
    Execute,
    Commit,
    Rollback,
    Savepoint,
    Query,
    RollbackToSavepoint,
    ReleaseSavepoint,
}

/// 所有操作列表（用于穷举，共 7 个操作）
const ALL_OPS: [FormalOp; 7] = [
    FormalOp::Execute,
    FormalOp::Commit,
    FormalOp::Rollback,
    FormalOp::Savepoint,
    FormalOp::Query,
    FormalOp::RollbackToSavepoint,
    FormalOp::ReleaseSavepoint,
];

/// 所有状态列表（用于穷举）
const ALL_STATES: [FormalTxState; 3] = [
    FormalTxState::Active,
    FormalTxState::Committed,
    FormalTxState::RolledBack,
];

/// 形式化状态转换函数（规约）
/// 返回 (新状态, 操作是否成功)
/// 这是"理想"的状态机规约（happy path），实际实现应与之匹配
/// 注意：R2 修复 — 本规约建模 happy path（Active 下操作成功）；
/// Active 下操作失败的分支由 `formal_active_failure_preserves_state` 单独验证
fn formal_transition(state: FormalTxState, op: FormalOp) -> (FormalTxState, bool) {
    match (state, op) {
        // Active 状态下的转换（R3：补充 RollbackToSavepoint/ReleaseSavepoint）
        (FormalTxState::Active, FormalOp::Execute) => (FormalTxState::Active, true),
        (FormalTxState::Active, FormalOp::Query) => (FormalTxState::Active, true),
        (FormalTxState::Active, FormalOp::Savepoint) => (FormalTxState::Active, true),
        (FormalTxState::Active, FormalOp::RollbackToSavepoint) => (FormalTxState::Active, true),
        (FormalTxState::Active, FormalOp::ReleaseSavepoint) => (FormalTxState::Active, true),
        (FormalTxState::Active, FormalOp::Commit) => (FormalTxState::Committed, true),
        (FormalTxState::Active, FormalOp::Rollback) => (FormalTxState::RolledBack, true),

        // 终态：所有操作失败，状态不变（I1, I2）
        (FormalTxState::Committed, _) => (FormalTxState::Committed, false),
        (FormalTxState::RolledBack, _) => (FormalTxState::RolledBack, false),
    }
}

/// 执行实际操作并返回 (新状态, 是否成功)
async fn actual_transition(
    tx: &mut Transaction,
    state_before: FormalTxState,
    op: FormalOp,
) -> (FormalTxState, bool) {
    let result = match op {
        FormalOp::Execute => tx.execute("SELECT 1").await.map(|_| ()),
        FormalOp::Query => tx.query("SELECT 1").await.map(|_| ()),
        FormalOp::Savepoint => tx.savepoint().await.map(|_| ()),
        FormalOp::Commit => tx.commit().await,
        FormalOp::Rollback => tx.rollback().await,
        FormalOp::RollbackToSavepoint => tx.rollback_to_savepoint("sp_test").await.map(|_| ()),
        FormalOp::ReleaseSavepoint => tx.release_savepoint("sp_test").await.map(|_| ()),
    };
    let success = result.is_ok();
    let state_after = match tx.state() {
        sz_orm_core::TransactionState::Active => FormalTxState::Active,
        sz_orm_core::TransactionState::Committed => FormalTxState::Committed,
        sz_orm_core::TransactionState::RolledBack => FormalTxState::RolledBack,
    };

    // 验证状态单调性（I7）：如果操作前是终态，操作后必须仍是同一终态
    if state_before.is_terminal() {
        assert_eq!(
            state_after, state_before,
            "I7 violated: terminal state {:?} changed to {:?} after {:?}",
            state_before, state_after, op
        );
    }

    // 验证 I1：终态后操作必失败
    if state_before.is_terminal() {
        assert!(
            !success,
            "I1 violated: operation {:?} succeeded in terminal state {:?}",
            op, state_before
        );
    }

    (state_after, success)
}

fn make_tx() -> Transaction {
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    let conn = common::MockConnection::new(db);
    Transaction::new(Box::new(conn), TransactOptions::default())
}

// ===================== I1 + I2: 终态不可逃逸 =====================

/// Formal 1：穷举 (Committed, op) 对，验证所有操作失败且状态不变
#[tokio::test]
async fn formal_committed_state_rejects_all_ops() {
    for op in ALL_OPS.iter() {
        let mut tx = make_tx();
        tx.commit().await.unwrap();
        assert_eq!(
            tx.state(),
            sz_orm_core::TransactionState::Committed,
            "precondition: should be Committed"
        );

        let state_before = FormalTxState::Committed;
        let (state_after, success) = actual_transition(&mut tx, state_before, *op).await;

        // I1：操作必失败
        assert!(
            !success,
            "I1 failed: {:?} should not succeed in Committed state",
            op
        );
        // I2：状态不变
        assert_eq!(
            state_after,
            FormalTxState::Committed,
            "I2 failed: state changed from Committed to {:?} after {:?}",
            state_after,
            op
        );
    }
}

/// Formal 2：穷举 (RolledBack, op) 对，验证所有操作失败且状态不变
#[tokio::test]
async fn formal_rolledback_state_rejects_all_ops() {
    for op in ALL_OPS.iter() {
        let mut tx = make_tx();
        tx.rollback().await.unwrap();
        assert_eq!(
            tx.state(),
            sz_orm_core::TransactionState::RolledBack,
            "precondition: should be RolledBack"
        );

        let state_before = FormalTxState::RolledBack;
        let (state_after, success) = actual_transition(&mut tx, state_before, *op).await;

        // I1：操作必失败
        assert!(
            !success,
            "I1 failed: {:?} should not succeed in RolledBack state",
            op
        );
        // I2：状态不变
        assert_eq!(
            state_after,
            FormalTxState::RolledBack,
            "I2 failed: state changed from RolledBack to {:?} after {:?}",
            state_after,
            op
        );
    }
}

// ===================== I3: 状态单调性 =====================

/// Formal 3：状态单调性 - Active 只能转向终态，终态不可回退
#[tokio::test]
async fn formal_state_monotonic_progress() {
    // Active → Committed → (任何 op) 仍是 Committed
    let mut tx = make_tx();
    assert!(tx.is_active());
    tx.commit().await.unwrap();
    assert_eq!(tx.state(), sz_orm_core::TransactionState::Committed);

    // 尝试所有操作，状态都不应变
    for op in ALL_OPS.iter() {
        let _ = actual_transition(&mut tx, FormalTxState::Committed, *op).await;
        assert_eq!(
            tx.state(),
            sz_orm_core::TransactionState::Committed,
            "monotonicity violated after {:?}",
            op
        );
    }

    // Active → RolledBack → (任何 op) 仍是 RolledBack
    let mut tx2 = make_tx();
    tx2.rollback().await.unwrap();
    assert_eq!(tx2.state(), sz_orm_core::TransactionState::RolledBack);

    for op in ALL_OPS.iter() {
        let _ = actual_transition(&mut tx2, FormalTxState::RolledBack, *op).await;
        assert_eq!(
            tx2.state(),
            sz_orm_core::TransactionState::RolledBack,
            "monotonicity violated after {:?}",
            op
        );
    }
}

// ===================== I4: 状态机穷举验证（精化匹配） =====================

/// Formal 4：穷举所有 (state, op) 对，验证实际实现匹配形式化规约
#[tokio::test]
async fn formal_state_machine_matches_specification() {
    for &initial_state in ALL_STATES.iter() {
        for &op in ALL_OPS.iter() {
            let mut tx = make_tx();

            // 将事务置入 initial_state
            match initial_state {
                FormalTxState::Active => { /* 默认即是 Active */ }
                FormalTxState::Committed => {
                    tx.commit().await.unwrap();
                }
                FormalTxState::RolledBack => {
                    tx.rollback().await.unwrap();
                }
            }

            // 验证初始状态正确
            let actual_initial = match tx.state() {
                sz_orm_core::TransactionState::Active => FormalTxState::Active,
                sz_orm_core::TransactionState::Committed => FormalTxState::Committed,
                sz_orm_core::TransactionState::RolledBack => FormalTxState::RolledBack,
            };
            assert_eq!(
                actual_initial, initial_state,
                "failed to set up initial state {:?}",
                initial_state
            );

            // 执行操作
            let (actual_new_state, actual_success) =
                actual_transition(&mut tx, initial_state, op).await;

            // 与形式化规约对比
            let (expected_new_state, expected_success) = formal_transition(initial_state, op);

            assert_eq!(
                actual_success, expected_success,
                "success mismatch: state={:?} op={:?} expected_success={} actual_success={}",
                initial_state, op, expected_success, actual_success
            );
            assert_eq!(
                actual_new_state, expected_new_state,
                "state mismatch: state={:?} op={:?} expected={:?} actual={:?}",
                initial_state, op, expected_new_state, actual_new_state
            );
        }
    }
}

// ===================== I5: savepoint 计数器单调递增 =====================

/// Formal 5：savepoint_counter 单调递增
/// 验证：连续创建 savepoint，名称中的数字单调递增
#[tokio::test]
async fn formal_savepoint_counter_monotonic() {
    let mut tx = make_tx();
    let mut prev_n: u32 = 0;

    for _ in 0..10 {
        let name = tx.savepoint().await.unwrap();
        // 解析 sp_N 中的 N
        let n: u32 = name.trim_start_matches("sp_").parse().unwrap();
        assert!(
            n > prev_n,
            "savepoint counter not monotonic: prev={} current={}",
            prev_n,
            n
        );
        prev_n = n;
    }
}

// ===================== I6: 操作序列属性测试 =====================

/// Formal 6：随机操作序列下状态机不变量保持
/// 验证：任意操作序列下，状态单调推进，终态后无操作成功
#[tokio::test]
async fn formal_random_operation_sequence_invariants() {
    let mut rng = common::Rng::new(12345);

    for seq_id in 0..20 {
        let mut tx = make_tx();
        let mut current_state = FormalTxState::Active;
        let seq_len = 5 + rng.next_usize(10);

        for step in 0..seq_len {
            let op = ALL_OPS[rng.next_usize(ALL_OPS.len())];

            let (new_state, success) = actual_transition(&mut tx, current_state, op).await;

            // 与规约对比
            let (expected_state, expected_success) = formal_transition(current_state, op);
            assert_eq!(
                success, expected_success,
                "seq={} step={} op={:?}: success mismatch",
                seq_id, step, op
            );
            assert_eq!(
                new_state, expected_state,
                "seq={} step={} op={:?}: state mismatch",
                seq_id, step, op
            );

            current_state = new_state;
        }
    }
}

// ===================== 连接池不变量 =====================

/// Formal 7：I3 - active_count <= max_size（穷举多并发场景）
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn formal_pool_active_never_exceeds_max_size() {
    let max_size: u32 = 5;
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    let factory = Arc::new(MockConnectionFactory::new(db));
    let config = PoolConfigBuilder::new()
        .max_size(max_size)
        .min_idle(0)
        .acquire_timeout(1)
        .build()
        .unwrap();
    let pool: &'static Pool = Box::leak(Box::new(Pool::new(config, factory).unwrap()));

    let max_observed = Arc::new(AtomicU32::new(0));
    let mut handles = Vec::new();

    for _ in 0..8 {
        let max_obs = max_observed.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..30 {
                if let Ok(conn) = pool.acquire().await {
                    let status = pool.status().await;
                    max_obs.fetch_max(status.active, Ordering::SeqCst);
                    // I3 验证：active 必须 <= max_size
                    assert!(
                        status.active <= max_size,
                        "I3 violated: active={} > max_size={}",
                        status.active,
                        max_size
                    );
                    tokio::task::yield_now().await;
                    pool.release(conn).await;
                }
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert!(
        max_observed.load(Ordering::SeqCst) <= max_size,
        "I3 violated: max observed active = {}",
        max_observed.load(Ordering::SeqCst)
    );
}

/// Formal 8：I4 - active = idle + borrowed（守恒律）
#[tokio::test]
async fn formal_pool_active_equals_idle_plus_borrowed() {
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    let factory = Arc::new(MockConnectionFactory::new(db));
    let config = PoolConfigBuilder::new()
        .max_size(10)
        .min_idle(0)
        .acquire_timeout(2)
        .build()
        .unwrap();
    let pool: &'static Pool = Box::leak(Box::new(Pool::new(config, factory).unwrap()));

    // 借出 3 个
    let mut borrowed = Vec::new();
    for _ in 0..3 {
        borrowed.push(pool.acquire().await.unwrap());
    }
    let status = pool.status().await;
    // I4: active = idle + borrowed
    // 此时 idle=0, borrowed=3, active 应=3
    assert_eq!(status.active, 3, "active should be 3 after borrowing 3");
    assert_eq!(status.idle, 0, "idle should be 0");
    let borrowed_count = status.active - status.idle;
    assert_eq!(
        borrowed_count, 3,
        "borrowed count = active - idle should be 3"
    );

    // 释放 2 个
    let conn1 = borrowed.pop().unwrap();
    let conn2 = borrowed.pop().unwrap();
    pool.release(conn1).await;
    pool.release(conn2).await;

    let status = pool.status().await;
    // 此时 idle=2, borrowed=1, active 应=3
    assert_eq!(status.active, 3, "active should be 3 (2 idle + 1 borrowed)");
    assert_eq!(status.idle, 2, "idle should be 2");
    let borrowed_count = status.active - status.idle;
    assert_eq!(borrowed_count, 1, "borrowed count should be 1");

    // 释放最后一个
    let conn3 = borrowed.pop().unwrap();
    pool.release(conn3).await;
    let status = pool.status().await;
    assert_eq!(status.active, 3, "active should be 3 (3 idle + 0 borrowed)");
    assert_eq!(status.idle, 3, "idle should be 3");
    let borrowed_count = status.active - status.idle;
    assert_eq!(borrowed_count, 0, "borrowed count should be 0");
}

/// Formal 9：I5 - close_all 后 release 必关闭连接，active_count 递减
#[tokio::test]
async fn formal_pool_close_all_invariant() {
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    let factory = Arc::new(MockConnectionFactory::new(db));
    let config = PoolConfigBuilder::new()
        .max_size(5)
        .min_idle(0)
        .acquire_timeout(2)
        .build()
        .unwrap();
    let pool: &'static Pool = Box::leak(Box::new(Pool::new(config, factory).unwrap()));

    // 借出 3 个
    let c1 = pool.acquire().await.unwrap();
    let c2 = pool.acquire().await.unwrap();
    let c3 = pool.acquire().await.unwrap();
    let status = pool.status().await;
    assert_eq!(status.active, 3);
    assert_eq!(status.idle, 0);

    // close_all
    pool.close_all().await;
    let status = pool.status().await;
    assert_eq!(status.idle, 0, "idle should be 0 after close_all");
    // active 仍为 3（借出的连接未归还）
    assert_eq!(
        status.active, 3,
        "active should still be 3 (borrowed not returned)"
    );

    // 释放借出的连接：应被直接关闭，active 递减
    pool.release(c1).await;
    let status = pool.status().await;
    assert_eq!(status.active, 2, "active should be 2 after releasing 1");
    assert_eq!(
        status.idle, 0,
        "idle should still be 0 (closed, not returned)"
    );

    pool.release(c2).await;
    pool.release(c3).await;
    let status = pool.status().await;
    assert_eq!(status.active, 0, "active should be 0 after releasing all");
    assert_eq!(status.idle, 0, "idle should be 0");
}

/// Formal 10：close_all 后 acquire 被拒绝（池已关闭）
/// 验证：close_all 后 acquire 返回 PoolError::Closed
#[tokio::test]
async fn formal_pool_close_all_then_acquire_creates_new() {
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    let factory = Arc::new(MockConnectionFactory::new(db));
    let config = PoolConfigBuilder::new()
        .max_size(5)
        .min_idle(0)
        .acquire_timeout(2)
        .build()
        .unwrap();
    let pool: &'static Pool = Box::leak(Box::new(Pool::new(config, factory).unwrap()));

    // 创建一个连接并释放
    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;
    let status = pool.status().await;
    assert_eq!(status.idle, 1);

    // close_all 清空 idle 并标记池为已关闭
    pool.close_all().await;
    let status = pool.status().await;
    assert_eq!(status.idle, 0);
    assert_eq!(status.active, 0);

    // close_all 后 acquire 应被拒绝（池已关闭）
    let conn = pool.acquire().await;
    assert!(
        conn.is_err(),
        "acquire after close_all should be rejected (pool closed)"
    );
}

// ===================== 综合属性测试 =====================

/// Formal 11：连接池在任意操作序列下守恒律保持
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn formal_pool_conservation_under_random_ops() {
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    let factory = Arc::new(MockConnectionFactory::new(db));
    let config = PoolConfigBuilder::new()
        .max_size(10)
        .min_idle(0)
        .acquire_timeout(2)
        .build()
        .unwrap();
    let pool: &'static Pool = Box::leak(Box::new(Pool::new(config, factory).unwrap()));

    let ops_completed = Arc::new(AtomicU32::new(0));
    let mut handles = Vec::new();
    let mut rng = common::Rng::new(42);

    for _ in 0..6 {
        let ops_c = ops_completed.clone();
        let seed = rng.next_u64();
        handles.push(tokio::spawn(async move {
            let mut local_rng = common::Rng::new(seed);
            let mut held: Vec<sz_orm_core::PooledConnection> = Vec::new();

            for _ in 0..40 {
                let action = local_rng.next_usize(3);
                match action {
                    0 => {
                        // acquire
                        if let Ok(conn) = pool.acquire().await {
                            let status = pool.status().await;
                            // I3 验证
                            assert!(
                                status.active <= 10,
                                "I3 violated: active={} > 10",
                                status.active
                            );
                            // I4 验证：active >= idle（borrowed >= 0）
                            assert!(
                                status.active >= status.idle,
                                "I4 violated: active={} < idle={}",
                                status.active,
                                status.idle
                            );
                            held.push(conn);
                        }
                    }
                    1 => {
                        // release
                        if let Some(conn) = held.pop() {
                            pool.release(conn).await;
                            let status = pool.status().await;
                            assert!(
                                status.active >= status.idle,
                                "I4 violated after release: active={} < idle={}",
                                status.active,
                                status.idle
                            );
                        }
                    }
                    _ => {
                        // status check
                        let status = pool.status().await;
                        assert!(
                            status.active <= 10,
                            "I3 violated on check: active={}",
                            status.active
                        );
                    }
                }
                ops_c.fetch_add(1, Ordering::Relaxed);
            }

            // 释放所有持有的连接
            for conn in held {
                pool.release(conn).await;
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // 最终守恒：active = idle（无借出）
    let status = pool.status().await;
    assert_eq!(
        status.active, status.idle,
        "I4 final: active should equal idle (no borrowed), got active={} idle={}",
        status.active, status.idle
    );
    assert!(
        status.active <= 10,
        "I3 final: active={} > max_size=10",
        status.active
    );
}

/// Formal 12：事务状态机 - 操作序列空间穷举（深度 3）
/// 验证：所有长度为 3 的操作序列下，状态机行为匹配规约
#[tokio::test]
async fn formal_state_machine_depth_3_exhaustive() {
    // 穷举所有长度为 3 的操作序列
    // 总数：5^3 = 125 个序列
    for &op1 in ALL_OPS.iter() {
        for &op2 in ALL_OPS.iter() {
            for &op3 in ALL_OPS.iter() {
                let mut tx = make_tx();
                let mut state = FormalTxState::Active;

                for (step, &op) in [op1, op2, op3].iter().enumerate() {
                    let (new_state, success) = actual_transition(&mut tx, state, op).await;
                    let (expected_state, expected_success) = formal_transition(state, op);

                    assert_eq!(
                        success, expected_success,
                        "depth=3 seq=({:?},{:?},{:?}) step={}: success mismatch",
                        op1, op2, op3, step
                    );
                    assert_eq!(
                        new_state, expected_state,
                        "depth=3 seq=({:?},{:?},{:?}) step={}: state mismatch",
                        op1, op2, op3, step
                    );

                    state = new_state;
                }
            }
        }
    }
}

/// Formal 13：连接池 - reap_idle 后所有 idle 连接均未过期
#[tokio::test]
async fn formal_pool_reap_idle_invariant() {
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    let factory = Arc::new(MockConnectionFactory::new(db));
    let config = PoolConfigBuilder::new()
        .max_size(5)
        .min_idle(0)
        .max_lifetime(1)
        .idle_timeout(1)
        .acquire_timeout(2)
        .build()
        .unwrap();
    let pool: &'static Pool = Box::leak(Box::new(Pool::new(config, factory).unwrap()));

    // 创建 3 个连接并释放
    let mut conns = Vec::new();
    for _ in 0..3 {
        conns.push(pool.acquire().await.unwrap());
    }
    for conn in conns {
        pool.release(conn).await;
    }

    // 等待一部分过期
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 再创建 2 个连接并释放（这些不应该过期）
    let mut conns2 = Vec::new();
    for _ in 0..2 {
        conns2.push(pool.acquire().await.unwrap());
    }
    for conn in conns2 {
        pool.release(conn).await;
    }

    // 等待前 3 个过期（但后 2 个不应过期）
    tokio::time::sleep(Duration::from_millis(700)).await;

    // reap_idle
    pool.reap_idle().await;

    // 验证：idle 中的连接都不应过期（reap_idle 已清理过期的）
    // 注意：reap_idle 后 idle 可能是 0（如果所有都过期）或 2（后创建的未过期）
    let status = pool.status().await;
    assert!(
        status.idle <= 2,
        "after reap_idle, idle should be at most 2 (the newer ones), got {}",
        status.idle
    );
    // active 应等于 idle（守恒）
    assert_eq!(
        status.active, status.idle,
        "after reap_idle: active should equal idle, got active={} idle={}",
        status.active, status.idle
    );
}

// ===================== R2: Active 状态失败分支不变量 =====================

/// Formal 14：R2 - Active 状态下操作失败时状态必须保持 Active
/// 验证：execute/commit/rollback/savepoint 失败不应改变事务状态
/// 规约：Active + op(fail) → (Active, false)（状态不变，操作失败）
#[tokio::test]
async fn formal_active_failure_preserves_state() {
    // 1. execute 失败 → 状态保持 Active
    {
        let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
        let mut conn = common::FaultyConnection::new(db);
        conn.fail_on_execute_n = Some(1); // 第 1 次 execute 失败
        let mut tx = Transaction::new(Box::new(conn), TransactOptions::default());
        let result = tx.execute("SELECT 1").await;
        assert!(result.is_err(), "execute should fail");
        assert_eq!(
            tx.state(),
            sz_orm_core::TransactionState::Active,
            "I7: state must stay Active after execute failure"
        );
    }

    // 2. commit 失败 → 状态保持 Active
    {
        let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
        let mut conn = common::FaultyConnection::new(db);
        conn.fail_on_commit = true;
        let mut tx = Transaction::new(Box::new(conn), TransactOptions::default());
        let result = tx.commit().await;
        assert!(result.is_err(), "commit should fail");
        assert_eq!(
            tx.state(),
            sz_orm_core::TransactionState::Active,
            "I7: state must stay Active after commit failure"
        );
    }

    // 3. rollback 失败 → 状态保持 Active
    {
        let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
        let mut conn = common::FaultyConnection::new(db);
        conn.fail_on_rollback = true;
        let mut tx = Transaction::new(Box::new(conn), TransactOptions::default());
        let result = tx.rollback().await;
        assert!(result.is_err(), "rollback should fail");
        assert_eq!(
            tx.state(),
            sz_orm_core::TransactionState::Active,
            "I7: state must stay Active after rollback failure"
        );
    }

    // 4. savepoint 失败（内部调用 execute 失败）→ 状态保持 Active
    {
        let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
        let mut conn = common::FaultyConnection::new(db);
        conn.fail_on_execute_n = Some(1); // savepoint 内部调用 execute
        let mut tx = Transaction::new(Box::new(conn), TransactOptions::default());
        let result = tx.savepoint().await;
        assert!(result.is_err(), "savepoint should fail (execute fails)");
        assert_eq!(
            tx.state(),
            sz_orm_core::TransactionState::Active,
            "I7: state must stay Active after savepoint failure"
        );
    }
}
