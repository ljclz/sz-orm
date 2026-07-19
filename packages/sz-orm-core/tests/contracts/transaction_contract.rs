//! Transaction 模块契约测试 — 对应 `docs/api-contracts.md` §6
//!
//! 锁定事务状态机、保存点命名、连接消费、v0.2.0 行为变更：
//! - `commit()` 后 state=Committed，再次 commit 返回 NotActive
//! - `commit()` 后 savepoint() 返回 NotActive(state)，不再返回 SavepointError
//! - savepoint 名称格式 "sp_N"
//! - `take_connection()` 仅在 Committed/RolledBack 状态可用
//! - `Transaction::new` 接受 `Box<dyn Connection>`，不接受 PooledConnection

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use sz_orm_core::IsolationLevel;
use sz_orm_core::TxError;
use sz_orm_core::{TransactOptions, Transaction, TransactionState};

use crate::common::{InMemoryDb, MockConnection};

// ===== 辅助函数 =====

fn make_tx() -> Transaction {
    let db = Arc::new(Mutex::new(InMemoryDb::new()));
    let conn = MockConnection::new(db);
    Transaction::new(Box::new(conn), TransactOptions::default())
}

// ===== §6.1 事务状态机契约 =====

#[tokio::test]
async fn test_new_transaction_is_active_contract() {
    let tx = make_tx();
    assert_eq!(tx.state(), TransactionState::Active);
    assert!(tx.is_active());
}

#[tokio::test]
async fn test_commit_transitions_state_contract() {
    let mut tx = make_tx();
    tx.commit().await.unwrap();
    assert_eq!(tx.state(), TransactionState::Committed);
    assert!(!tx.is_active());
}

#[tokio::test]
async fn test_rollback_transitions_state_contract() {
    let mut tx = make_tx();
    tx.rollback().await.unwrap();
    assert_eq!(tx.state(), TransactionState::RolledBack);
    assert!(!tx.is_active());
}

#[tokio::test]
async fn test_double_commit_returns_not_active_contract() {
    let mut tx = make_tx();
    tx.commit().await.unwrap();

    // 再次 commit 应返回 NotActive(Committed)
    let err = tx.commit().await.unwrap_err();
    match err {
        TxError::NotActive(state) => {
            assert_eq!(state, TransactionState::Committed);
        }
        other => panic!("期望 NotActive(Committed)，实际: {:?}", other),
    }
}

#[tokio::test]
async fn test_rollback_after_commit_returns_not_active_contract() {
    let mut tx = make_tx();
    tx.commit().await.unwrap();

    let err = tx.rollback().await.unwrap_err();
    match err {
        TxError::NotActive(state) => {
            assert_eq!(state, TransactionState::Committed);
        }
        other => panic!("期望 NotActive(Committed)，实际: {:?}", other),
    }
}

#[tokio::test]
async fn test_execute_after_commit_returns_not_active_contract() {
    let mut tx = make_tx();
    tx.commit().await.unwrap();

    let err = tx.execute("SELECT 1").await.unwrap_err();
    assert!(matches!(err, TxError::NotActive(_)));
}

#[tokio::test]
async fn test_query_after_commit_returns_not_active_contract() {
    let mut tx = make_tx();
    tx.commit().await.unwrap();

    let err = tx.query("SELECT 1").await.unwrap_err();
    assert!(matches!(err, TxError::NotActive(_)));
}

// ===== §6.1 v0.2.0 行为变更：savepoint 在非 Active 状态返回 NotActive =====

#[tokio::test]
async fn test_savepoint_after_commit_returns_not_active_contract() {
    // v0.2.0 行为变更：
    // commit/rollback 后调用 savepoint() 必须返回 Err(TxError::NotActive(state))
    // 而不是 Err(TxError::SavepointError(...))
    let mut tx = make_tx();
    tx.commit().await.unwrap();

    let err = tx.savepoint().await.unwrap_err();
    match err {
        TxError::NotActive(state) => {
            assert_eq!(state, TransactionState::Committed);
        }
        TxError::SavepointError(_) => {
            panic!("v0.2.0 契约违反：commit 后 savepoint 应返回 NotActive，而非 SavepointError");
        }
        other => panic!("期望 NotActive(Committed)，实际: {:?}", other),
    }
}

#[tokio::test]
async fn test_savepoint_after_rollback_returns_not_active_contract() {
    let mut tx = make_tx();
    tx.rollback().await.unwrap();

    let err = tx.savepoint().await.unwrap_err();
    assert!(matches!(err, TxError::NotActive(_)));
}

// ===== §6.1 savepoint 名称格式 "sp_N" 契约 =====

#[tokio::test]
async fn test_savepoint_name_format_contract() {
    let mut tx = make_tx();

    let sp1 = tx.savepoint().await.unwrap();
    assert_eq!(sp1, "sp_1");

    let sp2 = tx.savepoint().await.unwrap();
    assert_eq!(sp2, "sp_2");

    let sp3 = tx.savepoint().await.unwrap();
    assert_eq!(sp3, "sp_3");
}

