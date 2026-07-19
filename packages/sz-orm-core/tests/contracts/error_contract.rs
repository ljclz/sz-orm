//! Error 模块契约测试 — 对应 `docs/api-contracts.md` §3
//!
//! 锁定错误码稳定性、可重试性判断、错误变体完整性。
//! **错误码字符串不可变**——破坏会断日志匹配。

use sz_orm_core::TransactionState;
use sz_orm_core::{CacheError, DbError, PoolError, TxError};

// ===== §3.1 DbError 错误码稳定性契约 =====

#[test]
fn test_db_error_codes_are_stable_contract() {
    // 错误码字符串不可变（破坏会断日志匹配）
    assert_eq!(DbError::query("test").error_code(), "DB001");
    assert_eq!(DbError::connection("x").error_code(), "DB002");
    assert_eq!(DbError::PoolError(PoolError::Timeout).error_code(), "PL002");
}

#[test]
fn test_db_error_is_retryable_contract() {
    // 实际实现：ConnectionError / ConnectionTimeout / PoolError::Timeout 可重试
    assert!(DbError::ConnectionTimeout("x".to_string()).is_retryable());
    assert!(DbError::ConnectionError("x".to_string()).is_retryable());
    assert!(DbError::PoolError(PoolError::Timeout).is_retryable());
    // 其他错误不可重试（包括 ConnectionRefused — 当前实现未将其标记为可重试）
    assert!(!DbError::ConnectionRefused("x".to_string()).is_retryable());
    assert!(!DbError::query("x").is_retryable());
    assert!(!DbError::not_found("x").is_retryable());
}

#[test]
fn test_db_error_display_stability_contract() {
    // Display 实现稳定（影响日志/告警匹配）
    assert_eq!(
        format!("{}", DbError::query("invalid SQL")),
        "Query error: invalid SQL"
    );
    assert_eq!(
        format!("{}", DbError::not_found("user 123")),
        "Not found: user 123"
    );
}

#[test]
fn test_db_error_wrapping_pool_error_contract() {
    // DbError::PoolError(_) 包装 PoolError
    let err = DbError::PoolError(PoolError::Timeout);
    assert_eq!(err.error_code(), "PL002");
    // source() 应返回 Some(&PoolError)
    use std::error::Error;
    assert!(err.source().is_some());
}

// ===== §3.2 PoolError 错误码契约 =====

#[test]
fn test_pool_error_codes_contract() {
    // PoolError 8 变体，PL001-PL008
    assert_eq!(PoolError::Exhausted.error_code(), "PL001");
    assert_eq!(PoolError::Timeout.error_code(), "PL002");
    assert_eq!(PoolError::AlreadyAcquired.error_code(), "PL003");
    assert_eq!(PoolError::NotAcquired.error_code(), "PL004");
    assert_eq!(
        PoolError::InvalidConfig("x".to_string()).error_code(),
        "PL005"
    );
    assert_eq!(PoolError::Internal("x".to_string()).error_code(), "PL006");
    assert_eq!(PoolError::Closed.error_code(), "PL007");
    assert_eq!(
        PoolError::ConnectionFailed("x".to_string()).error_code(),
        "PL008"
    );
}

#[test]
fn test_pool_error_display_stability_contract() {
    assert_eq!(
        format!("{}", PoolError::Timeout),
        "Connection acquire timeout"
    );
    assert_eq!(format!("{}", PoolError::Closed), "Connection pool closed");
    assert_eq!(
        format!("{}", PoolError::Exhausted),
        "Connection pool exhausted"
    );
}

// ===== §3.3 TxError 契约 =====

#[test]
fn test_tx_error_variants_contract() {
    // TxError 9 变体
    let _ = TxError::NotStarted;
    let _ = TxError::AlreadyStarted;
    let _ = TxError::CommitFailed("x".to_string());
    let _ = TxError::RollbackFailed("x".to_string());
    let _ = TxError::SavepointError("x".to_string());
    let _ = TxError::NestedNotSupported;
    let _ = TxError::NotActive(TransactionState::Active);
    let _ = TxError::InvalidSavepointName("x".to_string());
    let _ = TxError::ConnectionTaken;
}

#[test]
fn test_tx_error_not_active_displays_state_contract() {
    // NotActive 变体应包含状态信息
    let err = TxError::NotActive(TransactionState::Committed);
    let msg = format!("{}", err);
    assert!(
        msg.contains("Committed"),
        "msg should contain state, got: {}",
        msg
    );
}

#[test]
fn test_tx_error_invalid_savepoint_name_contract() {
    let err = TxError::InvalidSavepointName("1bad".to_string());
    let msg = format!("{}", err);
    assert!(
        msg.contains("1bad"),
        "msg should contain name, got: {}",
        msg
    );
}

// ===== §3.4 CacheError 契约 =====

#[test]
fn test_cache_error_codes_contract() {
    // CacheError 6 变体，CH001-CH006
    assert_eq!(CacheError::NotFound("k".to_string()).error_code(), "CH001");
    assert_eq!(
        CacheError::SerializationError("x".to_string()).error_code(),
        "CH002"
    );
    assert_eq!(
        CacheError::DeserializationError("x".to_string()).error_code(),
        "CH003"
    );
    assert_eq!(
        CacheError::ConnectionError("x".to_string()).error_code(),
        "CH004"
    );
    assert_eq!(CacheError::Timeout("x".to_string()).error_code(), "CH005");
    assert_eq!(CacheError::Internal("x".to_string()).error_code(), "CH006");
}

#[test]
fn test_cache_error_display_stability_contract() {
    assert_eq!(
        format!("{}", CacheError::NotFound("cache_key".to_string())),
        "Cache key not found: cache_key"
    );
}

// ===== §3.3 TxError 行为变更契约（v0.2.0） =====

#[tokio::test]
async fn test_v020_savepoint_after_commit_returns_not_active_contract() {
    // v0.2.0 行为变更：
    // commit/rollback 后调用 savepoint() 必须返回 Err(TxError::NotActive(state))
    // 而不是 Err(TxError::SavepointError(...))
    use crate::common::{InMemoryDb, MockConnection};
    use std::sync::Arc;
    use sz_orm_core::{TransactOptions, Transaction};
    use tokio::sync::Mutex;

    let db = Arc::new(Mutex::new(InMemoryDb::new()));
    let conn = MockConnection::new(db);
    let mut tx = Transaction::new(Box::new(conn), TransactOptions::default());

    tx.commit().await.unwrap();

    let err = tx.savepoint().await.unwrap_err();
    match err {
        TxError::NotActive(state) => {
            assert_eq!(state, TransactionState::Committed);
        }
        TxError::SavepointError(_) => {
            panic!("v0.2.0 契约违反：savepoint() 后 commit 应返回 NotActive，而非 SavepointError");
        }
        other => panic!("期望 NotActive，实际: {:?}", other),
    }
}
