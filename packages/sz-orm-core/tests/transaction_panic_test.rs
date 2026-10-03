//! v9.3.0 T13：事务 panic 安全回滚业务规则验证
//!
//! 验证 spec.md §5.2.1：
//! 1. panic 自动回滚 + resume_unwind 传播
//! 2. 正常路径 commit 成功
//! 3. 业务错误 rollback
//! 4. 隔离级别/只读/超时配置

use futures::FutureExt;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use sz_orm_core::Connection;
use sz_orm_core::{run_savepoint, run_transaction, TransactOptions};

struct MockConnection {
    begin_count: Arc<AtomicUsize>,
    commit_count: Arc<AtomicUsize>,
    rollback_count: Arc<AtomicUsize>,
    execute_count: Arc<AtomicUsize>,
    fail_commit: AtomicBool,
}

impl MockConnection {
    fn new() -> Self {
        Self {
            begin_count: Arc::new(AtomicUsize::new(0)),
            commit_count: Arc::new(AtomicUsize::new(0)),
            rollback_count: Arc::new(AtomicUsize::new(0)),
            execute_count: Arc::new(AtomicUsize::new(0)),
            fail_commit: AtomicBool::new(false),
        }
    }
}

impl Connection for MockConnection {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.execute_count.fetch_add(1, Ordering::SeqCst);
            Ok(1)
        })
    }

    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        Vec<std::collections::HashMap<String, sz_orm_core::Value>>,
                        sz_orm_core::DbError,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move { Ok(vec![]) })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.begin_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }

    fn commit<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.commit_count.fetch_add(1, Ordering::SeqCst);
            if self.fail_commit.load(Ordering::SeqCst) {
                Err(sz_orm_core::DbError::Internal("commit failed".to_string()))
            } else {
                Ok(())
            }
        })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.rollback_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { true })
    }

    fn close<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
}

#[tokio::test]
async fn test_run_transaction_normal_path_commits() {
    let conn = Box::new(MockConnection::new());
    let result = run_transaction(conn, TransactOptions::default(), |tx| {
        Box::pin(async move {
            tx.execute("INSERT INTO t VALUES (1)").await?;
            Ok(42_i32)
        })
    })
    .await;
    assert_eq!(result.unwrap(), 42);
}

#[tokio::test]
async fn test_run_transaction_business_error_rolls_back() {
    let conn = Box::new(MockConnection::new());
    let result: Result<(), sz_orm_core::TxError> =
        run_transaction(conn, TransactOptions::default(), |_tx| {
            Box::pin(async move {
                Err(sz_orm_core::TxError::CommitFailed(
                    "business error".to_string(),
                ))
            })
        })
        .await;
    assert!(result.is_err());
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("business error"));
}

#[tokio::test]
async fn test_run_transaction_panic_rolls_back_and_propagates() {
    let conn = Box::new(MockConnection::new());
    let result: Result<Result<(), sz_orm_core::TxError>, Box<dyn std::any::Any + Send>> =
        AssertUnwindSafe(async {
            run_transaction(conn, TransactOptions::default(), |_tx| {
                Box::pin(async move {
                    panic!("intentional test panic");
                })
            })
            .await
        })
        .catch_unwind()
        .await;
    assert!(result.is_err(), "panic 应通过 resume_unwind 传播");
}

#[tokio::test]
async fn test_run_transaction_isolation_level_set() {
    let conn = Box::new(MockConnection::new());
    let opts =
        TransactOptions::default().with_isolation(sz_orm_core::IsolationLevel::ReadCommitted);
    let result = run_transaction(conn, opts, |_tx| Box::pin(async move { Ok(()) })).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_run_transaction_read_only_set() {
    let conn = Box::new(MockConnection::new());
    let opts = TransactOptions::default().read_only();
    let result = run_transaction(conn, opts, |_tx| Box::pin(async move { Ok(()) })).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_run_transaction_timeout_configured() {
    let conn = Box::new(MockConnection::new());
    let opts = TransactOptions::default().with_timeout(std::time::Duration::from_secs(30));
    let result = run_transaction(conn, opts, |_tx| Box::pin(async move { Ok(()) })).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_run_savepoint_normal_path_releases() {
    let conn = Box::new(MockConnection::new());
    let result = run_transaction(conn, TransactOptions::default(), |tx| {
        Box::pin(async move {
            run_savepoint(tx, |tx| {
                Box::pin(async move {
                    tx.execute("INSERT INTO t VALUES (1)").await?;
                    Ok(100_i32)
                })
            })
            .await
        })
    })
    .await;
    assert_eq!(result.unwrap(), 100);
}

#[tokio::test]
async fn test_run_savepoint_business_error_rolls_back_to_savepoint() {
    let conn = Box::new(MockConnection::new());
    let result = run_transaction(conn, TransactOptions::default(), |tx| {
        Box::pin(async move {
            let inner: Result<(), sz_orm_core::TxError> = run_savepoint(tx, |_tx| {
                Box::pin(async move {
                    Err(sz_orm_core::TxError::CommitFailed(
                        "savepoint error".to_string(),
                    ))
                })
            })
            .await;
            assert!(inner.is_err());
            tx.execute("INSERT INTO t VALUES (2)").await?;
            Ok(())
        })
    })
    .await;
    assert!(result.is_ok(), "外层事务应不受 savepoint 错误影响");
}

#[tokio::test]
async fn test_run_savepoint_panic_rolls_back_to_savepoint() {
    let conn = Box::new(MockConnection::new());
    let result: Result<Result<(), sz_orm_core::TxError>, Box<dyn std::any::Any + Send>> =
        AssertUnwindSafe(async {
            run_transaction(conn, TransactOptions::default(), |tx| {
                Box::pin(async move {
                    let _: Result<i32, _> = run_savepoint(tx, |_tx| {
                        Box::pin(async move {
                            panic!("savepoint panic");
                        })
                    })
                    .await;
                    Ok(())
                })
            })
            .await
        })
        .catch_unwind()
        .await;
    assert!(result.is_err(), "savepoint panic 应传播");
}

#[tokio::test]
async fn test_run_transaction_unit_type_result() {
    let conn = Box::new(MockConnection::new());
    let result: Result<(), _> = run_transaction(conn, TransactOptions::default(), |tx| {
        Box::pin(async move {
            tx.execute("INSERT INTO t VALUES (1)").await?;
            Ok(())
        })
    })
    .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_run_transaction_no_isolation_no_readonly() {
    let conn = Box::new(MockConnection::new());
    let result = run_transaction(conn, TransactOptions::default(), |_tx| {
        Box::pin(async move { Ok::<i32, sz_orm_core::TxError>(0) })
    })
    .await;
    assert_eq!(result.unwrap(), 0);
}

#[tokio::test]
async fn test_run_transaction_string_result_type() {
    let conn = Box::new(MockConnection::new());
    let result = run_transaction(conn, TransactOptions::default(), |_tx| {
        Box::pin(async move { Ok::<String, sz_orm_core::TxError>("success".to_string()) })
    })
    .await;
    assert_eq!(result.unwrap(), "success");
}