#[tokio::test]
async fn test_savepoint_name_monotonic_increment_contract() {
    let mut tx = make_tx();
    let mut names = Vec::new();
    for _ in 0..10 {
        names.push(tx.savepoint().await.unwrap());
    }
    // 名称应单调递增
    for (i, name) in names.iter().enumerate() {
        assert_eq!(name, &format!("sp_{}", i + 1));
    }
}

// ===== §6.1 rollback_to_savepoint / release_savepoint 契约 =====

#[tokio::test]
async fn test_rollback_to_savepoint_contract() {
    let mut tx = make_tx();
    let sp = tx.savepoint().await.unwrap();
    tx.rollback_to_savepoint(&sp).await.unwrap();
    tx.release_savepoint(&sp).await.unwrap();
}

#[tokio::test]
async fn test_rollback_to_invalid_savepoint_name_contract() {
    let mut tx = make_tx();
    // 非法名称（以数字开头）
    let err = tx.rollback_to_savepoint("1bad").await.unwrap_err();
    assert!(matches!(err, TxError::InvalidSavepointName(_)));

    // 非法名称（含特殊字符）
    let err = tx.rollback_to_savepoint("bad-name!").await.unwrap_err();
    assert!(matches!(err, TxError::InvalidSavepointName(_)));

    // 空名称
    let err = tx.rollback_to_savepoint("").await.unwrap_err();
    assert!(matches!(err, TxError::InvalidSavepointName(_)));
}

#[tokio::test]
async fn test_release_savepoint_invalid_name_contract() {
    let mut tx = make_tx();
    let err = tx.release_savepoint("1bad").await.unwrap_err();
    assert!(matches!(err, TxError::InvalidSavepointName(_)));
}

// ===== §6.1 take_connection 契约 =====

#[tokio::test]
async fn test_take_connection_in_active_returns_not_active_contract() {
    // take_connection() 在 Active 状态返回 NotActive
    let mut tx = make_tx();
    let result = tx.take_connection().await;
    match result {
        Err(TxError::NotActive(state)) => {
            assert_eq!(state, TransactionState::Active);
        }
        Err(other) => panic!("期望 NotActive(Active)，实际: {:?}", other),
        Ok(_) => panic!("Active 状态 take_connection 必须返回 Err，实际返回 Ok"),
    }
}

#[tokio::test]
async fn test_take_connection_after_commit_contract() {
    let mut tx = make_tx();
    tx.commit().await.unwrap();

    let conn = tx.take_connection().await.unwrap();
    assert!(conn.is_connected());
}

#[tokio::test]
async fn test_take_connection_after_rollback_contract() {
    let mut tx = make_tx();
    tx.rollback().await.unwrap();

    let conn = tx.take_connection().await.unwrap();
    assert!(conn.is_connected());
}

#[tokio::test]
async fn test_take_connection_twice_returns_connection_taken_contract() {
    let mut tx = make_tx();
    tx.commit().await.unwrap();

    let _first = tx.take_connection().await.unwrap();
    // 重复 take_connection 应返回 ConnectionTaken
    let result = tx.take_connection().await;
    match result {
        Err(TxError::ConnectionTaken) => { /* 契约满足 */ }
        Err(other) => panic!("期望 ConnectionTaken，实际: {:?}", other),
        Ok(_) => panic!("重复 take_connection 必须返回 Err，实际返回 Ok"),
    }
}

// ===== §6.2 TransactOptions 契约 =====

#[test]
fn test_transact_options_default_contract() {
    let opts = TransactOptions::default();
    // 默认无隔离级别（None）
    assert!(opts.isolation_level.is_none());
    assert!(!opts.read_only);
    assert!(opts.timeout.is_none());
}

#[test]
fn test_transact_options_with_isolation_contract() {
    let opts = TransactOptions::default().with_isolation(IsolationLevel::Serializable);
    assert_eq!(opts.isolation_level, Some(IsolationLevel::Serializable));
}

#[test]
fn test_transact_options_read_only_contract() {
    let opts = TransactOptions::default().read_only();
    assert!(opts.read_only);
}

#[test]
fn test_transact_options_with_timeout_contract() {
    let opts = TransactOptions::default().with_timeout(Duration::from_secs(30));
    assert_eq!(opts.timeout, Some(Duration::from_secs(30)));
}

#[test]
fn test_transact_options_chaining_contract() {
    // 链式 API 应返回 Self
    let opts = TransactOptions::default()
        .with_isolation(IsolationLevel::RepeatableRead)
        .read_only()
        .with_timeout(Duration::from_secs(10));
    assert_eq!(opts.isolation_level, Some(IsolationLevel::RepeatableRead));
    assert!(opts.read_only);
    assert_eq!(opts.timeout, Some(Duration::from_secs(10)));
}

// ===== §6.1 options() / state() 不 panic 契约 =====

#[tokio::test]
async fn test_options_and_state_dont_panic_contract() {
    let mut tx = make_tx();
    let _opts = tx.options();
    let _state = tx.state();
    tx.commit().await.unwrap();
    let _opts2 = tx.options();
    let _state2 = tx.state();
}
